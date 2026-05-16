mod config;
mod server;

use config::PluginConfig;
use server::SharedConfig;

use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "voice_control.json";
const STORE_KEY: &str = "config";

pub struct AppState {
    pub config: SharedConfig,
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
    persist(&app, &config);
    *state.config.lock().unwrap() = config;
    Ok(())
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

// ─── Entry Point ──────────────────────────────────────────────────────────────

pub fn run() {
    let shared_config: SharedConfig = Arc::new(Mutex::new(PluginConfig::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState { config: shared_config.clone() })
        .setup(move |app| {
            // Загружаем сохранённый конфиг
            let saved = load_from_store(app);
            // --port <num>: easySTT передаёт порт при запуске.
            let args: Vec<String> = std::env::args().collect();
            let port_arg: Option<u16> = args.windows(2)
                .find(|w| w[0] == "--port")
                .and_then(|w| w[1].parse().ok());
            let port = port_arg.unwrap_or(saved.port);
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
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска easySTT Voice Control");
}
