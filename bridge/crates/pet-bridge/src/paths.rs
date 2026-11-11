use std::path::{Path, PathBuf};

pub fn pet_dir() -> PathBuf {
    std::env::var("PET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_home().join(".claude-pet")
        })
}

pub fn events_log() -> PathBuf {
    pet_dir().join("events.jsonl")
}

pub fn tokens_dir() -> PathBuf {
    pet_dir().join("tokens")
}

pub fn channels_dir() -> PathBuf {
    pet_dir().join("channels")
}

pub fn config_path() -> PathBuf {
    pet_dir().join("config.json")
}

/// Лимиты подписки от обёртки statusLine (см. `usage.rs`).
pub fn usage_path() -> PathBuf {
    pet_dir().join("usage.json")
}

pub fn pending_dir() -> PathBuf {
    pet_dir().join("pending")
}

pub fn pending_path(session_id: &str) -> Option<PathBuf> {
    pending_path_in(&pending_dir(), session_id)
}

pub(crate) fn pending_path_in(dir: &Path, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.contains('/') || session_id.contains("..") {
        return None;
    }
    Some(dir.join(format!("{session_id}.json")))
}

/// Вердикты питомца для hook'а `PermissionRequest`: `verdicts/<request_id>.json`.
pub fn verdicts_dir() -> PathBuf {
    pet_dir().join("verdicts")
}

/// request_id становится именем файла — пускаем только `[A-Za-z0-9_-]`, без точек и слешей.
pub(crate) fn verdict_path_in(dir: &Path, request_id: &str) -> Option<PathBuf> {
    let safe = !request_id.is_empty()
        && request_id.len() <= 128
        && request_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| dir.join(format!("{request_id}.json")))
}

/// Pid работающего `pet-bridge serve` — признак, что питомец запущен.
pub fn serve_pid_path() -> PathBuf {
    pet_dir().join("serve.pid")
}

pub(crate) fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// `/tmp/cc-socks/12345.sock` → `12345`
pub fn pid_from_socket(socket: &str) -> Option<u32> {
    let path = Path::new(socket);
    let stem = path.file_stem()?.to_str()?;
    stem.parse().ok()
}
