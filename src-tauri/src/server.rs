use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::config::PluginConfig;

pub type SharedConfig = Arc<Mutex<PluginConfig>>;

#[derive(Clone)]
pub struct ServerState {
    pub config: SharedConfig,
    pub port: u16,
    pub app_handle: AppHandle,
}

#[derive(Deserialize)]
struct InterceptRequest {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterceptResponse {
    intercept: bool,
    /// True when the agent name was detected in the text but no command matched.
    /// easySTT uses this to trigger PTT recording (wake-word mode).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    agent_detected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    matched_trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback: Option<String>,
}

async fn intercept(
    State(state): State<ServerState>,
    Json(body): Json<InterceptRequest>,
) -> impl IntoResponse {
    let cfg = state.config.lock().unwrap().clone();

    if !cfg.enabled {
        return (
            StatusCode::OK,
            Json(InterceptResponse { intercept: false, agent_detected: false, matched_trigger: None, feedback: None }),
        );
    }

    let text = normalize(&body.text);
    let agent = normalize(&cfg.agent_name);

    let command_text = match strip_agent_prefix(&text, &agent) {
        Some(rest) => rest,
        None => {
            return (
                StatusCode::OK,
                Json(InterceptResponse { intercept: false, agent_detected: false, matched_trigger: None, feedback: None }),
            );
        }
    };

    // 1. Точное совпадение
    if let Some((trigger, exec_cmd, label)) = find_exact_match(&command_text, &cfg) {
        execute_shell(&exec_cmd);
        maybe_speak(&cfg, None);
        return (StatusCode::OK, Json(InterceptResponse {
            intercept: true, agent_detected: true,
            matched_trigger: Some(trigger),
            feedback: Some(format!("Выполняю: {label}")),
        }));
    }

    // 2. Последовательное выполнение: «открой браузер и открой калькулятор»
    let parts = split_conjunctions(&command_text);
    if parts.len() > 1 {
        let mut executed: Vec<String> = Vec::new();
        'parts: for part in &parts {
            for cmd in &cfg.commands {
                let trigger = normalize(&cmd.trigger);
                if matches_trigger(part, &trigger)
                    || cmd.aliases.iter().any(|a| matches_trigger(part, &normalize(a)))
                {
                    let exec_cmd = if cfg!(windows) { &cmd.windows_cmd } else { &cmd.linux_cmd };
                    if !exec_cmd.is_empty() {
                        execute_shell(exec_cmd);
                        executed.push(if cmd.description.is_empty() { cmd.trigger.clone() } else { cmd.description.clone() });
                    }
                    continue 'parts;
                }
                if !cmd.close_trigger.is_empty() {
                    let ct = normalize(&cmd.close_trigger);
                    if matches_trigger(part, &ct)
                        || cmd.close_aliases.iter().any(|a| matches_trigger(part, &normalize(a)))
                    {
                        let exec_cmd = if cfg!(windows) { &cmd.windows_close_cmd } else { &cmd.linux_close_cmd };
                        if !exec_cmd.is_empty() {
                            execute_shell(exec_cmd);
                            let label = if cmd.description.is_empty() { cmd.close_trigger.clone() } else { cmd.description.clone() };
                            executed.push(format!("закрыть {}", label));
                        }
                        continue 'parts;
                    }
                }
            }
        }
        if !executed.is_empty() {
            maybe_speak(&cfg, None);
            return (StatusCode::OK, Json(InterceptResponse {
                intercept: true, agent_detected: true,
                matched_trigger: None,
                feedback: Some(format!("Выполняю: {}", executed.join(" → "))),
            }));
        }
    }

