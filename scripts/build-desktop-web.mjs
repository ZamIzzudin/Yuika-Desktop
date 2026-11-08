// Static web build for the desktop shell, sourced from a Yuika studio
// checkout. Invoked by the tauri.conf beforeBuildCommand.
//
// Studio location: env YUIKA_STUDIO_DIR, else a sibling checkout
// (../your-yui or ../Yuika) next to this repository. The exported
// output (apps/web/out) is copied into this repo's dist/web, which is
// the shell's frontendDist.
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, rmSync } from "node:fs";
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
    "[build-desktop-web] studio checkout not found. Set YUIKA_STUDIO_DIR or clone the Yuika repo next to this one.",
  );
  process.exit(1);
}

const studio = findStudio();
const web = path.join(studio, "apps", "web");
const out = path.join(web, "out");
const dist = path.join(root, "dist", "web");

console.log(`[build-desktop-web] studio: ${studio}`);
// Fresh checkout (CI, new contributor): install studio deps first.
if (!existsSync(path.join(studio, "node_modules"))) {
  console.log("[build-desktop-web] studio node_modules missing, running npm ci");
  const ci = spawnSync("npm", ["ci"], {
    cwd: studio,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (ci.status !== 0) {
    console.error("[build-desktop-web] studio npm ci failed");
    process.exit(ci.status ?? 1);
  }
}
const r = spawnSync("npx", ["next", "build"], {
  cwd: web,
  stdio: "inherit",
  shell: process.platform === "win32",
  env: { ...process.env, NEXT_EXPORT: "1" },
});
if (r.status !== 0) {
  console.error("[build-desktop-web] next export build failed");
  process.exit(r.status ?? 1);
}
rmSync(dist, { recursive: true, force: true });
cpSync(out, dist, { recursive: true });
console.log(`[build-desktop-web] exported UI copied to ${dist}`);
