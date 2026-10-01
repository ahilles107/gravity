// Run the same repository gates from Windows, macOS, or Linux.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";
process.chdir(root);
process.env.PYTHONUTF8 ??= "1";

function run(program, args = []) {
  const result = program === "pnpm" && windows
    ? spawnSync("cmd.exe", ["/d", "/s", "/c", `pnpm ${args.join(" ")}`], { cwd: root, stdio: "inherit" })
    : spawnSync(program, args, { cwd: root, stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const skillTests = readdirSync(".claude/skills/lib")
  .filter((file) => file.endsWith(".test.mjs"))
  .map((file) => join(".claude/skills/lib", file));
run(process.execPath, ["--test", ...skillTests]);
const python = windows ? "python.exe" : "python3";
run(python, ["scripts/notices.py"]);
run(python, ["-m", "unittest", "discover", "-s", "scripts", "-p", "test_notices.py"]);
run("cargo", ["fmt", "--all", "--check"]);
run("cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
run("cargo", ["test", "--workspace", "--all-targets"]);

const files = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.rs", "*.css"], { encoding: "utf8" });
if (files.status !== 0) {
  throw new Error("Cannot enumerate source files for the length check");
}
for (const file of files.stdout.split("\0").filter(Boolean)) {
  const text = readFileSync(file, "utf8");
  const lines = text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
  if (lines > 400) {
    throw new Error(`${file}:${lines}: exceeds the 400-line source limit`);
  }
}
run("pnpm", ["--dir", "apps/desktop", "check"]);
run("pnpm", ["--dir", "apps/desktop", "build"]);
run("pnpm", ["--dir", "apps/marketing", "check"]);
if (windows) {
  run("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/prepare-sidecar.ps1"]);
} else {
  run("bash", ["scripts/prepare-sidecar.sh"]);
}
const native = ["--manifest-path", "apps/desktop/src-tauri/Cargo.toml"];
run("cargo", ["fmt", ...native, "--check"]);
run("cargo", ["clippy", ...native, "--all-targets", "--", "-D", "warnings"]);
run("cargo", ["test", ...native]);
const gitBash = join(process.env.ProgramFiles ?? "C:/Program Files", "Git/bin/bash.exe");
run(windows && existsSync(gitBash) ? gitBash : "bash", ["scripts/vr-ci.sh"]);
