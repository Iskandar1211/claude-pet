# Протокол `pet-bridge serve` (v1, утверждён 2026-10-22)

Связь Swift-приложения (`app/`) с мостом. Приложение запускает `pet-bridge serve` дочерним процессом
и общается с ним через stdin/stdout. Сетевых портов и токенов между ними нет.

## Почему stdin/stdout

- Процессом владеет приложение: закрылось приложение → stdin закрылся → мост завершился. Сирот нет.
- Никто снаружи не может подключиться к мосту — в отличие от HTTP на localhost, где нужен свой токен.
- Токены Claude Code (`inbox_token`, `channel_token`) и пути к сокетам остаются внутри моста и никогда не уходят в приложение.

## Формат

JSON Lines: один JSON-объект на строку, UTF-8, `\n` в конце. Логи моста (`tracing`) — только в **stderr**, stdout — только протокол.

## Мост → приложение (stdout)

### `hello` — первая строка после запуска

```json
{"type":"hello","protocol":1}
```

Приложение сверяет `protocol`; при несовпадении показывает ошибку, а не гадает.

### `sessions` — полный снимок при любом изменении

```json
{"type":"sessions","pet_status":"waiting","sessions":[
  {"session_id":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","short_id":"aaaaaaaa","entrypoint":"cli",
   "cwd":"/Users/me/claude-code-pet","status":"working","last_message":"assistant: Сейчас посмотрю…",
   "has_channel":true,"pending_permission":null},
  {"session_id":"bbbbbbbb-…","short_id":"bbbbbbbb","entrypoint":"claude-vscode",
   "cwd":"/Users/me/other","status":"waiting","last_message":"assistant: [tool:Bash]",
   "has_channel":true,
   "pending_permission":{"request_id":"r-42","tool_name":"Bash","input_preview":"rm -rf build"}}
]}
```

- Мост опрашивает реестр раз в ~1 с (как `watch`) и пишет строку, **только если снимок изменился**.
- Снимок, а не дифф: сессий единицы, а снимок нельзя «рассинхронизировать» потерянным событием.
- `status`: `idle` | `working` | `waiting` | `done` | `error` (`waiting` = `WaitingPermission` в Rust).
- `pet_status` — статус питомца по всем сессиям, приоритет как у ChatGPT Pets: `waiting` → `error` → `done` → `working` → `idle`. Считается в мосте, чтобы правило жило в одном месте и покрывалось `cargo test`.
- **Нет** полей `transcript_path`, `inbox_socket`, `inbox_token`, `channel_port`, `channel_token`.
- `last_message` и `pending_permission.input_preview` — **недоверенный текст** из сессии: приложение показывает его только как текст.

### `reply` — быстрый ответ через Channel (`mcp__pet__reply`)

```json
{"type":"reply","session_id":"aaaaaaaa-…","text":"Готово, тесты зелёные"}
```

Необязательный ускоритель: тот же текст позже придёт и в `last_message` из транскрипта (`reply` ненадёжен). **Пока не отправляется** — понадобится Quick Chat (шаг 5); до тех пор ответы приходят только через `last_message`.

### `result` — ответ на команду (по `id`)

```json
{"type":"result","id":"7","ok":true,"route":"inbox","delivery":"confirmed"}
{"type":"result","id":"8","ok":false,"error":"у сессии нет Channel (запусти claude-pet)"}
```

`route`: `channel` | `inbox`; `delivery`: `sent` | `confirmed` | `unconfirmed` (как у `pet-bridge send`).

### `history` — последние реплики сессии (дополнение v1 от 2026-10-22)

Ответ на команду `history`; `protocol` в `hello` остаётся `1` — старое приложение просто не шлёт эту команду.

```json
{"type":"history","id":"9","session_id":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","messages":[
  {"role":"user","kind":"text","text":"запусти тесты"},
  {"role":"assistant","kind":"tool","name":"Bash","summary":"cargo test --offline"},
  {"role":"tool","kind":"result","text":"running 35 tests\ntest result: ok. 35 passed","more_lines":12},
  {"role":"assistant","kind":"text","text":"Тесты зелёные.\n\n**Итог:** …"}]}
```

