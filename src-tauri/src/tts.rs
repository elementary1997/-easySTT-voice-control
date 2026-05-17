// ─── Edge TTS management ──────────────────────────────────────────────────────

pub fn edge_tts_installed() -> bool {
    // Используем where/which — мгновенно, не запускает Python
    #[cfg(windows)]
    let result = std::process::Command::new("where")
        .arg("edge-tts")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    #[cfg(not(windows))]
    let result = std::process::Command::new("which")
        .arg("edge-tts")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    result.map(|s| s.success()).unwrap_or(false)
}

pub fn install_edge_tts_sync() -> Result<(), String> {
    // Пробуем разные варианты pip в порядке приоритета
    let mut candidates: Vec<(&str, Vec<&str>)> = vec![
        ("pip3",    vec!["install", "edge-tts"]),
        ("pip",     vec!["install", "edge-tts"]),
        ("python3", vec!["-m", "pip", "install", "edge-tts"]),
        ("python",  vec!["-m", "pip", "install", "edge-tts"]),
    ];
    #[cfg(windows)]
    candidates.push(("py", vec!["-m", "pip", "install", "edge-tts"]));

    for (cmd, args) in &candidates {
        #[cfg(windows)]
        let result = {
            use std::os::windows::process::CommandExt;
            std::process::Command::new(cmd)
                .args(args)
                .creation_flags(0x08000000)
                .status()
        };
        #[cfg(not(windows))]
        let result = std::process::Command::new(cmd).args(args).status();

        if let Ok(status) = result {
            if status.success() {
                return Ok(());
            }
        }
    }
    Err("Не удалось установить edge-tts. Убедитесь что Python и pip установлены.".to_string())
}

// ─── TTS speak ────────────────────────────────────────────────────────────────

/// Произносит текст через системный TTS (неблокирующий — запускает поток).
pub fn speak(text: &str) {
    speak_with_engine(text, "system", "", "", 0, "");
}

/// Произносит текст с учётом выбранного движка (полная сигнатура).
pub fn speak_with_engine(text: &str, engine: &str, piper_voice: &str, edge_voice: &str, edge_rate: i32, custom_cmd: &str) {
    match engine {
        "piper" => {
            if crate::piper::is_binary_installed() && crate::piper::is_voice_installed(piper_voice) {
                crate::piper::speak(text, piper_voice);
            } else {
                let text = text.to_string();
                std::thread::spawn(move || {
                    #[cfg(windows)] speak_windows(&text);
                    #[cfg(not(windows))] speak_linux(&text);
                });
            }
        }
        "edge" => speak_edge(text, edge_voice, edge_rate),
        "custom" if !custom_cmd.is_empty() => {
            let text = text.to_string();
            let custom_cmd = custom_cmd.to_string();
            std::thread::spawn(move || speak_custom(&text, &custom_cmd));
        }
        _ => {
            let text = text.to_string();
            std::thread::spawn(move || {
                #[cfg(windows)]
                speak_windows(&text);
                #[cfg(not(windows))]
                speak_linux(&text);
            });
        }
    }
}

fn speak_custom(text: &str, cmd_template: &str) {
    let safe_text = text.replace('"', "'");
    let cmd = cmd_template.replace("{text}", &safe_text);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", &cmd])
            .creation_flags(0x08000000)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .spawn();
    }
}

#[cfg(windows)]
fn speak_windows(text: &str) {
    use std::os::windows::process::CommandExt;
    let safe = text.replace('"', "'");
    let script = format!(
        r#"Add-Type -AssemblyName System.Speech; \
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
$ru = $s.GetInstalledVoices() | Where-Object {{$_.VoiceInfo.Culture -like 'ru*'}}; \
if ($ru) {{ $s.SelectVoice($ru[0].VoiceInfo.Name) }}; \
$s.Speak("{safe}")"#
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(0x08000000)
        .spawn();
}

#[cfg(not(windows))]
fn speak_linux(text: &str) {
    let ok = std::process::Command::new("espeak-ng")
        .args(["-v", "ru", text])
        .spawn()
        .is_ok();
    if !ok {
        let ok2 = std::process::Command::new("espeak")
            .args(["-v", "ru", text])
            .spawn()
            .is_ok();
        if !ok2 {
            let _ = std::process::Command::new("festival")
                .arg("--tts")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .map(|mut c| {
                    use std::io::Write;
                    if let Some(stdin) = c.stdin.as_mut() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                });
        }
    }
}

fn speak_edge(text: &str, voice: &str, rate: i32) {
    let text = text.to_string();
    let voice = if voice.is_empty() { "ru-RU-SvetlanaNeural".to_string() } else { voice.to_string() };
    let rate_str = if rate >= 0 { format!("+{}%", rate) } else { format!("{}%", rate) };
    std::thread::spawn(move || {
        let temp = std::env::temp_dir().join("easystt_edge_tts.mp3");
        let temp_str = temp.to_string_lossy().to_string();
        let args = ["--voice", voice.as_str(), "--text", text.as_str(),
                    "--rate", rate_str.as_str(), "--write-media", temp_str.as_str()];
        #[cfg(windows)]
        let ok = {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("edge-tts")
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(0x08000000)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        #[cfg(not(windows))]
        let ok = std::process::Command::new("edge-tts")
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && temp.exists() { play_mp3(&temp); }
    });
}

fn play_mp3(path: &std::path::Path) {
    let path_str = path.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // MCI (winmm.dll) — есть в любой Windows, поддерживает MP3, не открывает окно
        let safe_path = path_str.replace('"', "'");
        let script = format!(
            r#"Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text;
public class WinMM {{
    [DllImport("winmm.dll", CharSet=CharSet.Auto)]
    public static extern int mciSendString(string cmd, StringBuilder ret, int retLen, IntPtr hwnd);
}}
'@
[WinMM]::mciSendString('open "{p}" type mpegvideo alias m', $null, 0, [IntPtr]::Zero) | Out-Null
[WinMM]::mciSendString('play m wait', $null, 0, [IntPtr]::Zero) | Out-Null
[WinMM]::mciSendString('close m', $null, 0, [IntPtr]::Zero) | Out-Null"#,
            p = safe_path
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(0x08000000)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let ok = std::process::Command::new("mpg123").args(["-q", &path_str]).spawn().is_ok();
        if !ok {
            let ok2 = std::process::Command::new("mpv").args(["--no-video", &path_str]).spawn().is_ok();
            if !ok2 {
                let _ = std::process::Command::new("ffplay")
                    .args(["-nodisp", "-autoexit", &path_str])
                    .spawn();
            }
        }
    }
}

/// Случайная фраза из пула (без зависимостей от rand).
pub fn random_response(style: &str) -> String {
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;

    let neutral = ["Выполняю", "Готово", "Принято", "Сделано", "Понял"];
    let fun = [
        "Слушаюсь, шеф!",
        "Уже бегу!",
        "Как прикажете!",
        "Сделано, капитан!",
        "Один момент, сэр!",
        "Выполнено!",
    ];

    let pool: &[&str] = if style == "fun" { &fun } else { &neutral };
    pool[idx % pool.len()].to_string()
}
