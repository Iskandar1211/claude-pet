//! Hook Claude Code `PermissionRequest` (`pet-bridge hook permission`): даёт кнопки
//! «Разрешить/Отклонить» в питомце сессиям без Channel (расширение VS Code / Cursor).
//!
//! Обмен файлами: hook пишет `pending/<session_id>.json` (его подхватывает реестр `serve`),
//! питомец отвечает файлом `verdicts/<request_id>.json` (`write_verdict`), hook печатает решение.
//! Любое «не знаем» — выход без вывода: тогда Claude Code показывает свой обычный диалог.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::liveness::process_alive;
use crate::paths::{pending_dir, pending_path_in, serve_pid_path, verdict_path_in, verdicts_dir};
use crate::pending::{load_pending_in, save_pending_in};
use crate::state::{PendingPermission, RememberRule};

/// Префикс request_id запросов от hook'а (у Channel id без него) — по нему serve выбирает маршрут вердикта.
pub const HOOK_REQUEST_PREFIX: &str = "hook-";
/// Сколько hook ждёт ответа питомца; дальше — без решения, отвечает пользователь в IDE.
pub const VERDICT_TIMEOUT: Duration = Duration::from_secs(300);
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Совпадает с `previewLimit` в `QuickChatView.swift`: длиннее — приложение считает превью неполным.
pub const PREVIEW_LIMIT: usize = 4_000;
/// Хвост обрезанного превью. Перевод строки отделяет «…» в собственную графему, поэтому
/// `hasSuffix("…")` в Swift срабатывает при любом последнем символе обрезанного текста.
const TRUNCATION_MARK: &str = "\n…";
/// Инструменты, где ответ из питомца бессмысленен: это вопрос пользователю, а не разрешение.
const SKIPPED_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];
/// Параллельно с диалогом hook работает только в расширении IDE. В терминале и SDK Claude Code
/// ждёт hook до показа своего диалога — там ожидание питомца заблокировало бы сессию.
const IDE_ENTRYPOINT: &str = "claude-vscode";
/// Больше правил или длиннее правило — показать человеку честно нельзя, кнопки «всегда» нет.
const MAX_REMEMBER_RULES: usize = 10;
const MAX_RULE_CHARS: usize = 300;
const DENY_MESSAGE: &str = "Пользователь отклонил запрос в Claude Pet";

#[derive(Debug, Clone)]
pub struct HookPaths {
    pub pending_dir: PathBuf,
    pub verdicts_dir: PathBuf,
    pub serve_pid: PathBuf,
}

