import { mkdir, readdir, rename } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const distDirectory = join(projectRoot, "dist");
const bundleDirectories = [
  join(distDirectory, "release", "bundle", "nsis"),
  join(distDirectory, "release", "bundle", "msi"),
  join(distDirectory, "x86_64-pc-windows-msvc", "release", "bundle", "nsis"),
];
const packageFiles = [];

for (const bundleDirectory of bundleDirectories) {
  const entries = await readdir(bundleDirectory, { withFileTypes: true }).catch((error) => {
    if (error.code === "ENOENT") return [];
    throw error;
  });
  packageFiles.push(
    ...entries
      .filter((entry) => entry.isFile() && /\.(exe|msi)$/i.test(entry.name))
      .map((entry) => join(bundleDirectory, entry.name)),
  );
}

if (packageFiles.length === 0) {
  throw new Error("No Windows installer was created in the expected bundle directories.");
}

await mkdir(distDirectory, { recursive: true });
for (const source of packageFiles) {
  const destination = join(distDirectory, basename(source));
  await rename(source, destination);
  console.log(`Windows installer ready: ${relative(projectRoot, destination)}`);
}
