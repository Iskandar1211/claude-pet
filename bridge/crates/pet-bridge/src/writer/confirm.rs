use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::transcript::line_has_text;

use super::DeliveryResult;

const CONFIRM_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const PREFIX_CHARS: usize = 40;

pub fn transcript_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Ждёт до 10 с, пока в транскрипте (после `start_len`) появится user/assistant-строка с префиксом `text`.
pub async fn wait_for_echo(path: &Path, start_len: u64, text: &str) -> DeliveryResult {
    let needle: String = text.trim().chars().take(PREFIX_CHARS).collect();
    if needle.is_empty() {
        return DeliveryResult::Unconfirmed;
    }
    let deadline = Instant::now() + CONFIRM_TIMEOUT;
    loop {
        if tail_has_text(path, start_len, &needle) {
            return DeliveryResult::Confirmed;
        }
        if Instant::now() >= deadline {
            return DeliveryResult::Unconfirmed;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn tail_has_text(path: &Path, start_len: u64, needle: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::Start(start_len)).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return false;
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .any(|line| line_has_text(line, needle))
}
