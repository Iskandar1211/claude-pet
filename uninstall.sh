#!/bin/bash
# Удаление Claude Pet: снимает hooks питомца и прежнюю statusLine возвращает (pet-bridge hooks uninstall),
# удаляет команду claude-pet и само приложение. ~/.claude-pet (бэкапы settings.json, логи) остаётся.
set -euo pipefail

APP="$HOME/Applications/Claude Pet.app"
BRIDGE="$APP/Contents/MacOS/pet-bridge"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }

osascript -e 'quit app "Claude Pet"' >/dev/null 2>&1 || true

if [[ -x "$BRIDGE" ]]; then
  say "Removing the pet's hooks from ~/.claude/settings.json (other hooks stay)"
  "$BRIDGE" hooks uninstall
  "$BRIDGE" launcher uninstall
else
  say "$APP not found — skipping hooks cleanup (use the menu bar item if the app lives elsewhere)."
fi

rm -rf "$APP"
say "Removed. ~/.claude-pet stays: backups of settings.json (may contain secrets from its env section), tokens and logs. Delete it with: rm -rf ~/.claude-pet"
