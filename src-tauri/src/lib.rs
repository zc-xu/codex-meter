use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
    WindowEvent,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const WINDOW_WIDTH: f64 = 516.0;
const WINDOW_HEIGHT: f64 = 184.0;
const SETTINGS_HEIGHT: f64 = 420.0;
const RPC_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_ROLLOUT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    always_on_top: bool,
    launch_at_login: bool,
    opacity: f64,
    refresh_interval_sec: u64,
    renewal_date: String,
    theme: String,
    codex_path: String,
    display_mode: String,
    window_x: Option<i32>,
    window_y: Option<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            always_on_top: true,
            launch_at_login: false,
            opacity: 0.96,
            refresh_interval_sec: 15,
            renewal_date: String::new(),
            theme: "system".into(),
            codex_path: String::new(),
            display_mode: "tray".into(),
            window_x: None,
            window_y: None,
        }
    }
}

impl Settings {
    fn sanitize(mut self) -> Self {
        self.opacity = self.opacity.clamp(0.72, 1.0);
        if ![10, 15, 30, 60].contains(&self.refresh_interval_sec) {
            self.refresh_interval_sec = 15;
        }
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        if !["tray", "desktop"].contains(&self.display_mode.as_str()) {
            self.display_mode = "tray".into();
        }
        if !self.renewal_date.is_empty()
            && (self.renewal_date.len() != 10
                || self
                    .renewal_date
                    .chars()
                    .enumerate()
                    .any(|(index, character)| {
                        if index == 4 || index == 7 {
                            character != '-'
                        } else {
                            !character.is_ascii_digit()
                        }
                    }))
        {
            self.renewal_date.clear();
        }
        self
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPatch {
    always_on_top: Option<bool>,
    launch_at_login: Option<bool>,
    opacity: Option<f64>,
    refresh_interval_sec: Option<u64>,
    renewal_date: Option<String>,
    theme: Option<String>,
    codex_path: Option<String>,
    display_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshot {
    connected: bool,
    source_error: Option<String>,
    executable: Option<String>,
    synced_at_ms: u64,
    plan: String,
    used_percent: f64,
    remaining_percent: f64,
    reset_at_sec: Option<u64>,
    window_duration_mins: Option<u64>,
    secondary_used_percent: Option<f64>,
    secondary_reset_at_sec: Option<u64>,
    today_tokens: u64,
    today_bucket_date: Option<String>,
    seven_day_average: f64,
    lifetime_tokens: u64,
    current_task_tokens: u64,
    last_turn_tokens: u64,
    active_task_count: usize,
    task_state: String,
    task_title: String,
    task_started_at_ms: Option<u64>,
    task_updated_at_sec: Option<u64>,
    renewal_date: Option<String>,
}

impl UsageSnapshot {
    fn disconnected(
        error: impl Into<String>,
        settings: &Settings,
        executable: Option<String>,
    ) -> Self {
        Self {
            connected: false,
            source_error: Some(error.into()),
            executable,
            synced_at_ms: now_ms(),
            plan: "Codex".into(),
            used_percent: 0.0,
            remaining_percent: 0.0,
            reset_at_sec: None,
            window_duration_mins: None,
            secondary_used_percent: None,
            secondary_reset_at_sec: None,
            today_tokens: 0,
            today_bucket_date: None,
            seven_day_average: 0.0,
            lifetime_tokens: 0,
            current_task_tokens: 0,
            last_turn_tokens: 0,
            active_task_count: 0,
            task_state: "idle".into(),
            task_title: "无法读取 Codex 本地数据".into(),
            task_started_at_ms: None,
            task_updated_at_sec: None,
            renewal_date: non_empty(&settings.renewal_date),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RolloutActivity {
    active: bool,
    task_started_at_ms: u64,
    task_finished_at_ms: u64,
    total_tokens: u64,
    last_tokens: u64,
}

#[derive(Debug, Clone)]
struct ThreadSummary {
    name: String,
    updated_at: u64,
    status: Value,
    activity: RolloutActivity,
}

struct AppState {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    settings_open: AtomicBool,
    snapshot: Mutex<Option<UsageSnapshot>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn non_empty(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn read_settings(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
        .sanitize()
}

fn write_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary_path = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(&temporary_path, format!("{text}\n")).map_err(|error| error.to_string())?;
    fs::rename(&temporary_path, path).map_err(|error| error.to_string())
}

fn apply_patch(settings: &mut Settings, patch: SettingsPatch) {
    if let Some(value) = patch.always_on_top {
        settings.always_on_top = value;
    }
    if let Some(value) = patch.launch_at_login {
        settings.launch_at_login = value;
    }
    if let Some(value) = patch.opacity {
        settings.opacity = value;
    }
    if let Some(value) = patch.refresh_interval_sec {
        settings.refresh_interval_sec = value;
    }
    if let Some(value) = patch.renewal_date {
        settings.renewal_date = value;
    }
    if let Some(value) = patch.theme {
        settings.theme = value;
    }
    if let Some(value) = patch.codex_path {
        settings.codex_path = value;
    }
    if let Some(value) = patch.display_mode {
        settings.display_mode = value;
    }
    *settings = settings.clone().sanitize();
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let autolaunch = app.autolaunch();
    if enabled {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }
}

fn apply_window_mode(app: &AppHandle, settings: &Settings) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let desktop = settings.display_mode == "desktop";
    let _ = window.set_always_on_top(false);
    let _ = window.set_always_on_bottom(false);
    if desktop {
        let _ = window.set_always_on_bottom(true);
        let _ = window.set_visible_on_all_workspaces(true);
        if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
            let _ = window.set_position(PhysicalPosition::new(x, y));
        } else {
            place_top_right(&window);
        }
        let _ = window.show();
    } else {
        let _ = window.set_always_on_top(settings.always_on_top);
        let _ = window.set_visible_on_all_workspaces(true);
    }
    let _ = window.emit("settings:changed", settings);
}

fn place_top_right(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        let _ = window.center();
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x = monitor_position.x + monitor_size.width as i32 - WINDOW_WIDTH as i32 - 24;
    let y = monitor_position.y + 34;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_near_tray(app: &AppHandle, click: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    if settings.display_mode == "desktop" {
        let _ = window.show();
        return;
    }

    let size = window
        .outer_size()
        .unwrap_or_else(|_| PhysicalSize::new(WINDOW_WIDTH as u32, WINDOW_HEIGHT as u32));
    let mut x = click.x - size.width as f64 / 2.0;
    #[cfg(target_os = "macos")]
    let mut y = click.y + 12.0;
    #[cfg(not(target_os = "macos"))]
    let mut y = click.y - size.height as f64 - 12.0;

    if let Ok(Some(monitor)) = window.monitor_from_point(click.x, click.y) {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let minimum_x = monitor_position.x as f64 + 8.0;
        let maximum_x =
            monitor_position.x as f64 + monitor_size.width as f64 - size.width as f64 - 8.0;
        let minimum_y = monitor_position.y as f64 + 8.0;
        let maximum_y =
            monitor_position.y as f64 + monitor_size.height as f64 - size.height as f64 - 8.0;
        x = x.clamp(minimum_x, maximum_x.max(minimum_x));
        y = y.clamp(minimum_y, maximum_y.max(minimum_y));
    }

    let _ = window.set_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("widget:shown", ());
}

fn open_settings_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    app.state::<AppState>()
        .settings_open
        .store(true, Ordering::Relaxed);
    let _ = window.set_size(LogicalSize::new(WINDOW_WIDTH, SETTINGS_HEIGHT));
    if !window.is_visible().unwrap_or(false) {
        place_top_right(&window);
        let _ = window.show();
    }
    let _ = window.set_focus();
    let _ = window.emit("settings:open", ());
}

fn refresh_and_emit(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let settings = app.state::<AppState>().settings.lock().unwrap().clone();
        let result =
            tauri::async_runtime::spawn_blocking(move || collect_snapshot(&settings)).await;
        if let Ok(snapshot) = result {
            *app.state::<AppState>().snapshot.lock().unwrap() = Some(snapshot.clone());
            let _ = app.emit("usage:snapshot", snapshot);
        }
    });
}

fn resolve_codex_binary(settings: &Settings) -> String {
    if !settings.codex_path.trim().is_empty() && Path::new(&settings.codex_path).is_file() {
        return settings.codex_path.clone();
    }
    if let Ok(candidate) = std::env::var("CODEX_WIDGET_CODEX_BIN")
        && Path::new(&candidate).is_file()
    {
        return candidate;
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/ChatGPT.app/Contents/Resources/codex".to_string(),
            std::env::var("HOME")
                .map(|home| format!("{home}/Applications/ChatGPT.app/Contents/Resources/codex"))
                .unwrap_or_default(),
        ];
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| Path::new(candidate).is_file())
        {
            return candidate.clone();
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();
        let candidates = [
            format!("{local}\\Programs\\ChatGPT\\resources\\codex.exe"),
            format!("{local}\\OpenAI\\ChatGPT\\resources\\codex.exe"),
            format!("{program_files}\\ChatGPT\\resources\\codex.exe"),
        ];
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| Path::new(candidate).is_file())
        {
            return candidate.clone();
        }
    }

    if cfg!(target_os = "windows") {
        "codex.exe".into()
    } else {
        "codex".into()
    }
}

fn send_rpc(stdin: &mut impl Write, id: u64, method: &str, params: Value) -> Result<(), String> {
    let message = json!({ "id": id, "method": method, "params": params });
    writeln!(stdin, "{message}").map_err(|error| error.to_string())?;
    stdin.flush().map_err(|error| error.to_string())
}

fn wait_for_responses(
    receiver: &mpsc::Receiver<Value>,
    wanted: &[u64],
) -> Result<HashMap<u64, Value>, String> {
    let deadline = Instant::now() + RPC_TIMEOUT;
    let mut responses = HashMap::new();
    while responses.len() < wanted.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Codex 本地接口响应超时".into());
        }
        let message = receiver
            .recv_timeout(remaining)
            .map_err(|_| "Codex 本地接口响应超时".to_string())?;
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            continue;
        };
        if !wanted.contains(&id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            let detail = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Codex 请求失败");
            return Err(detail.to_string());
        }
        responses.insert(id, message.get("result").cloned().unwrap_or(Value::Null));
    }
    Ok(responses)
}

