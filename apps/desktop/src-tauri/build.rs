fn main() {
  // ACL Tauri 2: command aplikasi sendiri hanya diizinkan otomatis untuk
  // origin yang diklasifikasi "local". Segera asal webview diklasifikasi
  // remote (dev server localhost, origin tauri.localhost yang dianggap
  // remote oleh resolver), invoke ditolak "not allowed by ACL" kecuali ada
  // permission eksplisit. Generate allow-/deny- untuk tiap command
  // (allow-pick-host dst.), lalu cantumkan di capabilities/default.json.
  tauri_build::try_build(
    tauri_build::Attributes::new().app_manifest(
      tauri_build::AppManifest::new().commands(&[
        "pick_host",
        "pick_remote",
        "get_last_setup",
        "clear_last_setup",
      ]),
    ),
  )
  .expect("failed to run tauri-build");
}
