import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

import { androidEnvironment } from "./android-environment.mjs";
import { androidAppDirectory, androidProjectDirectory, projectRoot } from "./android-project.mjs";

const signingFile = join(projectRoot, ".env.android");
const gradleFile = join(androidAppDirectory, "build.gradle.kts");
const keystorePropertiesFile = join(androidProjectDirectory, "keystore.properties");
const signingMarker = "// AI_APPLICATION_FACTORY_RELEASE_SIGNING";

if (!existsSync(androidProjectDirectory)) {
  console.error("Android 工程尚未初始化。请先执行 npm run init:android。");
  process.exit(1);
}

if (!existsSync(signingFile)) {
  console.error("缺少 .env.android。请先复制 .env.android.example 并填写 Android keystore 信息。");
  process.exit(1);
}

if (!existsSync(gradleFile)) {
  console.error(`找不到 Android Gradle 配置文件：${gradleFile}`);
  process.exit(1);
}

const signing = loadSigningEnvironment(signingFile);
for (const key of [
  "ANDROID_KEYSTORE_PATH",
  "ANDROID_KEY_ALIAS",
  "ANDROID_KEYSTORE_PASSWORD",
  "ANDROID_KEY_PASSWORD",
]) {
  if (!signing[key] || signing[key].startsWith("replace-with-") || signing[key].startsWith("/absolute/")) {
    console.error(`.env.android 中的 ${key} 尚未填写。`);
    process.exit(1);
  }
}

if (!existsSync(signing.ANDROID_KEYSTORE_PATH)) {
  console.error(`找不到 Android keystore：${signing.ANDROID_KEYSTORE_PATH}`);
  process.exit(1);
}

writeKeystoreProperties(signing);
configureReleaseSigning();

const command = process.platform === "win32" ? "npx.cmd" : "npx";
const result = spawnSync(
  command,
  ["tauri", "android", "build", "--apk", "--target", "aarch64", "--target", "armv7"],
  { env: androidEnvironment(), stdio: "inherit" },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);

function loadSigningEnvironment(file) {
  const values = {};
  for (const [lineNumber, rawLine] of readFileSync(file, "utf8").split(/\r?\n/).entries()) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const match = line.match(/^([A-Z][A-Z0-9_]*)=(.*)$/);
    if (!match) throw new Error(`${file}:${lineNumber + 1} 必须使用 KEY=VALUE 格式。`);
    const [, key, rawValue] = match;
    values[key] = rawValue.trim().replace(/^(['"])(.*)\1$/, "$2");
  }
  return values;
}

function writeKeystoreProperties(signing) {
  const escape = (value) => value.replace(/\\/g, "\\\\").replace(/\r/g, "\\r").replace(/\n/g, "\\n");
  const contents = [
    `keyAlias=${escape(signing.ANDROID_KEY_ALIAS)}`,
    `keyPassword=${escape(signing.ANDROID_KEY_PASSWORD)}`,
    `storeFile=${escape(signing.ANDROID_KEYSTORE_PATH)}`,
    `storePassword=${escape(signing.ANDROID_KEYSTORE_PASSWORD)}`,
    "",
  ].join("\n");
  writeFileSync(keystorePropertiesFile, contents, "utf8");
}

function configureReleaseSigning() {
  let contents = readFileSync(gradleFile, "utf8");
  if (contents.includes(signingMarker)) return;

  if (!contents.includes("plugins {")) {
    throw new Error(`无法识别 Android Gradle 模板：${gradleFile}`);
  }
  contents = contents.replace(
    "plugins {",
    "import java.io.FileInputStream\nimport java.util.Properties\n\nplugins {",
  );

  const signingConfiguration = `
    ${signingMarker}
    signingConfigs {
        create("release") {
            val keystoreProperties = Properties()
            keystoreProperties.load(FileInputStream(rootProject.file("keystore.properties")))

            keyAlias = keystoreProperties.getProperty("keyAlias")
            keyPassword = keystoreProperties.getProperty("keyPassword")
            storeFile = file(keystoreProperties.getProperty("storeFile"))
            storePassword = keystoreProperties.getProperty("storePassword")
        }
    }
`;
  const buildTypesPattern = /^(\s*)buildTypes\s*\{/m;
  if (!buildTypesPattern.test(contents)) {
    throw new Error(`无法在 Android Gradle 模板中找到 buildTypes：${gradleFile}`);
  }
  contents = contents.replace(buildTypesPattern, (match) => `${signingConfiguration}\n${match}`);

  const releasePattern = /^(\s*)getByName\("release"\)\s*\{$/m;
  if (!releasePattern.test(contents)) {
    throw new Error(`无法在 Android Gradle 模板中找到 release 构建类型：${gradleFile}`);
  }
  contents = contents.replace(
    releasePattern,
    (match, indentation) => `${match}\n${indentation}    signingConfig = signingConfigs.getByName("release")`,
  );
  writeFileSync(gradleFile, contents, "utf8");
}
