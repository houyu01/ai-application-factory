import { mkdir, readdir, rename } from "node:fs/promises";
import { basename, join, relative } from "node:path";

import { iosIpaDirectory, projectRoot } from "./ios-project.mjs";

const distDirectory = join(projectRoot, "dist");
const entries = await readdir(iosIpaDirectory, { withFileTypes: true }).catch((error) => {
  if (error.code === "ENOENT") {
    throw new Error(`No iPad IPA bundle directory was created: ${iosIpaDirectory}`);
  }
  throw error;
});
const ipaFiles = entries
  .filter((entry) => entry.isFile() && entry.name.endsWith(".ipa"))
  .map((entry) => join(iosIpaDirectory, entry.name));

if (ipaFiles.length === 0) {
  throw new Error(`No IPA files were created in: ${iosIpaDirectory}`);
}

await mkdir(distDirectory, { recursive: true });
for (const source of ipaFiles) {
  const destination = join(distDirectory, basename(source));
  await rename(source, destination);
  console.log(`iPad IPA ready: ${relative(projectRoot, destination)}`);
}
