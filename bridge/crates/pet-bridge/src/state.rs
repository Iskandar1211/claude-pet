use serde::{Deserialize, Serialize};

use crate::transcript::line_is_interrupt_signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Working,
    WaitingPermission,
    Done,
    /// Ход оборвался из-за ошибки API (`StopFailure`). Снимается следующим Working-событием или `SessionEnd`.
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool_name: String,
    pub input_preview: String,
    /// Что запомнит «Разрешать всегда» (только запросы hook'а): по элементу на правку прав Claude Code.
    /// Пусто — кнопки нет; в pending-файле и снимке поля тогда нет вовсе.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remember: Vec<RememberRule>,
}

/// Одно предложение Claude Code `addRules`/`allow` в виде для показа человеку.
/// Тексты недоверенные (берутся из предложения Claude Code).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RememberRule {
    /// Правила вида `Tool(ruleContent)` или `Tool`.
    pub rules: Vec<String>,
    /// `session` | `localSettings` | `projectSettings` | `userSettings` — как прислал Claude Code.
    pub destination: String,
}

pub fn apply_hook_event(
    status: SessionStatus,
    event: &str,
    notification_type: Option<&str>,
    notification: Option<&str>,
) -> SessionStatus {
    match event {
        "SessionEnd" => return SessionStatus::Idle,
        "UserPromptSubmit" => return SessionStatus::Working,
        "PreToolUse" | "PostToolUse" => {
            if status == SessionStatus::WaitingPermission {
                return SessionStatus::Working;
            }
            return SessionStatus::Working;
        }
        "Stop" => return SessionStatus::Done,
        "StopFailure" => return SessionStatus::Error,
        "Notification" => {
            if is_permission_notification(notification_type, notification) {
                return SessionStatus::WaitingPermission;
            }
            // Ошибку не затирает «Claude ждёт ввода» (idle_prompt приходит уже после сбоя).
            if matches!(status, SessionStatus::Working | SessionStatus::Error) {
                return status;
            }
            return SessionStatus::Done;
        }
        "SessionStart" => {
            if status == SessionStatus::Idle {
                return SessionStatus::Idle;
            }
            status
        }
        _ => status,
    }
}

/// Короткая понятная причина для `last_message` сессии со статусом «ошибка»; `kind` — поле `error` у `StopFailure`.
pub fn error_reason(kind: Option<&str>) -> String {
    match kind {
        Some("rate_limit") => "ошибка: лимит запросов".into(),
        Some("overloaded") => "ошибка: сервер перегружен".into(),
        Some("authentication_failed" | "oauth_org_not_allowed") => "ошибка: нужна авторизация".into(),
        Some("billing_error" | "account_on_hold") => "ошибка: проблема с оплатой".into(),
        Some(other) if !other.is_empty() => format!("ошибка API ({})", crate::transcript::one_line(other, 40)),
        _ => "ошибка API".into(),
    }
}

/// `line_end_offset` — байтовый offset в файле сразу после этой строки;
/// `file_len_at_tail` — длина файла на момент открытия хвоста (отсекает старые interrupt при полном дочитывании).
pub fn apply_transcript_signal(
    status: SessionStatus,
    line: &str,
    line_end_offset: u64,
    file_len_at_tail: u64,
) -> SessionStatus {
    if !line_is_interrupt_signal(line) {
        return status;
    }
    const TAIL_SLACK: u64 = 4096;
    if line_end_offset + TAIL_SLACK < file_len_at_tail {
        return status;
    }
    SessionStatus::Done
}

