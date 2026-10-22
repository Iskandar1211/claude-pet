#!/bin/bash
# Hook для любого события Claude Code: дописывает событие в лог питомца.
# Добавляет то, чего нет в stdin hook'а: путь inbox-сокета и откуда сессия (терминал / vscode).
# Никогда не роняет сессию: любые ошибки глотаются, exit всегда 0.

DIR="${PET_DIR:-$HOME/.claude-pet}"
LOG="$DIR/events.jsonl"
umask 077
mkdir -p "$DIR"
chmod 700 "$DIR" 2>/dev/null
touch "$LOG" 2>/dev/null && chmod 600 "$LOG" 2>/dev/null

INPUT="$(cat)"
EVENT="$(jq -r '.hook_event_name // empty' <<<"$INPUT" 2>/dev/null)"

jq -c \
  --arg socket "${CLAUDE_CODE_MESSAGING_SOCKET:-}" \
  --arg entrypoint "${CLAUDE_CODE_ENTRYPOINT:-}" \
  --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{ts: $ts, event: .hook_event_name, session_id, cwd, transcript_path,
    tool: .tool_name, socket: $socket, entrypoint: $entrypoint,
    notification: .message}
   + (if (.notification_type | type) == "string" then {notification_type: .notification_type} else {} end)' <<<"$INPUT" >> "$LOG" 2>/dev/null

# Пропуск в inbox сессии: сокет + токен. Хранится отдельно от лога, доступ только владельцу.
# Пишем на любом событии, если файла ещё нет — так подхватываются и уже запущенные сессии.
SID="${CLAUDE_CODE_SESSION_ID:-}"
if [ -n "$SID" ] && [ -n "${CLAUDE_CODE_MESSAGING_TOKEN:-}" ]; then
  TOKENS="$DIR/tokens"
  FILE="$TOKENS/$SID.json"
  if [ "$EVENT" = "SessionEnd" ]; then
    rm -f "$FILE"
  else
    mkdir -p "$TOKENS" && chmod 700 "$TOKENS"
    NEED_WRITE=0
    if [ ! -f "$FILE" ]; then NEED_WRITE=1
    else
      OLD_SOCK="$(jq -r '.socket // empty' "$FILE" 2>/dev/null)"
      [ "$OLD_SOCK" != "${CLAUDE_CODE_MESSAGING_SOCKET:-}" ] && NEED_WRITE=1
    fi
    if [ "$NEED_WRITE" = 1 ]; then
      TMP="$FILE.tmp.$$"
      (umask 077; jq -n --arg socket "$CLAUDE_CODE_MESSAGING_SOCKET" --arg token "$CLAUDE_CODE_MESSAGING_TOKEN" \
        '{socket: $socket, token: $token}' > "$TMP") 2>/dev/null && mv -f "$TMP" "$FILE" 2>/dev/null
      chmod 600 "$FILE" 2>/dev/null
      rm -f "$TMP" 2>/dev/null
    fi
  fi
fi

exit 0
