#!/usr/bin/env python3
"""Гифка для README: Анна во всех состояниях, кадры и тайминги как в приложении (PetScene.animation).

Нужен Pillow. Запуск из app/: python3 scripts/make_gif.py  →  ../docs/anna-states.gif
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

APP = Path(__file__).resolve().parent.parent
OUT = APP.parent / "docs" / "anna-states.gif"

FRAME = 392                  # кадр в Anna.png (сетка 4 столбца × 7 строк)
SPRITE = 148                 # размер кадра в гифке
PAD, GAP, LABEL_H = 14, 10, 34
BG, CARD = (27, 31, 43), (40, 45, 62)
TEXT, ACCENT = (205, 210, 225), (255, 150, 60)

STEP = 0.04                  # сетка времени, с; всё кратно ей, чтобы цикл замкнулся без швов
LOOP_STEPS = 132             # 5,28 с = НОК периодов ходьбы (3), поз (12) и покоя (66)

# Покой: кадр 0 держится 2,48 с, моргание (кадр 1) — 0,16 с; шаги по 0,04 с.
IDLE_PERIOD, BLINK_AT, BLINK_LEN = 66, 62, 4
WALK_STEPS, POSE_STEPS = 3, 11   # ходьба 0,12 с на кадр, позы ~0,45 с на кадр


def pick_idle(step: int) -> int:
    return 1 if BLINK_AT <= step % IDLE_PERIOD < BLINK_AT + BLINK_LEN else 0


# (подпись, строка в спрайт-листе, как выбрать кадр по шагу времени)
STATES = [
    ("idle", 0, pick_idle),
    ("working", 1, lambda s: (s // POSE_STEPS) % 2),       
    ("needs approval", 2, lambda s: (s // POSE_STEPS) % 2),
    ("done", 3, lambda s: (s // POSE_STEPS) % 2),
    ("walking", 5, lambda s: (s // WALK_STEPS) % 4),
    ("picked up", 6, lambda s: 0),
]


def font(size: int) -> ImageFont.FreeTypeFont:
    for path in ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"):
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


def main() -> None:
    sheet = Image.open(APP / "Resources" / "Anna.png").convert("RGBA")

    def sprite(row: int, col: int) -> Image.Image:
        tile = sheet.crop((col * FRAME, row * FRAME, (col + 1) * FRAME, (row + 1) * FRAME))
        return tile.resize((SPRITE, SPRITE), Image.LANCZOS)

    cache = {(row, col): sprite(row, col) for _, row, _ in STATES for col in range(4)}
    card_w, card_h = SPRITE + 2 * GAP, SPRITE + LABEL_H + GAP
    width = PAD * 2 + card_w * len(STATES) + GAP * (len(STATES) - 1)
    height = PAD * 2 + card_h
    label_font = font(15)

    def compose(step: int) -> Image.Image:
        img = Image.new("RGBA", (width, height), BG + (255,))
        draw = ImageDraw.Draw(img)
        for i, (label, row, pick) in enumerate(STATES):
            x = PAD + i * (card_w + GAP)
            draw.rounded_rectangle((x, PAD, x + card_w, PAD + card_h), 14, fill=CARD)
            img.alpha_composite(cache[(row, pick(step))], (x + GAP, PAD + GAP // 2))
            tw = draw.textlength(label, font=label_font)
            draw.text((x + (card_w - tw) / 2, PAD + GAP // 2 + SPRITE + 4), label, font=label_font,
                      fill=ACCENT if label == "needs approval" else TEXT)
        return img.convert("RGB")

    # Подряд идущие одинаковые кадры склеиваем в один с суммарной задержкой.
    frames: list[Image.Image] = []
    delays: list[int] = []
    last = None
    for step in range(LOOP_STEPS):
        key = tuple(pick(step) for _, _, pick in STATES)
        if key == last:
            delays[-1] += round(STEP * 100)
            continue
        frames.append(compose(step))
        delays.append(round(STEP * 100))
        last = key

    # Общая палитра на все кадры: иначе цвета «плывут» от кадра к кадру.
    montage = Image.new("RGB", (width, height * len(frames)))
    for i, frame in enumerate(frames):
        montage.paste(frame, (0, i * height))
    palette = montage.quantize(colors=128, method=Image.Quantize.MEDIANCUT)
    paletted = [f.quantize(palette=palette, dither=Image.Dither.NONE) for f in frames]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    paletted[0].save(OUT, save_all=True, append_images=paletted[1:], duration=[d * 10 for d in delays], loop=0, optimize=True, disposal=1)
    print(f"{OUT}: {len(frames)} кадров, {OUT.stat().st_size / 1024:.0f} КБ, {width}×{height}")


if __name__ == "__main__":
    main()
