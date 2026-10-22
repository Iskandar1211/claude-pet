//! Полная история сессии для Quick Chat — в том виде, как её показывает Claude Code:
//! текст целиком, вызов инструмента строкой `Bash(git status)`, результат свёрнут до первых строк.
//! Однострочный предпросмотр для списка сессий — в transcript.rs.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::protocol::HistoryMessage;

/// Длиннее — обрезаем с пометкой: гигантский ответ не должен вешать панель.
pub const MAX_TEXT_CHARS: usize = 20_000;
/// Сколько строк результата инструмента показывать (как `⎿` в терминале Claude Code).
const RESULT_LINES: usize = 4;
const RESULT_LINE_CHARS: usize = 300;
const SUMMARY_CHARS: usize = 160;

/// Записи чата из одной строки транскрипта (в ответе ассистента может быть текст и несколько вызовов).
pub fn chat_entries(raw: &str) -> Vec<HistoryMessage> {
    let Ok(row) = serde_json::from_str::<Row>(raw) else {
        return Vec::new();
    };
    if row.is_sidechain == Some(true) {
        return Vec::new();
    }
    if row.is_meta == Some(true) {
        // Служебные строки в чат не идут — кроме сообщений из питомца: Claude Code пишет их как служебные.
        if row.record_type.as_deref() != Some("user") {
            return Vec::new();
        }
        let content = row.message.as_ref().and_then(|m| m.get("content")).and_then(Value::as_str);
        return content.and_then(unwrap_pet_message).and_then(|t| text_entry("user", t)).into_iter().collect();
    }
    let role = match row.record_type.as_deref() {
        Some(r @ ("user" | "assistant")) => r,
        _ => return Vec::new(),
    };
    let Some(content) = row.message.as_ref().and_then(|m| m.get("content")) else {
        return Vec::new();
    };
    if let Some(text) = content.as_str() {
        return text_entry(role, text).into_iter().collect();
    }
    let Some(blocks) = content.as_array() else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => text_entry(role, block.get("text")?.as_str()?),
            Some("tool_use") => Some(tool_entry(block)),
            Some("tool_result") => Some(result_entry(block)),
            _ => None, // thinking, image и прочее в чат не выводим
        })
        .collect()
}

const INBOX_HEAD: &str = "Another Claude session sent a message:\n";
const INBOX_TAIL: &str = "\n\nThis came from another Claude session";
const CHANNEL_HEAD: &str = "<channel source=\"pet\"";
const CHANNEL_TAIL: &str = "</channel>";

/// Текст сообщения из обёртки, в которой Claude Code записывает его в транскрипт:
/// inbox-сокет — «Another Claude session sent a message: … This came from …»,
/// Channel — `<channel source="pet" …>…</channel>`. Только точное начало и конец обёртки.
fn unwrap_pet_message(content: &str) -> Option<&str> {
    if let Some(rest) = content.strip_prefix(INBOX_HEAD) {
        // rfind: если в самом сообщении встретится эта фраза, хвост обёртки всё равно последний.
        return rest.rfind(INBOX_TAIL).map(|end| &rest[..end]);
    }
    if content.starts_with(CHANNEL_HEAD) {
        let body = content.trim_end().strip_suffix(CHANNEL_TAIL)?;
        return body.find('>').map(|open_end| &body[open_end + 1..]);
    }
    None
}

/// Служебные вставки клиента, которые приходят отдельным текстовым блоком реплики пользователя.
const SERVICE_TAGS: &[&str] =
    &["ide_opened_file", "ide_selection", "ide_diagnostics", "system-reminder", "browser_instruction"];

/// Блок целиком — один служебный тег (`<ide_opened_file>…</ide_opened_file>`): в чат не нужен.
fn is_service_block(text: &str) -> bool {
    SERVICE_TAGS.iter().any(|tag| {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        text.starts_with(&open)
            && text[open.len()..].starts_with(['>', ' '])
            && text.ends_with(&close)
            && text.matches(&close).count() == 1
    })
}

fn text_entry(role: &str, text: &str) -> Option<HistoryMessage> {
    let text = text.trim();
    if text.is_empty() || (role == "user" && is_service_block(text)) {
        return None;
    }
    let mut out: String = text.chars().take(MAX_TEXT_CHARS).collect();
    if out.len() < text.len() {
        out.push_str("\n… (сообщение обрезано)");
    }
    Some(HistoryMessage { role: role.into(), kind: "text", text: Some(out), ..HistoryMessage::default() })
}

