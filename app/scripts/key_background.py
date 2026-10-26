"""assets/<Имя>_source.png (лист на чёрном фоне) -> Resources/<Имя>.png с прозрачным фоном.

Фон убирается заливкой по связным областям почти чёрного, а не порогом по яркости: обводка у Анны
тёмно-синяя (синий канал >= ~32), заливка на ней останавливается, и тёмные куртка/сапоги
внутри фигуры не страдают. Фон — область, касающаяся края кадра, или замкнутая область
от MIN_HOLE px (просвет между хвостом волос и плечом). Края фона — частичная прозрачность.

Запуск из app/:  python3 -I scripts/key_background.py [Имя]   (по умолчанию Anna; нужен Pillow)
"""
import json
import sys
from collections import deque
from pathlib import Path

from PIL import Image

APP = Path(__file__).resolve().parent.parent
NAME = sys.argv[1] if len(sys.argv) > 1 else "Anna"
SOURCE = APP / "assets" / f"{NAME}_source.png"
OUT = APP / "Resources" / f"{NAME}.png"
FRAME = json.loads((APP / "Resources" / f"{NAME.lower()}.json").read_text())["frameSize"]

BACKGROUND = 10  # max(r,g,b) <= BACKGROUND -> кандидат в фон
MIN_HOLE = 20    # замкнутая чёрная область от стольких px — тоже фон; мельче — часть рисунка
EDGE = 40        # max(r,g,b) < EDGE рядом с фоном -> полупрозрачный край


def main() -> None:
    im = Image.open(SOURCE).convert("RGBA")
    w, h = im.size
    assert w % FRAME == 0 and h % FRAME == 0, f"{w}x{h} не делится на кадр {FRAME}"
    px = im.load()
    bright = lambda x, y: max(px[x, y][:3])
    background = bytearray(w * h)

    # Связные области почти чёрного внутри каждого кадра: соседние кадры не «протекают» друг в друга.
    seen = bytearray(w * h)
    for sy in range(h):
        for sx in range(w):
            if seen[sy * w + sx] or bright(sx, sy) > BACKGROUND:
                continue
            fx, fy = sx - sx % FRAME, sy - sy % FRAME
            region, queue, touches_border = [], deque([(sx, sy)]), False
            seen[sy * w + sx] = 1
            while queue:
                x, y = queue.popleft()
                region.append(y * w + x)
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if not (fx <= nx < fx + FRAME and fy <= ny < fy + FRAME):
                        touches_border = True
                    elif not seen[ny * w + nx] and bright(nx, ny) <= BACKGROUND:
                        seen[ny * w + nx] = 1
                        queue.append((nx, ny))
            if touches_border or len(region) >= MIN_HOLE:
                for i in region:
                    background[i] = 1

    for y in range(h):
        for x in range(w):
            if background[y * w + x]:
                px[x, y] = (0, 0, 0, 0)
                continue
            touches_bg = any(
                0 <= nx < w and 0 <= ny < h and background[ny * w + nx]
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1))
            )
            if touches_bg and bright(x, y) < EDGE:
                r, g, b, _ = px[x, y]
                px[x, y] = (r, g, b, int(255 * bright(x, y) / EDGE))

    im.save(OUT)
    print(f"{OUT} {im.size}, кадр {FRAME}")


if __name__ == "__main__":
    main()
