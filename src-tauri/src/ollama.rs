use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

// ─── Model catalog ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub id: String,
    pub display_name: String,
    pub size_mb: u32,
    pub description: String,
    pub recommended: bool,
    pub installed: bool,
}

pub fn model_catalog_static() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: "llama3.2:1b".into(),
            display_name: "Llama 3.2 1B".into(),
            size_mb: 1300,
            description: "Быстрая, хорошо понимает русский. Рекомендуется.".into(),
            recommended: true,
            installed: false,
        },
        ModelCatalogEntry {
            id: "qwen2.5:1.5b".into(),
            display_name: "Qwen 2.5 1.5B".into(),
            size_mb: 986,
            description: "Лёгкая, отличный русский и многоязычность.".into(),
            recommended: false,
            installed: false,
        },
        ModelCatalogEntry {
            id: "llama3.2:3b".into(),
            display_name: "Llama 3.2 3B".into(),
            size_mb: 2000,
            description: "Умнее 1B, лучше понимает сложные формулировки.".into(),
            recommended: false,
            installed: false,
        },
        ModelCatalogEntry {
            id: "gemma2:2b".into(),
            display_name: "Gemma 2 2B".into(),
            size_mb: 1600,
            description: "От Google, хороший баланс скорость / качество.".into(),
            recommended: false,
            installed: false,
        },
        ModelCatalogEntry {
            id: "phi3.5:mini".into(),
            display_name: "Phi 3.5 Mini".into(),
            size_mb: 2200,
            description: "Лучшее качество понимания, чуть медленнее.".into(),
            recommended: false,
            installed: false,
        },
    ]
}

// ─── Status checks ────────────────────────────────────────────────────────────

pub async fn is_ollama_running(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}/api/tags", url.trim_end_matches('/')))
        .send()
        .await
        .is_ok()
}

pub async fn installed_models(url: &str) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return vec![];
    };
    let Ok(resp) = client
        .get(format!("{}/api/tags", url.trim_end_matches('/')))
        .send()
        .await
    else {
        return vec![];
    };
    let Ok(json) = resp.json::<serde_json::Value>().await else {
        return vec![];
    };
    json["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Возвращает каталог моделей с флагом installed для каждой.
pub async fn catalog_with_status(url: &str) -> Vec<ModelCatalogEntry> {
    let installed = installed_models(url).await;
    model_catalog_static()
        .into_iter()
        .map(|mut e| {
            e.installed = installed
                .iter()
                .any(|n| n == &e.id || n.starts_with(&format!("{}:", e.id.split(':').next().unwrap_or(""))));
            e
        })
        .collect()
}

// ─── Pull model ───────────────────────────────────────────────────────────────

pub async fn pull_model(
    url: &str,
    model_id: &str,
    on_progress: impl Fn(String, Option<f64>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let endpoint = format!("{}/api/pull", url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(7200))
        .build()?;

    let resp = client
        .post(&endpoint)
        .json(&serde_json::json!({ "model": model_id, "stream": true }))
        .send()
        .await?;

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("Отменено пользователем"));
        }
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos + 1..].to_string();
            if line.is_empty() {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                let status = json["status"].as_str().unwrap_or("").to_string();
                let percent = match (json["completed"].as_u64(), json["total"].as_u64()) {
                    (Some(c), Some(t)) if t > 0 => Some(c as f64 / t as f64 * 100.0),
                    _ => None,
                };
                on_progress(status.clone(), percent);
                if status == "success" {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

// ─── NLU + response generation ────────────────────────────────────────────────

pub struct NluResult {
    pub command_id: Option<String>,
    pub response_text: Option<String>,
}

/// Один вызов к Ollama: классификация команды + генерация ответа.
pub async fn nlu_and_respond(
    url: &str,
    model_id: &str,
    agent_name: &str,
    commands: &[crate::config::VoiceCommand],
    user_text: &str,
    style: &str,
) -> anyhow::Result<NluResult> {
    let endpoint = format!("{}/api/chat", url.trim_end_matches('/'));

    let commands_json: Vec<serde_json::Value> = commands
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "trigger": c.trigger,
                "aliases": c.aliases,
                "description": c.description,
                "close_trigger": c.close_trigger,
            })
        })
        .collect();

    let style_hint = if style == "fun" {
        "Стиль ответа: шутливый, с характером (например «Слушаюсь, шеф!», «Уже бегу!»)"
    } else {
        "Стиль ответа: нейтральный, краткий (например «Выполняю», «Готово»)"
    };

    let prompt = format!(
        r#"Ты голосовой ассистент {agent_name}. Задача — одновременно:
1. Определи, какую команду из списка имел в виду пользователь (или null если ни одна).
2. Сгенерируй короткий ответ на русском (1–5 слов).

Доступные команды:
{commands}

Пользователь сказал: "{user_text}"

{style_hint}

Ответь ТОЛЬКО валидным JSON (без текста до и после):
{{"command_id": "id или null", "response": "твой ответ или null"}}"#,
        commands = serde_json::to_string_pretty(&commands_json)?
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let resp = client
        .post(&endpoint)
        .json(&serde_json::json!({
            "model": model_id,
            "stream": false,
            "messages": [{ "role": "user", "content": prompt }],
            "options": { "temperature": 0.2, "num_predict": 64 }
        }))
        .send()
        .await?;

    let json = resp.json::<serde_json::Value>().await?;
    let content = json["message"]["content"].as_str().unwrap_or("{}");

    // Strip markdown code fences if model wraps the JSON
    let clean = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(clean)
        .unwrap_or_else(|_| serde_json::json!({ "command_id": null, "response": null }));

    let command_id = parsed["command_id"]
        .as_str()
        .filter(|&s| s != "null" && !s.is_empty())
        .map(|s| s.to_string());

    let response_text = parsed["response"]
        .as_str()
        .filter(|&s| s != "null" && !s.is_empty())
        .map(|s| s.to_string());

    Ok(NluResult { command_id, response_text })
}
