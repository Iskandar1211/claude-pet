#!/bin/bash
# Скриншоты Quick Chat для README (docs/quick-chat.png, docs/approval.png) с примерными данными.
# Собирает отдельную программу из КОПИЙ исходников приложения — сами исходники не меняются.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-../docs}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

for f in Sources/ClaudePet/*.swift; do
  case "$(basename "$f")" in main.swift|AppDelegate.swift) ;; *) cp "$f" "$WORK/" ;; esac
done
# В копиях открываем то, что в приложении закрыто: историю и выбранную сессию задаёт скрипт.
sed -i '' 's/private struct MessageBubble/struct MessageBubble/' "$WORK/QuickChatView.swift"
sed -i '' 's/private(set) var messages/var messages/; s/private(set) var selectedId/var selectedId/' "$WORK/QuickChatModel.swift"
sed -i '' 's/private(set) var isSpeaking/var isSpeaking/' "$WORK/VoiceOutput.swift"

mkdir -p "$OUT"
cp scripts/screenshots_main.swift "$WORK/main.swift"  # top-level код допустим только в файле main.swift
swiftc -o "$WORK/shots" "$WORK"/*.swift
"$WORK/shots" "$OUT"
echo "Готово: $OUT/quick-chat.png, $OUT/approval.png, $OUT/voice.png"
