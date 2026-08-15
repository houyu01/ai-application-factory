import { existsSync, readdirSync } from "node:fs";
import { homedir, platform } from "node:os";
import { delimiter, join } from "node:path";
import process from "node:process";

/**
 * Supply Tauri with the local Android SDK and its newest side-by-side NDK.
 * Android Studio installs the NDK under the SDK rather than exporting NDK_HOME.
 */
export function androidEnvironment() {
  const environment = { ...process.env };
  const sdkDirectory = environment.ANDROID_HOME || environment.ANDROID_SDK_ROOT || defaultSdkDirectory();

  const javaHome = validJavaHome(environment.JAVA_HOME) || defaultJavaHome();
  if (javaHome) {
    environment.JAVA_HOME = javaHome;
    environment.PATH = [join(javaHome, "bin"), environment.PATH].filter(Boolean).join(delimiter);
  }

  if (!sdkDirectory || !existsSync(sdkDirectory)) return environment;
  environment.ANDROID_HOME = sdkDirectory;
  environment.ANDROID_SDK_ROOT = sdkDirectory;

  if (!environment.NDK_HOME) {
    const ndkDirectory = latestNdkDirectory(sdkDirectory);
    if (ndkDirectory) {
      environment.NDK_HOME = ndkDirectory;
      environment.ANDROID_NDK_HOME = ndkDirectory;
    }
  }

  return environment;
}

function validJavaHome(directory) {
  if (!directory) return undefined;
  const executable = platform() === "win32" ? "java.exe" : "java";
  return existsSync(join(directory, "bin", executable)) ? directory : undefined;
}

function defaultJavaHome() {
  const home = homedir();
  const candidates = platform() === "darwin"
    ? [
        join(home, ".local", "jdk-17", "Contents", "Home"),
        "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
      ]
    : platform() === "win32"
      ? [join(home, ".local", "jdk-17"), "C:\\Program Files\\Android\\Android Studio\\jbr"]
      : [join(home, ".local", "jdk-17"), "/opt/android-studio/jbr"];
  return candidates.find(validJavaHome);
}

function defaultSdkDirectory() {
  const home = homedir();
  if (platform() === "darwin") return join(home, "Library", "Android", "sdk");
  if (platform() === "win32") return join(home, "AppData", "Local", "Android", "Sdk");
  return join(home, "Android", "Sdk");
}

function latestNdkDirectory(sdkDirectory) {
  const ndkRoot = join(sdkDirectory, "ndk");
  if (!existsSync(ndkRoot)) return undefined;
  return readdirSync(ndkRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(ndkRoot, entry.name))
    .filter((directory) => existsSync(join(directory, "source.properties")))
    .sort((left, right) => right.localeCompare(left, undefined, { numeric: true }))
    .at(0);
}
