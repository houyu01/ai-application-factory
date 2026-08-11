import { mkdir, readdir, rename, readFile } from "node:fs/promises";
import { join, relative } from "node:path";

import { androidReleaseApkDirectory, projectRoot } from "./android-project.mjs";

const configuration = JSON.parse(await readFile(join(projectRoot, "src-tauri", "tauri.conf.json"), "utf8"));
const distDirectory = join(projectRoot, "dist");
const apkFiles = await readdir(androidReleaseApkDirectory, { withFileTypes: true })
  .catch((error) => {
    if (error.code === "ENOENT") {
      throw new Error(`No universal Android release APK was created: ${androidReleaseApkDirectory}`);
    }
    throw error;
  })
  .then((entries) =>
    entries
      .filter((entry) => entry.isFile() && entry.name.endsWith("-release.apk"))
      .map((entry) => join(androidReleaseApkDirectory, entry.name)),
  );

if (apkFiles.length !== 1) {
  throw new Error(`Expected one universal Android release APK, found ${apkFiles.length}.`);
}

const version = configuration.version;
if (typeof version !== "string" || !version) {
  throw new Error("The Tauri configuration must define a version for Android APK naming.");
}

const destination = join(distDirectory, `ai-application-factory-${version}-android-pad.apk`);
await mkdir(distDirectory, { recursive: true });
await rename(apkFiles[0], destination);
console.log(`Android pad APK ready: ${relative(projectRoot, destination)}`);
