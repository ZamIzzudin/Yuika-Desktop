// Bundle engine lokal (P2-DTL-3) untuk build packaged: node.exe + API studio
// sebagai satu file cjs (esbuild, better-sqlite3 external karena native) +
// tree runtime better-sqlite3 + personas/skills/mcp.json → dist/desktop.
// tauri.conf `bundle.resources` mengambil dari sini → resource_dir/engine.
//
// Sumber studio: env YUIKA_STUDIO_DIR, else sibling ./your-yui atau ./Yuika
// (resolusi sama dengan build-desktop-web.mjs). Studio node_modules
// (npm ci) dijamin dulu — butuh esbuild transitif & .node prebuilt.
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, createWriteStream, existsSync, mkdirSync, rmSync } from "node:fs";
import { get as httpGet } from "node:http";
import { get as httpsGet } from "node:https";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { build } from "esbuild";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Versi node.exe yang dibundel HARUS cocok ABI dengan prebuilt better-sqlite3
// yang terpasang di node_modules studio — npm install memilih prebuild sesuai
// node yang MENJALANKAN npm (dev node 24 → ABI 137; CI setup-node 22 → 127).
// Jadi deteksi ABI terpasang lalu pilih versi node bundle yang cocok.
// Override manual: YUIKA_NODE_VERSION.
const ABI_NODE_MAP = { "127": "v22.23.4", "137": "v24.13.0" };
const systemAbi = spawnSync(process.execPath, ["-p", "process.versions.modules"], { encoding: "utf8" })
  .stdout?.trim();
const NODE_VERSION = process.env.YUIKA_NODE_VERSION || ABI_NODE_MAP[systemAbi || ""];
if (!NODE_VERSION) {
  console.error(
    `[bundle-engine] no bundled-node mapping for ABI ${systemAbi} (installed better-sqlite3 prebuilt). ` +
      `Set YUIKA_NODE_VERSION to a node version matching that ABI.`,
  );
  process.exit(1);
}
const NODE_URL = `https://nodejs.org/dist/${NODE_VERSION}/win-x64/node.exe`;

function findStudio() {
  if (process.env.YUIKA_STUDIO_DIR) return process.env.YUIKA_STUDIO_DIR;
  for (const name of ["your-yui", "Yuika"]) {
    const p = path.resolve(root, "..", name);
    if (existsSync(path.join(p, "apps", "api"))) return p;
  }
  console.error(
    "[bundle-engine] studio checkout not found. Set YUIKA_STUDIO_DIR or clone the Yuika repo next to this one.",
  );
  process.exit(1);
}

const studio = findStudio();

// Fresh checkout (CI): pastikan dependensi studio terpasang (esbuild via tsx,
// prebuilt better-sqlite3, dsb).
if (!existsSync(path.join(studio, "node_modules"))) {
  console.log("[bundle-engine] studio node_modules missing, running npm ci");
  const ci = spawnSync("npm", ["ci"], { cwd: studio, stdio: "inherit", shell: process.platform === "win32" });
  if (ci.status !== 0) {
    console.error("[bundle-engine] studio npm ci failed");
    process.exit(ci.status ?? 1);
  }
}
const bs3Addon = path.join(studio, "node_modules", "better-sqlite3", "build", "Release", "better_sqlite3.node");

// 0) Runtime Node — cache stabil DI LUAR dist (dist di-rmSync tiap build,
// sedangkan probe ABI & section 4 butuh node.exe lebih awal/berulang).
function downloadFile(url, dest, redirects = 0) {
  return new Promise((resolve, reject) => {
    const get = url.startsWith("https") ? httpsGet : httpGet;
    get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location && redirects < 5) {
        res.resume();
        return resolve(downloadFile(new URL(res.headers.location, url).toString(), dest, redirects + 1));
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`download failed: HTTP ${res.statusCode} for ${url}`));
      }
      const file = createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close(resolve));
      file.on("error", reject);
    }).on("error", reject);
  });
}
const nodeCache = path.join(root, ".cache", `node-${NODE_VERSION}-win-x64.exe`);
mkdirSync(path.dirname(nodeCache), { recursive: true });
if (!existsSync(nodeCache)) {
  console.log(`[bundle-engine] downloading ${NODE_URL} → ${path.relative(root, nodeCache)}`);
  await downloadFile(NODE_URL, nodeCache);
  console.log("[bundle-engine] node.exe cached");
}

