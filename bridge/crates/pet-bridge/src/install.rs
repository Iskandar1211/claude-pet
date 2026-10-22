//! Подключение hooks питомца ко всем проектам: `~/.claude/settings.json` получает записи,
//! которые запускают `~/.claude-pet/hooks/log-event.sh`. Чужие hooks (например, Orca) не трогаются:
//! свои записи мы только добавляем рядом и при удалении снимаем только их — по пути к скрипту.
//! `statusLine` в settings.json один, поэтому её мы заменяем обёрткой `statusline.sh`: прежняя
//! `statusLine` сохраняется в `statusline-prev.json`, обёртка вызывает её, удаление её возвращает.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

/// События, которые нужны мосту (см. `hook_event.rs`); `true` — с `matcher: "*"` (события инструментов).
const EVENTS: [(&str, bool); 7] = [
    ("SessionStart", false),
    ("SessionEnd", false),
    ("UserPromptSubmit", false),
    ("PreToolUse", true),
    ("PostToolUse", true),
    ("Notification", false),
    ("Stop", false),
];

/// По этому хвосту пути узнаём свои записи среди чужих.
const SCRIPT_TAIL: &str = ".claude-pet/hooks/log-event.sh";
const STATUSLINE_TAIL: &str = ".claude-pet/hooks/statusline.sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HooksStatus {
    Installed,
    /// Часть событий без нашей записи или `statusLine` не наша — например, после ручной правки
    /// settings.json или если другая программа снова прописала свою `statusLine`.
    Partial,
    NotInstalled,
}

pub struct Paths {
    pub settings: PathBuf,
    pub script: PathBuf,
    pub statusline_script: PathBuf,
    /// Прежняя `statusLine` (объект из settings.json), которую вызывает наша обёртка.
    pub statusline_prev: PathBuf,
    pub backups: PathBuf,
}

impl Paths {
    /// Настоящие пути: `~/.claude/settings.json`, `~/.claude-pet/hooks`, `~/.claude-pet/backups`.
    pub fn user() -> Self {
        let pet = crate::paths::pet_dir();
        Self {
            settings: crate::paths::dirs_home().join(".claude").join("settings.json"),
            script: pet.join("hooks").join("log-event.sh"),
            statusline_script: pet.join("hooks").join("statusline.sh"),
            statusline_prev: pet.join("statusline-prev.json"),
            backups: pet.join("backups"),
        }
    }
}

pub fn hook_command(script: &Path) -> String {
    format!("\"{}\"", script.display())
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(SCRIPT_TAIL))
}

fn group_has_ours(group: &Value) -> bool {
    group.get("hooks").and_then(Value::as_array).is_some_and(|hooks| hooks.iter().any(is_ours))
}

fn statusline_is_ours(settings: &Value) -> bool {
    settings.get("statusLine").is_some_and(|s| s.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(STATUSLINE_TAIL)))
}

/// Статус только hooks; общий, вместе со `statusLine`, — `current_status`.
pub fn status(settings: &Value) -> HooksStatus {
    let hooks = settings.get("hooks");
    let present = EVENTS
        .iter()
        .filter(|(event, _)| {
            hooks
                .and_then(|h| h.get(*event))
                .and_then(Value::as_array)
                .is_some_and(|groups| groups.iter().any(group_has_ours))
        })
        .count();
    match present {
        0 => HooksStatus::NotInstalled,
        n if n == EVENTS.len() => HooksStatus::Installed,
        _ => HooksStatus::Partial,
    }
}

/// Добавляет нашу запись в каждое событие, где её нет. Возвращает, изменилось ли что-то.
pub fn add_hooks(settings: &mut Value, command: &str) -> Result<bool> {
    let root = settings.as_object_mut().context("settings.json: корень — не объект")?;
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks.as_object_mut().context("settings.json: `hooks` — не объект")?;
    let mut changed = false;
    for (event, with_matcher) in EVENTS {
        let groups = hooks.entry(event).or_insert_with(|| Value::Array(Vec::new()));
        let groups = groups.as_array_mut().with_context(|| format!("settings.json: `hooks.{event}` — не массив"))?;
        if groups.iter().any(group_has_ours) {
            continue;
        }
        let mut group = json!({ "hooks": [{ "type": "command", "command": command }] });
        if with_matcher {
            group["matcher"] = json!("*");
        }
        groups.push(group);
        changed = true;
    }
    Ok(changed)
}

