#!/bin/bash
# Установка готовой сборки Claude Pet из GitHub Releases:
#   curl -fsSL https://raw.githubusercontent.com/Iskandar1211/claude-pet/main/install.sh | bash
# Скачивает zip под архитектуру этого Mac и файл контрольных сумм, сверяет SHA-256, кладёт приложение
# в ~/Applications и запускает. Подключение к Claude Code делает окно приветствия в самом приложении.
# Скрипт самодостаточен (работает через pipe, файлы репозитория не нужны). Сборка из исходников — build-from-source.sh.
# Приложение подписано ad-hoc и не нотаризовано; файлы, скачанные через curl, не получают метку карантина,
# поэтому Gatekeeper их не блокирует.
set -euo pipefail

APP_NAME="Claude Pet.app"
BUNDLE_ID="local.claude-code-pet"
# Переопределяются окружением только для тестов.
BASE_URL="${PET_BASE_URL:-https://github.com/Iskandar1211/claude-pet/releases/latest/download}"
DEST="${PET_DEST:-$HOME/Applications}"
SUMS_NAME="SHA256SUMS.txt"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[31mError:\033[0m %s\n' "$*" >&2; exit 1; }

[[ "$(uname)" == "Darwin" ]] || fail "Claude Pet runs on macOS only."

# hook-скрипты питомца используют jq, а он встроен в систему только с macOS 15.
os_major="$(sw_vers -productVersion | cut -d. -f1)"
if ((os_major < 15)); then
  fail "Claude Pet needs macOS 15 or later (found $(sw_vers -productVersion)). On older systems build it from source: https://github.com/Iskandar1211/claude-pet#build-from-source"
fi

arch="$(uname -m)"
# Терминал под Rosetta на Apple Silicon тоже показывает x86_64 — берём нативную сборку.
if [[ "$arch" == "x86_64" && "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" == "1" ]]; then
  arch="arm64"
fi
case "$arch" in
  arm64|x86_64) ;;
  *) fail "Unsupported architecture: $arch" ;;
esac

ZIP_NAME="ClaudePet-$arch.zip"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

download() {
  curl -fsSL --retry 3 --connect-timeout 15 -o "$2" "$1" ||
    fail "Could not download ${1} — check your internet connection and that a release exists: https://github.com/Iskandar1211/claude-pet/releases"
}

say "Downloading ${ZIP_NAME}…"
download "$BASE_URL/$ZIP_NAME" "$tmp/$ZIP_NAME"
download "$BASE_URL/$SUMS_NAME" "$tmp/$SUMS_NAME"

# Сверяем контрольную сумму: при несовпадении или отсутствии записи ничего не устанавливаем.
expected="$(awk -v n="$ZIP_NAME" '$2 == n || $2 == "*" n { print $1; exit }' "$tmp/$SUMS_NAME")"
[[ -n "$expected" ]] || fail "No checksum for $ZIP_NAME in $SUMS_NAME — aborting."
actual="$(shasum -a 256 "$tmp/$ZIP_NAME" | awk '{ print $1 }')"
[[ "$actual" == "$expected" ]] || fail "Checksum mismatch for $ZIP_NAME (expected $expected, got $actual) — aborting, nothing was installed."
say "Checksum OK"

mkdir "$tmp/unpacked"
ditto -x -k "$tmp/$ZIP_NAME" "$tmp/unpacked"
[[ -d "$tmp/unpacked/$APP_NAME" ]] || fail "The archive doesn't contain $APP_NAME — aborting."

say "Installing to $DEST/$APP_NAME"
osascript -e 'quit app "Claude Pet"' >/dev/null 2>&1 || true
mkdir -p "$DEST"
rm -rf "${DEST:?}/$APP_NAME"
ditto "$tmp/unpacked/$APP_NAME" "$DEST/$APP_NAME"

# Новая установка — снова показать окно приветствия, если питомец ещё не подключён.
defaults delete "$BUNDLE_ID" onboardingShown >/dev/null 2>&1 || true

open "$DEST/$APP_NAME"
say "Done. Look for the paw icon in the menu bar; the welcome window connects the pet to Claude Code."