fn is_permission_notification(notification_type: Option<&str>, notification: Option<&str>) -> bool {
    if let Some(t) = notification_type {
        let t = t.to_ascii_lowercase();
        if t.contains("permission") {
            return true;
        }
    }
    if let Some(msg) = notification {
        let m = msg.to_ascii_lowercase();
        if m.contains("permission") || m.contains("allow") && m.contains("tool") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_on_prompt() {
        assert_eq!(
            apply_hook_event(SessionStatus::Idle, "UserPromptSubmit", None, None),
            SessionStatus::Working
        );
    }

    #[test]
    fn done_on_stop() {
        assert_eq!(
            apply_hook_event(SessionStatus::Working, "Stop", None, None),
            SessionStatus::Done
        );
    }

    #[test]
    fn stop_failure_sets_error_and_next_work_event_clears_it() {
        let err = apply_hook_event(SessionStatus::Working, "StopFailure", None, None);
        assert_eq!(err, SessionStatus::Error);
        for ev in ["UserPromptSubmit", "PreToolUse", "PostToolUse"] {
            assert_eq!(apply_hook_event(err, ev, None, None), SessionStatus::Working, "{ev}");
        }
        assert_eq!(apply_hook_event(err, "SessionEnd", None, None), SessionStatus::Idle);
    }

    #[test]
    fn error_survives_idle_notification_and_session_start() {
        let err = SessionStatus::Error;
        assert_eq!(apply_hook_event(err, "Notification", Some("idle_prompt"), Some("Claude is waiting")), err);
        assert_eq!(apply_hook_event(err, "SessionStart", None, None), err);
        // Новый запрос разрешения — это уже другое состояние.
        assert_eq!(
            apply_hook_event(err, "Notification", Some("permission_prompt"), None),
            SessionStatus::WaitingPermission
        );
        assert_eq!(apply_hook_event(err, "Stop", None, None), SessionStatus::Done);
    }

    #[test]
    fn error_reason_known_and_fallback() {
        assert_eq!(error_reason(Some("rate_limit")), "ошибка: лимит запросов");
        assert_eq!(error_reason(Some("overloaded")), "ошибка: сервер перегружен");
        assert_eq!(error_reason(Some("oauth_org_not_allowed")), "ошибка: нужна авторизация");
        assert_eq!(error_reason(Some("billing_error")), "ошибка: проблема с оплатой");
        assert_eq!(error_reason(Some("server_error")), "ошибка API (server_error)");
        assert_eq!(error_reason(Some("")), "ошибка API");
        assert_eq!(error_reason(None), "ошибка API");
    }

    #[test]
    fn transcript_interrupt_near_eof() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Request interrupted by user"}]}}"#;
        assert_eq!(
            apply_transcript_signal(SessionStatus::Working, line, 100, 100),
            SessionStatus::Done
        );
        assert_eq!(
            apply_transcript_signal(SessionStatus::Working, line, 50, 10_000),
            SessionStatus::Working
        );
    }

    #[test]
    fn permission_notification() {
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                Some("permission_prompt"),
                None,
            ),
            SessionStatus::WaitingPermission
        );
    }

    #[test]
    fn permission_notification_type_is_case_insensitive() {
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                Some("PERMISSION_REQUEST"),
                None,
            ),
            SessionStatus::WaitingPermission
        );
    }

    #[test]
    fn permission_notification_from_message_body() {
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                None,
                Some("Claude wants permission to run a tool"),
            ),
            SessionStatus::WaitingPermission
        );
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                None,
                Some("Allow this tool invocation?"),
            ),
            SessionStatus::WaitingPermission
        );
    }

    #[test]
    fn non_permission_notification_keeps_working_or_marks_done() {
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                Some("info"),
                Some("build finished"),
            ),
            SessionStatus::Working
        );
        assert_eq!(
            apply_hook_event(
                SessionStatus::Idle,
                "Notification",
                Some("info"),
                None,
            ),
            SessionStatus::Done
        );
    }

    #[test]
    fn allow_without_tool_in_message_is_not_permission() {
        assert_eq!(
            apply_hook_event(
                SessionStatus::Working,
                "Notification",
                None,
                Some("allow listing files"),
            ),
            SessionStatus::Working
        );
    }
}
