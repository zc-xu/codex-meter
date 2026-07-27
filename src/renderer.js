"use strict";

const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const listen = tauri?.event?.listen;

const ids = [
  "widget", "dashboard", "settingsPanel", "plan", "modePill", "syncState", "syncLabel",
  "refreshButton", "settingsButton", "hideButton", "usageRing", "usedPercent", "windowLabel",
  "taskDot", "taskStatus", "taskCount", "taskTitle", "activityTrack", "taskElapsed",
  "taskTokens", "remainingPercent", "todayLabel", "todayTokens", "averageTokens",
  "lifetimeTokens", "resetAt", "secondaryLabel", "secondaryValue", "renewalAction",
  "renewalDate", "lastSync", "toast", "toastText", "closeSettingsButton", "trayModeButton",
  "desktopModeButton", "renewalInput", "refreshIntervalSelect", "themeSelect", "opacityInput",
  "opacityValue", "alwaysOnTopInput", "launchAtLoginInput", "codexPathInput",
  "dataSourceState", "quitButton"
];
const elements = Object.fromEntries(ids.map((id) => [id, document.getElementById(id)]));

let currentSnapshot = null;
let currentSettings = null;
let durationTimer = null;
let refreshTimer = null;
let toastTimer = null;

function localDateKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatTokens(value) {
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) return "—";
  return new Intl.NumberFormat("zh-CN", {
    notation: "compact",
    maximumFractionDigits: number >= 1_000_000 ? 1 : 0
  }).format(number);
}

function formatDuration(milliseconds) {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "—";
  const minutes = Math.floor(milliseconds / 60_000);
  if (minutes < 1) return "刚刚开始";
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} 小时 ${rest} 分` : `${hours} 小时`;
}

function formatReset(epochSeconds) {
  if (!epochSeconds) return "—";
  const difference = epochSeconds * 1000 - Date.now();
  if (difference <= 0) return "即将更新";
  if (difference < 48 * 60 * 60 * 1000) return `${formatDuration(difference)}后`;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false
  }).format(new Date(epochSeconds * 1000));
}

function formatClock(timestampMs) {
  if (!timestampMs) return "—";
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit", minute: "2-digit", hour12: false
  }).format(new Date(timestampMs));
}

function formatWindow(minutes) {
  if (!minutes) return "本周期";
  if (minutes % 10080 === 0) return `${minutes / 10080} 周周期`;
  if (minutes % 1440 === 0) return `${minutes / 1440} 天周期`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时周期`;
  return `${minutes} 分钟周期`;
}

function formatRenewal(dateKey) {
  if (!dateKey) return "未设置";
  const date = new Date(`${dateKey}T00:00:00`);
  if (Number.isNaN(date.getTime())) return "未设置";
  const days = Math.ceil((date.getTime() - Date.now()) / 86_400_000);
  if (days === 0) return "今天";
  if (days > 0 && days <= 30) return `${days} 天后`;
  return new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric" }).format(date);
}

function taskStateLabel(state, count) {
  if (state === "waitingApproval") return "等待审批";
  if (state === "waitingInput") return "等待输入";
  if (state === "running") return count > 1 ? `${count} 个任务运行中` : "任务运行中";
  return "当前空闲";
}

function updateTaskDuration() {
  if (!currentSnapshot) return;
  if (currentSnapshot.taskStartedAtMs && currentSnapshot.taskState !== "idle") {
    elements.taskElapsed.textContent =
      `已运行 ${formatDuration(Date.now() - currentSnapshot.taskStartedAtMs)}`;
  } else if (currentSnapshot.taskUpdatedAtSec) {
    elements.taskElapsed.textContent =
      `最近更新 ${formatClock(currentSnapshot.taskUpdatedAtSec * 1000)}`;
  } else {
    elements.taskElapsed.textContent = "当前空闲";
  }
}

