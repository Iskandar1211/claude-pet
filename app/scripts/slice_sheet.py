"""Лист персонажа от генератора (сетка «на глаз», кадры не попадают в ячейки 392) -> assets/<Name>_source.png.

Генератор рисует 4×7 поз, но ряды не совпадают с ячейками: ноги уходят в соседнюю строку, ряды бывают
выше ячейки, а лист — не 2744 px в высоту. Поэтому кадры ищем заново: каждое связное пятно рисунка
(тело, «?», искры) относится к той позе, на сетке 4×7 которой лежит его центр. Позу ставим в ячейку 392:
по центру тела (самое большое пятно) и ступнями на общую линию; масштаб один на всего персонажа,
чтобы рост не прыгал между состояниями. Дальше — как у Анны: scripts/key_background.py <Name>.

Запуск из app/:  python3 -I scripts/slice_sheet.py Resources/kay.jpeg Kai   (нужен Pillow)
"""
import sys
from array import array
from collections import deque
from pathlib import Path

from PIL import Image, ImageFilter

APP = Path(__file__).resolve().parent.parent
COLS, ROWS, FRAME = 4, 7, 392
INK = 24       # max(r,g,b) > INK — рисунок; ниже — фон (с запасом на шум JPEG)
MIN_BLOB = 6   # пятна мельче — шум сжатия
MARGIN = 8     # отступ фигуры от краёв ячейки; ступни — на FRAME - MARGIN


def blobs(mask: Image.Image) -> list[list[int]]:
    """Связные области рисунка (8-связность), каждая — список индексов пикселей."""
    w, h = mask.size
    data = mask.tobytes()
    seen = bytearray(w * h)
    found = []
    for start in range(w * h):
        if seen[start] or not data[start]:
            continue
        seen[start] = 1
        region, queue = [], deque([start])
        while queue:
            i = queue.popleft()
            region.append(i)
            x, y = i % w, i // w
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h:
                        j = ny * w + nx
                        if not seen[j] and data[j]:
                            seen[j] = 1
                            queue.append(j)
        if len(region) >= MIN_BLOB:
            found.append(region)
    return found


