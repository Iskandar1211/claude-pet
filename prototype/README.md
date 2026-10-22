# Этап 0 — проверка связи с Claude Code

## Что уже проверено

| Что | Терминал | Cursor / VS Code |
|---|---|---|
| Транскрипт в `~/.claude/projects/<проект>/<session>.jsonl` | по докам да | ✅ да, `entrypoint: "claude-vscode"` |
| Hooks срабатывают и знают `CLAUDE_CODE_MESSAGING_SOCKET` | по докам да | ✅ да (`.claude/settings.local.json` → `~/.claude-pet/events.jsonl`) |
| Inbox-сокет у сессии есть | по докам да | ✅ да, `/tmp/cc-socks/<pid>.sock` |
| Запись в сессию через inbox-сокет | тест A | тест A |
| Свой Channel (`--dangerously-load-development-channels`) | тест B | ❌ по докам нет: флаг игнорируется в SDK-режиме, а расширение работает через Agent SDK |

## Тест A — сообщение в сессию через inbox-сокет

1. Открой любую сессию Claude Code в этой папке (терминал или Cursor) и сделай любой запрос.
2. (Опционально) `session_id`: `tail -1 ~/.claude-pet/events.jsonl | jq -r .session_id` — или передай `last` в команде ниже.
3. В **отдельном** терминале:
   ```bash
   bridge/target/release/pet-bridge send last "привет от питомца, ответь одним словом"
   ```
   Вместо `last` можно подставить `session_id` или его префикс. (Раньше здесь был `prototype/inbox_send.py` — его заменил `pet-bridge send`.)
4. Запиши: дошло ли сообщение, как оно выглядит в сессии, ответил ли Claude и не было ли диалога одобрения.

## Тест B — свой Channel в терминале

```bash
cd prototype/channel
claude --dangerously-load-development-channels server:pet
# согласись на dev-channel и на MCP-сервер "pet"
```
Канал слушает случайный порт; смотри stderr процесса (`pet channel on 127.0.0.1:...`) или `cat ~/.claude-pet/channels/<pid>.json`.
Токен: `jq -r .channel_token ~/.claude-pet/channels/<pid>.json`

> Токен в аргументах `curl` виден другим пользователям Mac через `ps` — для отладки на своей машине допустимо, но на общей лучше передавать заголовок файлом (`curl -H @файл`).

Во втором терминале: `curl -N -H "X-Pet-Token: $(jq -r .channel_token ~/.claude-pet/channels/<pid>.json)" -H "Host: 127.0.0.1:<port>" http://127.0.0.1:<port>/events`
В третьем: `curl -H 'X-Pet-Token: …' -H 'Host: 127.0.0.1:<port>' -d 'что в этой папке?' http://127.0.0.1:<port>/`

Ожидается: в `/events` приходит `event: reply`. Если Claude спросит разрешение на `reply`, в `/events` придёт `event: permission` с `request_id`, и
`curl -H 'X-Pet-Token: …' -H 'Host: …' -d '{"request_id":"<id>","behavior":"allow"}' http://127.0.0.1:<port>/permission` закрывает диалог в терминале.

## Мост (шаг 2)

```bash
cd bridge && cargo build --release
./target/release/pet-bridge watch
```
