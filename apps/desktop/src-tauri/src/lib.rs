// Yuika desktop shell (P2-DTL-1, D47) — logika Rust.
//
// Onboarding (welcome.html) menawarkan dua mode (keputusan D23):
// - HOST   : shell men-spawn API lokal, menunggu health (TCP 127.0.0.1:3211),
//            lalu window menavigasi ke UI web (dev: http://localhost:3000,
//            build: hasil export statis di dist/web repo ini).
// - REMOTE : user memasukkan URL host lain → window menavigasi ke sana.
//
// v0 dev-first: spawn via `npm run dev -w @yuika/api` dari checkout repo
// STUDIO (sibling atau env YUIKA_STUDIO_DIR, resolusi sama dengan
// scripts/*.mjs di repo desktop ini).
// Packaging (sidecar node + static export + NSIS + auto-update = P2-DTL-3)
// menyusul — staging plan ada di docs/PLAN-DESKTOP.md.
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{Manager, RunEvent, State, WebviewWindow};

const API_ADDR: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::LOCALHOST,
    3211,
));
const API_BOOT_TIMEOUT: Duration = Duration::from_secs(90);

/// Child API yang di-spawn shell (None = API dijalankan pihak lain).
#[derive(Default)]
struct ApiProcess(Mutex<Option<Child>>);

/// Checkout repo STUDIO untuk dev (spawn API). Urutan resolusi: env
/// YUIKA_STUDIO_DIR → sibling ./your-yui atau ./Yuika di sebelah repo
/// desktop ini. Tidak ketemu → default sibling (spawn akan gagal dengan
/// pesan jelas, lebih baik daripada panic saat compile).
#[cfg(debug_assertions)]
fn repo_root() -> PathBuf {
    if let Ok(dir) = std::env::var("YUIKA_STUDIO_DIR") {
        return PathBuf::from(dir);
    }
    let beside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..");
    for name in ["your-yui", "Yuika"] {
        let p = beside.join(name);
        if p.join("apps").join("api").exists() {
            return p;
        }
    }
    beside.join("your-yui")
}

/// Cek TCP listener API (cukup untuk "sudah hidup": fastify listen duluan).
fn api_healthy() -> bool {
    TcpStream::connect_timeout(&API_ADDR, Duration::from_millis(300)).is_ok()
}

/// Spawn API lokal (dev): `npm run dev -w @yuika/api` dari root repo.
#[cfg(debug_assertions)]
fn spawn_api() -> std::io::Result<Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd")
            .args(["/C", "npm", "run", "dev", "-w", "@yuika/api"])
            .current_dir(repo_root())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
    }
    #[cfg(not(windows))]
    Command::new("npm")
        .args(["run", "dev", "-w", "@yuika/api"])
        .current_dir(repo_root())
        .spawn()
}

/// Build mode: API dibundel sebagai resource/sidecar (P2-DTL-3, belum aktif).
#[cfg(not(debug_assertions))]
fn spawn_api() -> std::io::Result<Child> {
    Err(std::io::Error::other(
        "host mode in packaged builds ships with P2-DTL-3 (api sidecar)",
    ))
}

/// P2-DTL-1 HOST: pastikan API hidup (spawn bila belum), tunggu health di
/// background, lalu navigasi window ke UI web. Status dilaporkan ke picker
/// via window.__hostStatus / window.__hostError di welcome.html.
#[tauri::command]
fn pick_host(window: WebviewWindow, api: State<'_, ApiProcess>) -> Result<(), String> {
    if !api_healthy() {
        let child =
            spawn_api().map_err(|e| format!("failed to start local engine: {e}"))?;
        *api.0.lock().map_err(|e| e.to_string())? = Some(child);
    }
    // Poll health di thread terpisah; jangan blok command thread.
    std::thread::spawn(move || {
        let deadline = Instant::now() + API_BOOT_TIMEOUT;
        loop {
            if api_healthy() {
                // './' = root UI web (dev: devUrl; build: frontendDist).
                let _ = window.eval("location.replace('./')");
                return;
            }
            if Instant::now() > deadline {
                let _ = window
                    .eval("window.__hostError && window.__hostError('local engine did not become ready in time')");
                return;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    Ok(())
}

/// P2-DTL-1 REMOTE: navigasi window ke URL host lain (web-host di mesin lain).
#[tauri::command]
fn pick_remote(window: WebviewWindow, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL must start with http:// or https://".into());
    }
    let js = serde_json::to_string(&url).map_err(|e| e.to_string())?;
    window
        .eval(&format!("location.replace({js})"))
        .map_err(|e| e.to_string())
}

/// Matikan API yang kita spawn sendiri (tree-kill; npm → node anak).
/// API milik proses lain (mis. `npm run dev` user) tidak disentuh.
fn kill_api(app: &tauri::AppHandle) {
    let taken = app
        .state::<ApiProcess>()
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.take());
    if let Some(child) = taken {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ApiProcess::default())
        .invoke_handler(tauri::generate_handler![pick_host, pick_remote])
        .build(tauri::generate_context!())
        .expect("error while building yuika desktop")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                kill_api(app);
            }
        });
}
