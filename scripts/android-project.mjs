import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const androidProjectDirectory = join(projectRoot, "src-tauri", "gen", "android");
export const androidAppDirectory = join(androidProjectDirectory, "app");
export const androidReleaseApkDirectory = join(
  androidAppDirectory,
  "build",
  "outputs",
  "apk",
  "universal",
  "release",
);
