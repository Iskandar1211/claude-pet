# Claude Pet

A desktop pet for [Claude Code](https://code.claude.com) on macOS. Anna — or one of her three friends — lives above your windows and shows what your Claude Code sessions are doing — working, waiting for approval, done — and a click opens a quick chat with the current session. Works with sessions in the terminal and in the VS Code / Cursor extension.

<p align="center">
  <img src="docs/anna-states.gif" alt="Anna's states: idle, working, needs approval, done, walking, picked up" width="760">
</p>

<p align="center">
  <img src="docs/characters.png" alt="The four characters: Anna, Kai, Lira and Nova" width="620">
  &nbsp;
  <img src="docs/pet-usage.png" alt="Anna working, with the 5-hour usage badge under her feet" width="110">
</p>

<p align="center"><sub>Pick a character in the paw menu · the 5-hour usage badge under the pet's feet</sub></p>

<p align="center">
  <img src="docs/quick-chat.png" alt="Quick Chat: the session list and the conversation with Markdown and tool calls" width="250">
  &nbsp;
  <img src="docs/approval.png" alt="Approval card: the full command with Allow and Deny buttons" width="250">
  &nbsp;
  <img src="docs/voice.png" alt="Voice mode: a warm glow above the input while the pet reads the reply aloud, with a Stop button" width="250">
</p>

<p align="center"><sub>Quick Chat, approvals and voice mode (sample data)</sub></p>

## What it does

- **Status at a glance.** Anna types on a laptop while Claude works, waves when a session needs your approval, cheers when it's done, and gets visibly annoyed with a red "error" badge under her feet when a turn was cut off by an API error (hover for the reason; it clears on your next request). With several sessions, the most urgent one wins: needs approval → error → a session that just finished → working → older finished ones.
- **Menu bar.** Every session with its folder, source (terminal / VS Code) and status; system notifications when a session needs approval or finishes a task that took 20 s or more; clicking one opens that session's chat.
- **Quick Chat.** Click Anna: the session's conversation as Claude Code shows it (full replies with Markdown, tool calls like `Bash(git status)`), a field to send a message, and Allow / Deny buttons for permission requests. Drag the window by its title bar and resize it from the edges. In VS Code / Cursor extension sessions the Allow / Deny buttons work through the `PermissionRequest` hook while the pet is running; the IDE's own dialog stays, and whichever answer comes first wins. In the terminal they work via `claude-pet`, as before. If you connected the pet before this hook existed, choose **Подключить** in the menu again to add the missing entry (other entries are left alone).
- **Voice** (macOS 26+). The 🎤 next to the send button dictates a message in Russian: the text appears in the field, you check it and send. When Claude finishes answering a dictated message, the pet reads the reply aloud — plain text only, code blocks, tables and tool calls are skipped. While you dictate, a warm glow above the field follows your voice; while the pet reads, it breathes, and **Стоп** (Stop) cuts it short. The pet uses the best Russian voice installed: for a more natural one, download *Milena (Enhanced)* in System Settings → Accessibility → Read & Speak → System voice → Manage Voices. Recognition runs on your Mac: the first dictation downloads Apple's Russian speech model (about a minute), audio never leaves the machine.
- **Usage.** The 5-hour subscription limit in a small badge under Anna's feet; hover for the reset time, click to re-read it right away. From 90% (fresh data, window not reset) the badge gets a warning mark, the tooltip says how long is left until the reset, and an idle Anna looks tired if the sprite sheet has a "tired" row. Only terminal sessions report it, so after the window resets the badge shows "—" until your next request in a terminal; the menu bar item shows the same status.
- Anna walks around when nothing is happening. Drop her anywhere and she stays there; throw her and she falls back to the bottom of the screen. She stays visible above full-screen apps and on every Space.
- **Characters.** Anna has three friends — Kai the fighter, Lira the bookworm and Nova the cyber girl. Pick one in the paw menu → **Персонаж** (Character); the switch is instant and remembered.

Everything runs locally. The app only talks to Claude Code on your machine.

> The app's interface is in Russian for now (labels are given below with translations).

## Requirements

- macOS 15 or later (voice input: macOS 26 or later)
- [Claude Code](https://code.claude.com)

That's all for the prebuilt app. Building from source needs more — see [Build from source](#build-from-source).

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Iskandar1211/claude-pet/main/install.sh | bash
```

The script downloads the prebuilt app for your Mac (Apple Silicon or Intel) from the [latest release](https://github.com/Iskandar1211/claude-pet/releases/latest), verifies its SHA-256 checksum (the install is aborted if it doesn't match), puts it in `~/Applications/Claude Pet.app` and opens it. No `sudo` is used. To update, run the same command again.

The app is **ad-hoc signed and not notarized** (there is no Apple Developer ID behind it). Files downloaded with `curl` don't get the quarantine flag, so macOS doesn't block the app; if you download the zip from the Releases page in a browser instead, Gatekeeper will complain and you'd have to allow it manually.

### Build from source

Needs Xcode Command Line Tools, Rust and `jq`; [Bun](https://bun.sh) is optional (only for replies and approvals from the pet chat — Channel).

- Xcode Command Line Tools — `xcode-select --install` (full Xcode is **not** needed)
- Rust — `curl https://sh.rustup.rs -sSf | sh` or `brew install rust`
- `jq` — built into macOS 15+; on macOS 14 `brew install jq` (building from source is the way to run the pet on macOS 14)
- [Bun](https://bun.sh) — *optional*, builds Channel into the app

```bash
git clone https://github.com/Iskandar1211/claude-pet.git
cd claude-pet
./build-from-source.sh
```

`build-from-source.sh` checks the requirements, builds the app and puts it in `~/Applications/Claude Pet.app`. The first build downloads Rust crates and takes a few minutes. You build it yourself, so macOS doesn't block it as a downloaded app.

### First launch

On first launch a welcome window offers two steps:

1. **Подключить к Claude Code** (Connect to Claude Code) — required. Adds the pet's hooks to `~/.claude/settings.json` (details below). Restart running Claude sessions to pick them up.
2. **Команда claude-pet** (the `claude-pet` command) — optional. Starts Claude Code with the pet's Channel, so you can reply and approve actions from the pet chat:

   ```bash
   claude-pet          # use instead of `claude` in a terminal
   ```

   If the command isn't found, add `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`.

You can reopen the window from the paw icon in the menu bar → **Настройка…** (Setup).

## What it changes on your Mac

| Where | What | How to undo |
|---|---|---|
| `~/.claude/settings.json` | 9 hook entries (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`, `PermissionRequest`) next to any existing hooks; the `PermissionRequest` one runs `~/.claude-pet/bin/pet-bridge hook permission` (below); `statusLine` replaced by a wrapper that records the 5-hour limit and then runs your previous status line | `./uninstall.sh` or the menu bar item «Подключён ко всем проектам ✓ — отключить…» — removes only the pet's entries and restores your status line |
| `~/.claude-pet/` | hook scripts, session event log, backups of every `settings.json` change | delete the folder |
| `~/.claude-pet/bin/pet-bridge` | a copy of the bridge binary for the `PermissionRequest` hook, so the entry in `settings.json` doesn't point inside the `.app` (refreshed on app start, only if that entry is installed) | `./uninstall.sh` (or delete the folder) |
| `~/.local/bin/claude-pet` | the optional launcher (an existing file of the same name is never overwritten) | `./uninstall.sh` |
| `~/Applications/Claude Pet.app` | the app | `./uninstall.sh` |

Before every change to `settings.json` the previous file is saved to `~/.claude-pet/backups/`. Note: the file is rewritten with keys in alphabetical order (same content).

### Security model

- Everything is local: no network ports are opened by the app; the optional Channel listens on `127.0.0.1` only, on a random port, guarded by a per-process token.
- Microphone and speech recognition access is asked once. The app's signature is tied to its bundle id (`local.claude-code-pet`) rather than to the binary hash, so a rebuild keeps the permission — and, like the tokens below, any app you build with that id gets it too.
- Tokens that let the pet send messages and approvals into your sessions live in `~/.claude-pet/` (mode `700`/`600`). **Any process running as your user can read them**, so it could send messages to your Claude sessions — the same trust level as your other dotfiles.
- Conversation text from transcripts is untrusted: links and images are stripped, and the approval card shows the full command. If a preview is incomplete, **Разрешить** (Allow) is disabled and you answer in the terminal. In the VS Code / Cursor extension a third button, **Разрешать всегда** (Always allow), appears when Claude Code itself suggests a permission rule; the rule and where it will be saved (this session, this project, or all projects) are shown above the buttons before you click.
- Backups in `~/.claude-pet/backups/` are copies of your `settings.json` and may contain secrets from its `env` section; delete them if you rotate keys.

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/Iskandar1211/claude-pet/main/uninstall.sh | bash
```

or `./uninstall.sh` from a clone of the repository.

## How it works

```
Claude Code ──hooks──▶ ~/.claude-pet/events.jsonl ─┐
     ▲   transcripts (~/.claude/projects/…) ───────┤
     │                                             ▼
     │   inbox socket / Channel (MCP)  ◀──  pet-bridge (Rust)  ──JSON lines──▶  Claude Pet.app (Swift, SpriteKit)
     └─────────────────────────────────────────────┘
```

- **Hooks** (`prototype/hooks/log-event.sh`) log session events; **`pet-bridge`** (`bridge/`) builds the session list and statuses from them and reads conversation history from Claude Code transcripts.
- **The app** (`app/`) starts `pet-bridge serve` as a child process and talks to it over stdin/stdout — no network ports. Protocol: [`bridge/PROTOCOL.md`](bridge/PROTOCOL.md).
- **Sending messages**: through the pet's [Channel](https://code.claude.com/docs/en/channels-reference) (`prototype/channel/`, compiled into the app with Bun) when the session was started with `claude-pet`, otherwise through Claude Code's per-session inbox socket.

## Limitations

- **Channels are a research preview** of Claude Code and need `--dangerously-load-development-channels`; `claude-pet` passes it for you. They work only in interactive terminal sessions — not in the VS Code / Cursor extension, so there the pet answers through the `PermissionRequest` hook instead (see Quick Chat); without that hook entry approvals stay in the IDE (the pet shows "waiting for approval in IDE").
- **The inbox socket format is undocumented** and may change with Claude Code updates. Messages sent this way appear in the session as coming from "another Claude session".
- **The 5-hour limit** is reported only by terminal sessions (the status line doesn't run in the IDE panel).
- The **error** status comes from the `StopFailure` hook (a turn ended by an API error). If you connected the pet before it existed, choose **Подключить** in the menu again to add the entry; the sprite sheets' `error` row is used if present.
- Tested with Claude Code 2.1.306 on macOS 26.

## Development

```bash
app/scripts/build_app.sh                    # build app/build/Claude Pet.app
open "app/build/Claude Pet.app"
cd bridge && cargo test                     # bridge tests
```

- `app/` — Swift (SwiftPM, no Xcode project): pet window, SpriteKit animations, Quick Chat (SwiftUI).
- `bridge/` — Rust: session registry, transcript reader, installer, `serve` protocol.
- `prototype/hooks/`, `prototype/channel/` — hook scripts and the Channel MCP server.
- Sprite sheets: `app/assets/<Name>_source.png` → `app/scripts/key_background.py <Name>` → `app/Resources/<Name>.png`. A sheet straight from an image generator first goes through `app/scripts/slice_sheet.py`, which re-packs the poses into the 4×7 grid of 392 px cells.
- README images (`docs/`): `app/scripts/make_gif.py` (Anna's states from the sprite sheet), `app/scripts/make_characters.py` (all characters in a row), `app/scripts/render_screenshots.sh` (Quick Chat, approvals and voice mode with sample data), `app/scripts/make_icon.py` (app icon).

## Credits

- Pet behavior and the sprite sheet format follow [WindowPet](https://github.com/SeakMengs/WindowPet) (MIT).
- Anna, Kai, Lira and Nova were generated with AI for this project; see [`app/ATTRIBUTION.md`](app/ATTRIBUTION.md).

## License

MIT