/// Ставит нашу `statusLine`, сохранив прочие поля прежней (например, `padding`). Возвращает
/// `Some(прежняя)` при замене (`Null`, если её не было) и `None`, если наша уже стоит.
pub fn add_statusline(settings: &mut Value, command: &str) -> Result<Option<Value>> {
    if statusline_is_ours(settings) {
        return Ok(None);
    }
    let root = settings.as_object_mut().context("settings.json: корень — не объект")?;
    let prev = root.get("statusLine").cloned().unwrap_or(Value::Null);
    let mut ours = match &prev {
        Value::Object(fields) => fields.clone(),
        _ => Map::new(),
    };
    ours.insert("type".into(), json!("command"));
    ours.insert("command".into(), json!(command));
    root.insert("statusLine".into(), Value::Object(ours));
    Ok(Some(prev))
}

/// Возвращает прежнюю `statusLine` (`Null` — её не было), если стоит наша. Возвращает, изменилось ли.
pub fn remove_statusline(settings: &mut Value, prev: Value) -> bool {
    if !statusline_is_ours(settings) {
        return false;
    }
    let Some(root) = settings.as_object_mut() else { return false };
    match prev {
        Value::Null => root.remove("statusLine"),
        prev => root.insert("statusLine".into(), prev),
    };
    true
}

/// Снимает только наши записи; опустевшие группы, события и `hooks` убирает. Возвращает, изменилось ли.
pub fn remove_hooks(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for groups in hooks.values_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        for group in groups.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = list.len();
                list.retain(|h| !is_ours(h));
                changed |= list.len() != before;
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).map_or(true, |l| !l.is_empty()));
    }
    hooks.retain(|_, groups| groups.as_array().map_or(true, |g| !g.is_empty()));
    if hooks.is_empty() {
        if let Some(root) = settings.as_object_mut() {
            root.remove("hooks");
        }
    }
    changed
}

/// Читает settings.json; нет файла — пустой объект. Битый JSON — ошибка: такой файл не трогаем.
fn read_settings(path: &Path) -> Result<(Value, Option<Vec<u8>>)> {
    match fs::read(path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("{} — не JSON, ничего не меняю", path.display()))?;
            Ok((value, Some(bytes)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((json!({}), None)),
        Err(e) => Err(e).with_context(|| format!("чтение {}", path.display())),
    }
}

/// Бэкап исходных байтов и атомарная запись (tmp + rename). Симлинк (dotfiles) пишем в его цель.
fn write_settings(paths: &Paths, original: Option<&[u8]>, value: &Value) -> Result<Option<PathBuf>> {
    let backup = match original {
        Some(bytes) => {
            create_private_dir(&paths.backups).context("папка бэкапов")?;
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            // Два изменения за одну секунду (install → uninstall) не должны затирать бэкап оригинала.
            let path = (0..)
                .map(|n| match n {
                    0 => paths.backups.join(format!("settings.json.{stamp}")),
                    n => paths.backups.join(format!("settings.json.{stamp}-{n}")),
                })
                .find(|p| !p.exists())
                .expect("бесконечный ряд имён");
            // В settings.json бывают ключи в `env` — бэкап не должен читаться другими.
            write_private(&path, &String::from_utf8_lossy(bytes)).with_context(|| format!("бэкап {}", path.display()))?;
            Some(path)
        }
        None => None,
    };
    let target = fs::canonicalize(&paths.settings).unwrap_or_else(|_| paths.settings.clone());
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).with_context(|| format!("папка {}", dir.display()))?;
    }
    let tmp = target.with_extension("json.claude-pet-tmp");
    let mut text = serde_json::to_string_pretty(value).context("сериализация settings.json")?;
    text.push('\n');
    write_private(&tmp, &text).with_context(|| format!("запись {}", tmp.display()))?;
    if let Ok(meta) = fs::metadata(&target) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, &target).with_context(|| format!("замена {}", target.display()))?;
    Ok(backup)
}

