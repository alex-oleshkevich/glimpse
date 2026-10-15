import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const website = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(website, "..");
const out = join(website, "public", "assets");

const files = {
  "logo.svg": "data/icons/hicolor/scalable/apps/me.aresa.Glimpse.svg",
  "shell-light.png": "docs/screenshots/shell-light.png",
  "shell-dark.png": "docs/screenshots/shell-dark.png",
  "applets-light.png": "docs/screenshots/applets-light.png",
  "applets-dark.png": "docs/screenshots/applets-dark.png",
};

mkdirSync(out, { recursive: true });
for (const [name, source] of Object.entries(files)) {
  copyFileSync(join(repo, source), join(out, name));
}
