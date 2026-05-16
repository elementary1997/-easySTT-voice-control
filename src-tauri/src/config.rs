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
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            autostart: true,
            agent_name: "Вилли".to_string(),
            port: 8790,
            categories: default_categories(),
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
