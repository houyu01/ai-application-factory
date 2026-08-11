import { spawnSync } from "node:child_process";
import process from "node:process";

const command = process.platform === "win32" ? "npx.cmd" : "npx";
const result = spawnSync(command, ["tauri", "android", "init", "--ci"], {
  env: process.env,
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
