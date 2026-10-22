//! Лимиты подписки из `~/.claude-pet/usage.json`: его пишет обёртка statusLine
//! (prototype/hooks/statusline.sh) из `rate_limits`, которые Claude Code отдаёт status line.

use std::path::Path;

use serde_json::Value;

/// 5-часовой лимит. Время — unix-секунды.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub five_hour_pct: f64,
    pub resets_at: i64,
    /// Когда status line последний раз прислала данные — без терминальной сессии они не обновляются.
    pub updated_at: i64,
}

pub fn parse_usage(text: &str) -> Option<Usage> {
    let v: Value = serde_json::from_str(text).ok()?;
    let five = v.get("rate_limits")?.get("five_hour")?;
    Some(Usage {
        five_hour_pct: five.get("used_percentage")?.as_f64()?,
        resets_at: five.get("resets_at")?.as_i64()?,
        updated_at: v.get("updated_at")?.as_i64()?,
    })
}

/// Нет файла или он не того вида — `None`.
pub fn read_usage(path: &Path) -> Option<Usage> {
    parse_usage(&std::fs::read_to_string(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_statusline_file() {
        let text = r#"{"updated_at":1792613444,"rate_limits":{"five_hour":{"used_percentage":93,"resets_at":1792621200},"seven_day":{"used_percentage":62,"resets_at":1792857600}}}"#;
        assert_eq!(
            parse_usage(text),
            Some(Usage { five_hour_pct: 93.0, resets_at: 1792621200, updated_at: 1792613444 })
        );
        // API-ключ без подписки: `rate_limits` нет или в нём нет `five_hour`.
        assert_eq!(parse_usage(r#"{"updated_at":1,"rate_limits":{}}"#), None);
        assert_eq!(parse_usage("{ broken"), None);
    }
}