    // 3. LM Studio NLU — запускаем в фоне, чтобы не ждать ответа модели.
    // easySTT имеет короткий таймаут на /intercept; мыслительные модели (qwen3 и т.п.)
    // генерируют reasoning_content перед answer — это может занять 5–30 с.
    // Отвечаем easySTT сразу «агент обнаружен», а команду выполняем когда модель ответит.
    if cfg.ollama_enabled && !cfg.ollama_url.is_empty() && !cfg.ollama_model.is_empty() {
        let cfg2 = cfg.clone();
        let text2 = command_text.clone();
        tokio::spawn(async move {
            match crate::ollama::nlu_and_respond(
                &cfg2.ollama_url,
                &cfg2.ollama_model,
                &cfg2.agent_name,
                &cfg2.commands,
                &text2,
                &cfg2.voice_feedback_style,
            )
            .await
            {
                Ok(nlu) => {
                    if let Some(ref trig) = nlu.trigger {
                        let norm = normalize(trig);
                        let found = cfg2.commands.iter().find(|c| {
                            normalize(&c.trigger) == norm
                                || normalize(&c.close_trigger) == norm
                                || c.aliases.iter().any(|a| normalize(a) == norm)
                                || c.close_aliases.iter().any(|a| normalize(a) == norm)
                        });
                        if let Some(cmd) = found {
                            let is_close = !cmd.close_trigger.is_empty()
                                && (normalize(&cmd.close_trigger) == norm
                                    || cmd.close_aliases.iter().any(|a| normalize(a) == norm));
                            let exec_cmd = if is_close {
                                if cfg!(windows) { &cmd.windows_close_cmd } else { &cmd.linux_close_cmd }
                            } else {
                                if cfg!(windows) { &cmd.windows_cmd } else { &cmd.linux_cmd }
                            };
                            if !exec_cmd.is_empty() {
                                execute_shell(exec_cmd);
                                maybe_speak(&cfg2, nlu.response_text.as_deref());
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[voice-control] NLU error: {e}");
                }
            }
        });

        // Немедленно возвращаем «перехвачено» — easySTT не покажет текст,
        // а команда выполнится фоновым тасками выше.
        return (StatusCode::OK, Json(InterceptResponse {
            intercept: true, agent_detected: true, matched_trigger: None,
            feedback: Some("AI обрабатывает команду...".to_string()),
        }));
    }

    // Агент обнаружен, команда не распознана → wake-word для easySTT
    (StatusCode::OK, Json(InterceptResponse {
        intercept: false, agent_detected: true, matched_trigger: None, feedback: None,
    }))
}

/// Воспроизводит голосовой ответ если включён voice_feedback.
/// `custom` — текст от Ollama; None → рандомная фраза из пула.
fn maybe_speak(cfg: &crate::config::PluginConfig, custom: Option<&str>) {
    if !cfg.voice_feedback_enabled {
        return;
    }
    let text = custom
        .map(|s| s.to_string())
        .unwrap_or_else(|| crate::tts::random_response(&cfg.voice_feedback_style));
    crate::tts::speak_with_engine(&text, &cfg.voice_engine, &cfg.piper_voice, &cfg.edge_tts_voice, cfg.edge_tts_rate, &cfg.voice_custom_cmd);
}

/// Ищет точное совпадение и возвращает (trigger, exec_cmd, label) если нашёл.
fn find_exact_match(
    command_text: &str,
    cfg: &crate::config::PluginConfig,
) -> Option<(String, String, String)> {
    for cmd in &cfg.commands {
        let trigger = normalize(&cmd.trigger);
        if matches_trigger(command_text, &trigger)
            || cmd.aliases.iter().any(|a| matches_trigger(command_text, &normalize(a)))
        {
            let exec = if cfg!(windows) { &cmd.windows_cmd } else { &cmd.linux_cmd };
            let label = if cmd.description.is_empty() { &cmd.trigger } else { &cmd.description };
            return Some((cmd.trigger.clone(), exec.clone(), label.clone()));
        }
        if !cmd.close_trigger.is_empty() {
            let ct = normalize(&cmd.close_trigger);
            if matches_trigger(command_text, &ct)
                || cmd.close_aliases.iter().any(|a| matches_trigger(command_text, &normalize(a)))
            {
                let exec = if cfg!(windows) { &cmd.windows_close_cmd } else { &cmd.linux_close_cmd };
                let label = if cmd.description.is_empty() { &cmd.close_trigger } else { &cmd.description };
                return Some((cmd.close_trigger.clone(), exec.clone(), label.clone()));
            }
        }
    }
    None
}


/// Разбивает текст по союзам-разделителям для последовательного выполнения команд.
fn split_conjunctions(text: &str) -> Vec<String> {
    let separators = [" и ", " затем ", " потом ", " а затем ", " а потом ", " также ", " and ", " then "];
    let mut parts = vec![text.to_string()];
    for sep in &separators {
        let mut new_parts = Vec::new();
        for part in &parts {
            for sub in part.split(sep) {
                let s = sub.trim().to_string();
                if !s.is_empty() {
                    new_parts.push(s);
                }
            }
        }
        parts = new_parts;
    }
    parts
}

/// Убираем имя агента + разделители (запятые, пробелы) из начала строки.
fn strip_agent_prefix(text: &str, agent: &str) -> Option<String> {
    if agent.is_empty() || !text.starts_with(agent) {
        return None;
    }
    let rest = &text[agent.len()..];
    let rest = rest.trim_matches(|c: char| c == ',' || c == '.' || c == '!' || c.is_whitespace());
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Нормализация: нижний регистр, только буквы и пробелы, схлопывание пробелов.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphabetic() || c.is_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Все слова триггера должны присутствовать в тексте команды (порядок не важен).
/// Это терпимо к минорным расхождениям в распознавании.
fn matches_trigger(text: &str, trigger: &str) -> bool {
    if text == trigger {
        return true;
    }
    if text.contains(trigger) {
        return true;
    }
    let trigger_words: Vec<&str> = trigger.split_whitespace().collect();
    let text_words: Vec<&str> = text.split_whitespace().collect();
    !trigger_words.is_empty() && trigger_words.iter().all(|tw| text_words.contains(tw))
}

fn execute_shell(cmd: &str) {
    let cmd = cmd.to_string();
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd")
                .args(["/C", &cmd])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("sh")
                .args(["-c", &cmd])
                .spawn();
        }
    });
}

async fn status() -> impl IntoResponse {
    Json(json!({
        "alive": true,
        "agent": "easystt-voice-control",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn plugin_manifest(State(state): State<ServerState>) -> impl IntoResponse {
    let cfg = state.config.lock().unwrap().clone();
    Json(json!({
        "name": "easySTT Voice Control",
        "version": env!("CARGO_PKG_VERSION"),
        "port": state.port,
        "agentName": cfg.agent_name,
        "description": "Перехват голосовых команд и выполнение системных команд"
    }))
}

async fn open_settings(State(state): State<ServerState>) -> impl IntoResponse {
    if let Some(w) = state.app_handle.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}

/// Удаляет сохранённый конфиг плагина и завершает процесс.
/// Вызывается easySTT при удалении плагина из списка.
async fn reset_config(State(state): State<ServerState>) -> impl IntoResponse {
    use tauri::Manager;
    if let Ok(config_dir) = state.app_handle.path().app_config_dir() {
        let _ = std::fs::remove_file(config_dir.join("voice_control.json"));
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        std::process::exit(0);
    });
    (StatusCode::OK, Json(json!({ "ok": true })))
}

pub fn build_router(config: SharedConfig, port: u16, app_handle: AppHandle) -> Router {
    let state = ServerState { config, port, app_handle };
    Router::new()
        .route("/status", get(status))
        .route("/plugin-manifest", get(plugin_manifest))
        .route("/intercept", post(intercept))
        .route("/open-settings", post(open_settings))
        .route("/reset", post(reset_config))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(config: SharedConfig, port: u16, app_handle: AppHandle) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[voice-control] HTTP сервер запущен на http://127.0.0.1:{port}");
    axum::serve(listener, build_router(config, port, app_handle)).await?;
    Ok(())
}
