/// Произносит текст через системный TTS (неблокирующий — запускает поток).
pub fn speak(text: &str) {
    let text = text.to_string();
    std::thread::spawn(move || {
        #[cfg(windows)]
        speak_windows(&text);
        #[cfg(not(windows))]
        speak_linux(&text);
    });
}

#[cfg(windows)]
fn speak_windows(text: &str) {
    use std::os::windows::process::CommandExt;
    // Экранируем кавычки чтобы не сломать PowerShell-строку
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
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn();
}

#[cfg(not(windows))]
fn speak_linux(text: &str) {
    // Пробуем espeak-ng → espeak → festival
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
            // festival принимает stdin
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

    let neutral = [
        "Выполняю", "Готово", "Принято", "Сделано", "Понял",
    ];
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