pub struct Outcome {
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

/// Файл только для владельца (600) с момента создания: в нём команда, которую выполняет обёртка.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// Папка с правами 700 с момента создания (включая недостающие родительские).
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

fn write_script(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("запись {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("права {}", path.display()))?;
    }
    Ok(())
}

/// Кладёт скрипты (права 700), прописывает hooks и `statusLine`. Повторный вызов ничего не меняет.
pub fn install(paths: &Paths, script: &str, statusline_script: &str) -> Result<Outcome> {
    let (mut settings, original) = read_settings(&paths.settings)?;
    if !settings.is_object() {
        bail!("{}: корень — не объект, ничего не меняю", paths.settings.display());
    }
    let dir = paths.script.parent().context("путь скрипта")?;
    fs::create_dir_all(dir).context("папка hooks")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    write_script(&paths.script, script)?;
    write_script(&paths.statusline_script, statusline_script)?;
    let mut changed = add_hooks(&mut settings, &hook_command(&paths.script))?;
    if let Some(prev) = add_statusline(&mut settings, &hook_command(&paths.statusline_script))? {
        // До записи settings.json: обёртка сразу должна найти, кого вызывать.
        match prev {
            Value::Null => {
                let _ = fs::remove_file(&paths.statusline_prev);
            }
            prev => write_private(&paths.statusline_prev, &serde_json::to_string_pretty(&prev)?)
                .context("запись statusline-prev.json")?,
        }
        changed = true;
    }
    if !changed {
        return Ok(Outcome { changed: false, backup: None });
    }
    let backup = write_settings(paths, original.as_deref(), &settings)?;
    Ok(Outcome { changed: true, backup })
}

/// Снимает наши hooks (чужие остаются), возвращает прежнюю `statusLine` и удаляет скрипты.
pub fn uninstall(paths: &Paths) -> Result<Outcome> {
    let (mut settings, original) = read_settings(&paths.settings)?;
    let prev = match fs::read(&paths.statusline_prev) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("statusline-prev.json — не JSON, ничего не меняю")?,
        Err(_) => Value::Null,
    };
    let mut changed = remove_hooks(&mut settings);
    changed |= remove_statusline(&mut settings, prev);
    let backup = if changed { write_settings(paths, original.as_deref(), &settings)? } else { None };
    let _ = fs::remove_file(&paths.script);
    let _ = fs::remove_file(&paths.statusline_script);
    let _ = fs::remove_file(&paths.statusline_prev);
    Ok(Outcome { changed, backup })
}

