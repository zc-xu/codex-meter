import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const projectRoot = new URL("..", import.meta.url).pathname;
const args = process.argv.slice(2);
const command = args.shift() || "--version";
const environment = { ...process.env };

if (process.platform === "darwin") {
  const localClt =
    environment.CODEX_METER_CLT_DIR ||
    join(
      homedir(),
      ".local",
      "share",
      "codex-meter-clt-root",
      "Payload",
      "Library",
      "Developer",
      "CommandLineTools"
    );

  if (existsSync(join(localClt, "usr", "bin", "clang"))) {
    environment.DEVELOPER_DIR = localClt;
    environment.SDKROOT = join(localClt, "SDKs", "MacOSX.sdk");
    environment.PATH = [
      join(homedir(), ".cargo", "bin"),
      join(localClt, "usr", "bin"),
      environment.PATH
    ].join(":");
  }
}

if (command === "test") {
  const child = spawn(
    join(homedir(), ".cargo", "bin", "cargo"),
    ["test", "--manifest-path", "src-tauri/Cargo.toml", ...args],
    {
      cwd: projectRoot,
      env: environment,
      stdio: "inherit"
    }
  );
  child.on("exit", (code) => process.exit(code ?? 1));
} else {
  const cli =
    process.platform === "win32"
      ? join(projectRoot, "node_modules", ".bin", "tauri.cmd")
      : join(projectRoot, "node_modules", ".bin", "tauri");
  const child = spawn(cli, [command, ...args], {
    cwd: projectRoot,
    env: environment,
    stdio: "inherit"
  });
  child.on("exit", (code) => process.exit(code ?? 1));
}