- `messages` — последние N записей в хронологическом порядке (старые первыми). Вид записи — `kind`, как в терминале Claude Code (изменено 2026-10-22, раньше был однострочный `text` с `[tool:Name]`):
  - `text` — реплика `user` / `assistant` **целиком**, с переводами строк; длиннее 20 000 символов — обрезается с пометкой «(сообщение обрезано)».
  - `tool` — вызов инструмента: `name` и `summary` — главный параметр, как в скобках у Claude Code (`Bash(команда)`, `Read(путь)`, `Grep(шаблон)`; у неизвестного инструмента — первое строковое поле входа), одна строка до 160 символов.
  - `result` — результат инструмента (`role`: `tool`): первые 4 непустые строки (каждая до 300 символов) и `more_lines` — сколько скрыто; `is_error: true` — только если инструмент вернул ошибку.
  - Пустые поля не выводятся. `thinking`, картинки, служебные строки и sidechain (субагенты) пропускаются.
- `session_id` в ответе — полный (даже если в запросе был короткий префикс).
- Мост читает транскрипт с конца окном 256 КиБ, удваивая до 8 МиБ, пока не наберётся N реплик, — файл целиком в память не грузится. Если реплик в последних 8 МиБ меньше N, вернётся меньше.
- Ошибка (нет сессии, нет транскрипта, файл не читается) → `result` с `ok:false`.
- Тексты — **недоверенные**, как и `last_message`.

### `usage` — 5-часовой лимит подписки (дополнение v1 от 2026-10-22)

```json
{"type":"usage","five_hour_pct":93.0,"resets_at":1792621200,"updated_at":1792613444}
```

- Источник — `~/.claude-pet/usage.json`. Его пишет обёртка statusLine (`prototype/hooks/statusline.sh`, ставится вместе с hooks) из `rate_limits`, которые Claude Code передаёт status line. Прежняя `statusLine` (например, Orca) сохраняется в `statusline-prev.json` и вызывается обёрткой.
- Status line работает только в терминальных сессиях, в панели VS Code/Cursor её нет. Лимит общий на аккаунт, поэтому данные обновляет любая терминальная сессия. `updated_at` показывает, насколько они свежие.
- Время — unix-секунды. Событие приходит при запуске, если файл есть, и потом при каждом изменении. Без подписки (API-ключ) `rate_limits` нет, и события нет.

## Приложение → мост (stdin)

У каждой команды строковый `id`, ответ — `result` с тем же `id`.

```json
{"id":"7","cmd":"send","session_id":"aaaaaaaa-…","text":"запусти тесты"}
{"id":"8","cmd":"permission","session_id":"bbbbbbbb-…","request_id":"r-42","allow":true}
{"id":"9","cmd":"history","session_id":"aaaaaaaa-…","limit":20}
```

- `send` — то же, что `pet-bridge send` (Channel, иначе inbox с токеном).
- `permission` — то же, что `pet-bridge approve`; после успеха мост снимает pending, и следующий `sessions` уже без него.
- `history` (дополнение от 2026-10-22) — последние `limit` записей чата (целое ≥ 0, мост режет до 100); ответ — `history`, не `result`.
- Неизвестная команда или битый JSON → `result` с `ok:false` (если `id` удалось прочитать) и строка в stderr; мост не падает.
- EOF на stdin → мост завершается с кодом 0.

## Жизненный цикл в приложении

- `pet-bridge` кладётся внутрь `.app` (`Contents/MacOS/pet-bridge`) скриптом сборки — без зависимости от PATH.
- Мост упал → приложение перезапускает его с паузой 1 → 2 → 4 … 30 с; пока моста нет, питомец в `idle`, в меню — «мост не запущен».

## Открытые вопросы

1. **Откуда брать `error`.** Сейчас мост такой статус не выставляет никогда. Кандидаты: hook-события об ошибке/сбое остановки и ошибки API в транскрипте — нужно проверить, какие события Claude Code реально пишет (`events.jsonl`) при сбое. До этого `error` в протоколе есть, но не приходит.
2. ~~Нужен ли поток сообщений для Quick Chat~~ — решено: команда `history` (см. выше). Живой поток новых реплик не нужен: приложение перезапрашивает `history`, когда в снимке меняется `last_message`.
