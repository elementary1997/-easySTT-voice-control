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

use crate::config::PluginConfig;

pub type SharedConfig = Arc<Mutex<PluginConfig>>;

#[derive(Deserialize)]
struct InterceptRequest {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterceptResponse {
    intercept: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    matched_trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback: Option<String>,
}

async fn intercept(
    State(shared): State<SharedConfig>,
    Json(body): Json<InterceptRequest>,
) -> impl IntoResponse {
    let cfg = shared.lock().unwrap().clone();

    if !cfg.enabled {
        return (
            StatusCode::OK,
            Json(InterceptResponse { intercept: false, matched_trigger: None, feedback: None }),
        );
    }

    let text = normalize(&body.text);
    let agent = normalize(&cfg.agent_name);

    let command_text = match strip_agent_prefix(&text, &agent) {
        Some(rest) => rest,
        None => {
            return (
                StatusCode::OK,
                Json(InterceptResponse { intercept: false, matched_trigger: None, feedback: None }),
            );
        }
    };

    for cmd in &cfg.commands {
        let trigger = normalize(&cmd.trigger);
        if matches_trigger(&command_text, &trigger) {
            let exec_cmd = if cfg!(windows) { &cmd.windows_cmd } else { &cmd.linux_cmd };
            let feedback = if exec_cmd.is_empty() {
                format!("Команда «{}» не задана для этой платформы", cmd.trigger)
            } else {
                execute_shell(exec_cmd);
                format!("Выполняю: {}", if cmd.description.is_empty() { &cmd.trigger } else { &cmd.description })
            };
            return (
                StatusCode::OK,
                Json(InterceptResponse {
                    intercept: true,
                    matched_trigger: Some(cmd.trigger.clone()),
                    feedback: Some(feedback),
                }),
            );
        }
    }

    (
        StatusCode::OK,
        Json(InterceptResponse { intercept: false, matched_trigger: None, feedback: None }),
    )
}

/// Убираем имя агента + разделители (запятые, пробелы) из начала строки.
fn strip_agent_prefix(text: &str, agent: &str) -> Option<String> {
    if !text.starts_with(agent.as_bytes()) {
        // Проверяем с учётом нормализации
        if !text.starts_with(agent) {
            return None;
        }
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

pub fn build_router(config: SharedConfig) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/intercept", post(intercept))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(config)
}

pub async fn serve(config: SharedConfig, port: u16) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[voice-control] HTTP сервер запущен на http://127.0.0.1:{port}");
    axum::serve(listener, build_router(config)).await?;
    Ok(())
}
