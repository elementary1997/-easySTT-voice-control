mod config;
mod ollama;
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
    pub pull_cancel: Arc<AtomicBool>,
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

/// Немедленно выполняет команду по её id (кнопка «Тест» в UI).
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
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
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

// ─── Ollama Commands ──────────────────────────────────────────────────────────

#[tauri::command]
async fn get_ollama_catalog(state: State<'_, AppState>) -> Result<Vec<ollama::ModelCatalogEntry>, String> {
    let url = state.config.lock().unwrap().ollama_url.clone();
    Ok(ollama::catalog_with_status(&url).await)
}

#[tauri::command]
async fn check_ollama(url: String) -> bool {
    ollama::is_ollama_running(&url).await
}

#[tauri::command]
fn pull_ollama_model(
    model_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) {
    let url = state.config.lock().unwrap().ollama_url.clone();
    let cancel = state.pull_cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    let app2 = app.clone();
    let mid = model_id.clone();
    let mid2 = model_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = ollama::pull_model(&url, &mid, move |status, percent| {
            let _ = app.emit("ollama-pull-progress", serde_json::json!({
                "model": mid2,
                "status": status,
                "percent": percent,
            }));
        }, cancel).await;
        match result {
            Ok(()) => { let _ = app2.emit("ollama-pull-done", serde_json::json!({ "model": mid })); }
            Err(e) => { let _ = app2.emit("ollama-pull-error", serde_json::json!({ "model": mid, "error": e.to_string() })); }
        }
    });
}

#[tauri::command]
fn cancel_ollama_pull(state: State<'_, AppState>) {
    state.pull_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn test_tts(text: String) {
    tts::speak(&text);
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
    let pull_cancel = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .manage(AppState { config: shared_config.clone(), pull_cancel: pull_cancel.clone() })
        .setup(move |app| {
            // Загружаем сохранённый конфиг
            let saved = load_from_store(app);
            // --port <num>: easySTT передаёт порт при запуске.
            let args: Vec<String> = std::env::args().collect();
            let port_arg: Option<u16> = args.windows(2)
                .find(|w| w[0] == "--port")
                .and_then(|w| w[1].parse().ok());
            let port = port_arg.unwrap_or(saved.port);
            sync_autostart(app.handle(), saved.autostart);
            *shared_config.lock().unwrap() = saved;

            // HTTP-сервер
            let cfg_for_server = shared_config.clone();
            let handle_for_server = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server::serve(cfg_for_server, port, handle_for_server).await {
                    eprintln!("[voice-control] Ошибка сервера: {e}");
                }
            });

            // Закрытие окна = скрыть, не выходить (HTTP-сервер продолжает работать).
            if let Some(w) = app.get_webview_window("main") {
                let w_clone = w.clone();
                w.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w_clone.hide();
                    }
                });
            }

            // --background: запущен easySTT'ом → окно скрыто.
            // Без флага (ручной запуск) → открываем настройки.
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
            get_ollama_catalog,
            check_ollama,
            pull_ollama_model,
            cancel_ollama_pull,
            test_tts,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска easySTT Voice Control");
}
