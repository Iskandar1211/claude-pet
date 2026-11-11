#!/bin/bash
# Собирает build/Claude Pet.app без Xcode: swift build + pet-bridge (cargo) + pet-channel (bun --compile)
# + Info.plist + ресурсы + ad-hoc подпись. Bun нужен только тому, кто собирает: у пользователя Channel — один файл.
# Запуск: app/scripts/build_app.sh && open "app/build/Claude Pet.app"
# Для релиза: app/scripts/build_app.sh [arm64|x86_64] [--zip]
#   arm64|x86_64 — собрать под эту архитектуру (по умолчанию — под текущую машину);
#   --zip        — дополнительно упаковать в app/build/ClaudePet-<arch>.zip (ditto сохраняет подпись и права).
# Переменные окружения:
#   PET_VERSION          — версия, которая попадёт в Info.plist собранного бандла (исходный файл не меняется);
#   PET_REQUIRE_CHANNEL=1 — упасть, если Channel (bun) собрать не удалось, вместо сборки без него.
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH=""
MAKE_ZIP=0
for arg in "$@"; do
  case "$arg" in
    arm64|x86_64) ARCH="$arg" ;;
    --zip) MAKE_ZIP=1 ;;
    *) echo "Ошибка: неизвестный аргумент '$arg'. Использование: build_app.sh [arm64|x86_64] [--zip]" >&2; exit 2 ;;
  esac
done

SWIFT_ARGS=()
CARGO_ARGS=()
BUN_ARGS=()
if [[ -n "$ARCH" ]]; then
  SWIFT_ARGS=(--triple "$ARCH-apple-macosx")
  if [[ "$ARCH" == "arm64" ]]; then
    CARGO_TARGET="aarch64-apple-darwin"; BUN_TARGET="bun-darwin-arm64"
  else
    CARGO_TARGET="x86_64-apple-darwin"; BUN_TARGET="bun-darwin-x64"
  fi
  CARGO_ARGS=(--target "$CARGO_TARGET")
  BUN_ARGS=(--target "$BUN_TARGET")
fi

swift build -c release ${SWIFT_ARGS[@]+"${SWIFT_ARGS[@]}"}
# Cursor sandbox подставляет CARGO_TARGET_DIR вне репо — тогда cp берёт старый pet-bridge.
unset CARGO_TARGET_DIR
(cd ../bridge && cargo build --release -p pet-bridge-cli ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"})
if [[ -n "$ARCH" ]]; then
  BRIDGE="../bridge/target/$CARGO_TARGET/release/pet-bridge"
else
  BRIDGE="../bridge/target/release/pet-bridge"
fi
if [[ ! -f "$BRIDGE" ]]; then
  echo "Ошибка: не собран $BRIDGE" >&2
  exit 1
fi
BIN="$(swift build -c release ${SWIFT_ARGS[@]+"${SWIFT_ARGS[@]}"} --show-bin-path)/ClaudePet"
# Channel (ответы и одобрения из Quick Chat) — самодостаточный исполняемый файл со средой Bun внутри.
# Bun необязателен: без него питомец работает, только без Channel.
CHANNEL="build/pet-channel"
rm -f "$CHANNEL"
if command -v bun >/dev/null; then
  (cd ../prototype/channel && bun install --frozen-lockfile >/dev/null && bun build --compile ${BUN_ARGS[@]+"${BUN_ARGS[@]}"} pet-channel.ts --outfile "../../app/$CHANNEL" >/dev/null)
elif [[ "${PET_REQUIRE_CHANNEL:-}" == "1" ]]; then
  echo "Ошибка: PET_REQUIRE_CHANNEL=1, но Bun не найден." >&2
  exit 1
else
  echo "Bun не найден — собираю без Channel (одобрения из Quick Chat будут недоступны). Bun: https://bun.sh" >&2
fi
if [[ "${PET_REQUIRE_CHANNEL:-}" == "1" && ! -f "$CHANNEL" ]]; then
  echo "Ошибка: PET_REQUIRE_CHANNEL=1, но $CHANNEL не собран." >&2
  exit 1
fi

APP="build/Claude Pet.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/ClaudePet"
# Мост запускается приложением как дочерний процесс (bridge/PROTOCOL.md).
cp "$BRIDGE" "$APP/Contents/MacOS/pet-bridge"
if [[ -f "$CHANNEL" ]]; then cp "$CHANNEL" "$APP/Contents/MacOS/pet-channel"; fi
cp Resources/Info.plist "$APP/Contents/Info.plist"
# Версия релиза подставляется только в собранный бандл, исходный Info.plist остаётся прежним.
if [[ -n "${PET_VERSION:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $PET_VERSION" "$APP/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $PET_VERSION" "$APP/Contents/Info.plist"
fi
# Персонажи — PetCharacter в SpriteSheet.swift: лист <Имя>.png + разметка <имя>.json.
for name in Anna Kai Lira Nova; do
  cp "Resources/$name.png" "Resources/$(echo "$name" | tr '[:upper:]' '[:lower:]').json" "$APP/Contents/Resources/"
done
cp Resources/AppIcon.icns "$APP/Contents/Resources/"

# Ad-hoc подпись: для себя хватает, для раздачи нужен Developer ID + notarytool.
# Вложенный исполняемый файл подписывается раньше самого .app.
codesign --force --sign - "$APP/Contents/MacOS/pet-bridge"
if [[ -f "$APP/Contents/MacOS/pet-channel" ]]; then codesign --force --sign - "$APP/Contents/MacOS/pet-channel"; fi
# Требование подписи — по bundle id, а не по хешу (как у ad-hoc по умолчанию): иначе после каждой
# пересборки macOS забывает выданный доступ к микрофону и распознаванию речи и спрашивает снова.
codesign --force --sign - -r='designated => identifier "local.claude-code-pet"' "$APP"
echo "Собрано: $(pwd)/$APP"

if ((MAKE_ZIP)); then
  ZIP="build/ClaudePet-${ARCH:-$(uname -m)}.zip"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
  echo "Архив: $(pwd)/$ZIP"
fi
