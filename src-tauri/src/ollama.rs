use std::time::Duration;

// ─── Status + model list (OpenAI-compatible API — LM Studio) ─────────────────

pub async fn is_ollama_running(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}/v1/models", url.trim_end_matches('/')))
        .send()
        .await
        .is_ok()
}

/// Возвращает список ID моделей доступных в LM Studio.
pub async fn list_models(url: &str) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return vec![];
    };
    let Ok(resp) = client
        .get(format!("{}/v1/models", url.trim_end_matches('/')))
        .send()
        .await
    else {
        return vec![];
    };
    let Ok(json) = resp.json::<serde_json::Value>().await else {
        return vec![];
    };
    json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

// ─── NLU + response generation ────────────────────────────────────────────────

pub struct NluResult {
    pub trigger: Option<String>,
    pub response_text: Option<String>,
}

/// Один вызов к OpenAI-compatible API: классификация + генерация ответа.
pub async fn nlu_and_respond(
    url: &str,
    model_id: &str,
    agent_name: &str,
    commands: &[crate::config::VoiceCommand],
    user_text: &str,
    style: &str,
) -> anyhow::Result<NluResult> {
    if model_id.is_empty() {
        return Ok(NluResult { trigger: None, response_text: None });
    }

    let endpoint = format!("{}/v1/chat/completions", url.trim_end_matches('/'));

    let commands_list: Vec<String> = commands
        .iter()
        .map(|c| {
            let mut line = format!("- \"{}\"", c.trigger);
            if !c.description.is_empty() {
                line.push_str(&format!(" ({})", c.description));
            }
            if !c.close_trigger.is_empty() {
                line.push_str(&format!(" | закрыть: \"{}\"", c.close_trigger));
            }
            for alias in &c.aliases {
                if !alias.is_empty() {
                    line.push_str(&format!(" | alias: \"{}\"", alias));
                }
            }
            line
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

Доступные команды (верни точную фразу из кавычек в поле trigger):
{commands}

Пользователь сказал: "{user_text}"

{style_hint}

Ответь ТОЛЬКО валидным JSON (без текста до и после). Поле trigger — точная фраза триггера из списка выше, или null:
{{"trigger": "точная фраза или null", "response": "твой ответ или null"}}"#,
        commands = commands_list.join("\n")
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
            "temperature": 0.1,
            "max_tokens": 128,
            // отключаем reasoning для qwen3 и аналогичных thinking-моделей
            "enable_thinking": false,
        }))
        .send()
        .await?;

    let json = resp.json::<serde_json::Value>().await?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    let clean = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(clean)
        .unwrap_or_else(|_| serde_json::json!({ "trigger": null, "response": null }));

    let trigger = parsed["trigger"]
        .as_str()
        .filter(|&s| s != "null" && !s.is_empty())
        .map(|s| s.to_string());

    let response_text = parsed["response"]
        .as_str()
        .filter(|&s| s != "null" && !s.is_empty())
        .map(|s| s.to_string());

    Ok(NluResult { trigger, response_text })
}
