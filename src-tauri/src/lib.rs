mod config;
mod ollama;
mod piper;
mod server;
mod tts;

use config::PluginConfig;
use server::SharedConfig;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "voice_control.json";
const STORE_KEY: &str = "config";

pub struct AppState {
    pub config: SharedConfig,
    pub dl_cancel: Arc<AtomicBool>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn load_from_store(app: &tauri::App) -> PluginConfig {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(STORE_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn persist(app: &AppHandle, cfg: &PluginConfig) {
    if let Ok(store) = app.store(STORE_FILE) {
        if let Ok(val) = serde_json::to_value(cfg) {
            let _ = store.set(STORE_KEY, val);
            let _ = store.save();
        }
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> PluginConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(
    config: PluginConfig,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    sync_autostart(&app, config.autostart);
    persist(&app, &config);
    *state.config.lock().unwrap() = config;
    Ok(())
}

fn sync_autostart(app: &AppHandle, enabled: bool) {
    let mgr = app.autolaunch();
    if enabled {
        let _ = mgr.enable();
    } else {
        let _ = mgr.disable();
    }
}

#[tauri::command]
fn test_command(command_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config.lock().unwrap().clone();
    let cmd = cfg
        .commands
        .iter()
        .find(|c| c.id == command_id)
        .ok_or("Команда не найдена")?;

    let exec = if cfg!(windows) { &cmd.windows_cmd } else { &cmd.linux_cmd };
    if exec.is_empty() {
        return Err("Команда не задана для текущей платформы".into());
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", exec])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(windows))]
    std::process::Command::new("sh")
        .args(["-c", exec])
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(format!("Запущено: {exec}"))
}

#[tauri::command]
fn get_current_platform() -> &'static str {
    if cfg!(windows) { "windows" } else { "linux" }
}

// ─── LM Studio Commands ───────────────────────────────────────────────────────

#[tauri::command]
async fn get_ai_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let url = state.config.lock().unwrap().ollama_url.clone();
    Ok(ollama::list_models(&url).await)
}

#[tauri::command]
async fn check_ollama(url: String) -> bool {
    ollama::is_ollama_running(&url).await
}

// ─── TTS Commands ─────────────────────────────────────────────────────────────

#[tauri::command]
fn test_tts(text: String, engine: String, piper_voice: String, edge_voice: String, edge_rate: i32, custom_cmd: String) {
    tts::speak_with_engine(&text, &engine, &piper_voice, &edge_voice, edge_rate, &custom_cmd);
}

// ─── Edge TTS Commands ────────────────────────────────────────────────────────

#[tauri::command]
async fn get_edge_tts_status() -> bool {
    tokio::task::spawn_blocking(tts::edge_tts_installed)
        .await
        .unwrap_or(false)
}

#[tauri::command]
fn install_edge_tts(app: AppHandle) {
    std::thread::spawn(move || {
        match tts::install_edge_tts_sync() {
            Ok(()) => { let _ = app.emit("edge-tts-installed", true); }
            Err(e) => { let _ = app.emit("edge-tts-error", e); }
        }
    });
}

// ─── Piper Commands ───────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PiperStatus {
    binary_installed: bool,
    voices: Vec<piper::PiperVoice>,
}

#[tauri::command]
fn get_piper_status() -> PiperStatus {
    PiperStatus {
        binary_installed: piper::is_binary_installed(),
        voices: piper::voice_catalog(),
    }
}

#[tauri::command]
fn download_piper_binary(state: State<'_, AppState>, app: AppHandle) {
    let cancel = state.dl_cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        let app2 = app.clone();
        let result = piper::download_binary(
            move |downloaded, total| {
                let pct = if total > 0 { downloaded * 100 / total } else { 0 };
                let _ = app.emit("piper-progress", serde_json::json!({
                    "kind": "binary", "id": "binary", "pct": pct
                }));
            },
            cancel,
        )
        .await;
        match result {
            Ok(()) => { let _ = app2.emit("piper-done", serde_json::json!({ "kind": "binary", "id": "binary" })); }
            Err(e) => { let _ = app2.emit("piper-error", serde_json::json!({ "kind": "binary", "id": "binary", "error": e.to_string() })); }
        }
    });
}

#[tauri::command]
fn download_piper_voice(voice_id: String, state: State<'_, AppState>, app: AppHandle) {
    let voices = piper::voice_catalog();
    let hf_path = match voices.iter().find(|v| v.id == voice_id) {
        Some(v) => v.hf_path.clone(),
        None => { return; }
    };
    let cancel = state.dl_cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    let vid = voice_id.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let vid2 = vid.clone();
        let result = piper::download_voice(
            &vid,
            &hf_path,
            move |downloaded, total| {
                let pct = if total > 0 { downloaded * 100 / total } else { 0 };
                let _ = app.emit("piper-progress", serde_json::json!({
                    "kind": "voice", "id": vid2, "pct": pct
                }));
            },
            cancel,
        )
        .await;
        match result {
            Ok(()) => { let _ = app2.emit("piper-done", serde_json::json!({ "kind": "voice", "id": vid })); }
            Err(e) => { let _ = app2.emit("piper-error", serde_json::json!({ "kind": "voice", "id": vid, "error": e.to_string() })); }
        }
    });
}

#[tauri::command]
fn cancel_piper_download(state: State<'_, AppState>) {
    state.dl_cancel.store(true, Ordering::SeqCst);
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[tauri::command]
fn export_commands(state: State<'_, AppState>) -> Result<String, String> {
    let commands = state.config.lock().unwrap().commands.clone();
    let json = serde_json::to_string_pretty(&commands).map_err(|e| e.to_string())?;
    let path = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("voice-commands.json");
    std::fs::write(&path, &json).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

pub fn run() {
    let shared_config: SharedConfig = Arc::new(Mutex::new(PluginConfig::default()));
    let dl_cancel = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .manage(AppState { config: shared_config.clone(), dl_cancel: dl_cancel.clone() })
        .setup(move |app| {
            let saved = load_from_store(app);
            let args: Vec<String> = std::env::args().collect();
            let port_arg: Option<u16> = args.windows(2)
                .find(|w| w[0] == "--port")
                .and_then(|w| w[1].parse().ok());
            let port = port_arg.unwrap_or(saved.port);
            sync_autostart(app.handle(), saved.autostart);
            *shared_config.lock().unwrap() = saved;

            let cfg_for_server = shared_config.clone();
            let handle_for_server = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server::serve(cfg_for_server, port, handle_for_server).await {
                    eprintln!("[voice-control] Ошибка сервера: {e}");
                }
            });

            if let Some(w) = app.get_webview_window("main") {
                let w_clone = w.clone();
                w.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w_clone.hide();
                    }
                });
            }

            if !std::env::args().any(|a| a == "--background") {
                show_window(app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            test_command,
            get_current_platform,
            export_commands,
            get_ai_models,
            check_ollama,
            test_tts,
            get_edge_tts_status,
            install_edge_tts,
            get_piper_status,
            download_piper_binary,
            download_piper_voice,
            cancel_piper_download,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска easySTT Voice Control");
}
