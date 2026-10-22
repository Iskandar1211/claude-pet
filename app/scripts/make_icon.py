#!/usr/bin/env python3
"""Иконка приложения: Resources/Anna.png (кадр idle) → Resources/AppIcon.icns.

Нужны Pillow и iconutil (есть в macOS). Запуск: python3 scripts/make_icon.py (из app/).
"""
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

APP = Path(__file__).resolve().parent.parent
FRAME = 392                      # размер кадра в Anna.png
CANVAS = 1024
BODY = 824                       # сам значок внутри холста — по сетке macOS
RADIUS = 185
CROP = (20, 0, 340, 320)         # голова и плечи в первом кадре idle


def squircle_mask(size: int, radius: int) -> Image.Image:
    scale = 4  # сглаживание краёв
    mask = Image.new("L", (size * scale, size * scale), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size * scale - 1, size * scale - 1), radius * scale, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def background(size: int) -> Image.Image:
    # Тёмный сине-серый градиент сверху вниз + тёплое оранжевое свечение за головой.
    top, bottom = (52, 58, 78), (24, 27, 38)
    bg = Image.new("RGB", (size, size))
    px = bg.load()
    for y in range(size):
        t = y / (size - 1)
        row = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(size):
            px[x, y] = row
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((size * 0.18, size * 0.12, size * 0.82, size * 0.76), fill=(255, 150, 60, 150))
    glow = glow.filter(ImageFilter.GaussianBlur(size * 0.11))
    bg = bg.convert("RGBA")
    bg.alpha_composite(glow)
    return bg


def build() -> Image.Image:
    sheet = Image.open(APP / "Resources" / "Anna.png").convert("RGBA")
    anna = sheet.crop(CROP)
    side = round(BODY * 0.92)
    anna = anna.resize((side, round(side * anna.height / anna.width)), Image.LANCZOS)

    body = background(BODY)
    body.alpha_composite(anna, ((BODY - anna.width) // 2, BODY - anna.height))  # прижата к низу значка
    body.putalpha(squircle_mask(BODY, RADIUS))

    # Мягкая тень под значком, как у системных иконок.
    icon = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    offset = (CANVAS - BODY) // 2
    shadow.paste((0, 0, 0, 110), (offset, offset + 10), squircle_mask(BODY, RADIUS))
    icon.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    icon.alpha_composite(body, (offset, offset))
    return icon


def main() -> None:
    icon = build()
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "AppIcon.iconset"
        iconset.mkdir()
        for base in (16, 32, 128, 256, 512):
            icon.resize((base, base), Image.LANCZOS).save(iconset / f"icon_{base}x{base}.png")
            icon.resize((base * 2, base * 2), Image.LANCZOS).save(iconset / f"icon_{base}x{base}@2x.png")
        out = APP / "Resources" / "AppIcon.icns"
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(out)], check=True)
    print(f"Готово: {out}")


if __name__ == "__main__":
    main()
