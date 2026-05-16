use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

const PIPER_VERSION: &str = "2023.11.14-2";
const VOICES_BASE: &str =
    "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";

// ─── Paths ────────────────────────────────────────────────────────────────────

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("easystt-voice-control")
        .join("piper")
}

pub fn piper_exe() -> PathBuf {
    #[cfg(windows)]
    return data_dir().join("piper").join("piper.exe");
    #[cfg(not(windows))]
    return data_dir().join("piper").join("piper");
}

fn voices_dir() -> PathBuf {
    data_dir().join("voices")
}

// ─── Status ───────────────────────────────────────────────────────────────────

pub fn is_binary_installed() -> bool {
    piper_exe().exists()
}

pub fn is_voice_installed(id: &str) -> bool {
    voices_dir().join(format!("{id}.onnx")).exists()
        && voices_dir().join(format!("{id}.onnx.json")).exists()
}

// ─── Voice catalog ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiperVoice {
    pub id: String,
    pub display_name: String,
    pub gender: String,
    pub size_mb: u32,
    pub installed: bool,
    /// Путь внутри репозитория HuggingFace (без .onnx/.json)
    pub hf_path: String,
}

pub fn voice_catalog() -> Vec<PiperVoice> {
    let voices: &[(&str, &str, &str, u32, &str)] = &[
        (
            "ru_RU-irina-medium",
            "Ирина ♀",
            "female",
            65,
            "ru/ru_RU/irina/medium/ru_RU-irina-medium",
        ),
        (
            "ru_RU-irina-low",
            "Ирина ♀ (low)",
            "female",
            18,
            "ru/ru_RU/irina/low/ru_RU-irina-low",
        ),
        (
            "ru_RU-denis-medium",
            "Денис ♂",
            "male",
            65,
            "ru/ru_RU/denis/medium/ru_RU-denis-medium",
        ),
        (
            "ru_RU-ruslan-medium",
            "Руслан ♂",
            "male",
            65,
            "ru/ru_RU/ruslan/medium/ru_RU-ruslan-medium",
        ),
    ];
    voices
        .iter()
        .map(|(id, name, gender, size_mb, hf_path)| PiperVoice {
            id: id.to_string(),
            display_name: name.to_string(),
            gender: gender.to_string(),
            size_mb: *size_mb,
            installed: is_voice_installed(id),
            hf_path: hf_path.to_string(),
        })
        .collect()
}

// ─── Download helpers ─────────────────────────────────────────────────────────

async fn download_file(
    url: &str,
    dest: &std::path::Path,
    on_progress: &(impl Fn(u64, u64) + Sync),
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(7200))
        .build()?;
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!("Ошибка загрузки: HTTP {status} для {url}"));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut file = std::fs::File::create(dest)?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(anyhow::anyhow!("Отменено"));
        }
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    Ok(())
}

// ─── Download binary ──────────────────────────────────────────────────────────

pub async fn download_binary(
    on_progress: impl Fn(u64, u64) + Send + Sync + 'static,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let base = data_dir();
    std::fs::create_dir_all(&base)?;

    #[cfg(windows)]
    let archive_name = "piper_win.zip";
    #[cfg(all(not(windows), target_arch = "aarch64"))]
    let archive_name = "piper_linux_arm64.tar.gz";
    #[cfg(all(not(windows), not(target_arch = "aarch64")))]
    let archive_name = "piper_linux_x86.tar.gz";

    #[cfg(windows)]
    let url_suffix = "piper_windows_amd64.zip";
    #[cfg(all(not(windows), target_arch = "aarch64"))]
    let url_suffix = "piper_linux_aarch64.tar.gz";
    #[cfg(all(not(windows), not(target_arch = "aarch64")))]
    let url_suffix = "piper_linux_x86_64.tar.gz";

    let url = format!(
        "https://github.com/rhasspy/piper/releases/download/{PIPER_VERSION}/{url_suffix}"
    );
    let archive_path = base.join(archive_name);

    download_file(&url, &archive_path, &on_progress, &cancel).await?;

    let piper_dir = base.join("piper");
    std::fs::create_dir_all(&piper_dir)?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Распаковываем в уникальный temp-каталог, ищем piper.exe где бы он ни оказался,
        // копируем всё содержимое его директории в piper_dir.
        // Это работает независимо от структуры zip (с вложенной папкой или без).
        let archive_str = archive_path.to_string_lossy().replace('\'', "''");
        let dest_str = piper_dir.to_string_lossy().replace('\'', "''");
        let ps_script = format!(
            r#"try {{
    $tmp = Join-Path $env:TEMP ('piper_' + [guid]::NewGuid().ToString('N'));
    Add-Type -Assembly System.IO.Compression.FileSystem;
    [IO.Compression.ZipFile]::ExtractToDirectory('{archive}', $tmp);
    $exe = Get-ChildItem -Path $tmp -Filter 'piper.exe' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1;
    if ($exe -eq $null) {{ Write-Error 'piper.exe not found in archive'; exit 2 }};
    New-Item -ItemType Directory -Force -Path '{dest}' | Out-Null;
    Get-ChildItem -Path $exe.DirectoryName | ForEach-Object {{ Copy-Item -Path $_.FullName -Destination '{dest}' -Force -Recurse }};
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue;
    exit 0
}} catch {{ Write-Error $_; exit 1 }}"#,
            archive = archive_str,
            dest = dest_str
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .creation_flags(0x08000000)
            .output();
        match output {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(anyhow::anyhow!(
                    "Ошибка распаковки (код {}): {}",
                    out.status.code().unwrap_or(-1),
                    stderr.trim()
                ));
            }
            Err(e) => return Err(anyhow::anyhow!("Не удалось запустить PowerShell: {e}")),
        }
    }

    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("tar")
            .args([
                "-xzf",
                &archive_path.to_string_lossy(),
                "--strip-components=1",
                "-C",
                &piper_dir.to_string_lossy(),
            ])
            .status();
        if let Err(e) = status {
            return Err(anyhow::anyhow!("Ошибка распаковки: {e}"));
        }
        let _ = std::process::Command::new("chmod")
            .args(["+x", &piper_exe().to_string_lossy()])
            .status();
    }

    let _ = std::fs::remove_file(&archive_path);

    if !piper_exe().exists() {
        return Err(anyhow::anyhow!(
            "piper.exe не найден после распаковки (ожидался по пути: {}). Попробуйте скачать снова.",
            piper_exe().display()
        ));
    }

    Ok(())
}

