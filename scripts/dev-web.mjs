// Dev helper: start the studio web dev server (serves welcome.html and the
// app UI on http://localhost:3000, which tauri.conf devUrl points at).
// Studio location resolution mirrors build-desktop-web.mjs.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function findStudio() {
  if (process.env.YUIKA_STUDIO_DIR) return process.env.YUIKA_STUDIO_DIR;
  for (const name of ["your-yui", "Yuika"]) {
    const p = path.resolve(root, "..", name);
    if (existsSync(path.join(p, "apps", "web"))) return p;
  }
  console.error(
    "[dev-web] studio checkout not found. Set YUIKA_STUDIO_DIR or clone the Yuika repo next to this one.",
  );
  process.exit(1);
}

const r = spawnSync("npm", ["run", "dev", "-w", "@yuika/web"], {
  cwd: findStudio(),
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(r.status ?? 0);