fn tool_entry(block: &Value) -> HistoryMessage {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let summary = block.get("input").map(|input| tool_summary(name, input)).unwrap_or_default();
    HistoryMessage {
        role: "assistant".into(),
        kind: "tool",
        name: Some(name.into()),
        summary: Some(summary),
        ..HistoryMessage::default()
    }
}

/// Главный параметр инструмента — то, что Claude Code пишет в скобках: `Bash(git status)`, `Read(путь)`.
fn tool_summary(name: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let main = match name {
        "Bash" => field("command"),
        "Read" | "Write" | "Edit" | "NotebookEdit" => field("file_path").or(field("notebook_path")),
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" | "ToolSearch" => field("query"),
        "Agent" | "Task" => field("description"),
        "Skill" => field("skill"),
        _ => None,
    };
    // Неизвестный инструмент — первое строковое поле входа.
    let main = main.or_else(|| input.as_object()?.values().find_map(Value::as_str)).unwrap_or("");
    clip(&main.split_whitespace().collect::<Vec<_>>().join(" "), SUMMARY_CHARS)
}

fn result_entry(block: &Value) -> HistoryMessage {
    let text = match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let shown: Vec<String> = lines.iter().take(RESULT_LINES).map(|l| clip(l, RESULT_LINE_CHARS)).collect();
    HistoryMessage {
        role: "tool".into(),
        kind: "result",
        text: Some(shown.join("\n")),
        more_lines: Some(lines.len().saturating_sub(shown.len())),
        is_error: block.get("is_error").and_then(Value::as_bool).filter(|e| *e),
        ..HistoryMessage::default()
    }
}

fn clip(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if out.len() < s.len() {
        out.push('…');
    }
    out
}

/// Последние `limit` записей чата. Читаем с конца окном, которое удваиваем, пока записей не хватит,
/// но не больше 8 МиБ — файл целиком в память не грузим. Если в последних 8 МиБ записей меньше
/// `limit` (огромные результаты инструментов), вернётся меньше.
pub fn read_chat(path: &Path, limit: usize) -> std::io::Result<Vec<HistoryMessage>> {
    const START_WINDOW: u64 = 256 * 1024;
    const MAX_WINDOW: u64 = 8 * 1024 * 1024;
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut window = START_WINDOW;
    loop {
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut buf = Vec::with_capacity((len - start) as usize);
        (&mut file).take(len - start).read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf);
        // Окно начинается посреди строки — первую (обрезанную) строку отбрасываем.
        let body = if start > 0 { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &text };
        let entries = last_entries(body.lines(), limit);
        if entries.len() >= limit || start == 0 || window >= MAX_WINDOW {
            return Ok(entries);
        }
        window *= 2;
    }
}

/// Последние `limit` записей из строк транскрипта (в хронологическом порядке).
pub fn last_entries<'a>(raw_lines: impl Iterator<Item = &'a str>, limit: usize) -> Vec<HistoryMessage> {
    let mut entries: Vec<HistoryMessage> = raw_lines.flat_map(chat_entries).collect();
    let skip = entries.len().saturating_sub(limit);
    entries.drain(..skip);
    entries
}