pub fn current_status(paths: &Paths) -> Result<HooksStatus> {
    let settings = read_settings(&paths.settings)?.0;
    Ok(match (status(&settings), statusline_is_ours(&settings)) {
        (HooksStatus::Installed, false) | (HooksStatus::NotInstalled, true) => HooksStatus::Partial,
        (hooks, _) => hooks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "\"/Users/me/.claude-pet/hooks/log-event.sh\"";
    const STATUS_CMD: &str = "\"/Users/me/.claude-pet/hooks/statusline.sh\"";

    fn orca() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "PreToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "orca-hook.sh" }] }],
                "StopFailure": [{ "hooks": [{ "type": "command", "command": "orca-hook.sh" }] }]
            }
        })
    }

    #[test]
    fn add_keeps_foreign_hooks_and_is_idempotent() {
        let mut s = orca();
        assert_eq!(status(&s), HooksStatus::NotInstalled);
        assert!(add_hooks(&mut s, CMD).unwrap());
        assert_eq!(status(&s), HooksStatus::Installed);
        // Чужая запись на месте, наша — отдельной группой рядом.
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0]["hooks"][0]["command"], "orca-hook.sh");
        assert_eq!(pre[1], json!({ "matcher": "*", "hooks": [{ "type": "command", "command": CMD }] }));
        assert!(s["hooks"]["Stop"][0].get("matcher").is_none());
        assert_eq!(s["model"], "opus");
        // Второй раз — без изменений.
        assert!(!add_hooks(&mut s, CMD).unwrap());
    }

    #[test]
    fn remove_takes_only_ours() {
        let mut s = orca();
        add_hooks(&mut s, CMD).unwrap();
        assert!(remove_hooks(&mut s));
        assert_eq!(s, orca());
        assert!(!remove_hooks(&mut s));
        // Только наши hooks — после снятия ключа `hooks` не остаётся.
        let mut only = json!({ "model": "opus" });
        add_hooks(&mut only, CMD).unwrap();
        remove_hooks(&mut only);
        assert_eq!(only, json!({ "model": "opus" }));
    }

    #[test]
    fn partial_and_bad_shapes() {
        let mut s = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": CMD }] }] } });
        assert_eq!(status(&s), HooksStatus::Partial);
        assert!(add_hooks(&mut json!([]), CMD).is_err());
        assert!(add_hooks(&mut json!({ "hooks": [] }), CMD).is_err());
        assert!(add_hooks(&mut json!({ "hooks": { "Stop": {} } }), CMD).is_err());
        add_hooks(&mut s, CMD).unwrap();
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1, "дубль не добавляется");
    }

    #[test]
    fn statusline_wraps_previous_and_restores_it() {
        let orca_line = json!({ "type": "command", "command": "orca-statusline.sh", "padding": 0 });
        let mut s = orca();
        s["statusLine"] = orca_line.clone();
        let prev = add_statusline(&mut s, STATUS_CMD).unwrap();
        assert_eq!(prev, Some(orca_line.clone()));
        assert_eq!(s["statusLine"], json!({ "type": "command", "command": STATUS_CMD, "padding": 0 }));
        // Уже наша — прежнюю не перезаписываем своей же.
        assert_eq!(add_statusline(&mut s, STATUS_CMD).unwrap(), None);
        assert!(remove_statusline(&mut s, orca_line.clone()));
        assert_eq!(s["statusLine"], orca_line);
        // Чужую не снимаем.
        assert!(!remove_statusline(&mut s, Value::Null));

        // Не было `statusLine` — после снятия ключа нет.
        let mut none = orca();
        assert_eq!(add_statusline(&mut none, STATUS_CMD).unwrap(), Some(Value::Null));
        assert!(remove_statusline(&mut none, Value::Null));
        assert_eq!(none, orca());
    }

    #[test]
    fn install_and_uninstall_on_disk() {
        let dir = std::env::temp_dir().join(format!("pet-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = Paths {
            settings: dir.join(".claude/settings.json"),
            script: dir.join(".claude-pet/hooks/log-event.sh"),
            statusline_script: dir.join(".claude-pet/hooks/statusline.sh"),
            statusline_prev: dir.join(".claude-pet/statusline-prev.json"),
            backups: dir.join(".claude-pet/backups"),
        };
        fs::create_dir_all(dir.join(".claude")).unwrap();
        let mut before = orca();
        before["statusLine"] = json!({ "type": "command", "command": "orca-statusline.sh" });
        let original = serde_json::to_string_pretty(&before).unwrap();
        fs::write(&paths.settings, &original).unwrap();

        let out = install(&paths, "#!/bin/bash\n", "#!/bin/bash\n").unwrap();
        assert!(out.changed);
        assert_eq!(fs::read_to_string(out.backup.unwrap()).unwrap(), original);
        assert_eq!(current_status(&paths).unwrap(), HooksStatus::Installed);
        assert!(paths.script.exists() && paths.statusline_script.exists());
        let prev: Value = serde_json::from_str(&fs::read_to_string(&paths.statusline_prev).unwrap()).unwrap();
        assert_eq!(prev, before["statusLine"]);
        assert!(!install(&paths, "#!/bin/bash\n", "#!/bin/bash\n").unwrap().changed);

        // Другая программа вернула свою `statusLine` — статус «частично».
        let mut s: Value = serde_json::from_str(&fs::read_to_string(&paths.settings).unwrap()).unwrap();
        let ours = s["statusLine"].clone();
        s["statusLine"] = before["statusLine"].clone();
        fs::write(&paths.settings, s.to_string()).unwrap();
        assert_eq!(current_status(&paths).unwrap(), HooksStatus::Partial);
        s["statusLine"] = ours;
        fs::write(&paths.settings, s.to_string()).unwrap();

        let first_backup = fs::read_dir(&paths.backups).unwrap().count();
        let out = uninstall(&paths).unwrap();
        assert!(out.changed && out.backup.is_some());
        // Бэкап оригинала не затёрт, даже если обе операции в одну секунду.
        assert_eq!(fs::read_dir(&paths.backups).unwrap().count(), first_backup + 1);
        assert!(!paths.script.exists() && !paths.statusline_script.exists() && !paths.statusline_prev.exists());
        let after: Value = serde_json::from_str(&fs::read_to_string(&paths.settings).unwrap()).unwrap();
        assert_eq!(after, before);

        // Битый JSON не трогаем.
        fs::write(&paths.settings, "{ broken").unwrap();
        assert!(install(&paths, "#!/bin/bash\n", "#!/bin/bash\n").is_err());
        assert_eq!(fs::read_to_string(&paths.settings).unwrap(), "{ broken");
        fs::remove_dir_all(&dir).ok();
    }
}
