# pet-bridge

Локальный мост Claude Pet: реестр сессий из `~/.claude-pet/events.jsonl`, чтение транскриптов, токены inbox и регистрация Channel, протокол `serve` для приложения, hook `PermissionRequest` для VS Code / Cursor.

Полный контракт stdin/stdout с приложением: [`PROTOCOL.md`](PROTOCOL.md).

## Сборка

```bash
cd bridge
cargo build --release
```

### Если rustup падает с `could not rename ... .partial`

Обычно это прерванная установка или **два `rustup` одновременно**. Останови другие установки, затем один раз:

```bash
pkill -f rustup || true
rm -rf ~/.rustup/toolchains/stable-* ~/.rustup/downloads/*.partial
rustup toolchain install stable
rustup default stable
rustc --version
```

Альтернатива: `brew install rust` (отдельный toolchain, без rustup).

Бинарь: `bridge/target/release/pet-bridge` (в `.app` — `Contents/MacOS/pet-bridge`).

## CLI

```bash
pet-bridge list
pet-bridge watch
pet-bridge send <session_id|prefix|last> "текст"
pet-bridge approve <session_id> yes
pet-bridge hooks install      # записывает hooks питомца в ~/.claude/settings.json
pet-bridge hooks uninstall
pet-bridge serve              # JSON Lines на stdout — для Claude Pet.app
pet-bridge hook permission    # stdin JSON от Claude Code PermissionRequest hook
```

`approve` подхватывает `request_id` из `~/.claude-pet/pending/<session>.json`, если перед этим работал `watch` (SSE Channel). Иначе укажи `--request-id <id>`.

```bash
pet-bridge approve <session_id> yes --request-id <id>
```

### `serve`

Дочерний процесс приложения: читает команды (`send`, `permission`, `history`) из stdin, шлёт `hello`, `sessions`, `usage`, `history`, `result` в stdout. См. [`PROTOCOL.md`](PROTOCOL.md).

### `hook permission`

Вызывается из `~/.claude/settings.json` (копия бинаря в `~/.claude-pet/bin/pet-bridge`, обновляется при старте приложения). Только сессии расширения (`entrypoint: claude-vscode`). Пишет pending, ждёт вердикт в `~/.claude-pet/verdicts/<request_id>.json` из Quick Chat; по таймауту или если питомец не запущен — выходит без решения (остаётся диалог IDE). В терминале approvals по-прежнему через Channel / `claude-pet`.

Запись в сессию: если для pid из inbox-сокета есть `~/.claude-pet/channels/<pid>.json` (Channel из `claude-pet`), идёт HTTP на `127.0.0.1:<port>`, иначе inbox unix-сокет с токеном.

Токен Channel — свой на каждый процесс Claude: `pet-channel.ts` генерирует случайный `channel_token` и пишет его в `channels/<pid>.json` вместе с `port`. Мост берёт токен оттуда (HTTP POST и SSE `/events`); только если поля нет (старый Channel) — используется глобальный `channel_token` из `~/.claude-pet/config.json`.

## Обслуживание `~/.claude-pet`

При загрузке событий мост обрезает `events.jsonl` с ~8 МиБ до хвоста ~1 МиБ. Примерно раз в час удаляет токены сессий, которых больше нет в реестре.

## Решения

- Язык демона: **Rust** (библиотека `pet-bridge`; приложение — Swift + `serve`).
- Привязка Channel к сессии: **pid** из пути `/tmp/cc-socks/<pid>.sock` ↔ `channels/<pid>.json`.
- Статусы, pending, модель/effort/режим и счётчик субагентов — в реестре; снимок для UI — одна строка `sessions` в протоколе v1.
