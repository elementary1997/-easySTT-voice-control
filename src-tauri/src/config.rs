use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    /// Включён ли плагин
    pub enabled: bool,
    /// Регистрировать в автозапуске ОС (Windows/Linux)
    #[serde(default = "default_true")]
    pub autostart: bool,
    /// Имя агента (произносится перед командой), например «Вилли»
    pub agent_name: String,
    /// Порт HTTP-сервера (смена вступает в силу после перезапуска)
    pub port: u16,
    /// Список голосовых команд
    pub commands: Vec<VoiceCommand>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            autostart: true,
            agent_name: "Вилли".to_string(),
            port: 8790,
            commands: vec![
                VoiceCommand {
                    id: "1".to_string(),
                    trigger: "открой проводник".to_string(),
                    aliases: vec![],
                    windows_cmd: "explorer.exe".to_string(),
                    linux_cmd: "xdg-open ~".to_string(),
                    description: "Файловый менеджер".to_string(),
                    category: "Приложения".to_string(),
                },
                VoiceCommand {
                    id: "2".to_string(),
                    trigger: "открой браузер".to_string(),
                    aliases: vec![],
                    windows_cmd: "start msedge".to_string(),
                    linux_cmd: "xdg-open https://google.com".to_string(),
                    description: "Браузер".to_string(),
                    category: "Приложения".to_string(),
                },
                VoiceCommand {
                    id: "3".to_string(),
                    trigger: "открой калькулятор".to_string(),
                    aliases: vec![],
                    windows_cmd: "calc.exe".to_string(),
                    linux_cmd: "gnome-calculator || kcalc || galculator".to_string(),
                    description: "Калькулятор".to_string(),
                    category: "Приложения".to_string(),
                },
                VoiceCommand {
                    id: "4".to_string(),
                    trigger: "открой блокнот".to_string(),
                    aliases: vec![],
                    windows_cmd: "notepad.exe".to_string(),
                    linux_cmd: "gedit || kate || nano".to_string(),
                    description: "Текстовый редактор".to_string(),
                    category: "Приложения".to_string(),
                },
                VoiceCommand {
                    id: "5".to_string(),
                    trigger: "заблокируй компьютер".to_string(),
                    aliases: vec![],
                    windows_cmd: "rundll32.exe user32.dll,LockWorkStation".to_string(),
                    linux_cmd: "loginctl lock-session".to_string(),
                    description: "Блокировка экрана".to_string(),
                    category: "Система".to_string(),
                },
            ],
        }
    }
}