function renderSnapshot(snapshot) {
  if (!snapshot) return;
  currentSnapshot = snapshot;
  const connected = Boolean(snapshot.connected);
  const used = connected ? Math.round(snapshot.usedPercent || 0) : 0;
  const taskRunning = snapshot.taskState && snapshot.taskState !== "idle";
  const waiting = snapshot.taskState === "waitingApproval" || snapshot.taskState === "waitingInput";

  elements.widget.classList.toggle("disconnected", !connected);
  elements.plan.textContent = connected ? snapshot.plan || "Codex" : "未连接";
  elements.syncLabel.textContent = connected ? "本地实时" : "连接失败";
  elements.syncState.title = snapshot.sourceError || "数据只在本机读取";
  elements.dataSourceState.textContent = connected
    ? "数据只在本机读取，不上传对话内容"
    : snapshot.sourceError || "Codex 数据源不可用";

  elements.usageRing.style.setProperty("--used", `${used * 3.6}deg`);
  elements.usedPercent.textContent = connected ? `${used}%` : "—";
  elements.windowLabel.textContent = formatWindow(snapshot.windowDurationMins);

  elements.taskDot.classList.toggle("idle", !taskRunning);
  elements.taskDot.classList.toggle("waiting", waiting);
  elements.taskStatus.textContent = taskStateLabel(snapshot.taskState, snapshot.activeTaskCount || 0);
  elements.taskCount.textContent = snapshot.activeTaskCount > 1 ? `${snapshot.activeTaskCount} active` : "";
  elements.taskTitle.textContent = connected
    ? snapshot.taskTitle || "目前没有任务运行"
    : "无法读取 Codex 本地数据";
  elements.taskTitle.title = connected ? snapshot.taskTitle || "" : snapshot.sourceError || "";
  elements.activityTrack.classList.toggle("idle", !taskRunning);
  elements.taskTokens.textContent = `${formatTokens(snapshot.currentTaskTokens)} Token`;
  updateTaskDuration();

  elements.remainingPercent.textContent = connected
    ? `${Math.round(snapshot.remainingPercent || 0)}%`
    : "—";
  const todayKey = localDateKey(new Date());
  elements.todayLabel.textContent =
    snapshot.todayBucketDate && snapshot.todayBucketDate !== todayKey
      ? snapshot.todayBucketDate.slice(5).replace("-", "/")
      : "今日";
  elements.todayTokens.textContent = formatTokens(snapshot.todayTokens);
  elements.averageTokens.textContent = formatTokens(snapshot.sevenDayAverage);
  elements.lifetimeTokens.textContent = formatTokens(snapshot.lifetimeTokens);
  elements.resetAt.textContent = formatReset(snapshot.resetAtSec);

  if (snapshot.secondaryUsedPercent !== null && snapshot.secondaryUsedPercent !== undefined) {
    elements.secondaryLabel.textContent = "短周期用量";
    elements.secondaryValue.textContent =
      `${Math.round(snapshot.secondaryUsedPercent)}% · ${formatReset(snapshot.secondaryResetAtSec)}`;
  } else {
    elements.secondaryLabel.textContent = "最近一轮";
    elements.secondaryValue.textContent = `${formatTokens(snapshot.lastTurnTokens)} Token`;
  }
  elements.renewalDate.textContent = formatRenewal(snapshot.renewalDate);
  elements.lastSync.textContent = formatClock(snapshot.syncedAtMs);

  clearInterval(durationTimer);
  if (taskRunning) durationTimer = setInterval(updateTaskDuration, 30_000);
}

function updateDragRegions(mode) {
  document.querySelectorAll("[data-drag-handle]").forEach((node) => {
    if (mode === "desktop") node.setAttribute("data-tauri-drag-region", "");
    else node.removeAttribute("data-tauri-drag-region");
  });
}

function scheduleRefresh() {
  clearInterval(refreshTimer);
  const seconds = currentSettings?.refreshIntervalSec || 15;
  refreshTimer = setInterval(refresh, seconds * 1000);
}

function renderSettings(settings) {
  if (!settings) return;
  currentSettings = settings;
  elements.widget.dataset.theme = settings.theme || "system";
  elements.widget.dataset.mode = settings.displayMode || "tray";
  elements.modePill.textContent = settings.displayMode === "desktop" ? "桌面常驻" : "菜单栏";
  elements.trayModeButton.classList.toggle("active", settings.displayMode === "tray");
  elements.desktopModeButton.classList.toggle("active", settings.displayMode === "desktop");
  elements.hideButton.title =
    settings.displayMode === "desktop" ? "暂时隐藏，可从菜单栏找回" : "收起到菜单栏";
  updateDragRegions(settings.displayMode);

  elements.renewalInput.value = settings.renewalDate || "";
  elements.refreshIntervalSelect.value = String(settings.refreshIntervalSec || 15);
  elements.themeSelect.value = settings.theme || "system";
  elements.opacityInput.value = String(Math.round((settings.opacity || 0.96) * 100));
  elements.opacityValue.textContent = `${elements.opacityInput.value}%`;
  elements.alwaysOnTopInput.checked = Boolean(settings.alwaysOnTop);
  elements.launchAtLoginInput.checked = Boolean(settings.launchAtLogin);
  elements.codexPathInput.value = settings.codexPath || "";
  document.body.style.opacity = String(settings.opacity || 0.96);
  scheduleRefresh();
}

