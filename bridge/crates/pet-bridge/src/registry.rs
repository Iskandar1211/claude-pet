use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::channel_events::{ChannelEvent, SessionChannelEvent};
use crate::hook_event::HookEvent;
use crate::liveness::{process_alive, session_alive};
use crate::paths::{channels_dir, pending_dir, pid_from_socket, tokens_dir};
use crate::pending::{clear_pending_file, load_pending, save_pending};
use crate::state::{apply_hook_event, apply_transcript_signal, PendingPermission, SessionStatus};
use crate::transcript::{one_line, parse_transcript_line};

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub entrypoint: Option<String>,
    pub cwd: Option<String>,
    pub transcript_path: Option<PathBuf>,
    pub inbox_socket: Option<String>,
    pub inbox_token: Option<String>,
    pub channel_port: Option<u16>,
    /// Токен из `channels/<pid>.json`; `None` — старый Channel, берём глобальный из config.json.
    pub channel_token: Option<String>,
    pub status: SessionStatus,
    pub last_message: Option<String>,
    pub pending_permission: Option<PendingPermission>,
    pub transcript_offset: u64,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: HashMap<String, SessionRecord>,
    events_offset: u64,
    last_session_id: Option<String>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sessions(&self) -> impl Iterator<Item = &SessionRecord> {
        self.sessions.values()
    }

    pub fn get(&self, id: &str) -> Option<&SessionRecord> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut SessionRecord> {
        self.sessions.get_mut(id)
    }

    pub fn refresh(&mut self, events_path: &PathBuf) -> Result<()> {
        self.ingest_events(events_path)?;
        self.load_inbox_tokens();
        self.load_channels();
        self.load_pending_files();
        self.tail_transcripts()?;
        self.drop_dead();
        Ok(())
    }

    fn ingest_events(&mut self, events_path: &PathBuf) -> Result<()> {
        if !events_path.exists() {
            return Ok(());
        }
        let len = fs::metadata(events_path)?.len();
        if len < self.events_offset {
            self.events_offset = 0;
        }
        let file = fs::File::open(events_path).context("open events.jsonl")?;
        use std::io::{BufRead, Seek, SeekFrom};
        let mut handle = file;
        handle.seek(SeekFrom::Start(self.events_offset))?;
        let mut reader = std::io::BufReader::new(handle);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf)? != 0 {
            if buf.last() != Some(&b'\n') {
                break;
            }
            self.events_offset += buf.len() as u64;
            let line = String::from_utf8_lossy(&buf[..buf.len() - 1]);
            if !line.trim().is_empty() {
                if let Ok(ev) = serde_json::from_str::<HookEvent>(&line) {
                    if ev.is_claude_code() {
                        self.apply_event(ev);
                    }
                }
            }
            buf.clear();
        }
        Ok(())
    }

    fn apply_event(&mut self, ev: HookEvent) {
        self.last_session_id = Some(ev.session_id.clone());
        if ev.event == "SessionEnd" {
            self.sessions.remove(&ev.session_id);
            return;
        }

        let entry = self
            .sessions
            .entry(ev.session_id.clone())
            .or_insert_with(|| SessionRecord {
                session_id: ev.session_id.clone(),
                entrypoint: None,
                cwd: None,
                transcript_path: None,
                inbox_socket: None,
                inbox_token: None,
                channel_port: None,
                channel_token: None,
                status: SessionStatus::Idle,
                last_message: None,
                pending_permission: None,
                transcript_offset: 0,
            });

        if ev.entrypoint.is_some() {
            entry.entrypoint = ev.entrypoint;
        }
        if ev.cwd.is_some() {
            entry.cwd = ev.cwd;
        }
        if ev.transcript_path.is_some() {
            entry.transcript_path = ev.transcript_path.map(PathBuf::from);
        }
        if let Some(sock) = ev.socket.filter(|s| !s.is_empty()) {
            entry.inbox_socket = Some(sock);
        }

        entry.status = apply_hook_event(
            entry.status,
            &ev.event,
            ev.notification_type.as_deref(),
            ev.notification.as_deref(),
        );
        if entry.status == SessionStatus::Working {
            entry.pending_permission = None;
            clear_pending_file(&ev.session_id);
        }
    }

    pub(crate) fn apply_channel_event(&mut self, ev: SessionChannelEvent) {
        let Some(rec) = self.sessions.get_mut(&ev.session_id) else {
            return;
        };
        match ev.event {
            ChannelEvent::Permission(p) => {
                rec.pending_permission = Some(p.clone());
                rec.status = SessionStatus::WaitingPermission;
                let _ = save_pending(&ev.session_id, &p);
            }
            ChannelEvent::Reply { text } => {
                rec.last_message = Some(format!("pet: {}", one_line(&text, 200)));
            }
        }
    }

    /// Сбросить pending permission после вердикта allow/deny.
    pub fn clear_pending_permission(&mut self, id: &str) {
        clear_pending_file(id);
        if let Some(rec) = self.sessions.get_mut(id) {
            rec.pending_permission = None;
            if rec.status == SessionStatus::WaitingPermission {
                rec.status = SessionStatus::Working;
            }
        }
    }

    fn load_pending_files(&mut self) {
        let dir = pending_dir();
        if !dir.is_dir() {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if let Some(perm) = load_pending(&sid) {
                if let Some(rec) = self.sessions.get_mut(&sid) {
                    rec.pending_permission = Some(perm);
                    rec.status = SessionStatus::WaitingPermission;
                }
            }
        }
    }

    fn load_inbox_tokens(&mut self) {
        let dir = tokens_dir();
        if !dir.is_dir() {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("read tokens dir: {e}");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let raw = match fs::read_to_string(&path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("read token file {:?}: {e}", path);
                    continue;
                }
            };
            let creds: InboxCreds = match serde_json::from_str(&raw) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("parse token file {:?}: {e}", path);
                    continue;
                }
            };
            if let Some(rec) = self.sessions.get_mut(&sid) {
                rec.inbox_socket = Some(creds.socket);
                rec.inbox_token = Some(creds.token);
            }
        }
    }

    fn load_channels(&mut self) {
        let dir = channels_dir();
        if !dir.is_dir() {
            return;
        }
        let mut by_pid: HashMap<u32, (u16, Option<String>)> = HashMap::new();
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("read channels dir: {e}");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if let Ok(info) = serde_json::from_str::<ChannelFile>(&raw) {
                if process_alive(info.pid) {
                    by_pid.insert(info.pid, (info.port, info.channel_token));
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        for rec in self.sessions.values_mut() {
            rec.channel_port = None;
            rec.channel_token = None;
            if let Some(sock) = rec.inbox_socket.as_deref() {
                if let Some(pid) = pid_from_socket(sock) {
                    if let Some((port, token)) = by_pid.get(&pid) {
                        rec.channel_port = Some(*port);
                        rec.channel_token = token.clone();
                    }
                }
            }
        }
    }

    fn tail_transcripts(&mut self) -> Result<()> {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            if let Err(e) = self.tail_one_transcript(&id) {
                tracing::warn!("transcript tail {id}: {e:#}");
            }
        }
        Ok(())
    }

    fn tail_one_transcript(&mut self, id: &str) -> Result<()> {
        let rec = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("session gone"))?;
        let path = match &rec.transcript_path {
            Some(p) if p.exists() => p.clone(),
            _ => return Ok(()),
        };
        let file_len_at_tail = fs::metadata(&path)?.len();
        if file_len_at_tail < rec.transcript_offset {
            rec.transcript_offset = 0;
        }
        let mut handle = fs::File::open(&path)?;
        use std::io::{BufRead, Seek, SeekFrom};
        handle.seek(SeekFrom::Start(rec.transcript_offset))?;
        let mut reader = std::io::BufReader::new(handle);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf)? != 0 {
            if buf.last() != Some(&b'\n') {
                break;
            }
            rec.transcript_offset += buf.len() as u64;
            let line_end_offset = rec.transcript_offset;
            let line = String::from_utf8_lossy(&buf[..buf.len() - 1]);
            rec.status =
                apply_transcript_signal(rec.status, &line, line_end_offset, file_len_at_tail);
            if let Some(chat) = parse_transcript_line(&line) {
                rec.last_message = Some(format!("{}: {}", chat.role, chat.preview));
            }
            buf.clear();
        }
        Ok(())
    }

    fn drop_dead(&mut self) {
        self.sessions.retain(|_, rec| {
            match rec.inbox_socket.as_deref() {
                Some(sock) if !sock.is_empty() => session_alive(Some(sock)),
                _ => true,
            }
        });
    }
}

