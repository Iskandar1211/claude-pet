#!/usr/bin/env python3
"""Картинка для README: все персонажи в ряд (кадр покоя), карточки как в make_gif.py.

Нужен Pillow. Запуск из app/: python3 scripts/make_characters.py  →  ../docs/characters.png
(без -I: стиль карточек импортируется из соседнего make_gif.py)
"""
from pathlib import Path

from PIL import Image, ImageDraw

from make_gif import ACCENT, BG, CARD, FRAME, GAP, LABEL_H, PAD, TEXT, font

APP = Path(__file__).resolve().parent.parent
OUT = APP.parent / "docs" / "characters.png"

# Как PetCharacter в SpriteSheet.swift: файл листа и подпись.
CHARACTERS = [("Anna", "Anna"), ("Kai", "Kai"), ("Lira", "Lira"), ("Nova", "Nova")]
SPRITE = 180
# Поза для витрины: строка и кадр в листе (покой, первый кадр).
ROW, COL = 0, 0


def main() -> None:
    card_w, card_h = SPRITE + 2 * GAP, SPRITE + LABEL_H + GAP
    width = PAD * 2 + card_w * len(CHARACTERS) + GAP * (len(CHARACTERS) - 1)
    height = PAD * 2 + card_h
    img = Image.new("RGBA", (width, height), BG + (255,))
    draw = ImageDraw.Draw(img)
    label_font = font(16)

    for i, (sheet_name, label) in enumerate(CHARACTERS):
        sheet = Image.open(APP / "Resources" / f"{sheet_name}.png").convert("RGBA")
        tile = sheet.crop((COL * FRAME, ROW * FRAME, (COL + 1) * FRAME, (ROW + 1) * FRAME))
        sprite = tile.resize((SPRITE, SPRITE), Image.LANCZOS)
        x = PAD + i * (card_w + GAP)
        draw.rounded_rectangle((x, PAD, x + card_w, PAD + card_h), 14, fill=CARD)
        img.alpha_composite(sprite, (x + GAP, PAD + GAP // 2))
        tw = draw.textlength(label, font=label_font)
        draw.text((x + (card_w - tw) / 2, PAD + GAP // 2 + SPRITE + 4), label, font=label_font,
                  fill=ACCENT if i == 0 else TEXT)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.convert("RGB").save(OUT, optimize=True)
    print(f"{OUT}: {width}×{height}, {OUT.stat().st_size / 1024:.0f} КБ")


if __name__ == "__main__":
    main()
