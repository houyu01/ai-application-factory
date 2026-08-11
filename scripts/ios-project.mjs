import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const iosProjectDirectory = join(projectRoot, "dist", "gen", "apple");
export const iosIpaDirectory = join(iosProjectDirectory, "build", "arm64");
