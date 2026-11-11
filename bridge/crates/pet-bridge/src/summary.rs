//! Сводка дня по `events.jsonl`: сессии, завершённые задачи и время работы Claude.
//!
//! Файл читается построчно. Считаются только события Claude Code (регистр имени важен,
//! чужие `stop` / `preToolUse` от Cursor игнорируются), битые строки пропускаются.

use std::collections::{HashMap, HashSet};
use std::io::BufRead;

use serde::Deserialize;

use crate::hook_event::HookEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Разные сессии, где в интервале был хотя бы один `UserPromptSubmit`.
    pub sessions: u32,
    /// Пары `UserPromptSubmit` → ближайший следующий `Stop` в той же сессии.
    pub tasks: u32,
    /// Сумма интервалов «промпт → Stop» по всем сессиям (машинное время, параллельные складываются).
    pub work_seconds: u64,
    /// Самое раннее событие Claude Code в файле позже `since`: начала дня в логе, возможно, нет
    /// (мост обрезает лог до последнего ~1 МиБ). Вчерашние события доказывают, что начало на месте.
    pub partial: bool,
}

#[derive(Deserialize)]
struct Row {
    ts: String,
    #[serde(flatten)]
    ev: HookEvent,
}

#[derive(Default)]
struct Open {
    /// Время открытого `UserPromptSubmit`, на который ещё не пришёл `Stop`.
    start: Option<i64>,
    /// Время последнего события сессии: им закрывается интервал без `Stop`.
    last: i64,
}

/// Считает сводку за `[since, ∞)`. Задача, чей `UserPromptSubmit` раньше `since`
/// (началась вчера, закончилась сегодня), не учитывается целиком — считаем только
/// интервалы, чей промпт `>= since`.
pub fn summarize(reader: impl BufRead, since: i64) -> Summary {
    let mut out = Summary::default();
    let mut sessions = HashSet::new();
    let mut state: HashMap<String, Open> = HashMap::new();
    let mut earliest: Option<i64> = None;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(row) = serde_json::from_str::<Row>(&line) else { continue };
        if !row.ev.is_claude_code() {
            continue;
        }
        let Some(ts) = parse_ts(&row.ts) else { continue };
        earliest = Some(earliest.map_or(ts, |e| e.min(ts)));
        if ts < since {
            continue;
        }
        let s = state.entry(row.ev.session_id.clone()).or_default();
        match row.ev.event.as_str() {
            "UserPromptSubmit" => {
                if let Some(start) = s.start {
                    out.work_seconds += (s.last - start).max(0) as u64;
                }
                s.start = Some(ts);
                sessions.insert(row.ev.session_id);
            }
            "Stop" => {
                if let Some(start) = s.start.take() {
                    out.work_seconds += (ts - start).max(0) as u64;
                    out.tasks += 1;
                }
            }
            _ => {}
        }
        s.last = s.last.max(ts);
    }
    for s in state.values() {
        if let Some(start) = s.start {
            out.work_seconds += (s.last - start).max(0) as u64;
        }
    }
    out.sessions = sessions.len() as u32;
    out.partial = earliest.is_some_and(|e| e > since);
    out
}