// ─── Download voice ───────────────────────────────────────────────────────────

pub async fn download_voice(
    voice_id: &str,
    hf_path: &str,
    on_progress: impl Fn(u64, u64) + Send + Sync + 'static,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let dir = voices_dir();
    std::fs::create_dir_all(&dir)?;

    let onnx_url = format!("{VOICES_BASE}/{hf_path}.onnx");
    let json_url = format!("{VOICES_BASE}/{hf_path}.onnx.json");

    let onnx_path = dir.join(format!("{voice_id}.onnx"));
    let json_path = dir.join(format!("{voice_id}.onnx.json"));

    // ONNX — крупный файл, показываем прогресс
    download_file(&onnx_url, &onnx_path, &on_progress, &cancel).await?;
    // JSON — маленький конфиг, прогресс не нужен
    download_file(&json_url, &json_path, &|_, _| {}, &cancel).await?;

    Ok(())
}

// ─── Speak ────────────────────────────────────────────────────────────────────

pub fn speak(text: &str, voice_id: &str) {
    let text = text.to_string();
    let voice_id = voice_id.to_string();

    std::thread::spawn(move || {
        let exe = piper_exe();
        let model = voices_dir().join(format!("{voice_id}.onnx"));

        if !exe.exists() || !model.exists() {
            eprintln!("[piper] Binary or model not found: {}", exe.display());
            return;
        }

        use std::process::{Command, Stdio};

        #[cfg(not(windows))]
        {
            let exe_dir = exe.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();

            let mut piper = match Command::new(&exe)
                .args(["--model", &model.to_string_lossy(), "--output_raw"])
                .env("LD_LIBRARY_PATH", &exe_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[piper] Failed to spawn piper: {e}");
                    return;
                }
            };

            if let Some(mut stdin) = piper.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
                let _ = stdin.write_all(b"\n");
            }

            if let Some(stdout) = piper.stdout.take() {
                let _ = Command::new("aplay")
                    .args(["-r", "22050", "-f", "S16_LE", "-c", "1", "-q"])
                    .stdin(stdout)
                    .stderr(Stdio::null())
                    .spawn()
                    .and_then(|mut c| c.wait());
            }

            let _ = piper.wait();
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            let temp_wav = std::env::temp_dir().join("easystt_piper.wav");

            let mut piper = match Command::new(&exe)
                .args([
                    "--model",
                    &model.to_string_lossy(),
                    "--output_file",
                    &temp_wav.to_string_lossy(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
            {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[piper] Failed to spawn piper: {e}");
                    return;
                }
            };

            if let Some(mut stdin) = piper.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            let _ = piper.wait();

            if temp_wav.exists() {
                let script = format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    temp_wav.display()
                );
                let _ = Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-WindowStyle",
                        "Hidden",
                        "-Command",
                        &script,
                    ])
                    .creation_flags(0x08000000)
                    .spawn();
            }
        }
    });
}
