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

STEP = 0.04                  # сетка времени, с
LOOP_STEPS = 250             # 10 с; периоды сценок разные, поэтому на стыке цикла возможен малозаметный шов

# Сценки как в PetScene.animation: (кадр, секунды) по кругу. «done» и «picked up» в приложении
# не повторяются (ликует и остаётся довольной; паникует, пока держат) — в гифке крутятся целиком.
SCENES = {
    "idle": [(0, 2.5), (1, 0.08), (2, 0.14), (3, 2.0), (1, 0.08), (2, 0.14)],
    "working": [(0, 0.25), (1, 0.25), (2, 0.25)] * 3 + [(3, 1.2)],
    "waiting": [(0, 0.3), (1, 0.3), (2, 0.3), (3, 0.3)],
    "done": [(0, 0.3), (1, 0.3), (2, 0.3)] * 2 + [(3, 3.0)],
    "walk": [(0, 0.12), (1, 0.12), (2, 0.12), (3, 0.12)],
    "drag": [(0, 0.2), (1, 0.2)] + [(2, 0.25), (3, 0.25)] * 6,
}


def picker(scene: list[tuple[int, float]]):
    period = sum(seconds for _, seconds in scene)

    def pick(step: int) -> int:
        t = (step * STEP) % period
        for frame, seconds in scene:
            if t < seconds:
                return frame
            t -= seconds
        return scene[-1][0]

    return pick


# (подпись, строка в спрайт-листе, как выбрать кадр по шагу времени)
STATES = [
    ("idle", 0, picker(SCENES["idle"])),
    ("working", 1, picker(SCENES["working"])),
    ("needs approval", 2, picker(SCENES["waiting"])),
    ("done", 3, picker(SCENES["done"])),
    ("walking", 5, picker(SCENES["walk"])),
    ("picked up", 6, picker(SCENES["drag"])),
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
