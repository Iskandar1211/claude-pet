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
    if session_id.is_empty() || session_id.contains('/') || session_id.contains("..") {
        return None;
    }
    Some(pending_dir().join(format!("{session_id}.json")))
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
