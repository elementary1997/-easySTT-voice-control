use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCommand {
    pub id: String,
    pub trigger: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub windows_cmd: String,
    pub linux_cmd: String,
    pub description: String,
    /// Категория для группировки в UI. Пустая строка = «Другое».
    #[serde(default)]
    pub category: String,
    // ── Действие «Закрыть» (необязательно) ──────────────────────────────────
    #[serde(default)]
    pub close_trigger: String,
    #[serde(default)]
    pub close_aliases: Vec<String>,
    #[serde(default)]
    pub windows_close_cmd: String,
    #[serde(default)]
    pub linux_close_cmd: String,
}

fn default_true() -> bool { true }
fn default_ollama_url() -> String { "http://localhost:1234".to_string() }
fn default_ollama_model() -> String { String::new() }
fn default_voice_style() -> String { "neutral".to_string() }
fn default_voice_engine() -> String { "system".to_string() }
fn default_piper_voice() -> String { "ru_RU-denis-medium".to_string() }
fn default_edge_voice() -> String { "ru-RU-SvetlanaNeural".to_string() }
fn default_edge_rate() -> i32 { 0 }

fn default_categories() -> Vec<String> {
    vec![
        "Приложения".to_string(),
        "Браузер".to_string(),
        "Система".to_string(),
        "Настройки".to_string(),
        "Утилиты".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub autostart: bool,
    pub agent_name: String,
    pub port: u16,
    pub commands: Vec<VoiceCommand>,
    #[serde(default = "default_categories")]
    pub categories: Vec<String>,

    // ── Ollama / NLU ──────────────────────────────────────────────────────────
    /// Включить Ollama для умного распознавания команд.
    #[serde(default)]
    pub ollama_enabled: bool,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    /// ID модели Ollama (напр. "llama3.2:1b").
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    /// Дополнительные инструкции к системному промпту (добавляются после базового промпта).
    #[serde(default)]
    pub ollama_system_prompt: String,

    // ── TTS / Голосовой ответ ─────────────────────────────────────────────────
    #[serde(default)]
    pub voice_feedback_enabled: bool,
    /// "neutral" | "fun"
    #[serde(default = "default_voice_style")]
    pub voice_feedback_style: String,
    /// "system" | "piper" | "edge" | "custom"
    #[serde(default = "default_voice_engine")]
    pub voice_engine: String,
    /// Выбранный голос Piper (id из каталога).
    #[serde(default = "default_piper_voice")]
    pub piper_voice: String,
    /// Голос Edge TTS (напр. "ru-RU-SvetlanaNeural").
    #[serde(default = "default_edge_voice")]
    pub edge_tts_voice: String,
    /// Скорость Edge TTS: -50..+100, 0 = норма.
    #[serde(default = "default_edge_rate")]
    pub edge_tts_rate: i32,
    /// Шаблон команды для кастомного движка. {text} заменяется на текст.
    #[serde(default)]
    pub voice_custom_cmd: String,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            autostart: true,
            agent_name: "Вилли".to_string(),
            port: 8790,
            categories: default_categories(),
            ollama_enabled: false,
            ollama_url: default_ollama_url(),
            ollama_model: default_ollama_model(),
            ollama_system_prompt: String::new(),
            voice_feedback_enabled: false,
            voice_feedback_style: default_voice_style(),
            voice_engine: default_voice_engine(),
            piper_voice: default_piper_voice(),
            edge_tts_voice: default_edge_voice(),
            edge_tts_rate: 0,
            voice_custom_cmd: String::new(),
            commands: vec![
                VoiceCommand {
                    id: "1".to_string(),
                    trigger: "открой проводник".to_string(),
                    windows_cmd: "explorer.exe".to_string(),
                    linux_cmd: "xdg-open ~".to_string(),
                    description: "Файловый менеджер".to_string(),
                    category: "Приложения".to_string(),
                    close_trigger: "закрой проводник".to_string(),
                    windows_close_cmd: "taskkill /f /im explorer.exe".to_string(),
                    linux_close_cmd: "pkill nautilus || pkill thunar || pkill dolphin".to_string(),
                    ..VoiceCommand::default()
                },
                VoiceCommand {
                    id: "2".to_string(),
                    trigger: "открой браузер".to_string(),
                    windows_cmd: "start msedge".to_string(),
                    linux_cmd: "xdg-open https://google.com".to_string(),
                    description: "Браузер".to_string(),
                    category: "Приложения".to_string(),
                    ..VoiceCommand::default()
                },
                VoiceCommand {
                    id: "3".to_string(),
                    trigger: "открой калькулятор".to_string(),
                    windows_cmd: "calc.exe".to_string(),
                    linux_cmd: "gnome-calculator || kcalc || galculator".to_string(),
                    description: "Калькулятор".to_string(),
                    category: "Приложения".to_string(),
                    ..VoiceCommand::default()
                },
                VoiceCommand {
                    id: "4".to_string(),
                    trigger: "открой блокнот".to_string(),
                    windows_cmd: "notepad.exe".to_string(),
                    linux_cmd: "gedit || kate || nano".to_string(),
                    description: "Текстовый редактор".to_string(),
                    category: "Приложения".to_string(),
                    ..VoiceCommand::default()
                },
                VoiceCommand {
                    id: "5".to_string(),
                    trigger: "заблокируй компьютер".to_string(),
                    windows_cmd: "rundll32.exe user32.dll,LockWorkStation".to_string(),
                    linux_cmd: "loginctl lock-session".to_string(),
                    description: "Блокировка экрана".to_string(),
                    category: "Система".to_string(),
                    ..VoiceCommand::default()
                },
            ],
        }
    }
}
