#!/bin/bash
# Собирает build/Claude Pet.app без Xcode: swift build + pet-bridge (cargo) + pet-channel (bun --compile)
# + Info.plist + ресурсы + ad-hoc подпись. Bun нужен только тому, кто собирает: у пользователя Channel — один файл.
# Запуск: app/scripts/build_app.sh && open "app/build/Claude Pet.app"
set -euo pipefail
cd "$(dirname "$0")/.."

swift build -c release
# Cursor sandbox подставляет CARGO_TARGET_DIR вне репо — тогда cp берёт старый pet-bridge.
unset CARGO_TARGET_DIR
(cd ../bridge && cargo build --release -p pet-bridge-cli)
BRIDGE="../bridge/target/release/pet-bridge"
if [[ ! -f "$BRIDGE" ]]; then
  echo "Ошибка: не собран $BRIDGE" >&2
  exit 1
fi
BIN="$(swift build -c release --show-bin-path)/ClaudePet"
# Channel (ответы и одобрения из Quick Chat) — самодостаточный исполняемый файл со средой Bun внутри.
# Bun необязателен: без него питомец работает, только без Channel.
CHANNEL="build/pet-channel"
rm -f "$CHANNEL"
if command -v bun >/dev/null; then
  (cd ../prototype/channel && bun install --frozen-lockfile >/dev/null && bun build --compile pet-channel.ts --outfile "../../app/$CHANNEL" >/dev/null)
else
  echo "Bun не найден — собираю без Channel (одобрения из Quick Chat будут недоступны). Bun: https://bun.sh" >&2
fi

APP="build/Claude Pet.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/ClaudePet"
# Мост запускается приложением как дочерний процесс (bridge/PROTOCOL.md).
cp "$BRIDGE" "$APP/Contents/MacOS/pet-bridge"
if [[ -f "$CHANNEL" ]]; then cp "$CHANNEL" "$APP/Contents/MacOS/pet-channel"; fi
cp Resources/Info.plist "$APP/Contents/Info.plist"
cp Resources/Anna.png Resources/anna.json Resources/AppIcon.icns "$APP/Contents/Resources/"

# Ad-hoc подпись: для себя хватает, для раздачи нужен Developer ID + notarytool.
# Вложенный исполняемый файл подписывается раньше самого .app.
codesign --force --sign - "$APP/Contents/MacOS/pet-bridge"
if [[ -f "$APP/Contents/MacOS/pet-channel" ]]; then codesign --force --sign - "$APP/Contents/MacOS/pet-channel"; fi
codesign --force --sign - "$APP"
echo "Собрано: $(pwd)/$APP"
