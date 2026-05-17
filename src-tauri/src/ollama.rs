use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ─── Status + model list ──────────────────────────────────────────────────────

pub async fn is_ollama_running(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(2)).build() else {
        return false;
    };
    client.get(format!("{}/v1/models", url.trim_end_matches('/'))).send().await.is_ok()
}

pub async fn list_models(url: &str) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(3)).build() else {
        return vec![];
    };
    let Ok(resp) = client.get(format!("{}/v1/models", url.trim_end_matches('/'))).send().await else {
        return vec![];
    };
    let Ok(json) = resp.json::<Value>().await else { return vec![]; };
    json["data"].as_array()
        .map(|arr| arr.iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

// ─── OpenAI API types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Deserialize, Serialize, Clone)]
struct AssistantMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

fn default_tool_type() -> String { "function".to_string() }

#[derive(Deserialize, Serialize, Clone)]
struct ToolCall {
    id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    kind: String,
    function: ToolCallFn,
}

#[derive(Deserialize, Serialize, Clone)]
struct ToolCallFn {
    name: String,
    arguments: String,
}

// ─── NluResult ────────────────────────────────────────────────────────────────

pub struct NluResult {
    /// Триггер зарегистрированной команды для выполнения в server.rs.
    pub trigger: Option<String>,
    /// Текст для озвучивания.
    pub response_text: Option<String>,
    /// true = озвучить даже если voice_feedback выключен (информационный ответ).
    pub must_speak: bool,
}

// ─── Tool definitions ─────────────────────────────────────────────────────────

fn tool_definitions(commands: &[crate::config::VoiceCommand]) -> Value {
    let mut all_triggers: Vec<String> = commands.iter().map(|c| c.trigger.clone()).collect();
    for c in commands {
        if !c.close_trigger.is_empty() {
            all_triggers.push(c.close_trigger.clone());
        }
    }

    json!([
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Выполнить зарегистрированную голосовую команду. Используй когда пользователь хочет открыть/закрыть приложение или выполнить настроенное действие.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "trigger": {
                            "type": "string",
                            "description": format!("Точная триггерная фраза из списка: {}", all_triggers.join("; "))
                        }
                    },
                    "required": ["trigger"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "shell_query",
                "description": "Выполнить команду и получить вывод для голосового ответа. Используй ТОЛЬКО когда результат можно кратко пересказать вслух (1–2 предложения): сколько RAM занято, статус одного процесса, текущий IP, размер одной папки. Для таблиц, списков и длинного вывода используй show_terminal.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": if cfg!(windows) {
                                "PowerShell команда. Примеры: 'Get-Process | Sort WS -Desc | Select -First 5 Name,@{N=\"MB\";E={[int]($_.WS/1MB)}}'"
                            } else {
                                "Bash команда. Примеры: 'ps aux --sort=-%mem | head -6', 'df -h', 'docker ps'"
                            }
                        }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "kill_process",
                "description": "Завершить/убить процесс по имени. Используй когда пользователь хочет закрыть программу которой нет в списке зарегистрированных команд.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Имя процесса (например: chrome, firefox, notepad, Code)"
                        }
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "show_terminal",
                "description": "Открыть видимый терминал и выполнить команду — пользователь сам читает вывод. Используй когда результат лучше ПОКАЗАТЬ, а не зачитывать вслух: таблицы процессов, списки файлов, логи, docker ps, netstat, дерево директорий и т.п. В отличие от shell_query, вывод НЕ передаётся тебе — ты просто подтверждаешь что открыл окно.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Команда для выполнения в видимом терминале"
                        },
                        "title": {
                            "type": "string",
                            "description": "Короткий заголовок окна (например: «Процессы», «Docker», «Диск»)"
                        }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "open_url",
                "description": "Открыть URL в браузере по умолчанию.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "Полный URL включая https://" }
                    },
                    "required": ["url"]
                }
            }
        }
    ])
}

// ─── Tool execution ───────────────────────────────────────────────────────────

fn exec_tool(name: &str, args: &Value) -> String {
    match name {
        "shell_query" => {
            let cmd = args["command"].as_str().unwrap_or("echo пусто");
            shell_capture(cmd)
        }
        "show_terminal" => {
            let cmd = args["command"].as_str().unwrap_or("echo пусто");
            let title = args["title"].as_str().unwrap_or("Агент");
            show_in_terminal(cmd, title);
            "Терминал открыт, пользователь видит вывод".to_string()
        }
        "kill_process" => {
            let pname = args["name"].as_str().unwrap_or("");
            kill_proc(pname)
        }
        "open_url" => {
            let url = args["url"].as_str().unwrap_or("");
            if !url.is_empty() { open_url(url); }
            format!("Открываю {url}")
        }
        _ => "Неизвестный инструмент".to_string(),
    }
}