// Prebuilt better-sqlite3 HARUS cocok ABI dengan node.exe yang dibundel
// (NODE_VERSION dipilih dari ABI terpasang). Probe = load addon dengan node
// target; gagal + "NODE_MODULE_VERSION" di stderr → mismatch → fail jelas
// (solusinya: jalankan ulang `npm ci` studio dengan node yang diinginkan,
// atau set YUIKA_NODE_VERSION).
function addonAbiMismatch() {
  if (!existsSync(nodeCache) || !existsSync(bs3Addon)) return false;
  const probe = spawnSync(nodeCache, ["-e", `require(${JSON.stringify(bs3Addon)})`], {
    encoding: "utf8",
  });
  return probe.status !== 0 && /NODE_MODULE_VERSION/.test(String(probe.stderr));
}
if (!existsSync(bs3Addon)) {
  console.error(`[bundle-engine] better-sqlite3 prebuilt missing: ${bs3Addon}`);
  process.exit(1);
}
if (addonAbiMismatch()) {
  console.error(
    `[bundle-engine] better-sqlite3 prebuilt ABI mismatch with ${NODE_VERSION} ` +
      `(installed by node ABI ${systemAbi}). Re-run studio npm ci with the intended node, ` +
      `or set YUIKA_NODE_VERSION.`,
  );
  process.exit(1);
}
console.log(`[bundle-engine] node ${NODE_VERSION} matches better-sqlite3 ABI ${systemAbi}`);

const out = path.join(root, "dist", "desktop");
rmSync(out, { recursive: true, force: true });
mkdirSync(path.join(out, "api", "node_modules"), { recursive: true });

// 1) API → satu file CJS. Dynamic import() specifier statis dibundle
//    esbuild menjadi require; hanya better-sqlite3 (native) yang external.
console.log("[bundle-engine] esbuild apps/api → yuika-api.cjs");
await build({
  entryPoints: [path.join(studio, "apps", "api", "src", "server.ts")],
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node22",
  outfile: path.join(out, "api", "yuika-api.cjs"),
  external: ["better-sqlite3"],
  // CJS tidak punya import.meta.url (esbuild mengosongkannya); modul yang
  // membaca file relatif ke import.meta.url (mis. app-update ke repo root)
  // akan ERR_INVALID_URL di boot. Shim: ganti dengan file URL dari __filename
  // (absolut) — lookup miss tertangani catch masing-masing.
  logLevel: "info",
  define: { "import.meta.url": "__yuika_filename_url" },
  banner: {
    js: [
      "/* Yuika desktop engine — generated by scripts/bundle-engine.mjs; do not edit. */",
      "var __yuika_filename_url = require(\"node:url\").pathToFileURL(__filename).href;",
    ].join(String.fromCharCode(10)),
  },
});

// 2) Tree runtime better-sqlite3 (native addon + resolver bindings).
function copyPkg(name, { exclude = [] } = {}) {
  const src = path.join(studio, "node_modules", name);
  const dst = path.join(out, "api", "node_modules", name);
  cpSync(src, dst, {
    recursive: true,
    filter: (p) => !exclude.some((e) => p === path.join(src, e) || p.startsWith(path.join(src, e) + path.sep)),
  });
  console.log(`[bundle-engine] node_modules/${name}`);
}
copyPkg("better-sqlite3", { exclude: ["deps", "docs", "test", "benchmark", "src", "idea"] });
copyPkg("bindings");
copyPkg("file-uri-to-path");

// 3) Resource studio yang dibaca runtime: personas, skills, katalog MCP.
mkdirSync(path.join(out, "api", "packages"), { recursive: true });
cpSync(path.join(studio, "packages", "personas"), path.join(out, "api", "packages", "personas"), { recursive: true });
cpSync(path.join(studio, "packages", "skills"), path.join(out, "api", "packages", "skills"), { recursive: true });
cpSync(path.join(studio, "mcp.json"), path.join(out, "api", "mcp.json"));
console.log("[bundle-engine] personas + skills + mcp.json");

// 4) Runtime Node (win-x64) dari cache → dist.
mkdirSync(path.join(out, "node"), { recursive: true });
copyFileSync(nodeCache, path.join(out, "node", "node.exe"));
console.log(`[bundle-engine] node.exe (${NODE_VERSION}) → engine/node`);

console.log(`[bundle-engine] done → ${out}`);