impl HookPaths {
    /// Настоящие пути в `~/.claude-pet` (или `$PET_DIR`).
    pub fn user() -> Self {
        Self { pending_dir: pending_dir(), verdicts_dir: verdicts_dir(), serve_pid: serve_pid_path() }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WaitLimits {
    pub timeout: Duration,
    pub poll: Duration,
}

impl Default for WaitLimits {
    fn default() -> Self {
        Self { timeout: VERDICT_TIMEOUT, poll: POLL_INTERVAL }
    }
}

#[derive(Debug, Deserialize)]
struct HookInput {
    hook_event_name: String,
    session_id: String,
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
    /// Обновления прав, которые предлагает сам Claude Code (необязательно).
    #[serde(default)]
    permission_suggestions: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Behavior {
    Allow,
    Deny,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct VerdictFile {
    behavior: Behavior,
    /// «Разрешать всегда»; в файле поля нет, если это обычный ответ.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    always: bool,
}

/// Из `permission_suggestions` берём только `addRules` + `allow` (как есть, для `updatedPermissions`)
/// и их же в виде для показа. Всё, что не получится показать честно (кривая форма, больше
/// `MAX_REMEMBER_RULES` правил, правило длиннее `MAX_RULE_CHARS`), — пустой результат: кнопки нет.
fn remember_offer(suggestions: &Value) -> (Vec<Value>, Vec<RememberRule>) {
    let selected: Vec<&Value> = suggestions
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|s| s["type"] == "addRules" && s["behavior"] == "allow")
                .collect()
        })
        .unwrap_or_default();
    let shown: Option<Vec<RememberRule>> = selected.iter().map(|s| describe_suggestion(s)).collect();
    match shown {
        Some(shown) if shown.iter().map(|r| r.rules.len()).sum::<usize>() <= MAX_REMEMBER_RULES => {
            (selected.into_iter().cloned().collect(), shown)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

fn describe_suggestion(s: &Value) -> Option<RememberRule> {
    let destination = s["destination"].as_str()?.to_string();
    let rules: Vec<String> = s["rules"]
        .as_array()?
        .iter()
        .map(|r| {
            let tool = r["toolName"].as_str().filter(|t| !t.is_empty())?;
            let text = match r["ruleContent"].as_str().filter(|c| !c.is_empty()) {
                Some(content) => format!("{tool}({content})"),
                None => tool.to_string(),
            };
            (text.chars().count() <= MAX_RULE_CHARS).then_some(text)
        })
        .collect::<Option<_>>()?;
    (!rules.is_empty()).then_some(RememberRule { rules, destination })
}

/// Весь hook: stdin → строка для stdout (`Some`) или выход без решения (`None`).
/// `entrypoint` — `CLAUDE_CODE_ENTRYPOINT` процесса hook'а.
pub fn run_permission_hook(
    input: &str,
    entrypoint: Option<&str>,
    paths: &HookPaths,
    limits: WaitLimits,
) -> Option<String> {
    let input: HookInput = serde_json::from_str(input).ok()?;
    if input.hook_event_name != "PermissionRequest" || SKIPPED_TOOLS.contains(&input.tool_name.as_str()) {
        return None;
    }
    if entrypoint != Some(IDE_ENTRYPOINT) || !serve_running(&paths.serve_pid) {
        return None;
    }
    let request_id = new_request_id();
    let verdict_path = verdict_path_in(&paths.verdicts_dir, &request_id)?;
    let (offered, remember) = remember_offer(&input.permission_suggestions);
    let perm = PendingPermission {
        request_id: request_id.clone(),
        tool_name: input.tool_name.clone(),
        input_preview: input_preview(&input.tool_name, &input.tool_input),
        remember,
    };
    if let Err(e) = save_pending_in(&paths.pending_dir, &input.session_id, &perm) {
        tracing::warn!("hook permission: pending: {e:#}");
        return None;
    }

    let verdict = wait_verdict(&input.session_id, &request_id, &verdict_path, paths, limits);
    let _ = fs::remove_file(&verdict_path);
    clear_own_pending(&paths.pending_dir, &input.session_id, &request_id);
    verdict.map(|v| decision_json(&v, &offered))
}

/// Порядок проверок важен: вердикт раньше pending — serve пишет вердикт до того, как pending снимается.
fn wait_verdict(
    session_id: &str,
    request_id: &str,
    verdict_path: &Path,
    paths: &HookPaths,
    limits: WaitLimits,
) -> Option<VerdictFile> {
    let started = Instant::now();
    loop {
        if let Some(verdict) = read_verdict(verdict_path) {
            return verdict;
        }
        let ours = load_pending_in(&paths.pending_dir, session_id).is_some_and(|p| p.request_id == request_id);
        if !ours {
            // Pending сняли (ответ в IDE, сессия пошла дальше) — но вердикт мог успеть лечь между проверками.
            return read_verdict(verdict_path).flatten();
        }
        if !serve_running(&paths.serve_pid) || started.elapsed() >= limits.timeout {
            return None;
        }
        std::thread::sleep(limits.poll);
    }
}

/// `None` — файла нет; `Some(None)` — файл есть, но не разобран (выходим без решения).
fn read_verdict(path: &Path) -> Option<Option<VerdictFile>> {
    let raw = fs::read_to_string(path).ok()?;
    Some(serde_json::from_str::<VerdictFile>(&raw).ok())
}

/// `offered` — отобранные предложения Claude Code: в `updatedPermissions` уходят дословно и только при
/// «всегда». Если «всегда» пришло, а предложений нет (рассинхрон), — обычный allow.
fn decision_json(verdict: &VerdictFile, offered: &[Value]) -> String {
    let decision = match verdict.behavior {
        Behavior::Allow if verdict.always && !offered.is_empty() => {
            json!({ "behavior": "allow", "updatedPermissions": offered })
        }
        Behavior::Allow => json!({ "behavior": "allow" }),
        Behavior::Deny => json!({ "behavior": "deny", "message": DENY_MESSAGE }),
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }).to_string()
}

/// Снимаем pending, только если он всё ещё наш: новый запрос той же сессии не трогаем.
fn clear_own_pending(dir: &Path, session_id: &str, request_id: &str) {
    if load_pending_in(dir, session_id).is_some_and(|p| p.request_id == request_id) {
        if let Some(path) = pending_path_in(dir, session_id) {
            let _ = fs::remove_file(path);
        }
    }
}

/// `hook-<pid>-<наносекунды>`: уникально без новых зависимостей, pid нужен реестру,
/// чтобы убрать pending hook'а, которого Claude Code уже завершил.
fn new_request_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{HOOK_REQUEST_PREFIX}{}-{nanos}", std::process::id())
}

pub(crate) fn is_hook_request(request_id: &str) -> bool {
    request_id.starts_with(HOOK_REQUEST_PREFIX)
}

pub(crate) fn hook_request_pid(request_id: &str) -> Option<u32> {
    request_id.strip_prefix(HOOK_REQUEST_PREFIX)?.split('-').next()?.parse().ok()
}

/// Bash — команда целиком; остальное — `tool_input` как JSON с отступами.
/// Длиннее `PREVIEW_LIMIT` — обрезка с `TRUNCATION_MARK`: приложение увидит «…» в конце
/// (и длину больше лимита) и не даст нажать «Разрешить».
fn input_preview(tool_name: &str, tool_input: &Value) -> String {
    let full = match (tool_name, tool_input.get("command").and_then(Value::as_str)) {
        ("Bash", Some(command)) => command.to_string(),
        _ => serde_json::to_string_pretty(tool_input).unwrap_or_default(),
    };
    if full.chars().count() <= PREVIEW_LIMIT {
        return full;
    }
    let mut cut: String = full.chars().take(PREVIEW_LIMIT).collect();
    cut.push_str(TRUNCATION_MARK);
    cut
}

fn serve_running(pid_file: &Path) -> bool {
    fs::read_to_string(pid_file)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .is_some_and(process_alive)
}

/// Ответ питомца для hook'а: атомарно (tmp + rename) пишет `verdicts/<request_id>.json`.
pub fn write_verdict(request_id: &str, allow: bool, always: bool) -> Result<()> {
    write_verdict_in(&verdicts_dir(), request_id, allow, always)
}

pub fn write_verdict_in(dir: &Path, request_id: &str, allow: bool, always: bool) -> Result<()> {
    anyhow::ensure!(allow || !always, "«всегда» возможно только вместе с разрешением");
    let path = verdict_path_in(dir, request_id).context("небезопасный request_id")?;
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    let behavior = if allow { Behavior::Allow } else { Behavior::Deny };
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string(&VerdictFile { behavior, always })?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Вердикт питомца на запрос hook'а: пишем только если pending сессии — ровно этот запрос
/// (иначе вердикты можно было бы класть на произвольные id). Pending снимаем ПОСЛЕ записи вердикта:
/// hook сначала ищет вердикт, а уже потом смотрит, не пропал ли pending.
pub fn answer_hook_request(session_id: &str, request_id: &str, allow: bool, always: bool) -> Result<()> {
    answer_hook_request_in(&HookPaths::user(), session_id, request_id, allow, always)
}

pub fn answer_hook_request_in(
    paths: &HookPaths,
    session_id: &str,
    request_id: &str,
    allow: bool,
    always: bool,
) -> Result<()> {
    let current = load_pending_in(&paths.pending_dir, session_id).is_some_and(|p| p.request_id == request_id);
    if !current {
        anyhow::bail!("запрос уже неактуален");
    }
    write_verdict_in(&paths.verdicts_dir, request_id, allow, always)?;
    clear_own_pending(&paths.pending_dir, session_id, request_id);
    Ok(())
}

/// Признак живости `serve`: pid-файл на время работы. Drop убирает файл, если он всё ещё наш.
pub struct ServePidFile {
    path: PathBuf,
    pid: u32,
}

impl ServePidFile {
    pub fn create() -> Result<Self> {
        Self::create_at(serve_pid_path())
    }

    pub fn create_at(path: PathBuf) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let pid = std::process::id();
        let tmp = path.with_extension("pid.tmp");
        fs::write(&tmp, pid.to_string())?;
        fs::rename(&tmp, &path).context("serve.pid")?;
        Ok(Self { path, pid })
    }
}

impl Drop for ServePidFile {
    fn drop(&mut self) {
        let ours = fs::read_to_string(&self.path).is_ok_and(|raw| raw.trim() == self.pid.to_string());
        if ours {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "11111111-2222-3333-4444-555555555555";

    fn temp_paths(name: &str) -> HookPaths {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("pet-bridge-hook-{name}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        HookPaths {
            pending_dir: root.join("pending"),
            verdicts_dir: root.join("verdicts"),
            serve_pid: root.join("serve.pid"),
        }
    }

    /// serve «жив»: в pid-файле pid самого теста.
    fn with_serve(paths: &HookPaths) {
        fs::write(&paths.serve_pid, std::process::id().to_string()).unwrap();
    }

    fn bash_input(command: &str) -> String {
        json!({
            "session_id": SID,
            "transcript_path": "/tmp/t.jsonl",
            "cwd": "/tmp",
            "permission_mode": "default",
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "tool_input": { "command": command, "description": "d" }
        })
        .to_string()
    }

    fn limits(timeout_ms: u64) -> WaitLimits {
        WaitLimits { timeout: Duration::from_millis(timeout_ms), poll: Duration::from_millis(10) }
    }

    /// Ждёт pending hook'а и возвращает его request_id.
    fn wait_pending(dir: &Path) -> PendingPermission {
        let started = Instant::now();
        loop {
            if let Some(p) = load_pending_in(dir, SID) {
                return p;
            }
            assert!(started.elapsed() < Duration::from_secs(5), "pending не появился");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn run_with_responder(
        paths: &HookPaths,
        input: String,
        responder: impl FnOnce(&HookPaths, PendingPermission) + Send + 'static,
    ) -> Option<String> {
        let p = paths.clone();
        let handle = std::thread::spawn(move || {
            let pending = wait_pending(&p.pending_dir);
            responder(&p, pending);
        });
        let out = run_permission_hook(&input, Some("claude-vscode"), paths, limits(5_000));
        handle.join().unwrap();
        out
    }

    fn dir_is_empty(dir: &Path) -> bool {
        fs::read_dir(dir).map(|mut e| e.next().is_none()).unwrap_or(true)
    }

    #[test]
    fn allow_verdict_prints_decision_and_cleans_up() {
        let paths = temp_paths("allow");
        with_serve(&paths);
        let out = run_with_responder(&paths, bash_input("ls -la"), |p, pending| {
            assert!(pending.request_id.starts_with(HOOK_REQUEST_PREFIX));
            assert_eq!(pending.tool_name, "Bash");
            assert_eq!(pending.input_preview, "ls -la");
            write_verdict_in(&p.verdicts_dir, &pending.request_id, true, false).unwrap();
        })
        .expect("решение");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(v["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
        assert!(dir_is_empty(&paths.pending_dir));
        assert!(dir_is_empty(&paths.verdicts_dir));
    }

    #[test]
    fn deny_verdict_prints_deny() {
        let paths = temp_paths("deny");
        with_serve(&paths);
        let out = run_with_responder(&paths, bash_input("rm -rf build"), |p, pending| {
            write_verdict_in(&p.verdicts_dir, &pending.request_id, false, false).unwrap();
        })
        .expect("решение");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(v["hookSpecificOutput"]["decision"]["message"].is_string());
        assert!(dir_is_empty(&paths.verdicts_dir));
    }

    #[test]
    fn pending_removed_externally_exits_without_decision() {
        let paths = temp_paths("ide");
        with_serve(&paths);
        let out = run_with_responder(&paths, bash_input("ls"), |p, _| {
            fs::remove_file(pending_path_in(&p.pending_dir, SID).unwrap()).unwrap();
        });
        assert_eq!(out, None);
    }

    #[test]
    fn pending_replaced_by_other_request_is_left_alone() {
        let paths = temp_paths("replaced");
        with_serve(&paths);
        let other = PendingPermission { request_id: "hook-1-2".into(), tool_name: "Bash".into(), input_preview: "x".into(), ..Default::default() };
        let other2 = other.clone();
        let out = run_with_responder(&paths, bash_input("ls"), move |p, _| {
            save_pending_in(&p.pending_dir, SID, &other2).unwrap();
        });
        assert_eq!(out, None);
        assert_eq!(load_pending_in(&paths.pending_dir, SID), Some(other));
    }

    #[test]
    fn verdict_written_just_before_pending_cleared_still_counts() {
        let paths = temp_paths("race");
        with_serve(&paths);
        let out = run_with_responder(&paths, bash_input("ls"), |p, pending| {
            write_verdict_in(&p.verdicts_dir, &pending.request_id, true, false).unwrap();
            let _ = fs::remove_file(pending_path_in(&p.pending_dir, SID).unwrap());
        });
        assert!(out.is_some_and(|o| o.contains("\"allow\"")));
    }

    #[test]
    fn timeout_exits_without_decision_and_cleans_up() {
        let paths = temp_paths("timeout");
        with_serve(&paths);
        let out = run_permission_hook(&bash_input("ls"), Some("claude-vscode"), &paths, limits(100));
        assert_eq!(out, None);
        assert!(dir_is_empty(&paths.pending_dir));
        assert!(dir_is_empty(&paths.verdicts_dir));
    }

    #[test]
    fn serve_not_running_skips_without_pending() {
        let paths = temp_paths("noserve");
        let out = run_permission_hook(&bash_input("ls"), Some("claude-vscode"), &paths, limits(5_000));
        assert_eq!(out, None);
        assert!(!paths.pending_dir.exists());
        // Мёртвый pid в файле — то же самое.
        fs::write(&paths.serve_pid, "0").unwrap();
        assert_eq!(run_permission_hook(&bash_input("ls"), Some("claude-vscode"), &paths, limits(5_000)), None);
        assert!(!paths.pending_dir.exists());
    }

    #[test]
    fn serve_exit_while_waiting_exits_without_decision() {
        let paths = temp_paths("serve-exit");
        with_serve(&paths);
        let out = run_with_responder(&paths, bash_input("ls"), |p, _| {
            fs::remove_file(&p.serve_pid).unwrap();
        });
        assert_eq!(out, None);
        assert!(dir_is_empty(&paths.pending_dir));
    }

    #[test]
    fn ask_user_question_and_exit_plan_mode_are_skipped() {
        let paths = temp_paths("ask");
        with_serve(&paths);
        for tool in SKIPPED_TOOLS {
            let input = json!({
                "session_id": SID,
                "hook_event_name": "PermissionRequest",
                "tool_name": tool,
                "tool_input": { "questions": [] }
            })
            .to_string();
            assert_eq!(run_permission_hook(&input, Some("claude-vscode"), &paths, limits(5_000)), None);
        }
        assert!(!paths.pending_dir.exists());
    }

    #[test]
    fn non_ide_entrypoints_and_bad_input_are_skipped() {
        let paths = temp_paths("cli");
        with_serve(&paths);
        for ep in [None, Some("cli"), Some("sdk-ts")] {
            assert_eq!(run_permission_hook(&bash_input("ls"), ep, &paths, limits(5_000)), None);
        }
        assert_eq!(run_permission_hook("не json", Some("claude-vscode"), &paths, limits(5_000)), None);
        let bad_sid = bash_input("ls").replace(SID, "../evil");
        assert_eq!(run_permission_hook(&bad_sid, Some("claude-vscode"), &paths, limits(5_000)), None);
        assert!(!paths.pending_dir.exists());
    }

    /// Та же проверка, что `isTruncated` в QuickChatView.swift.
    fn app_considers_truncated(preview: &str) -> bool {
        preview.chars().count() > PREVIEW_LIMIT || preview.ends_with('…') || preview.ends_with("...")
    }

    #[test]
    fn long_preview_is_recognised_as_truncated() {
        let long = "echo x && ".repeat(1_000);
        let preview = input_preview("Bash", &json!({ "command": long }));
        assert!(app_considers_truncated(&preview));
        assert!(preview.ends_with("\n…"));
        assert!(long.starts_with(preview.trim_end_matches("\n…")));

        let exact = "a".repeat(PREVIEW_LIMIT);
        assert_eq!(input_preview("Bash", &json!({ "command": exact })), exact);
        assert!(!app_considers_truncated(&exact));
    }

    #[test]
    fn preview_formats() {
        assert_eq!(input_preview("Bash", &json!({ "command": "git status", "description": "d" })), "git status");
        let edit = input_preview("Edit", &json!({ "file_path": "/a.rs", "old_string": "x", "new_string": "y" }));
        assert!(edit.contains("\"file_path\": \"/a.rs\""));
        assert!(edit.contains("\"new_string\": \"y\""));
    }

    #[test]
    fn unsafe_request_ids_are_rejected() {
        let paths = temp_paths("unsafe");
        for rid in ["", "../x", "a/b", "a.b", "..", "a\0b", &"a".repeat(129)] {
            assert!(write_verdict_in(&paths.verdicts_dir, rid, true, false).is_err(), "{rid:?}");
        }
        assert!(!paths.verdicts_dir.exists());
        write_verdict_in(&paths.verdicts_dir, "hook-123-456", true, false).unwrap();
        assert!(paths.verdicts_dir.join("hook-123-456.json").exists());
    }

    #[test]
    fn verdict_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let paths = temp_paths("perm");
        write_verdict_in(&paths.verdicts_dir, "hook-1-1", false, false).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&paths.verdicts_dir), 0o700);
        assert_eq!(mode(&paths.verdicts_dir.join("hook-1-1.json")), 0o600);
        assert_eq!(fs::read_to_string(paths.verdicts_dir.join("hook-1-1.json")).unwrap(), r#"{"behavior":"deny"}"#);
    }

    #[test]
    fn request_id_shape() {
        let rid = new_request_id();
        assert!(is_hook_request(&rid));
        assert_eq!(hook_request_pid(&rid), Some(std::process::id()));
        assert!(verdict_path_in(Path::new("/tmp"), &rid).is_some());
        assert_eq!(hook_request_pid("abc"), None);
    }

    #[test]
    fn answer_matching_request_writes_verdict_then_clears_pending() {
        let paths = temp_paths("answer-ok");
        let perm = PendingPermission { request_id: "hook-1-2".into(), tool_name: "Bash".into(), input_preview: "ls".into(), ..Default::default() };
        save_pending_in(&paths.pending_dir, SID, &perm).unwrap();
        answer_hook_request_in(&paths, SID, "hook-1-2", true, false).unwrap();
        assert_eq!(
            read_verdict(&paths.verdicts_dir.join("hook-1-2.json")),
            Some(Some(VerdictFile { behavior: Behavior::Allow, always: false }))
        );
        assert_eq!(load_pending_in(&paths.pending_dir, SID), None);
    }

    #[test]
    fn answer_stale_or_missing_request_is_error_without_verdict() {
        let paths = temp_paths("answer-stale");
        // Pending нет.
        assert!(answer_hook_request_in(&paths, SID, "hook-1-2", true, false).is_err());
        // Pending другого запроса остаётся нетронутым.
        let other = PendingPermission { request_id: "hook-3-4".into(), tool_name: "Bash".into(), input_preview: "x".into(), ..Default::default() };
        save_pending_in(&paths.pending_dir, SID, &other).unwrap();
        assert!(answer_hook_request_in(&paths, SID, "hook-1-2", false, false).is_err());
        assert_eq!(load_pending_in(&paths.pending_dir, SID), Some(other));
        assert!(!paths.verdicts_dir.exists());
    }


    fn suggestion(rules: Value, behavior: &str, destination: &str) -> Value {
        json!({ "type": "addRules", "rules": rules, "behavior": behavior, "destination": destination })
    }

    fn input_with(suggestions: Value) -> String {
        let mut v: Value = serde_json::from_str(&bash_input("rm -rf node_modules")).unwrap();
        v["permission_suggestions"] = suggestions;
        v.to_string()
    }

    #[test]
    fn remember_offer_keeps_only_add_rules_allow() {
        let good = suggestion(json!([{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }]), "allow", "localSettings");
        let all = json!([
            { "type": "setMode", "mode": "acceptEdits", "destination": "session" },
            suggestion(json!([{ "toolName": "Bash" }]), "deny", "localSettings"),
            { "type": "replaceRules", "rules": [{ "toolName": "Bash" }], "behavior": "allow", "destination": "session" },
            { "type": "addDirectories", "directories": ["/x"], "destination": "session" },
            good,
        ]);
        let (raw, shown) = remember_offer(&all);
        assert_eq!(raw, vec![good]);
        assert_eq!(
            shown,
            vec![RememberRule { rules: vec!["Bash(rm -rf node_modules)".into()], destination: "localSettings".into() }]
        );
        // Только отсеянное — кнопки нет; нет поля или не массив — тоже.
        assert_eq!(remember_offer(&json!([{ "type": "setMode", "mode": "plan", "destination": "session" }])), (vec![], vec![]));
        assert_eq!(remember_offer(&Value::Null), (vec![], vec![]));
        assert_eq!(remember_offer(&json!({ "type": "addRules" })), (vec![], vec![]));
    }

    #[test]
    fn remember_rule_format_with_and_without_content() {
        let s = suggestion(
            json!([{ "toolName": "mcp__claude-in-chrome__computer" }, { "toolName": "Bash", "ruleContent": "git status" }]),
            "allow",
            "userSettings",
        );
        let (_, shown) = remember_offer(&json!([s]));
        assert_eq!(shown[0].rules, vec!["mcp__claude-in-chrome__computer", "Bash(git status)"]);
        assert_eq!(shown[0].destination, "userSettings");
    }

    #[test]
    fn remember_offer_refuses_what_cannot_be_shown_honestly() {
        let many: Vec<Value> = (0..=MAX_REMEMBER_RULES).map(|i| json!({ "toolName": format!("T{i}") })).collect();
        assert_eq!(remember_offer(&json!([suggestion(json!(many), "allow", "session")])), (vec![], vec![]));
        let ten: Vec<Value> = many.into_iter().take(MAX_REMEMBER_RULES).collect();
        assert!(!remember_offer(&json!([suggestion(json!(ten), "allow", "session")])).0.is_empty());
        // Лимит общий на все элементы.
        let five: Vec<Value> = (0..6).map(|i| json!({ "toolName": format!("T{i}") })).collect();
        let two = json!([suggestion(json!(five), "allow", "session"), suggestion(json!(five), "allow", "localSettings")]);
        assert_eq!(remember_offer(&two), (vec![], vec![]));
        // Слишком длинное правило.
        let long = json!([{ "toolName": "Bash", "ruleContent": "x".repeat(MAX_RULE_CHARS) }]);
        assert_eq!(remember_offer(&json!([suggestion(long, "allow", "session")])), (vec![], vec![]));
        // Кривая форма: нет destination / toolName / пустой rules.
        for bad in [
            json!({ "type": "addRules", "behavior": "allow", "rules": [{ "toolName": "Bash" }] }),
            suggestion(json!([{ "ruleContent": "x" }]), "allow", "session"),
            suggestion(json!([]), "allow", "session"),
        ] {
            assert_eq!(remember_offer(&json!([bad])), (vec![], vec![]));
        }
    }

    #[test]
    fn pending_without_remember_serializes_as_before_and_old_file_reads() {
        let plain = PendingPermission { request_id: "hook-1-2".into(), tool_name: "Bash".into(), input_preview: "ls".into(), ..Default::default() };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"request_id":"hook-1-2","tool_name":"Bash","input_preview":"ls"}"#
        );
        let old: PendingPermission =
            serde_json::from_str(r#"{"request_id":"hook-1-2","tool_name":"Bash","input_preview":"ls"}"#).unwrap();
        assert_eq!(old, plain);
        let with = PendingPermission {
            remember: vec![RememberRule { rules: vec!["Bash(ls)".into()], destination: "session".into() }],
            ..plain
        };
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            r#"{"request_id":"hook-1-2","tool_name":"Bash","input_preview":"ls","remember":[{"rules":["Bash(ls)"],"destination":"session"}]}"#
        );
    }

    fn run_always(name: &str, suggestions: Value, allow: bool, always: bool) -> (Option<String>, PendingPermission) {
        let paths = temp_paths(name);
        with_serve(&paths);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen2 = seen.clone();
        let out = run_with_responder(&paths, input_with(suggestions), move |p, pending| {
            *seen2.lock().unwrap() = Some(pending.clone());
            // Как serve: сначала проверка, потом вердикт.
            answer_hook_request_in(p, SID, &pending.request_id, allow, always).unwrap();
        });
        let pending = seen.lock().unwrap().take().unwrap();
        (out, pending)
    }

    #[test]
    fn allow_always_echoes_selected_suggestions_verbatim() {
        let good = suggestion(json!([{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }]), "allow", "localSettings");
        let mode = json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" });
        let (out, pending) = run_always("always", json!([mode, good]), true, true);
        assert_eq!(pending.remember.len(), 1);
        let v: Value = serde_json::from_str(&out.expect("решение")).unwrap();
        assert_eq!(
            v["hookSpecificOutput"]["decision"],
            json!({ "behavior": "allow", "updatedPermissions": [good] })
        );
    }

    #[test]
    fn plain_allow_has_no_updated_permissions() {
        let good = suggestion(json!([{ "toolName": "Bash" }]), "allow", "session");
        let (out, pending) = run_always("plain-allow", json!([good]), true, false);
        assert_eq!(pending.remember.len(), 1);
        let v: Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
    }

    #[test]
    fn always_without_suggestions_is_plain_allow() {
        let (out, pending) = run_always("always-none", Value::Null, true, true);
        assert!(pending.remember.is_empty());
        let v: Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
    }

    #[test]
    fn always_with_deny_is_rejected_and_writes_nothing() {
        let paths = temp_paths("always-deny");
        assert!(write_verdict_in(&paths.verdicts_dir, "hook-1-1", false, true).is_err());
        let perm = PendingPermission { request_id: "hook-1-2".into(), tool_name: "Bash".into(), input_preview: "ls".into(), ..Default::default() };
        save_pending_in(&paths.pending_dir, SID, &perm).unwrap();
        assert!(answer_hook_request_in(&paths, SID, "hook-1-2", false, true).is_err());
        assert!(!paths.verdicts_dir.exists());
        assert_eq!(load_pending_in(&paths.pending_dir, SID), Some(perm));
        // Вердикт allow+always на диске выглядит так.
        write_verdict_in(&paths.verdicts_dir, "hook-1-3", true, true).unwrap();
        assert_eq!(
            fs::read_to_string(paths.verdicts_dir.join("hook-1-3.json")).unwrap(),
            r#"{"behavior":"allow","always":true}"#
        );
    }

    #[test]
    fn serve_pid_file_lifecycle() {
        let paths = temp_paths("pidfile");
        let guard = ServePidFile::create_at(paths.serve_pid.clone()).unwrap();
        assert!(serve_running(&paths.serve_pid));
        drop(guard);
        assert!(!paths.serve_pid.exists());
        // Файл другого serve Drop не трогает.
        let guard = ServePidFile::create_at(paths.serve_pid.clone()).unwrap();
        fs::write(&paths.serve_pid, "1").unwrap();
        drop(guard);
        assert!(paths.serve_pid.exists());
    }
}