fn show_in_terminal(cmd: &str, title: &str) {
    let cmd = cmd.to_string();
    let title = title.to_string();
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            // PowerShell с -NoExit чтобы окно не закрылось сразу
            let script = format!(
                "$host.UI.RawUI.WindowTitle = '{title}'; {cmd}; Write-Host ''; Write-Host 'Нажмите Enter для закрытия...' -ForegroundColor DarkGray; Read-Host",
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .spawn();
        }
        #[cfg(not(windows))]
        {
            // Пробуем разные терминальные эмуляторы
            let bash_cmd = format!("{cmd}; echo; read -p 'Нажмите Enter для закрытия...'");
            let launched =
                std::process::Command::new("gnome-terminal")
                    .args(["--title", &title, "--", "bash", "-c", &bash_cmd])
                    .spawn().is_ok()
                || std::process::Command::new("konsole")
                    .args(["--title", &title, "-e", "bash", "-c", &bash_cmd])
                    .spawn().is_ok()
                || std::process::Command::new("xfce4-terminal")
                    .args(["--title", &title, "-e", &format!("bash -c '{bash_cmd}'")])
                    .spawn().is_ok();
            if !launched {
                let _ = std::process::Command::new("xterm")
                    .args(["-title", &title, "-e", "bash", "-c", &bash_cmd])
                    .spawn();
            }
        }
    });
}

fn shell_capture(cmd: &str) -> String {
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", cmd])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
    };
    #[cfg(not(windows))]
    let result = std::process::Command::new("sh").args(["-c", cmd]).output();

    match result {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if e.is_empty() { "Нет вывода".to_string() } else { e }
            } else {
                // Ограничиваем вывод чтобы не перегружать LLM
                if s.len() > 2000 { format!("{}…(обрезано)", &s[..2000]) } else { s }
            }
        }
        Err(e) => format!("Ошибка: {e}"),
    }
}

fn kill_proc(name: &str) -> String {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let exe = if name.ends_with(".exe") { name.to_string() } else { format!("{name}.exe") };
        match std::process::Command::new("taskkill")
            .args(["/f", "/im", &exe])
            .creation_flags(0x08000000)
            .output()
        {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { format!("Процесс {name} завершён") } else { s }
            }
            Err(e) => format!("Ошибка: {e}"),
        }
    }
    #[cfg(not(windows))]
    {
        match std::process::Command::new("pkill").arg(name).output() {
            Ok(_) => format!("Процесс {name} завершён"),
            Err(e) => format!("Ошибка: {e}"),
        }
    }
}

fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .creation_flags(0x08000000)
            .spawn();
    }
    #[cfg(not(windows))]
    { let _ = std::process::Command::new("xdg-open").arg(url).spawn(); }
}

// ─── Main NLU entry point ─────────────────────────────────────────────────────

