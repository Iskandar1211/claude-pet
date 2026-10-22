//! Строки протокола `pet-bridge serve` (см. bridge/PROTOCOL.md). Только то, что безопасно отдать
//! приложению: токены, сокеты и пути к транскриптам сюда не попадают по построению DTO.

use serde::{Deserialize, Serialize};

use crate::registry::SessionRegistry;
use crate::state::{PendingPermission, SessionStatus};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionDto {
    pub session_id: String,
    pub short_id: String,
    pub entrypoint: Option<String>,
    pub cwd: Option<String>,
    pub status: &'static str,
    pub last_message: Option<String>,
    pub has_channel: bool,
    pub pending_permission: Option<PendingPermission>,
}

/// Запись чата в `history`, как в Claude Code: `kind` = `text` (реплика целиком), `tool` (вызов
/// `name(summary)`), `result` (первые строки результата + `more_lines`). Пустые поля не выводятся.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Верхняя граница `limit` в `history` — защита от запроса «всего файла».
pub const HISTORY_MAX_LIMIT: usize = 100;

/// Мост → приложение, одна строка JSON.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Hello {
        protocol: u32,
    },
    Sessions {
        pet_status: &'static str,
        sessions: Vec<SessionDto>,
    },
    /// Дополнение v1 (2026-10-22): ответ на `history`.
    History {
        id: String,
        session_id: String,
        messages: Vec<HistoryMessage>,
    },
    /// Дополнение v1 (2026-10-22): 5-часовой лимит подписки, при изменении `usage.json`.
    Usage {
        five_hour_pct: f64,
        resets_at: i64,
        updated_at: i64,
    },
    Result {
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        route: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        delivery: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Приложение → мост: `{"id":"7","cmd":"send",...}`.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Send {
        session_id: String,
        text: String,
    },
    Permission {
        session_id: String,
        request_id: String,
        allow: bool,
    },
    /// Дополнение v1 (2026-10-22): последние `limit` реплик транскрипта.
    History {
        session_id: String,
        limit: usize,
    },
}

pub fn status_name(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Idle => "idle",
        SessionStatus::Working => "working",
        SessionStatus::WaitingPermission => "waiting",
        SessionStatus::Done => "done",
    }
}

/// Сессии по id — порядок стабилен, иначе снимок «менялся» бы на каждом опросе.
pub fn sessions_snapshot(registry: &SessionRegistry) -> Vec<SessionDto> {
    let mut sessions: Vec<SessionDto> = registry
        .sessions()
        .map(|s| SessionDto {
            session_id: s.session_id.clone(),
            short_id: s.session_id.chars().take(8).collect(),
            entrypoint: s.entrypoint.clone(),
            cwd: s.cwd.clone(),
            status: status_name(s.status),
            last_message: s.last_message.clone(),
            has_channel: s.channel_port.is_some(),
            pending_permission: s.pending_permission.clone(),
        })
        .collect();
    sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    sessions
}

/// Статус питомца по всем сессиям, приоритет как у ChatGPT Pets:
/// Needs input → Blocked → Ready → Running, иначе idle. `error` мост пока не выставляет.
pub fn pet_status(sessions: &[SessionDto]) -> &'static str {
    const PRIORITY: [&str; 4] = ["waiting", "error", "done", "working"];
    PRIORITY
        .into_iter()
        .find(|p| sessions.iter().any(|s| s.status == *p))
        .unwrap_or("idle")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, status: &'static str) -> SessionDto {
        SessionDto {
            session_id: id.into(),
            short_id: id.chars().take(8).collect(),
            entrypoint: Some("cli".into()),
            cwd: None,
            status,
            last_message: None,
            has_channel: false,
            pending_permission: None,
        }
    }

    #[test]
    fn pet_status_priority() {
        assert_eq!(pet_status(&[]), "idle");
        assert_eq!(pet_status(&[session("a", "idle")]), "idle");
        assert_eq!(pet_status(&[session("a", "working"), session("b", "done")]), "done");
        assert_eq!(
            pet_status(&[session("a", "done"), session("b", "waiting"), session("c", "working")]),
            "waiting"
        );
    }

    #[test]
    fn sessions_event_shape() {
        let line = serde_json::to_string(&Event::Sessions {
            pet_status: "working",
            sessions: vec![session("aaaaaaaa-1111", "working")],
        })
        .unwrap();
        assert!(line.starts_with(r#"{"type":"sessions","pet_status":"working","sessions":[{"session_id":"aaaaaaaa-1111","short_id":"aaaaaaaa""#));
        assert!(!line.contains('\n'));
        assert!(!line.contains("token") && !line.contains("socket") && !line.contains("transcript"));
    }

    #[test]
    fn result_omits_empty_fields() {
        let line = serde_json::to_string(&Event::Result {
            id: "8".into(),
            ok: false,
            route: None,
            delivery: None,
            error: Some("нет Channel".into()),
        })
        .unwrap();
        assert_eq!(line, r#"{"type":"result","id":"8","ok":false,"error":"нет Channel"}"#);
    }

    #[test]
    fn history_event_shape() {
        let line = serde_json::to_string(&Event::History {
            id: "9".into(),
            session_id: "aaaaaaaa-1111".into(),
            messages: vec![
                HistoryMessage { role: "user".into(), kind: "text", text: Some("привет".into()), ..Default::default() },
                HistoryMessage {
                    role: "assistant".into(),
                    kind: "tool",
                    name: Some("Bash".into()),
                    summary: Some("git status".into()),
                    ..Default::default()
                },
            ],
        })
        .unwrap();
        assert_eq!(
            line,
            r#"{"type":"history","id":"9","session_id":"aaaaaaaa-1111","messages":[{"role":"user","kind":"text","text":"привет"},{"role":"assistant","kind":"tool","name":"Bash","summary":"git status"}]}"#
        );
    }

    #[test]
    fn usage_event_shape() {
        let line = serde_json::to_string(&Event::Usage { five_hour_pct: 93.0, resets_at: 1792621200, updated_at: 1792613444 })
            .unwrap();
        assert_eq!(line, r#"{"type":"usage","five_hour_pct":93.0,"resets_at":1792621200,"updated_at":1792613444}"#);
    }

    #[test]
    fn parses_history_command() {
        let r: Request =
            serde_json::from_str(r#"{"id":"9","cmd":"history","session_id":"s","limit":5}"#).unwrap();
        assert!(matches!(r.command, Command::History { limit: 5, .. }));
        // Отрицательный limit — ошибка разбора, мост ответит result ok:false.
        assert!(serde_json::from_str::<Request>(r#"{"id":"9","cmd":"history","session_id":"s","limit":-1}"#).is_err());
    }

    #[test]
    fn parses_commands() {
        let r: Request =
            serde_json::from_str(r#"{"id":"7","cmd":"send","session_id":"s","text":"hi"}"#).unwrap();
        assert_eq!(r.id, "7");
        assert!(matches!(r.command, Command::Send { ref text, .. } if text == "hi"));

        let r: Request = serde_json::from_str(
            r#"{"id":"8","cmd":"permission","session_id":"s","request_id":"r-42","allow":true}"#,
        )
        .unwrap();
        assert!(matches!(r.command, Command::Permission { allow: true, .. }));
    }
}