#[derive(Debug, Deserialize)]
struct InboxCreds {
    socket: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct ChannelFile {
    pid: u32,
    port: u16,
    #[serde(default)]
    channel_token: Option<String>,
}

pub fn resolve_session_id(registry: &SessionRegistry, query: &str) -> Result<String> {
    if query.is_empty() {
        anyhow::bail!("пустой идентификатор сессии");
    }
    if query == "last" {
        let id = registry
            .last_session_id
            .clone()
            .context("ещё не было hook-событий")?;
        if registry.get(&id).is_some() {
            return Ok(id);
        }
        anyhow::bail!("последняя сессия уже не в реестре");
    }
    if registry.get(query).is_some() {
        return Ok(query.to_string());
    }
    let short = query.to_lowercase();
    let matches: Vec<String> = registry
        .sessions()
        .filter(|s| s.session_id.starts_with(&short))
        .map(|s| s.session_id.clone())
        .collect();
    match matches.len() {
        0 => anyhow::bail!("сессия не найдена"),
        1 => Ok(matches[0].clone()),
        _ => anyhow::bail!(
            "неоднозначный префикс «{query}»: {}",
            matches
                .iter()
                .map(|s| s.chars().take(8).collect::<String>())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
impl SessionRegistry {
    fn with_sessions(ids: &[&str], last_session_id: Option<&str>) -> Self {
        let mut reg = Self::new();
        for id in ids {
            reg.sessions.insert(
                id.to_string(),
                SessionRecord {
                    session_id: id.to_string(),
                    entrypoint: None,
                    cwd: None,
                    transcript_path: None,
                    inbox_socket: None,
                    inbox_token: None,
                    channel_port: None,
                    channel_token: None,
                    status: SessionStatus::Idle,
                    last_message: None,
                    pending_permission: None,
                    transcript_offset: 0,
                },
            );
        }
        reg.last_session_id = last_session_id.map(str::to_string);
        reg
    }

    fn events_byte_offset(&self) -> u64 {
        self.events_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn session_start_line(session_id: &str) -> String {
        format!(
            r#"{{"event":"SessionStart","session_id":"{session_id}"}}"#,
        )
    }

    fn user_prompt_line(session_id: &str) -> String {
        format!(
            r#"{{"event":"UserPromptSubmit","session_id":"{session_id}"}}"#,
        )
    }

    #[test]
    fn resolve_session_id_rejects_empty_query() {
        let reg = SessionRegistry::new();
        let err = resolve_session_id(&reg, "").unwrap_err();
        assert!(err.to_string().contains("пустой"));
    }

    #[test]
    fn resolve_session_id_exact_match() {
        let id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let reg = SessionRegistry::with_sessions(&[id], None);
        assert_eq!(resolve_session_id(&reg, id).unwrap(), id);
    }

    #[test]
    fn resolve_session_id_unique_prefix() {
        let id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let reg = SessionRegistry::with_sessions(&[id], None);
        assert_eq!(resolve_session_id(&reg, "AAAAAAA").unwrap(), id);
    }

    #[test]
    fn resolve_session_id_ambiguous_prefix() {
        let a = "aaaaaaaa-1111-2222-3333-444444444444";
        let b = "aaaaaaaa-5555-6666-7777-888888888888";
        let reg = SessionRegistry::with_sessions(&[a, b], None);
        let err = resolve_session_id(&reg, "aaaaaaaa").unwrap_err();
        assert!(err.to_string().contains("неоднозначный"));
    }

    #[test]
    fn resolve_session_id_last_without_events() {
        let reg = SessionRegistry::new();
        let err = resolve_session_id(&reg, "last").unwrap_err();
        assert!(err.to_string().contains("hook-событий"));
    }

    #[test]
    fn resolve_session_id_last_when_session_still_registered() {
        let id = "bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee";
        let reg = SessionRegistry::with_sessions(&[id], Some(id));
        assert_eq!(resolve_session_id(&reg, "last").unwrap(), id);
    }

    #[test]
    fn resolve_session_id_last_when_session_removed_from_registry() {
        let id = "cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee";
        let reg = SessionRegistry::with_sessions(&[], Some(id));
        let err = resolve_session_id(&reg, "last").unwrap_err();
        assert!(err.to_string().contains("не в реестре"));
    }

    #[test]
    fn ingest_events_waits_for_trailing_newline_before_committing_offset() {
        // ingest_events only advances events_offset for lines terminated with `\n`.
        // A trailing partial JSON line stays buffered in the file until the writer
        // completes the line and adds the newline; the next refresh picks it up.
        let dir = std::env::temp_dir().join(format!(
            "pet-bridge-ingest-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let sid = "dddddddd-bbbb-cccc-dddd-eeeeeeeeeeee";

        let complete = session_start_line(sid);
        let partial = user_prompt_line(sid);
        let mut file = fs::File::create(&path).unwrap();
        write!(file, "{complete}\n{partial}").unwrap();
        drop(file);

        let mut reg = SessionRegistry::new();
        reg.ingest_events(&path).unwrap();
        assert_eq!(reg.events_byte_offset(), (complete.len() + 1) as u64);
        let rec = reg.get(sid).expect("first complete line registers session");
        assert_eq!(rec.status, SessionStatus::Idle);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "\n").unwrap();
        drop(file);

        reg.ingest_events(&path).unwrap();
        assert_eq!(
            reg.events_byte_offset(),
            (complete.len() + 1 + partial.len() + 1) as u64
        );
        assert_eq!(
            reg.get(sid).unwrap().status,
            SessionStatus::Working
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ingest_events_ignores_cursor_agent_hooks() {
        // Агент Cursor пишет в тот же лог свои события (camelCase, без entrypoint) — это не сессии Claude Code.
        let dir = std::env::temp_dir().join(format!("pet-bridge-cursor-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let claude = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let cursor = "cb5afd16-367e-406d-8201-65e4ca45449f";

        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "{}", session_start_line(claude)).unwrap();
        writeln!(
            file,
            r#"{{"event":"preToolUse","session_id":"{cursor}","tool":"Grep","socket":"","entrypoint":""}}"#
        )
        .unwrap();
        writeln!(file, r#"{{"event":"beforeSubmitPrompt","session_id":"{cursor}"}}"#).unwrap();
        drop(file);

        let mut reg = SessionRegistry::new();
        reg.ingest_events(&path).unwrap();
        assert!(reg.get(claude).is_some());
        assert!(reg.get(cursor).is_none());

        let _ = fs::remove_dir_all(dir);
    }
}
