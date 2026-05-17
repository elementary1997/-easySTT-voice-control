use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, serde::Serialize)]
pub struct LogEntry {
    pub ts: String,
    pub level: String,
    pub msg: String,
}

pub type SharedLog = Arc<Mutex<VecDeque<LogEntry>>>;

pub fn new_log() -> SharedLog {
    Arc::new(Mutex::new(VecDeque::with_capacity(500)))
}

pub fn push(log: &SharedLog, level: &str, msg: impl Into<String>) -> LogEntry {
    let entry = LogEntry { ts: now_utc(), level: level.to_string(), msg: msg.into() };
    let mut buf = log.lock().unwrap();
    if buf.len() >= 500 { buf.pop_front(); }
    buf.push_back(entry.clone());
    entry
}

fn now_utc() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let s = d.as_secs();
    let ms = d.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", (s / 3600) % 24, (s / 60) % 60, s % 60, ms)
}