def main(source: Path, name: str) -> None:
    im = Image.open(source).convert("RGB")
    w, h = im.size
    mask = Image.new("L", im.size)
    mask.putdata([255 if max(p) > INK else 0 for p in im.getdata()])

    # Границы поз: около каждой 1/4 ширины и 1/7 высоты — линия, где рисунка меньше всего. Режем маску
    # по ней: соседние позы иногда касаются (хвост одной задевает ботинки другой) и слиплись бы в одно пятно.
    ink = mask.load()

    # ±110 px: у генератора ряды «уплывают» от равной сетки больше чем на 70 px.
    DRIFT = 110
    # Пары пикселей по разные стороны разреза, которые соединял рисунок на самом разрезе.
    severed: list[tuple[int, int]] = []

    # Колонки разделены пустыми полосами — хватает прямой линии.
    col_cuts = []
    for k in range(1, COLS):
        guess = k * w // COLS
        x = min(range(guess - DRIFT, guess + DRIFT + 1), key=lambda x: sum(1 for y in range(h) if ink[x, y]))
        col_cuts.append(x)
        for y in range(h):
            if ink[x, y]:
                severed.append((y * w + x - 1, y * w + x + 1))
                ink[x, y] = 0

    # Ряды режем в каждой колонке отдельно: ступни одной позы бывают ниже макушки соседней по ряду,
    # и общая линия на всю ширину прошла бы по чьим-то ногам.
    spans = list(zip([0] + col_cuts, col_cuts + [w]))
    row_cuts = []
    for a, b in spans:
        cuts = []
        for k in range(1, ROWS):
            guess = k * h // ROWS
            y = min(range(guess - DRIFT, guess + DRIFT + 1), key=lambda y: sum(1 for x in range(a, b) if ink[x, y]))
            cuts.append(y)
            for x in range(a, b):
                if ink[x, y]:
                    severed.append(((y - 1) * w + x, (y + 1) * w + x))
                    ink[x, y] = 0
        row_cuts.append(cuts)

    regions = blobs(mask)
    label = array("i", [-1]) * (w * h)
    cells = []
    for n, region in enumerate(regions):
        for i in region:
            label[i] = n
        xs = [i % w for i in region]
        ys = [i // w for i in region]
        cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
        col = sum(cx > x for x in col_cuts)
        cells.append((col, sum(cy > y for y in row_cuts[col])))

    # Тело позы — самое большое пятно в ячейке. Ряды заходят друг на друга, и прямая линия отрезает
    # от позы кусок (ботинок, кончик хвоста), центр которого попадает в чужую ячейку. Куски, разрезанные
    # линией, склеиваем обратно в группы; группа с телом уходит к телу, без тела — в ячейку самого
    # большого своего куска. Два тела в одну группу не попадают: ради этого и резали.
    body = {}
    for n, cell in enumerate(cells):
        if cell not in body or len(regions[n]) > len(regions[body[cell]]):
            body[cell] = n
    parent = list(range(len(regions)))
    group_body = {n: n for n in body.values()}

    def root(n: int) -> int:
        while parent[n] != n:
            parent[n] = parent[parent[n]]
            n = parent[n]
        return n

    for p, q in severed:
        if not (0 <= p < w * h and 0 <= q < w * h) or label[p] < 0 or label[q] < 0:
            continue
        a, b = root(label[p]), root(label[q])
        if a == b or (a in group_body and b in group_body):
            continue
        if b in group_body:
            a, b = b, a
        parent[b] = a

    groups: dict[int, list[int]] = {}
    for n in range(len(regions)):
        groups.setdefault(root(n), []).append(n)
    for r, members in groups.items():
        owner = group_body.get(r, max(members, key=lambda n: len(regions[n])))
        for n in members:
            cells[n] = cells[owner]

    poses: dict[tuple[int, int], list[list[int]]] = {}
    for region, cell in zip(regions, cells):
        poses.setdefault(cell, []).append(region)

    missing = [(c, r) for r in range(ROWS) for c in range(COLS) if (c, r) not in poses]
    assert not missing, f"нет рисунка в позах (колонка, строка): {missing}"

    # Опорные точки позы: центр тела по x и низ тела (ступни); и как далеко рисунок уходит от них.
    layout = {}
    for cell, regions in poses.items():
        body = max(regions, key=len)
        bx = [i % w for i in body]
        anchor_x, anchor_y = (min(bx) + max(bx)) / 2, max(i // w for i in body)
        all_px = [i for r in regions for i in r]
        x0, x1 = min(i % w for i in all_px), max(i % w for i in all_px)
        y0, y1 = min(i // w for i in all_px), max(i // w for i in all_px)
        layout[cell] = (anchor_x, anchor_y, x0, x1, y0, y1, regions)

    # Один масштаб: самая широкая/высокая поза ровно влезает в ячейку с отступами. Маленький лист
    # (генератор отдал 768×1024) так увеличивается до роста Анны, большой — уменьшается.
    half = FRAME / 2 - MARGIN
    tall = FRAME - 2 * MARGIN
    scale = min(
        *(half / max(ax - x0, x1 - ax, 1) for ax, _, x0, x1, _, _, _ in layout.values()),
        *(tall / max(ay - y0, 1) for _, ay, _, _, y0, _, _ in layout.values()),
    )

    out = Image.new("RGB", (COLS * FRAME, ROWS * FRAME), (0, 0, 0))
    for (col, row), (ax, ay, x0, x1, y0, y1, regions) in layout.items():
        # Берём только пиксели своей позы (+2 px мягкого края), чтобы соседние не попали в кадр.
        own = Image.new("L", (w, h), 0)
        px = own.load()
        for region in regions:
            for i in region:
                px[i % w, i // w] = 255
        box = (max(x0 - 3, 0), max(y0 - 3, 0), min(x1 + 4, w), min(y1 + 4, h))
        piece_mask = own.crop(box).filter(ImageFilter.MaxFilter(5))
        piece = Image.new("RGB", piece_mask.size, (0, 0, 0))
        piece.paste(im.crop(box), mask=piece_mask)
        piece = piece.resize((round(piece.width * scale), round(piece.height * scale)), Image.LANCZOS)
        if scale > 1.2:
            # После заметного увеличения контуры мылятся — слегка возвращаем резкость.
            piece = piece.filter(ImageFilter.UnsharpMask(radius=2, percent=80, threshold=2))
        # Опорная точка позы -> центр ячейки по x, ступни -> FRAME - MARGIN.
        left = col * FRAME + FRAME / 2 - (ax - box[0]) * scale
        top = row * FRAME + FRAME - MARGIN - (ay - box[1]) * scale
        # Что ниже ступней (тень, огонь) не должно залезть в следующий ряд — сдвигаем вверх.
        overflow = top + piece.height - (row + 1) * FRAME
        if overflow > 0:
            top -= overflow
        out.paste(piece, (round(left), round(top)))

    target = APP / "assets" / f"{name}_source.png"
    out.save(target)
    print(f"{target} {out.size}, масштаб {scale:.3f}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), sys.argv[2])
