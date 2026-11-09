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

use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow};

const API_ADDR: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::LOCALHOST,
    3211,
));
const API_BOOT_TIMEOUT: Duration = Duration::from_secs(90);

/// Setup terakhir (mode onboarding) dipersist di app_config_dir, launch
/// berikutnya auto-resume dari welcome.html (bukan mengulang picker).
/// "Switch mode" di dalam UI memanggil clear_last_setup via ?reset=1.
fn setup_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("setup.json"))
}

fn read_setup(app: &AppHandle) -> serde_json::Value {
    setup_file(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn write_setup(app: &AppHandle, v: serde_json::Value) {
    if let Some(p) = setup_file(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, v.to_string());
    }
}

/// Child API yang di-spawn shell (None = API dijalankan pihak lain).
/// Menyimpan PID saja: objek Child dimiliki thread monitor (wait butuh
/// &mut); kill pakai taskkill/kill via PID. D70: exit code 87 = data reset
/// → bersihkan setup.json (factory) → respawn engine → reload webview.
#[derive(Default)]
struct ApiProcess(Mutex<Option<u32>>);

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

/// Spawn API lokal (dev): `npm run dev -w @yuika/api` dari root repo
/// studio (checkout sibling; path engine hanya dipakai mode build).
#[cfg(debug_assertions)]
fn spawn_api(_app: &AppHandle) -> std::io::Result<Child> {
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

/// Buang prefix verbatim `\\?\` dari path (resource_dir Tauri memakai
/// bentuk extended-length; node.exe tidak memahaminya sebagai entry arg —
/// resolveMainPath-nya menerima 'C:' dan EISDIR). Bentuk UNC dikembalikan ke
/// `\\server\\share`.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn plain_path(p: PathBuf) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    p
}

/// Build mode (P2-DTL-3): engine dibundel sebagai resource — node.exe +
/// yuika-api.cjs (esbuild) + personas/skills/mcp.json, diproduksi
/// scripts/bundle-engine.mjs → tauri.conf `resources` (resource_dir/engine).
/// Data user (db, workspace, log, env override) di app_data_dir.
#[cfg(not(debug_assertions))]
fn spawn_api(app: &AppHandle) -> std::io::Result<Child> {
    let res = plain_path(
        app.path()
            .resource_dir()
            .map_err(|e| std::io::Error::other(format!("resource dir: {e}")))?,
    );
    let engine = res.join("engine");
    let node = engine.join("node").join("node.exe");
    let api = engine.join("api").join("yuika-api.cjs");
    if !node.exists() {
        return Err(std::io::Error::other(format!(
            "bundled engine missing: {}",
            node.display()
        )));
    }
    let data = plain_path(
        app.path()
            .app_data_dir()
            .map_err(|e| std::io::Error::other(format!("app data dir: {e}")))?,
    );
    std::fs::create_dir_all(&data)?;

    let api_dir = engine.join("api");
    let mut cmd = Command::new(&node);
    cmd.arg(&api)
        .env("YUI_API_PORT", "3211")
        .env("YUI_DB_PATH", data.join("demo.db"))
        .env("YUI_WORKSPACE_DIR", data.join("workspace"))
        .env("YUI_PERSONAS_DIR", api_dir.join("packages").join("personas"))
        .env("YUI_SKILLS_DIR", api_dir.join("packages").join("skills"))
        .env("YUI_MCP_CONFIG", api_dir.join("mcp.json"))
        .current_dir(&data);

    // Override opsional user: app_data_dir/yuika.env (KEY=VALUE, # komentar)
    // — mis. YUI_PORTAL_URL/YUI_RELAY_URL untuk host SSO; env yang sudah
    // di-set proses shell TIDAK ditimpa (konsisten dengan env-boot).
    if let Ok(txt) = std::fs::read_to_string(data.join("yuika.env")) {
        for line in txt.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if std::env::var(k.trim()).is_err() {
                    cmd.env(k.trim(), v.trim());
                }
            }
        }
    }

    // Log engine ke file (rotasi kasar >10 MB agar tidak tumbuh tanpa batas).
    let log_path = data.join("api.log");
    if log_path
        .metadata()
        .map(|m| m.len() > 10 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(&log_path);
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    cmd.stdout(std::process::Stdio::from(
        log.try_clone().map_err(|e| std::io::Error::other(e))?,
    ));
    cmd.stderr(std::process::Stdio::from(log));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.spawn()
}

