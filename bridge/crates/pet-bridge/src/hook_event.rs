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
    "SubagentStop",
    "PreCompact",
];

impl HookEvent {
    pub fn is_claude_code(&self) -> bool {
        CLAUDE_CODE_EVENTS.contains(&self.event.as_str())
    }
}
