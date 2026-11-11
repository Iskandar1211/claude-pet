use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HookEvent {
    pub event: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub transcript_path: Option<String>,
    pub socket: Option<String>,
    pub entrypoint: Option<String>,
    pub notification: Option<String>,
    pub notification_type: Option<String>,
    /// Только у `StopFailure`: тип ошибки API (`rate_limit`, `overloaded`, …).
    pub error: Option<String>,
    /// Только у `SubagentStart` / `SubagentStop`: идентификатор субагента.
    pub agent_id: Option<String>,
}

/// Имена hook-событий Claude Code (регистр важен). Агент Cursor тоже запускает hooks из
/// `.claude/settings` и пишет в тот же лог свои `preToolUse` / `beforeSubmitPrompt` / `stop`
/// без `entrypoint` и сокета — это не сессии Claude Code.
const CLAUDE_CODE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
];

impl HookEvent {
    pub fn is_claude_code(&self) -> bool {
        CLAUDE_CODE_EVENTS.contains(&self.event.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_error_field_of_stop_failure() {
        let ev: HookEvent =
            serde_json::from_str(r#"{"event":"StopFailure","session_id":"s","error":"rate_limit"}"#).unwrap();
        assert_eq!(ev.error.as_deref(), Some("rate_limit"));
        assert!(ev.is_claude_code());
    }

    #[test]
    fn error_field_is_optional() {
        let ev: HookEvent = serde_json::from_str(r#"{"event":"Stop","session_id":"s"}"#).unwrap();
        assert_eq!(ev.error, None);
    }

    #[test]
    fn parses_agent_id_of_subagent_events() {
        let ev: HookEvent =
            serde_json::from_str(r#"{"event":"SubagentStart","session_id":"s","agent_id":"a1"}"#).unwrap();
        assert_eq!(ev.agent_id.as_deref(), Some("a1"));
        assert!(ev.is_claude_code());
        let ev: HookEvent = serde_json::from_str(r#"{"event":"SubagentStop","session_id":"s"}"#).unwrap();
        assert_eq!(ev.agent_id, None);
        assert!(ev.is_claude_code());
    }
}