/// Build mode: saat health timeout, engine kemungkinan mati/exit duluan —
/// tampilkan penyebab ASLI (ekor api.log, mis. "Env belum lengkap")
/// alih-alih pesan generik. Dev mode tak punya log file → pesan generik.
#[cfg(not(debug_assertions))]
fn boot_failure_reason(app: &AppHandle) -> String {
    let log = app
        .path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("api.log"))
        .and_then(|p| std::fs::read_to_string(p).ok());
    let tail = log
        .map(|t| {
            t.lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if tail.is_empty() {
        "local engine did not become ready in time".into()
    } else {
        format!("local engine failed to start:\n{tail}")
    }
}

#[cfg(debug_assertions)]
fn boot_failure_reason(_app: &AppHandle) -> String {
    "local engine did not become ready in time".into()
}

/// P2-DTL-1 HOST: pastikan API hidup (spawn bila belum), tunggu health di
/// background, lalu navigasi window ke UI web. Status dilaporkan ke picker
/// via window.__hostStatus / window.__hostError di welcome.html.
#[tauri::command]
fn pick_host(window: WebviewWindow, app: AppHandle, api: State<'_, ApiProcess>) -> Result<(), String> {
    write_setup(&app, serde_json::json!({ "mode": "host" }));
    if !api_healthy() {
        let child =
            spawn_api(&app).map_err(|e| format!("failed to start local engine: {e}"))?;
        *api.0.lock().map_err(|e| e.to_string())? = Some(child.id());
        monitor_engine(app.clone(), child);
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
                let msg = boot_failure_reason(&app);
                let js = serde_json::to_string(&msg)
                    .unwrap_or_else(|_| "local engine did not become ready in time".into());
                let _ = window.eval(&format!("window.__hostError && window.__hostError({js})"));
                return;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    Ok(())
}

/// P2-DTL-1 REMOTE: navigasi window ke URL host lain (web-host di mesin lain).
/// Engine host lokal yang kita spawn sendiri dimatikan dulu — ganti mode
/// tidak boleh meninggalkan sidecar jalan diam-diam di belakang.
#[tauri::command]
fn pick_remote(window: WebviewWindow, app: AppHandle, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL must start with http:// or https://".into());
    }
    kill_api(&app);
    write_setup(&app, serde_json::json!({ "mode": "remote", "url": url }));
    let js = serde_json::to_string(&url).map_err(|e| e.to_string())?;
    window
        .eval(&format!("location.replace({js})"))
        .map_err(|e| e.to_string())
}

/// Setup terakhir untuk auto-resume welcome.html (objek kosong bila belum ada).
#[tauri::command]
fn get_last_setup(app: AppHandle) -> serde_json::Value {
    read_setup(&app)
}

/// Hapus setup tersimpan (dipanggil welcome.html saat ?reset=1).
#[tauri::command]
fn clear_last_setup(app: AppHandle) {
    if let Some(p) = setup_file(&app) {
        let _ = std::fs::remove_file(p);
    }
}

/// File override env engine (spawn_api membaca ini tiap boot).
fn env_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("yuika.env"))
}

