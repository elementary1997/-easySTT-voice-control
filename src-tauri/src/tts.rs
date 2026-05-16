/// Произносит текст через системный TTS (неблокирующий — запускает поток).
pub fn speak(text: &str) {
    speak_with_engine(text, "system", "");
}

/// Произносит текст с учётом выбранного движка.
pub fn speak_with_engine(text: &str, engine: &str, piper_voice: &str, custom_cmd: &str) {
    match engine {
        "piper" => crate::piper::speak(text, piper_voice),
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
