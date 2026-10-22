# pet-bridge (шаг 2)

Локальный мост: реестр сессий из `~/.claude-pet/events.jsonl`, токенов inbox и регистрации Channel.

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

Бинарь: `bridge/target/release/pet-bridge`.

## CLI

```bash
pet-bridge list
pet-bridge watch
pet-bridge send <session_id|prefix|last> "текст"
pet-bridge approve <session_id> yes
```

`approve` подхватывает `request_id` из `~/.claude-pet/pending/<session>.json`, если перед этим работал `watch` (SSE Channel). Иначе укажи `--request-id <id>`.

```bash
pet-bridge approve <session_id> yes --request-id <id>
```

Запись в сессию: если для pid из inbox-сокета есть `~/.claude-pet/channels/<pid>.json` (Channel из `claude-pet`), идёт HTTP на `127.0.0.1:<port>`, иначе inbox unix-сокет с токеном.

Токен Channel — свой на каждый процесс Claude: `pet-channel.ts` генерирует случайный `channel_token` и пишет его в `channels/<pid>.json` вместе с `port`. Мост берёт токен оттуда (HTTP POST и SSE `/events`); только если поля нет (старый Channel) — используется глобальный `channel_token` из `~/.claude-pet/config.json`.

## Решения

- Язык демона: **Rust** (библиотека `pet-bridge` для Tauri на шаге 3).
- Привязка Channel к сессии: **pid** из пути `/tmp/cc-socks/<pid>.sock` ↔ `channels/<pid>.json`.
