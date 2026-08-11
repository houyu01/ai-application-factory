import { spawnSync } from "node:child_process";
import { mkdir } from "node:fs/promises";
import process from "node:process";

import { iosProjectDirectory } from "./ios-project.mjs";

await mkdir(iosProjectDirectory, { recursive: true });

const result = spawnSync(
  "npx",
  ["tauri", "ios", "init", "--ci", "--skip-targets-install"],
  { env: process.env, stdio: "inherit" },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