async function saveSettings(patch) {
  const settings = await invoke("update_settings", { patch });
  renderSettings(settings);
}

async function refresh() {
  if (!invoke) return;
  elements.widget.classList.add("refreshing");
  elements.refreshButton.disabled = true;
  try {
    renderSnapshot(await invoke("refresh_usage"));
  } finally {
    elements.widget.classList.remove("refreshing");
    elements.refreshButton.disabled = false;
  }
}

async function openSettings() {
  elements.dashboard.hidden = true;
  elements.settingsPanel.hidden = false;
  await invoke("set_panel_open", { open: true });
}

async function closeSettings() {
  elements.settingsPanel.hidden = true;
  elements.dashboard.hidden = false;
  await invoke("set_panel_open", { open: false });
}

function showToast(text, duration = 5000) {
  clearTimeout(toastTimer);
  elements.toastText.textContent = text;
  elements.toast.hidden = false;
  toastTimer = setTimeout(() => {
    elements.toast.hidden = true;
  }, duration);
}

elements.refreshButton.addEventListener("click", refresh);
elements.settingsButton.addEventListener("click", openSettings);
elements.closeSettingsButton.addEventListener("click", closeSettings);
elements.hideButton.addEventListener("click", () => invoke("hide_widget"));
elements.renewalAction.addEventListener("click", openSettings);
elements.trayModeButton.addEventListener("click", () => saveSettings({ displayMode: "tray" }));
elements.desktopModeButton.addEventListener("click", () => saveSettings({ displayMode: "desktop" }));
elements.renewalInput.addEventListener("change", (event) =>
  saveSettings({ renewalDate: event.target.value })
);
elements.refreshIntervalSelect.addEventListener("change", (event) =>
  saveSettings({ refreshIntervalSec: Number(event.target.value) })
);
elements.themeSelect.addEventListener("change", (event) =>
  saveSettings({ theme: event.target.value })
);
elements.opacityInput.addEventListener("input", (event) => {
  elements.opacityValue.textContent = `${event.target.value}%`;
  document.body.style.opacity = String(Number(event.target.value) / 100);
});
elements.opacityInput.addEventListener("change", (event) =>
  saveSettings({ opacity: Number(event.target.value) / 100 })
);
elements.alwaysOnTopInput.addEventListener("change", (event) =>
  saveSettings({ alwaysOnTop: event.target.checked })
);
elements.launchAtLoginInput.addEventListener("change", (event) =>
  saveSettings({ launchAtLogin: event.target.checked })
);
elements.codexPathInput.addEventListener("change", (event) =>
  saveSettings({ codexPath: event.target.value.trim() }).then(refresh)
);
elements.quitButton.addEventListener("click", () => invoke("quit_app"));

async function initialize() {
  if (!invoke || !listen) {
    elements.syncLabel.textContent = "仅限 Tauri";
    return;
  }
  const [settings, cached] = await Promise.all([
    invoke("get_settings"),
    invoke("get_cached_snapshot")
  ]);
  renderSettings(settings);
  if (cached) renderSnapshot(cached);

  await Promise.all([
    listen("usage:snapshot", (event) => renderSnapshot(event.payload)),
    listen("settings:changed", (event) => renderSettings(event.payload)),
    listen("widget:shown", () => refresh()),
    listen("settings:open", () => openSettings()),
    listen("first-run", () =>
      showToast("已驻留菜单栏，点击仪表图标可随时打开", 7000)
    )
  ]);
  await refresh();
}

initialize().catch((error) => {
  renderSnapshot({
    connected: false,
    sourceError: String(error?.message || error),
    syncedAtMs: Date.now(),
    taskState: "idle",
    taskTitle: "无法读取 Codex 本地数据"
  });
});