/// First-run wizard (D67): status konfigurasi engine untuk welcome.html.
/// Nilai dikirim penuh agar form bisa prefill, KECUALI API key yang hanya
/// dikirim dalam bentuk mask.
#[tauri::command]
fn env_status(app: AppHandle) -> serde_json::Value {
    let required = ["YUI_OPENAI_BASE_URL", "YUI_OPENAI_API_KEY", "YUI_MODEL"];
    let mut values = serde_json::Map::new();
    if let Some(p) = env_file(&app) {
        if let Ok(txt) = std::fs::read_to_string(&p) {
            for line in txt.lines() {
                let l = line.trim();
                if l.is_empty() || l.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = l.split_once('=') {
                    values.insert(k.trim().to_string(), serde_json::json!(v.trim()));
                }
            }
        }
    }
    let get = |k: &str| values.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let complete = required.iter().all(|k| !get(k).is_empty());
    let key = get("YUI_OPENAI_API_KEY");
    let key_masked = if key.chars().count() > 12 {
        let tail_start = key.chars().count().saturating_sub(4);
        format!("{}…{}", key.chars().take(5).collect::<String>(), key.chars().skip(tail_start).collect::<String>())
    } else {
        key.to_string()
    };
    serde_json::json!({
        "exists": !values.is_empty(),
        "complete": complete,
        "required": required,
        "baseUrl": get("YUI_OPENAI_BASE_URL"),
        "model": get("YUI_MODEL"),
        "providerKind": get("YUI_PROVIDER_KIND"),
        "hasApiKey": !key.is_empty(),
        "apiKeyMasked": key_masked,
    })
}

/// First-run wizard (D67): merge KEY=VALUE ke yuika.env (preserve baris &
/// komentar lain). Entry dengan value kosong di-skip (tidak menghapus).
#[tauri::command]
fn write_env(app: AppHandle, entries: std::collections::BTreeMap<String, String>) -> Result<(), String> {
    let Some(p) = env_file(&app) else {
        return Err("app data dir unavailable".into());
    };
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut lines: Vec<String> = std::fs::read_to_string(&p)
        .map(|t| t.lines().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    for (k, v) in &entries {
        let k = k.trim();
        if k.is_empty() || v.trim().is_empty() {
            continue;
        }
        let new_line = format!("{k}={}", v.trim());
        let pos = lines.iter().position(|l| {
            let lt = l.trim();
            !lt.starts_with('#')
                && lt
                    .split_once('=')
                    .map(|(kk, _)| kk.trim() == k)
                    .unwrap_or(false)
        });
        match pos {
            Some(i) => lines[i] = new_line,
            None => {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push(new_line);
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    std::fs::write(&p, out).map_err(|e| e.to_string())
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
    if let Some(pid) = taken {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).spawn();
        }
    }
}

/// D70: tunggu engine exit. Kode 87 = data reset — engine sudah menulis
/// reset.pending; hapus setup.json bila scope factory (welcome picker jalan
/// lagi), respawn engine (boot baru mengeksekusi wipe), reload webview.
/// Exit lain (crash): arahkan kembali ke welcome supaya user bisa restart.
fn monitor_engine(app: AppHandle, mut child: Child) {
    std::thread::spawn(move || {
        loop {
            let code = child.wait().ok().and_then(|s| s.code());
            if let Some(state) = app.try_state::<ApiProcess>() {
                if let Ok(mut g) = state.0.lock() {
                    *g = None;
                }
            }
            if code == Some(87) {
                let factory = app
                    .path()
                    .app_data_dir()
                    .ok()
                    .and_then(|d| std::fs::read_to_string(d.join("reset.pending")).ok())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .map(|v| v.get("scope") == Some(&serde_json::json!("factory")))
                    .unwrap_or(false);
                if factory {
                    if let Some(cfg) = app.path().app_config_dir().ok() {
                        let _ = std::fs::remove_file(cfg.join("setup.json"));
                    }
                }
                match spawn_api(&app) {
                    Ok(c) => {
                        if let Some(state) = app.try_state::<ApiProcess>() {
                            if let Ok(mut g) = state.0.lock() {
                                *g = Some(c.id());
                            }
                        }
                        let target = if factory { "./welcome.html?reset=1" } else { "./" };
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.eval(&format!("location.replace('{target}')"));
                        }
                        child = c;
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[yuika] reset: respawn gagal: {e}");
                    }
                }
            } else {
                // engine mati bukan karena reset: ke welcome, user bisa start ulang
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.eval("location.replace('./welcome.html?reset=1')");
                }
            }
            return;
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ApiProcess::default())
        .invoke_handler(tauri::generate_handler![
            pick_host,
            pick_remote,
            get_last_setup,
            clear_last_setup,
            env_status,
            write_env
        ])
        .build(tauri::generate_context!())
        .expect("error while building yuika desktop")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                kill_api(app);
            }
        });
}
