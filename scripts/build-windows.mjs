import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { delimiter } from "node:path";

const windowsTarget = "x86_64-pc-windows-msvc";
const isWindows = process.platform === "win32";
const environment = { ...process.env };

if (!isWindows && process.platform === "darwin") {
  const llvmDirectories = ["/opt/homebrew/opt/llvm/bin", "/usr/local/opt/llvm/bin"];
  const llvmDirectory = llvmDirectories.find(existsSync);
  if (llvmDirectory) {
    environment.PATH = `${llvmDirectory}${delimiter}${environment.PATH ?? ""}`;
  }
}

if (!isWindows) {
  const missingCommands = ["makensis", "llvm-rc", "lld-link", "cargo-xwin"].filter(
    (command) => !isAvailable(command, environment),
  );

  if (missingCommands.length > 0) {
    const setupCommands =
      process.platform === "darwin"
        ? [
            "brew install nsis llvm",
            `rustup target add ${windowsTarget}`,
            "cargo install --locked cargo-xwin",
          ]
        : [
            "Install NSIS and LLVM with your distribution package manager.",
            `rustup target add ${windowsTarget}`,
            "cargo install --locked cargo-xwin",
          ];
    console.error(`Missing cross-build tools: ${missingCommands.join(", ")}.`);
    console.error("Install the prerequisites, then run this command again:");
    for (const command of setupCommands) console.error(`  ${command}`);
    process.exit(1);
  }
}

const args = isWindows
  ? ["build", "--bundles", "nsis,msi"]
  : [
      "build",
      "--bundles",
      "nsis",
      "--runner",
      "cargo-xwin",
      "--target",
      windowsTarget,
    ];
const result = spawnSync("tauri", args, {
  env: environment,
  shell: isWindows,
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);

function isAvailable(command, env) {
  const result = spawnSync(command, ["--version"], {
    env,
    shell: process.platform === "win32",
    stdio: "ignore",
  });
  return result.status === 0;
}