fn spawn_app_server(executable: &str) -> Result<(Child, mpsc::Receiver<Value>), String> {
    let mut command = Command::new(executable);
    command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("找不到 Codex 可执行文件，请在设置中指定路径：{error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 Codex 本地接口".to_string())?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                let _ = sender.send(message);
            }
        }
    });
    Ok((child, receiver))
}

fn rpc_snapshot(executable: &str) -> Result<(Value, Value, Value), String> {
    let (mut child, receiver) = spawn_app_server(executable)?;
    let result = (|| {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "无法连接 Codex 本地接口".to_string())?;
        send_rpc(
            stdin,
            1,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "codex-meter",
                    "title": "Codex Meter",
                    "version": "0.2.0"
                },
                "capabilities": null
            }),
        )?;
        wait_for_responses(&receiver, &[1])?;

        send_rpc(stdin, 2, "account/rateLimits/read", json!({}))?;
        send_rpc(stdin, 3, "account/usage/read", json!({}))?;
        send_rpc(
            stdin,
            4,
            "thread/list",
            json!({
                "limit": 20,
                "sortKey": "updated_at",
                "sortDirection": "desc",
                "archived": false,
                "useStateDbOnly": true
            }),
        )?;
        let mut responses = wait_for_responses(&receiver, &[2, 3, 4])?;
        Ok((
            responses.remove(&2).unwrap_or(Value::Null),
            responses.remove(&3).unwrap_or(Value::Null),
            responses.remove(&4).unwrap_or(Value::Null),
        ))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn parse_timestamp_ms(timestamp: &str) -> u64 {
    // Rollout timestamps are RFC3339. Lexicographic ordering is enough for
    // lifecycle comparison, while task elapsed time uses this compact parser.
    let bytes = timestamp.as_bytes();
    if bytes.len() < 20 {
        return 0;
    }
    let number = |start: usize, end: usize| -> Option<u64> {
        std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
    };
    let year = number(0, 4).unwrap_or(1970) as i64;
    let month = number(5, 7).unwrap_or(1) as i64;
    let day = number(8, 10).unwrap_or(1) as i64;
    let hour = number(11, 13).unwrap_or(0) as i64;
    let minute = number(14, 16).unwrap_or(0) as i64;
    let second = number(17, 19).unwrap_or(0) as i64;
    let millis = if bytes.get(19) == Some(&b'.') {
        let end = timestamp[20..]
            .find(|character: char| !character.is_ascii_digit())
            .map(|index| 20 + index)
            .unwrap_or(timestamp.len());
        let fraction = &timestamp[20..end.min(timestamp.len())];
        let mut padded = fraction.chars().take(3).collect::<String>();
        while padded.len() < 3 {
            padded.push('0');
        }
        padded.parse::<i64>().unwrap_or(0)
    } else {
        0
    };

    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146_097 + day_of_era - 719_468;
    ((days_since_epoch * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000 + millis).max(0)
        as u64
}

fn parse_rollout_text(text: &str) -> RolloutActivity {
    let mut activity = RolloutActivity::default();
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = record.get("payload") else {
            continue;
        };
        let event_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let timestamp_ms = record
            .get("timestamp")
            .and_then(Value::as_str)
            .map(parse_timestamp_ms)
            .unwrap_or(0);
        match event_type {
            "task_started" => {
                activity.task_started_at_ms = activity.task_started_at_ms.max(timestamp_ms)
            }
            "task_complete" | "turn_aborted" | "turn_interrupted" => {
                activity.task_finished_at_ms = activity.task_finished_at_ms.max(timestamp_ms)
            }
            "token_count" => {
                if let Some(info) = payload.get("info") {
                    activity.total_tokens = info
                        .pointer("/total_token_usage/total_tokens")
                        .or_else(|| info.pointer("/totalTokenUsage/totalTokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(activity.total_tokens);
                    activity.last_tokens = info
                        .pointer("/last_token_usage/total_tokens")
                        .or_else(|| info.pointer("/lastTokenUsage/totalTokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(activity.last_tokens);
                }
            }
            _ => {}
        }
    }
    activity.active = activity.task_started_at_ms > activity.task_finished_at_ms;
    activity
}

fn inspect_rollout(path: &str) -> RolloutActivity {
    let Ok(mut file) = File::open(path) else {
        return RolloutActivity::default();
    };
    let Ok(metadata) = file.metadata() else {
        return RolloutActivity::default();
    };
    let start = metadata.len().saturating_sub(MAX_ROLLOUT_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return RolloutActivity::default();
    }
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        return RolloutActivity::default();
    }
    if start > 0 {
        if let Some(index) = text.find('\n') {
            text = text[index + 1..].to_string();
        } else {
            text.clear();
        }
    }
    parse_rollout_text(&text)
}

fn plan_label(plan_type: &str) -> String {
    match plan_type {
        "free" => "Free",
        "go" => "Go",
        "plus" => "Plus",
        "pro" => "Pro",
        "prolite" => "Pro Lite",
        "team" => "Team",
        "business" | "self_serve_business_usage_based" => "Business",
        "enterprise" | "enterprise_cbp_usage_based" => "Enterprise",
        "edu" => "Edu",
        _ => "Codex",
    }
    .into()
}

fn safe_title(thread: &Value) -> String {
    let value = thread
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| thread.get("preview").and_then(Value::as_str))
        .unwrap_or("Codex 任务");
    value
        .replace(['\r', '\n', '\t'], " ")
        .chars()
        .take(96)
        .collect()
}

fn collect_threads(response: &Value) -> Vec<ThreadSummary> {
    let mut threads = response
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(12)
        .map(|thread| {
            let mut activity = thread
                .get("path")
                .and_then(Value::as_str)
                .map(inspect_rollout)
                .unwrap_or_default();
            let status = thread.get("status").cloned().unwrap_or(Value::Null);
            if status.get("type").and_then(Value::as_str) == Some("active") {
                activity.active = true;
            }
            ThreadSummary {
                name: safe_title(thread),
                updated_at: thread.get("updatedAt").and_then(Value::as_u64).unwrap_or(0),
                status,
                activity,
            }
        })
        .collect::<Vec<_>>();
    threads.sort_by(|left, right| {
        right
            .activity
            .active
            .cmp(&left.activity.active)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    threads
}

fn collect_snapshot(settings: &Settings) -> UsageSnapshot {
    let executable = resolve_codex_binary(settings);
    let executable_display = Some(executable.clone());
    let (rate_response, usage_response, thread_response) = match rpc_snapshot(&executable) {
        Ok(response) => response,
        Err(error) => {
            return UsageSnapshot::disconnected(error, settings, executable_display);
        }
    };

    let rate_limit = rate_response
        .pointer("/rateLimitsByLimitId/codex")
        .or_else(|| rate_response.get("rateLimits"))
        .unwrap_or(&Value::Null);
    let primary = rate_limit.get("primary").unwrap_or(&Value::Null);
    let secondary = rate_limit.get("secondary").filter(|value| !value.is_null());
    let used_percent = primary
        .get("usedPercent")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);

    let daily = usage_response
        .get("dailyUsageBuckets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let latest_bucket = daily.last();
    let recent = daily.iter().rev().take(7).collect::<Vec<_>>();
    let seven_day_average = if recent.is_empty() {
        0.0
    } else {
        recent
            .iter()
            .map(|bucket| bucket.get("tokens").and_then(Value::as_u64).unwrap_or(0) as f64)
            .sum::<f64>()
            / recent.len() as f64
    };

    let threads = collect_threads(&thread_response);
    let active_task_count = threads
        .iter()
        .filter(|thread| thread.activity.active)
        .count();
    let focus = threads.first();
    let active_flags = focus
        .and_then(|thread| thread.status.get("activeFlags"))
        .and_then(Value::as_array);
    let waiting_approval = active_flags
        .map(|flags| {
            flags
                .iter()
                .any(|flag| flag.as_str() == Some("waitingOnApproval"))
        })
        .unwrap_or(false);
    let waiting_input = active_flags
        .map(|flags| {
            flags
                .iter()
                .any(|flag| flag.as_str() == Some("waitingOnUserInput"))
        })
        .unwrap_or(false);
    let task_state = if waiting_approval {
        "waitingApproval"
    } else if waiting_input {
        "waitingInput"
    } else if active_task_count > 0 {
        "running"
    } else {
        "idle"
    };

    UsageSnapshot {
        connected: true,
        source_error: None,
        executable: executable_display,
        synced_at_ms: now_ms(),
        plan: plan_label(
            rate_limit
                .get("planType")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        ),
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        reset_at_sec: primary.get("resetsAt").and_then(Value::as_u64),
        window_duration_mins: primary.get("windowDurationMins").and_then(Value::as_u64),
        secondary_used_percent: secondary
            .and_then(|value| value.get("usedPercent"))
            .and_then(Value::as_f64),
        secondary_reset_at_sec: secondary
            .and_then(|value| value.get("resetsAt"))
            .and_then(Value::as_u64),
        today_tokens: latest_bucket
            .and_then(|bucket| bucket.get("tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        today_bucket_date: latest_bucket
            .and_then(|bucket| bucket.get("startDate"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        seven_day_average,
        lifetime_tokens: usage_response
            .pointer("/summary/lifetimeTokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        current_task_tokens: focus
            .map(|thread| thread.activity.total_tokens)
            .unwrap_or(0),
        last_turn_tokens: focus.map(|thread| thread.activity.last_tokens).unwrap_or(0),
        active_task_count,
        task_state: task_state.into(),
        task_title: focus
            .map(|thread| thread.name.clone())
            .unwrap_or_else(|| "目前没有任务运行".into()),
        task_started_at_ms: focus
            .filter(|_| active_task_count > 0)
            .map(|thread| thread.activity.task_started_at_ms)
            .filter(|timestamp| *timestamp > 0),
        task_updated_at_sec: focus.map(|thread| thread.updated_at),
        renewal_date: non_empty(&settings.renewal_date),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let mut settings = state.settings.lock().unwrap();
    apply_patch(&mut settings, patch);
    write_settings(&state.settings_path, &settings)?;
    let result = settings.clone();
    drop(settings);
    apply_autostart(&app, result.launch_at_login);
    apply_window_mode(&app, &result);
    Ok(result)
}

#[tauri::command]
async fn refresh_usage(state: State<'_, AppState>) -> Result<UsageSnapshot, String> {
    let settings = state.settings.lock().unwrap().clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || collect_snapshot(&settings))
        .await
        .map_err(|error| error.to_string())?;
    *state.snapshot.lock().unwrap() = Some(snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
fn get_cached_snapshot(state: State<'_, AppState>) -> Option<UsageSnapshot> {
    state.snapshot.lock().unwrap().clone()
}

#[tauri::command]
fn set_panel_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    open: bool,
) -> Result<(), String> {
    state.settings_open.store(open, Ordering::Relaxed);
    window
        .set_size(LogicalSize::new(
            WINDOW_WIDTH,
            if open { SETTINGS_HEIGHT } else { WINDOW_HEIGHT },
        ))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_widget(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            refresh_usage,
            get_cached_snapshot,
            set_panel_open,
            hide_widget,
            quit_app
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let first_run = !settings_path.exists();
            let settings = read_settings(&settings_path);
            if first_run {
                let _ = write_settings(&settings_path, &settings);
            }
            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                settings_path,
                settings_open: AtomicBool::new(false),
                snapshot: Mutex::new(None),
            });

            apply_autostart(app.handle(), settings.launch_at_login);
            apply_window_mode(app.handle(), &settings);

            let show_item = MenuItem::with_id(app, "toggle", "显示 / 隐藏", true, None::<&str>)?;
            let refresh_item = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
            let desktop_item = CheckMenuItem::with_id(
                app,
                "desktop",
                "贴在桌面",
                true,
                settings.display_mode == "desktop",
                None::<&str>,
            )?;
            let settings_item = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出 Codex Meter", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show_item,
                    &refresh_item,
                    &separator,
                    &desktop_item,
                    &settings_item,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            let desktop_item_for_menu = desktop_item.clone();
            TrayIconBuilder::with_id("codex-meter-tray")
                .tooltip("Codex Meter")
                .icon(Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        position,
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_near_tray(tray.app_handle(), position);
                    }
                })
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "toggle" => {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                place_top_right(&window);
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.emit("widget:shown", ());
                            }
                        }
                    }
                    "refresh" => refresh_and_emit(app.clone()),
                    "desktop" => {
                        let state = app.state::<AppState>();
                        let mut settings = state.settings.lock().unwrap();
                        settings.display_mode = if settings.display_mode == "desktop" {
                            "tray".into()
                        } else {
                            "desktop".into()
                        };
                        let _ = write_settings(&state.settings_path, &settings);
                        let current = settings.clone();
                        drop(settings);
                        let _ =
                            desktop_item_for_menu.set_checked(current.display_mode == "desktop");
                        apply_window_mode(app, &current);
                    }
                    "settings" => open_settings_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if first_run
                && settings.display_mode == "tray"
                && let Some(window) = app.get_webview_window("main")
            {
                place_top_right(&window);
                let _ = window.show();
                let _ = window.emit("first-run", ());
            }
            refresh_and_emit(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) => {
                let state = window.app_handle().state::<AppState>();
                let settings = state.settings.lock().unwrap().clone();
                if settings.display_mode == "tray" && !state.settings_open.load(Ordering::Relaxed) {
                    let _ = window.hide();
                }
            }
            WindowEvent::Moved(position) => {
                let state = window.app_handle().state::<AppState>();
                let mut settings = state.settings.lock().unwrap();
                if settings.display_mode == "desktop" {
                    settings.window_x = Some(position.x);
                    settings.window_y = Some(position.y);
                    let _ = write_settings(&state.settings_path, &settings);
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Meter");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(timestamp: &str, payload: Value) -> String {
        json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": payload
        })
        .to_string()
    }

    #[test]
    fn detects_running_rollout_and_tokens() {
        let text = [
            event("2026-07-27T01:00:00.000Z", json!({"type": "task_started"})),
            event(
                "2026-07-27T01:05:00.000Z",
                json!({
                    "type": "token_count",
                    "info": {
                        "total_token_usage": {"total_tokens": 86400},
                        "last_token_usage": {"total_tokens": 1200}
                    }
                }),
            ),
        ]
        .join("\n");
        let activity = parse_rollout_text(&text);
        assert!(activity.active);
        assert_eq!(activity.total_tokens, 86400);
        assert_eq!(activity.last_tokens, 1200);
    }

    #[test]
    fn completion_makes_rollout_idle() {
        let text = [
            event("2026-07-27T01:00:00.000Z", json!({"type": "task_started"})),
            event("2026-07-27T01:10:00.000Z", json!({"type": "task_complete"})),
        ]
        .join("\n");
        let activity = parse_rollout_text(&text);
        assert!(!activity.active);
        assert!(activity.task_finished_at_ms > activity.task_started_at_ms);
    }

    #[test]
    fn timestamps_are_parsed_in_milliseconds() {
        assert_eq!(parse_timestamp_ms("1970-01-01T00:00:01.250Z"), 1_250);
        assert_eq!(
            parse_timestamp_ms("2026-07-27T01:00:00.000Z"),
            1_785_114_000_000
        );
    }

    #[test]
    fn settings_are_sanitized() {
        let settings = Settings {
            opacity: 3.0,
            refresh_interval_sec: 17,
            theme: "neon".into(),
            display_mode: "window".into(),
            ..Settings::default()
        }
        .sanitize();
        assert_eq!(settings.opacity, 1.0);
        assert_eq!(settings.refresh_interval_sec, 15);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.display_mode, "tray");
    }

    #[test]
    fn plan_names_are_human_readable() {
        assert_eq!(plan_label("prolite"), "Pro Lite");
        assert_eq!(plan_label("plus"), "Plus");
        assert_eq!(plan_label("future"), "Codex");
    }

    #[test]
    fn reads_live_codex_snapshot_when_codex_is_installed() {
        let settings = Settings::default();
        let binary = resolve_codex_binary(&settings);
        if !Path::new(&binary).is_file() {
            return;
        }
        let snapshot = collect_snapshot(&settings);
        assert!(snapshot.connected, "{:?}", snapshot.source_error);
        assert!(snapshot.reset_at_sec.is_some());
        assert!(!snapshot.plan.is_empty());
    }
}
