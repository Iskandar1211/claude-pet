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
   "has_channel":true,"pending_permission":null,
   "model":"claude-fable-5-2","model_name":"Fable 5.2","effort":"medium","permission_mode":"auto","subagents":2},
  {"session_id":"bbbbbbbb-…","short_id":"bbbbbbbb","entrypoint":"claude-vscode",
   "cwd":"/Users/me/other","status":"waiting","last_message":"assistant: [tool:Bash]",
   "has_channel":true,
   "pending_permission":{"request_id":"r-42","tool_name":"Bash","input_preview":"rm -rf build"},
   "subagents":0}
]}
```

- Мост опрашивает реестр раз в ~1 с (как `watch`) и пишет строку, **только если снимок изменился**.
- Снимок, а не дифф: сессий единицы, а снимок нельзя «рассинхронизировать» потерянным событием.
- `status`: `idle` | `working` | `waiting` | `done` | `error` (`waiting` = `WaitingPermission` в Rust).
- `pet_status` — статус питомца по всем сессиям, приоритет как у ChatGPT Pets: `waiting` → `error` → `done` → `working` → `idle`. Считается в мосте, чтобы правило жило в одном месте и покрывалось `cargo test`.
- Дополнение v1 (2026-11-11), для строки состояния чата («Fable 5.2 · Medium · Auto · 2 agents»), все поля необязательны для приложения:
  - `model` — сырой id основной сессии (`claude-fable-5-2`), `model_name` — имя для показа («Fable 5.2»; не разобрали id — он же). `effort` (`low`…`max`) и `permission_mode` (`auto`, `default`, `acceptEdits`, `plan`, …) — сырые строки. Пустые поля в строке **отсутствуют**.
  - Берутся из транскрипта основной сессии (не субагентов): модель и effort — из последней записи ассистента, режим — из последней записи пользователя. Значения появляются при первом чтении транскрипта сессии и обновляются со следующей записью в него; у только что начатой сессии их может не быть.
  - `subagents` — число работающих субагентов (`SubagentStart` без `SubagentStop`), всегда есть, `0` если нет. Обнуляется на `Stop`, `StopFailure`, `UserPromptSubmit`.
  - Все строковые значения — **недоверенный текст**: приложение показывает их только как текст.
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

`route`: `channel` | `inbox` | `hook`; `delivery`: `sent` | `confirmed` | `unconfirmed` (как у `pet-bridge send`). `hook` — ответ на `permission` для запроса hook'а (см. ниже), `delivery` в нём нет.

#### Запрос от hook'а `PermissionRequest`

`pending_permission` может прийти и от hook'а `PermissionRequest` (`pet-bridge hook permission`, только сессии расширения VS Code / Cursor): его `request_id` начинается с `hook-`, а у сессии при этом может не быть Channel (`has_channel:false`). Команда `permission` для такого id не требует Channel: мост проверяет, что pending сессии — ровно этот запрос (иначе `ok:false`, «запрос уже неактуален»), пишет файл вердикта `~/.claude-pet/verdicts/<request_id>.json`, только потом снимает pending и отвечает `route:"hook"`. Hook забирает вердикт и печатает решение Claude Code; диалог IDE остаётся, побеждает первый ответ.

**«Разрешать всегда»** (дополнение v1, `protocol` остаётся `1`). Если Claude Code прислал hook'у `permission_suggestions`, hook оставляет только элементы `type:"addRules"` с `behavior:"allow"` и кладёт их в `pending_permission` в виде для показа — необязательное поле `remember` (его нет, если показывать нечего):

```json
"pending_permission":{"request_id":"hook-4242-1700000000000000000","tool_name":"Bash","input_preview":"rm -rf node_modules",
  "remember":[{"rules":["Bash(rm -rf node_modules)"],"destination":"localSettings"}]}
```

`rules` — строки `Tool(ruleContent)` или `Tool`, `destination` — `session` | `localSettings` | `projectSettings` | `userSettings` (как прислал Claude Code; неизвестное значение приложение не показывает и кнопку не рисует). Тексты недоверенные. Больше 10 правил всего или правило длиннее 300 символов — поля нет (честно показать нельзя). Команда `permission` получает необязательное `"always":true` (по умолчанию `false`, старое приложение его не шлёт):

```json
{"id":"8","cmd":"permission","session_id":"s","request_id":"hook-4242-1700000000000000000","allow":true,"always":true}
```

Мост пишет вердикт `{"behavior":"allow","always":true}`, а hook добавляет в решение `updatedPermissions` — отобранные элементы дословно. Если предложений у hook'а нет, это обычный allow. `always` вместе с `allow:false` и для запросов Channel (без `hook-`) — ошибка `ok:false`, ничего не отправляется.

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

1. ~~Откуда брать `error`~~ — решено: из hook-события `StopFailure` (срабатывает вместо `Stop`, когда ход оборван ошибкой API; `log-event.sh` пишет в `events.jsonl` поле `error` — тип ошибки). Статус `error` снимается следующим `UserPromptSubmit` / `PreToolUse` / `PostToolUse` или `SessionEnd`; `Notification` его не затирает, `Stop` — заменяет на `done`. Pending-разрешение сессии при ошибке снимается. Причина — в `last_message` по-русски: «ошибка: лимит запросов» (`rate_limit`), «ошибка: сервер перегружен» (`overloaded`), «ошибка: нужна авторизация» (`authentication_failed`, `oauth_org_not_allowed`), «ошибка: проблема с оплатой» (`billing_error`, `account_on_hold`), иначе «ошибка API (<тип>)»; пока статус `error`, реплики транскрипта `last_message` не перезаписывают. Формат протокола не менялся. Ошибки API в транскрипте мост не разбирает.
2. ~~Нужен ли поток сообщений для Quick Chat~~ — решено: команда `history` (см. выше). Живой поток новых реплик не нужен: приложение перезапрашивает `history`, когда в снимке меняется `last_message`.
