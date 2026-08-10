import { mkdir, readdir, rename } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const distDirectory = join(projectRoot, "dist");
const dmgDirectory = join(distDirectory, "release", "bundle", "dmg");
const entries = await readdir(dmgDirectory, { withFileTypes: true }).catch((error) => {
  if (error.code === "ENOENT") {
    throw new Error(`No DMG bundle directory was created: ${dmgDirectory}`);
  }
  throw error;
});
const dmgFiles = entries
  .filter((entry) => entry.isFile() && entry.name.endsWith(".dmg"))
  .map((entry) => entry.name);

if (dmgFiles.length === 0) {
  throw new Error(`No DMG files were created in: ${dmgDirectory}`);
}

await mkdir(distDirectory, { recursive: true });
for (const dmgFile of dmgFiles) {
  const source = join(dmgDirectory, dmgFile);
  const destination = join(distDirectory, dmgFile);
  await rename(source, destination);
  console.log(`DMG ready: ${relative(projectRoot, destination)}`);
}