pub async fn nlu_and_respond<F>(
    url: &str,
    model_id: &str,
    agent_name: &str,
    commands: &[crate::config::VoiceCommand],
    user_text: &str,
    style: &str,
    extra_system: &str,
    log: &F,
) -> anyhow::Result<NluResult>
where
    F: Fn(&str, &str) + Send,
{
    if model_id.is_empty() {
        return Ok(NluResult { trigger: None, response_text: None, must_speak: false });
    }
    log("debug", &format!("→ tools: {n} команд", n = commands.len()));

    let endpoint = format!("{}/v1/chat/completions", url.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(Duration::from_secs(90)).build()?;

    let is_thinking = model_id.contains("qwen3") || model_id.contains("qwq");
    let os_hint = if cfg!(windows) { "Windows (используй PowerShell)" } else { "Linux (используй bash)" };
    let style_hint = if style == "fun" { "Стиль: с характером, шутливый." } else { "Стиль: нейтральный, краткий." };

    let base = format!(
        "Ты голосовой ассистент {agent_name}. ОС: {os_hint}.\n\
         Используй доступные инструменты для выполнения запросов. \
         Ответы короткие — они будут озвучены вслух (максимум 2–3 предложения). \
         {style_hint} Отвечай на русском."
    );
    let system = if extra_system.is_empty() {
        base
    } else {
        format!("{base}\n\n{extra_system}")
    };

    let mut req_body = json!({
        "model": model_id,
        "stream": false,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user_text}
        ],
        "tools": tool_definitions(commands),
        "tool_choice": "auto",
        "temperature": 0.3,
        "max_tokens": 512,
    });
    if is_thinking { req_body["enable_thinking"] = json!(false); }

    let resp = client.post(&endpoint).json(&req_body).send().await?;
    let chat: ChatResponse = resp.json().await?;
    let msg = chat.choices.into_iter().next()
        .map(|c| c.message)
        .ok_or_else(|| anyhow::anyhow!("Нет ответа от модели"))?;

    // ── Если модель вернула tool_calls ────────────────────────────────────────
    if let Some(ref tool_calls) = msg.tool_calls {
        let mut run_trigger: Option<String> = None;
        let mut shell_query_results: Vec<Value> = Vec::new(); // требуют второго запроса
        let mut action_responses: Vec<String> = Vec::new();  // уже готовый текст для TTS

        for tc in tool_calls {
            let args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();

            match tc.function.name.as_str() {
                "run_command" => {
                    let trig = args["trigger"].as_str().unwrap_or("").to_string();
                    log("info", &format!("🔧 run_command(«{trig}»)"));
                    run_trigger = Some(trig);
                }
                "shell_query" => {
                    // Вывод нужно интерпретировать через LLM → второй запрос
                    log("info", &format!("🔧 shell_query({})", tc.function.arguments));
                    let result = exec_tool("shell_query", &args);
                    let preview = if result.len() > 120 { format!("{}…", &result[..120]) } else { result.clone() };
                    log("debug", &format!("📤 {preview}"));
                    shell_query_results.push(json!({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": result
                    }));
                }
                tool_name => {
                    // show_terminal / kill_process / open_url — выполняем и берём их строку ответа
                    log("info", &format!("🔧 {tool_name}({})", tc.function.arguments));
                    let result = exec_tool(tool_name, &args);
                    let preview = if result.len() > 120 { format!("{}…", &result[..120]) } else { result.clone() };
                    log("debug", &format!("📤 {preview}"));
                    action_responses.push(result);
                }
            }
        }

        // Быстрый путь: только run_command и/или action tools — второй запрос не нужен
        if shell_query_results.is_empty() {
            let response_text = if action_responses.is_empty() {
                None
            } else {
                Some(action_responses.join("; "))
            };
            return Ok(NluResult { trigger: run_trigger, response_text, must_speak: !action_responses.is_empty() });
        }

        // Второй запрос: LLM суммирует вывод shell_query в голосовой ответ
        let mut assistant_msg = json!({
            "role": "assistant",
            "tool_calls": msg.tool_calls
        });
        if let Some(ref c) = msg.content {
            assistant_msg["content"] = json!(c);
        }

        let mut messages = vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": user_text}),
            assistant_msg,
        ];
        messages.extend(shell_query_results);

        let mut req2 = json!({
            "model": model_id,
            "stream": false,
            "messages": messages,
            "temperature": 0.5,
            "max_tokens": 256,
        });
        if is_thinking { req2["enable_thinking"] = json!(false); }

        let resp2 = client.post(&endpoint).json(&req2).send().await?;
        // Читаем тело как текст чтобы видеть ошибку в логах если что-то пошло не так
        let body2 = resp2.text().await?;
        let chat2 = serde_json::from_str::<ChatResponse>(&body2).map_err(|e| {
            let snippet = if body2.len() > 300 { &body2[..300] } else { &body2 };
            log("error", &format!("Ответ модели (2й запрос): {snippet}"));
            anyhow::anyhow!("Ошибка парсинга ответа: {e}")
        })?;

        let response_text = chat2.choices.into_iter().next()
            .and_then(|c| c.message.content)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Готово".to_string());

        return Ok(NluResult {
            trigger: run_trigger,
            response_text: Some(response_text),
            must_speak: true,
        });
    }

    log("warn", "⚠ Модель не вернула tool_calls — fallback к JSON-парсингу");
    // Fallback: парсим как старый JSON-формат на случай если модель не умеет tool calling
    let text = msg.content.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<Value>(
        text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim()
    ) {
        let trigger = parsed["trigger"].as_str()
            .filter(|&s| s != "null" && !s.is_empty()).map(|s| s.to_string());
        let response_text = parsed["response"].as_str()
            .filter(|&s| s != "null" && !s.is_empty()).map(|s| s.to_string());
        return Ok(NluResult { trigger, response_text, must_speak: false });
    }

    Ok(NluResult { trigger: None, response_text: None, must_speak: false })
}
