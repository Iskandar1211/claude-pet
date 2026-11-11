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

/// Что транскрипт говорит о самой сессии: модель, effort, режим разрешений. Поля, которых в строке нет, — `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionMeta {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission_mode: Option<String>,
}

/// Модель и effort — из `assistant`, режим — из `user`; только основная сессия (не `isSidechain`).
/// Разбор структурный: «model» во входе инструмента (`Agent`) сюда не попадает.
pub fn parse_session_meta(raw: &str) -> SessionMeta {
    let mut meta = SessionMeta::default();
    let Ok(row) = serde_json::from_str::<TranscriptRow>(raw) else { return meta };
    if row.is_sidechain == Some(true) {
        return meta;
    }
    let text = |v: &Option<Value>| v.as_ref().and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    match row.record_type.as_deref() {
        Some("assistant") => {
            // Служебные записи несут псевдомодель вроде "<synthetic>" — настоящие id начинаются с "claude".
            meta.model = row
                .message
                .as_ref()
                .and_then(|m| m.get("model"))
                .and_then(Value::as_str)
                .filter(|m| m.starts_with("claude"))
                .map(str::to_string);
            meta.effort = text(&row.effort);
        }
        Some("user") => meta.permission_mode = text(&row.permission_mode),
        _ => {}
    }
    meta
}

/// Человекочитаемое имя модели из сырого id без таблицы моделей: `claude-fable-5-2` → «Fable 5.2»,
/// `claude-3-5-sonnet-20241022` → «Sonnet 3.5». Не разобрали — возвращаем id как есть.
pub fn model_display_name(id: &str) -> String {
    parse_model_name(id).unwrap_or_else(|| id.to_string())
}

fn parse_model_name(id: &str) -> Option<String> {
    // Суффикс контекста вроде «[1m]» — не часть имени.
    let base = id.split('[').next()?;
    let mut parts: Vec<&str> = base.strip_prefix("claude-")?.split('-').collect();
    // Хвост из 8 цифр — дата выпуска.
    if parts.last().is_some_and(|p| p.len() == 8 && p.bytes().all(|b| b.is_ascii_digit())) {
        parts.pop();
    }
    let is_num = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let name: Vec<&str> = parts.iter().copied().filter(|p| !is_num(p)).collect();
    let nums: Vec<&str> = parts.iter().copied().filter(|p| is_num(p)).collect();
    // Одно слово-имя и не больше двух чисел (мажор, минор); иначе формат незнаком.
    if name.len() != 1 || nums.is_empty() || nums.len() > 2 || !name[0].bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut chars = name[0].chars();
    let first = chars.next()?.to_ascii_uppercase();
    Some(format!("{first}{} {}", chars.as_str(), nums.join(".")))
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
    /// Только у `assistant`; `Value`, чтобы нестроковое значение не ломало разбор всей строки.
    effort: Option<Value>,
    /// Только у `user`.
    #[serde(rename = "permissionMode")]
    permission_mode: Option<Value>,
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

    fn meta(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
        let m = parse_session_meta(raw);
        (m.model, m.effort, m.permission_mode)
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn meta_from_assistant_and_user_rows() {
        let a = r#"{"type":"assistant","effort":"medium","message":{"model":"claude-fable-5-2","content":[]}}"#;
        assert_eq!(meta(a), (s("claude-fable-5-2"), s("medium"), None));
        let a = r#"{"type":"assistant","message":{"model":"claude-opus-5-5","content":[]}}"#;
        assert_eq!(meta(a), (s("claude-opus-5-5"), None, None), "effort может отсутствовать");
        let u = r#"{"type":"user","permissionMode":"auto","message":{"content":"hi"}}"#;
        assert_eq!(meta(u), (None, None, s("auto")));
    }

    #[test]
    fn meta_ignores_sidechain_synthetic_and_tool_input_model() {
        let side = r#"{"type":"assistant","isSidechain":true,"effort":"low","message":{"model":"claude-haiku-5-5"}}"#;
        assert_eq!(meta(side), (None, None, None));
        let side_user = r#"{"type":"user","isSidechain":true,"permissionMode":"plan","message":{"content":"x"}}"#;
        assert_eq!(meta(side_user), (None, None, None));
        let synthetic = r#"{"type":"assistant","message":{"model":"<synthetic>","content":[]}}"#;
        assert_eq!(meta(synthetic), (None, None, None));
        let tool = r#"{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","name":"Agent","input":{"model":"sonnet"}}]}}"#;
        assert_eq!(meta(tool).0, s("claude-opus-5-5"));
        let tool_only = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","input":{"model":"sonnet"}}]}}"#;
        assert_eq!(meta(tool_only), (None, None, None));
        assert_eq!(meta("не json"), (None, None, None));
        // Нестроковый effort не ломает разбор.
        assert_eq!(meta(r#"{"type":"assistant","effort":5,"message":{"model":"claude-opus-5-5"}}"#), (s("claude-opus-5-5"), None, None));
    }

    #[test]
    fn model_names() {
        for (id, name) in [
            ("claude-fable-5-2", "Fable 5.2"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-4-5-20250929", "Sonnet 4.5"),
            ("claude-haiku-5-5", "Haiku 5.5"),
            ("claude-3-5-sonnet-20241022", "Sonnet 3.5"),
            ("claude-opus-5-5[1m]", "Opus 5.5"),
            ("claude-opus-4", "Opus 4"),
            // Не разобрали — сырой id.
            ("gpt-4o", "gpt-4o"),
            ("claude-", "claude-"),
            ("claude-sonnet", "claude-sonnet"),
            ("claude-opus-5-5-preview-2", "claude-opus-5-5-preview-2"),
            ("", ""),
        ] {
            assert_eq!(model_display_name(id), name, "{id}");
        }
    }
}
