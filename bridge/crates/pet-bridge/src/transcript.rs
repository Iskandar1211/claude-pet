use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ChatLine {
    pub role: String,
    pub preview: String,
}

pub fn parse_transcript_line(raw: &str) -> Option<ChatLine> {
    let row: TranscriptRow = serde_json::from_str(raw).ok()?;
    if row.is_meta == Some(true) || row.is_sidechain == Some(true) {
        return None;
    }
    match row.record_type.as_deref() {
        Some("user") => parse_user(&row.message),
        Some("assistant") => parse_assistant(&row.message),
        _ => None,
    }
}

fn parse_user(message: &Option<Value>) -> Option<ChatLine> {
    let content = message.as_ref()?.get("content")?;
    let preview = extract_text(content)?;
    if preview.contains("<channel source=\"pet\"") {
        return Some(ChatLine {
            role: "user".into(),
            preview: format!("[pet] {}", one_line(&preview, 120)),
        });
    }
    Some(ChatLine {
        role: "user".into(),
        preview: one_line(&preview, 200),
    })
}

fn parse_assistant(message: &Option<Value>) -> Option<ChatLine> {
    let content = message.as_ref()?.get("content")?;
    if let Some(arr) = content.as_array() {
        let mut parts: Vec<String> = Vec::new();
        for block in arr {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        parts.push(t.to_string());
                    }
                }
                Some("tool_use") => {
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool");
                    parts.push(format!("[tool:{name}]"));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            return None;
        }
        return Some(ChatLine {
            role: "assistant".into(),
            preview: one_line(&parts.join(" "), 200),
        });
    }
    extract_text(content).map(|preview| ChatLine {
        role: "assistant".into(),
        preview: one_line(&preview, 200),
    })
}

/// Предпросмотр в одну строку: переводы строк и повторные пробелы -> один пробел, не длиннее `max` символов.
pub fn one_line(text: &str, max: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

const INTERRUPT_SIGNAL: &str = "Request interrupted by user";

/// Структурированный сигнал прерывания (текст в content user/assistant), не подстрока по сырой JSONL.
pub fn line_is_interrupt_signal(raw: &str) -> bool {
    line_has_text(raw, INTERRUPT_SIGNAL)
}

/// Есть ли в user/assistant-строке транскрипта текст `needle` (подстрока).
pub fn line_has_text(raw: &str, needle: &str) -> bool {
    let Ok(row) = serde_json::from_str::<TranscriptRow>(raw) else {
        return false;
    };
    if !matches!(row.record_type.as_deref(), Some("user") | Some("assistant")) {
        return false;
    }
    row.message
        .as_ref()
        .and_then(|m| m.get("content"))
        .and_then(extract_text)
        .map_or(false, |t| t.contains(needle))
}

fn extract_text(content: &Value) -> Option<String> {
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct TranscriptRow {
    #[serde(rename = "type")]
    record_type: Option<String>,
    message: Option<Value>,
    #[serde(rename = "isMeta")]
    is_meta: Option<bool>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_single_line() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Готово.\n\n**Что удалено:**\n-  ui/"}]}}"#;
        let line = parse_transcript_line(raw).unwrap();
        assert_eq!(line.preview, "Готово. **Что удалено:** - ui/");
        assert_eq!(one_line("abcdef", 3), "abc");
    }

    #[test]
    fn skips_meta() {
        let raw = r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#;
        assert!(parse_transcript_line(raw).is_none());
    }

    #[test]
    fn parses_assistant_text() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#;
        let line = parse_transcript_line(raw).unwrap();
        assert_eq!(line.role, "assistant");
        assert_eq!(line.preview, "hi");
    }

    #[test]
    fn interrupt_signal_structured_only() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Request interrupted by user"}]}}"#;
        assert!(line_is_interrupt_signal(raw));
        let noise = r#"{"type":"queue-operation","payload":"Request interrupted by user"}"#;
        assert!(!line_is_interrupt_signal(noise));
    }
}
