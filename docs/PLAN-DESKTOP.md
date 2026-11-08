# PLAN-DESKTOP — Shell Tauri (P2-DTL)

> Dibuat D47 saat eksekusi ROADMAP-NEXT #7. Dokumen ini = rencana bertahap +
> keputusan desain shell desktop. TASKS.md tetap sumber definisi task;
> ROADMAP-NEXT #7 menandai progres.

## Keputusan arsitektur (D47)

- **Reuse, jangan fork**: shell Tauri TIDAK punya UI sendiri. Window memuat UI
  web yang sama (`apps/web`); mode dev = `http://localhost:3000` (devUrl),
  mode build = export statis `apps/web/out` (frontendDist; menyusul D48).
- **Onboarding = halaman `apps/web/public/welcome.html`** (vanilla, tanpa
  React): disajikan web dev server (dev) dan ikut di-copy ke export (prod).
  Shell membukanya sebagai window pertama (`url: "welcome.html"`), lalu
  `window.__TAURI__` (withGlobalTauri) dipakai untuk `invoke`. Dibuka di
  browser biasa → fallback note + link ke app (dijaga e2e dbg-space).
- **Mode switch host/remote (D23)**:
  - HOST: Rust spawn API lokal → poll health TCP `127.0.0.1:3211` (timeout
    90s) → `location.replace('./')` ke UI web. Bila API sudah hidup (mis.
    user menjalankan `npm run dev`), TIDAK spawn ulang — hanya pakai.
  - REMOTE: input URL host (mesin lain) → navigasi window ke sana. v0 =
    web-host URL; pairing relay (relay-client) menyusul saat butuh.
- **Higienis proses**: hanya API yang shell spawn sendiri yang dibunuh saat
  exit (Windows: `taskkill /T /F`; npm → node tree). CREATE_NO_WINDOW untuk
  semua child Windows.
- **CORS sudah cocok tanpa perubahan**: API `origin: true` (reflect), web
  `API_BASE` default `http://127.0.0.1:3211` → desktop tidak butuh env baru.

## Staging

### D47 (selesai) — dev slice vertikal
- `apps/desktop` (Tauri 2): Cargo, tauri.conf (window 1280x800, csp null,
  bundle NSIS target + icons generated dari logo 512), capabilities
  (core:default + remote urls dev origin), lib.rs (pick_host/pick_remote,
  spawn/kill API, health poll).
- `welcome.html` + fallback browser; root script `npm run desktop`.
- Verifikasi: cargo check/build ✓, `tauri dev` runtime (window + welcome
  ter-load dari dev server) ✓, e2e dbg-space welcome fallback ✓.

### D48 — prod host mode (SEBAGIAN SELESAI di sesi D48)
1. Static export web: `output: "export"` di next.config build desktop +
   `NEXT_PUBLIC_API_BASE` tetap default 127.0.0.1:3211. Perhatikan route
   dinamis /dev & /auth/callback (client-only, aman).
2. Bundling API: esbuild `apps/api` → `yuika-api.cjs` single file; native
   deps (better-sqlite3 dll) di-external + dikirim sebagai resources
   (node_modules pruned) — atau sidecar `node.exe`. Putuskan saat eksekusi
   dengan uji cold-start di Windows bersih.
3. Release path Rust: `spawn_api()` mode release menjalankan sidecar/resource
   (masih error eksplisit "ships with P2-DTL-3" — sidecar node = pekerjaan D49).

**Selesai di sesi D48:** EPERM `next build` hilang; /auth/callback dibungkus
Suspense (useSearchParams); next.config dapat mode `NEXT_EXPORT=1` →
`output: "export"`; static export HIJAU (`apps/web/out`, 14 MB, welcome.html
ikut); tauri.conf beforeBuildCommand → scripts/build-desktop-web.mjs.
BONUS sesi yang sama: welcome.html REDESIGN (oval+maskot float, kartu
mode ikon); tombol "Switch desktop mode" (login gate + footer sidebar,
hanya saat window.__TAURI__); CharacterStage breakpoint 1100→1000 + narrow
h-auto (fix tumpang tindih di window desktop 1280−sidebar 272).

### D49 — packaging + auto-update (P2-DTL-3)
- NSIS aktif (sudah di config), updater `tauri-plugin-updater` +
  `tauri-plugin-process` (endpoint JSON latest; signing key dikelola aman,
  jangan commit private key).
- CI workflow `desktop.yml`: build win (matrix mac/linux menyusul), upload
  artefak NSIS + updater manifest.
- DMG (macOS) menyusul bila ada kebutuhan test relay lintas platform.

## FAQ (D48, jawaban untuk pertanyaan user)

**Apakah source desktop terpisah dari web?** TIDAK. Shell Tauri hanya
window native yang MEMUAT UI web yang sama (`apps/web`). Di mode dev,
window membuka http://localhost:3000 (next dev) → HMR: ubah kode web,
desktop ikut berubah detik itu juga. Di paket build, UI web di-export
statis (`apps/web/out`) lalu DIBUNDL ke dalam exe — installer membawa
snapshot; perubahan web = rebuild installer (`npm run tauri build -w
@yuika/desktop` atau via CI). Tidak ada dua codebase UI.

**Cara bikin executable untuk user?**`cd apps/desktop && npx tauri build`
(butuh Rust + WebView2; beforeBuildCommand men-export web otomatis) →
hasil di `src-tauri/target/release/bundle/nsis/*.exe` (installer) &
`target/release/yuika-desktop.exe` (portabel). Catatan jujur D48: HOST
mode versi build masih stub (`spawn_api` release error eksplisit — API
sidecar belum dibundel); jalur RELOTE (connect ke host lain) sudah berfungsi
penuh di build. Bundel API node = D49.

## Catatan uji manual (D47)

1. `npm run desktop` (repo root) — window "Yuika" terbuka di welcome page.
2. Klik "On this device" → status "Starting the local engine…" → UI Home
   (playground maskot) muncul; API hidup di 3211.
3. Atau klik "Connect to a host" → isi URL host lain → Connect.
4. Tutup app → proses `node` API yang di-spawn shell ikut mati.