/// Сводка по файлу событий; нет файла — нули.
pub fn summarize_file(path: &std::path::Path, since: i64) -> std::io::Result<Summary> {
    match std::fs::File::open(path) {
        Ok(f) => Ok(summarize(std::io::BufReader::new(f), since)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Summary::default()),
        Err(e) => Err(e),
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` → unix-секунды (UTC).
pub fn parse_ts(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // Дни от гражданской даты (алгоритм Говарда Хиннанта).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: &str, ev: &str, sid: &str) -> String {
        format!(r#"{{"ts":"{t}","event":"{ev}","session_id":"{sid}"}}"#)
    }

    fn run(lines: &[String], since: &str) -> Summary {
        summarize(lines.join("\n").as_bytes(), parse_ts(since).unwrap())
    }

    fn l(hms: &str, ev: &str, sid: &str) -> String {
        line(&format!("2026-11-11T{hms}Z"), ev, sid)
    }

    const S: &str = "2026-11-11T09:00:00Z";

    #[test]
    fn parse_ts_basic_and_edges() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-11-11T14:04:26Z"), Some(1_794_405_866));
        // граница года
        let a = parse_ts("2025-12-31T23:59:59Z").unwrap();
        assert_eq!(parse_ts("2026-01-01T00:00:00Z"), Some(a + 1));
        // високосный день
        let f = parse_ts("2024-02-28T00:00:00Z").unwrap();
        assert_eq!(parse_ts("2024-02-29T00:00:00Z"), Some(f + 86400));
        assert_eq!(parse_ts("2024-03-01T00:00:00Z"), Some(f + 2 * 86400));
        // 1900 не високосный, 2000 — високосный
        assert_eq!(parse_ts("2000-03-01T00:00:00Z").unwrap() - parse_ts("2000-02-28T00:00:00Z").unwrap(), 2 * 86400);
        assert_eq!(parse_ts("2026-11-11 14:04:26"), None);
        assert_eq!(parse_ts("2026-13-11T14:04:26Z"), None);
    }

    #[test]
    fn one_session_prompt_stop() {
        let r = run(&[l("10:00:00", "UserPromptSubmit", "a"), l("10:05:00", "PostToolUse", "a"), l("10:10:00", "Stop", "a")], S);
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 600));
    }

    #[test]
    fn double_stop_counts_once() {
        let r = run(&[l("10:00:00", "UserPromptSubmit", "a"), l("10:01:00", "Stop", "a"), l("10:02:00", "Stop", "a")], S);
        assert_eq!((r.tasks, r.work_seconds), (1, 60));
    }

    #[test]
    fn prompt_without_stop_closes_at_last_event() {
        let r = run(&[l("10:00:00", "UserPromptSubmit", "a"), l("10:03:00", "PostToolUse", "a")], S);
        assert_eq!((r.tasks, r.work_seconds), (0, 180));
    }

    #[test]
    fn stop_failure_closes_interval_but_is_not_a_finished_task() {
        let r = run(&[l("10:00:00", "UserPromptSubmit", "a"), l("10:04:00", "StopFailure", "a")], S);
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 0, 240));
        // Повторный запрос после сбоя: первый интервал закрыт сбоем, второй — обычным Stop.
        let r = run(
            &[
                l("10:00:00", "UserPromptSubmit", "a"),
                l("10:04:00", "StopFailure", "a"),
                l("10:10:00", "UserPromptSubmit", "a"),
                l("10:11:00", "Stop", "a"),
            ],
            S,
        );
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 300));
    }

    #[test]
    fn new_prompt_before_stop_closes_at_last_event_before_it() {
        let r = run(
            &[
                l("10:00:00", "UserPromptSubmit", "a"),
                l("10:02:00", "PostToolUse", "a"),
                l("10:05:00", "UserPromptSubmit", "a"),
                l("10:06:00", "Stop", "a"),
            ],
            S,
        );
        // первый: 10:00..10:02 (без задачи), второй: 10:05..10:06 (задача)
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 180));
    }

    #[test]
    fn parallel_sessions_add_up() {
        let r = run(
            &[
                l("10:00:00", "UserPromptSubmit", "a"),
                l("10:00:00", "UserPromptSubmit", "b"),
                l("10:01:00", "Stop", "a"),
                l("10:02:00", "Stop", "b"),
            ],
            S,
        );
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (2, 2, 180));
    }

    #[test]
    fn events_before_since_ignored() {
        let r = run(
            &[
                l("08:00:00", "UserPromptSubmit", "a"),
                l("08:10:00", "Stop", "a"),
                l("08:55:00", "UserPromptSubmit", "b"),
                l("09:05:00", "Stop", "b"), // задача началась до since — не считается
                l("10:00:00", "UserPromptSubmit", "c"),
                l("10:01:00", "Stop", "c"),
            ],
            S,
        );
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 60));
    }

    #[test]
    fn foreign_events_ignored() {
        let r = run(
            &[
                l("10:00:00", "beforeSubmitPrompt", "x"),
                l("10:00:01", "preToolUse", "x"),
                l("10:00:02", "stop", "x"),
                l("10:00:00", "UserPromptSubmit", "a"),
                l("10:01:00", "stop", "a"),
                l("10:02:00", "Stop", "a"),
            ],
            S,
        );
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 120));
    }

    #[test]
    fn broken_lines_skipped() {
        let r = run(
            &["{oops".into(), "".into(), r#"{"ts":"bad","event":"Stop","session_id":"a"}"#.into(),
              l("10:00:00", "UserPromptSubmit", "a"), l("10:01:00", "Stop", "a")],
            S,
        );
        assert_eq!((r.sessions, r.tasks, r.work_seconds), (1, 1, 60));
    }

    #[test]
    fn partial_rule() {
        // самое раннее событие позже since, ничего раньше — неполные данные
        assert!(run(&[l("10:00:00", "UserPromptSubmit", "a")], S).partial);
        // вчерашнее событие доказывает, что начало дня в файле есть
        let y = line("2026-11-10T20:00:00Z", "Stop", "a");
        assert!(!run(&[y, l("10:00:00", "UserPromptSubmit", "a")], S).partial);
        // событие ровно в since
        assert!(!run(&[l("09:00:00", "UserPromptSubmit", "a")], S).partial);
        // чужое событие не считается началом
        let foreign = line("2026-11-10T20:00:00Z", "stop", "a");
        assert!(run(&[foreign, l("10:00:00", "UserPromptSubmit", "a")], S).partial);
    }

    #[test]
    fn empty_and_missing_file() {
        assert_eq!(run(&[], S), Summary::default());
        let p = std::env::temp_dir().join("pet-summary-definitely-missing.jsonl");
        assert_eq!(summarize_file(&p, 0).unwrap(), Summary::default());
    }
}