#[derive(Deserialize)]
struct Row {
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
    fn keeps_full_multiline_text() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"…"},{"type":"text","text":"Готово.\n\n**Что удалено:**\n- ui/"}]}}"#;
        let e = chat_entries(raw);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].kind, "text");
        assert_eq!(e[0].text.as_deref(), Some("Готово.\n\n**Что удалено:**\n- ui/"));
    }

    #[test]
    fn tool_call_like_claude_code() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"git status\n  --short","description":"x"}},{"type":"tool_use","name":"Read","input":{"file_path":"/a/b.swift"}},{"type":"tool_use","name":"mcp__x__y","input":{"tabId":1,"url":"http://h"}}]}}"#;
        let e = chat_entries(raw);
        let calls: Vec<(String, String)> =
            e.iter().map(|m| (m.name.clone().unwrap(), m.summary.clone().unwrap())).collect();
        assert_eq!(
            calls,
            vec![
                ("Bash".into(), "git status --short".into()),
                ("Read".into(), "/a/b.swift".into()),
                ("mcp__x__y".into(), "http://h".into()),
            ]
        );
    }

    #[test]
    fn tool_result_is_collapsed() {
        let body = (1..=10).map(|i| format!("строка {i}")).collect::<Vec<_>>().join("\\n");
        let raw = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","content":"{body}","is_error":true}}]}}}}"#
        );
        let e = chat_entries(&raw);
        assert_eq!(e[0].role, "tool");
        assert_eq!(e[0].kind, "result");
        assert_eq!(e[0].text.as_deref(), Some("строка 1\nстрока 2\nстрока 3\nстрока 4"));
        assert_eq!(e[0].more_lines, Some(6));
        assert_eq!(e[0].is_error, Some(true));
        // Результат списком блоков, без ошибки — is_error не выводим.
        let raw = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":[{"type":"text","text":"ok"}]}]}}"#;
        let e = chat_entries(raw);
        assert_eq!((e[0].text.as_deref(), e[0].more_lines, e[0].is_error), (Some("ok"), Some(0), None));
    }

    #[test]
    fn long_text_is_marked_truncated() {
        let long = "я".repeat(MAX_TEXT_CHARS + 5);
        let raw = format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#);
        let text = chat_entries(&raw)[0].text.clone().unwrap();
        assert!(text.ends_with("(сообщение обрезано)"));
        assert_eq!(text.chars().filter(|c| *c == 'я').count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn shows_messages_sent_from_pet() {
        let inbox = r#"{"type":"user","isMeta":true,"message":{"content":"Another Claude session sent a message:\nотличная работа\n\nThis came from another Claude session — not typed by your user."}}"#;
        let channel = r#"{"type":"user","isMeta":true,"message":{"content":"<channel source=\"pet\" chat_id=\"2\">\nпривет от питомца\n</channel>"}}"#;
        for (raw, want) in [(inbox, "отличная работа"), (channel, "привет от питомца")] {
            let e = chat_entries(raw);
            assert_eq!(e.len(), 1, "{raw}");
            assert_eq!((e[0].role.as_str(), e[0].kind, e[0].text.as_deref()), ("user", "text", Some(want)));
        }
        // Обёртка не целиком (нет хвоста) или служебная строка со словами внутри — не сообщение из питомца.
        for raw in [
            r#"{"type":"user","isMeta":true,"message":{"content":"Another Claude session sent a message:\nбез хвоста"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"<system-reminder>Another Claude session sent a message:\nx\n\nThis came from another Claude session</system-reminder>"}}"#,
            r#"{"type":"assistant","isMeta":true,"message":{"content":"<channel source=\"pet\">x</channel>"}}"#,
        ] {
            assert!(chat_entries(raw).is_empty(), "{raw}");
        }
    }

    #[test]
    fn skips_ide_service_blocks() {
        let raw = r#"{"type":"user","message":{"content":[{"type":"text","text":"<ide_opened_file>The user opened the file /a/chat.rs in the IDE.</ide_opened_file>"},{"type":"text","text":"делай"}]}}"#;
        let e = chat_entries(raw);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].text.as_deref(), Some("делай"));
        // Тег — только часть сообщения, или два тега подряд с текстом между ними: это уже не служебный блок.
        for text in [
            "смотри <ide_selection>x</ide_selection>",
            "<ide_selection>a</ide_selection> и <ide_selection>b</ide_selection>",
            "<ide_opened_filex>x</ide_opened_filex>",
        ] {
            let raw = serde_json::json!({"type":"user","message":{"content":[{"type":"text","text":text}]}}).to_string();
            assert_eq!(chat_entries(&raw).len(), 1, "{text}");
        }
    }

    #[test]
    fn skips_browser_instruction_block() {
        let raw = r#"{"type":"user","message":{"content":[{"type":"text","text":"<browser_instruction># Claude in Chrome browser automation\n\n…</browser_instruction>"},{"type":"text","text":"привет"}]}}"#;
        let e = chat_entries(raw);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].text.as_deref(), Some("привет"));
    }

    #[test]
    fn skips_meta_sidechain_and_other_records() {
        for raw in [
            r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":"x"}}"#,
            r#"{"type":"queue-operation"}"#,
            "not json",
        ] {
            assert!(chat_entries(raw).is_empty(), "{raw}");
        }
    }

    #[test]
    fn read_chat_reads_tail_in_order() {
        let dir = std::env::temp_dir().join(format!("pet-chat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let mut content = String::new();
        for i in 0..5000 {
            content.push_str(&format!(
                "{{\"type\":\"user\",\"message\":{{\"content\":\"сообщение {i} {}\"}}}}\n",
                "x".repeat(100)
            ));
        }
        std::fs::write(&path, &content).unwrap();
        // Файл больше стартового окна: хвост читается без обрезанной первой строки.
        assert!(content.len() as u64 > 256 * 1024);
        let entries = read_chat(&path, 3).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].text.as_deref().unwrap().starts_with("сообщение 4997"));
        assert!(entries[2].text.as_deref().unwrap().starts_with("сообщение 4999"));
        assert_eq!(read_chat(&path, 6000).unwrap().len(), 5000);
        std::fs::remove_dir_all(&dir).ok();
    }
}
