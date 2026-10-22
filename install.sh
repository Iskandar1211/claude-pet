#!/bin/bash
# Сборка и установка Claude Pet из исходников: проверяет зависимости, собирает .app
# (app/scripts/build_app.sh), кладёт в ~/Applications и запускает. Подключение к Claude Code
# делает окно приветствия в самом приложении — с согласия пользователя.
# Собранное локально приложение не помечается как скачанное, поэтому Gatekeeper его не блокирует.
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="Claude Pet.app"
DEST="$HOME/Applications"
BUNDLE_ID="local.claude-code-pet"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[31mError:\033[0m %s\n' "$*" >&2; exit 1; }

[[ "$(uname)" == "Darwin" ]] || fail "Claude Pet runs on macOS only."

# Обязательные зависимости: без них не собрать или питомец ничего не увидит.
missing=()
xcode-select -p >/dev/null 2>&1 && command -v swift >/dev/null || missing+=("Xcode Command Line Tools:  xcode-select --install")
command -v cargo >/dev/null || missing+=("Rust:  curl https://sh.rustup.rs -sSf | sh   (or: brew install rust)")
command -v jq >/dev/null || missing+=("jq (built into macOS 15+):  brew install jq")
if ((${#missing[@]})); then
  printf '\033[31mMissing requirements:\033[0m\n' >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 1
fi
command -v bun >/dev/null || say "Bun not found — building without Channel (replies and approvals from the pet chat). Get it at https://bun.sh and re-run to add it."

# У части сетей cargo зависает на HTTP/2-мультиплексировании при загрузке crates.io.
export CARGO_HTTP_MULTIPLEXING="${CARGO_HTTP_MULTIPLEXING:-false}"

say "Building (first build downloads Rust crates and takes a few minutes)…"
app/scripts/build_app.sh

say "Installing to $DEST/$APP_NAME"
osascript -e 'quit app "Claude Pet"' >/dev/null 2>&1 || true
mkdir -p "$DEST"
rm -rf "${DEST:?}/$APP_NAME"
ditto "app/build/$APP_NAME" "$DEST/$APP_NAME"

# Новая установка — снова показать окно приветствия, если питомец ещё не подключён.
defaults delete "$BUNDLE_ID" onboardingShown >/dev/null 2>&1 || true

open "$DEST/$APP_NAME"
say "Done. Look for the paw icon in the menu bar; the welcome window connects the pet to Claude Code."
