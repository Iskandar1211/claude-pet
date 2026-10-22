#!/bin/bash
# statusLine Claude Code: сохраняет лимиты подписки (rate_limits) для питомца в usage.json
# и передаёт stdin прежней команде statusLine (например, Orca) — её вывод и есть строка статуса.
# Прежнюю команду установщик кладёт в statusline-prev.json. Ошибки питомца строку не ломают.

DIR="${PET_DIR:-$HOME/.claude-pet}"
INPUT="$(cat)"

LIMITS="$(jq -c --argjson now "$(date +%s)" 'select(.rate_limits) | {updated_at: $now, rate_limits}' <<<"$INPUT" 2>/dev/null)"
if [ -n "$LIMITS" ]; then
  umask 077
  printf '%s\n' "$LIMITS" >"$DIR/usage.json.tmp" 2>/dev/null && mv "$DIR/usage.json.tmp" "$DIR/usage.json" 2>/dev/null
fi

PREV="$(jq -r '.command // empty' "$DIR/statusline-prev.json" 2>/dev/null)"
if [ -n "$PREV" ]; then
  printf '%s' "$INPUT" | sh -c "$PREV"
fi
