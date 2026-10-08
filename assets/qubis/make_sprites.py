"""Render the Qubis pixel-art sprites from their text grids.

Each sprite is a grid of characters, one per pixel. The grids are the source of
truth; the PNG files are generated from them. Qubis comes in two drawings:

- TINY fits a 16 x 16 canvas and is used for the 16-pixel icon.
- MEDIUM fits a 24 x 24 canvas and is used for the 24-pixel icon (the taskbar).
- SMALL fits a 32 x 32 canvas and is used for the 32-pixel icon.
- LARGE fits a 64 x 64 canvas and is used for bigger icons and in the interface.

Run from any folder (requires Pillow):

    python make_sprites.py [output folder]
"""

import io
import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw

PALETTE = {
    ".": None,  # transparent
    "N": (0x10, 0x27, 0x5B),  # body navy
    "D": (0x0B, 0x1A, 0x3D),  # pupil
    "W": (0xFF, 0xFF, 0xFF),  # eye white and highlight
    "G": (0x24, 0x47, 0x9A),  # body seen through the magnifying glass
    "L": (0xC8, 0xD8, 0xF5),  # magnifying glass ring
    "H": (0xB9, 0x7A, 0x3C),  # magnifying glass handle
    "S": (0x9C, 0xC3, 0xFF),  # sweat drop
    "Q": (0x45, 0x8A, 0xFD),  # question mark
}
BACKGROUND = (0x45, 0x8A, 0xFD)  # tile blue

TINY = """
.....NNNN.....
...NNNNNNNN...
..NNNNNNNNNN..
..NWWWNNWWWN..
.NNWDWNNWDWNN.
.NNWWWNNWWWNN.
NNNNNNNNNNNNNN
NNNNNNNNNNNNNN
.N.NNNNNNNN.N.
...NNNNNNNN...
...NNN..NNN...
"""

MEDIUM = """
........NNNN........
......NNNNNNNN......
.....NNNNNNNNNN.....
....NNWWNNNNWWNN....
...NNWWWWNNWWWWNN...
...NNWDDWNNWDDWNN...
..NNNWDDWNNWDDWNNN..
..NNNWWWWNNWWWWNNN..
.NNNNNWWNNNNWWNNNNN.
NNNNNNNNNNNNNNNNNNNN
NNNNNNNNNNNNNNNNNNNN
.NN.NNNNNNNNNNNN.NN.
....NNNNNNNNNNNN....
.....NNNNNNNNNN.....
.....NNNN..NNNN.....
.....NNNN..NNNN.....
"""

SMALL = """
..........NNNNNN..........
........NNNNNNNNNN........
.......NNNNNNNNNNNN.......
......NNNNNNNNNNNNNN......
.....NNNNNNNNNNNNNNNN.....
.....NNNWWWNNNNWWWNNN.....
....NNNWWDDWNNWWDDWNNN....
....NNNWDDDWNNWDDDWNNN....
...NNNNWDDDWNNWDDDWNNNN...
..NNNNNWWWWWNNWWWWWNNNNN..
.NNNNNNNWWWNNNNWWWNNNNNNN.
NNNNNNNNNNNNNNNNNNNNNNNNNN
NNNNNNNNNNNNNNNNNNNNNNNNNN
.NNN.NNNNNNNNNNNNNNNN.NNN.
.....NNNNNNNNNNNNNNNN.....
......NNNNNNNNNNNNNN......
.......NNNN....NNNN.......
.......NNNN....NNNN.......
"""

LARGE = """
...................NNNNNNNNNN...................
...................NNNNNNNNNN...................
...............NNNNNNNNNNNNNNNNNN...............
...............NNNNNNNNNNNNNNNNNN...............
.............NNNNNNNNNNNNNNNNNNNNNN.............
.............NNNNNNNNNNNNNNNNNNNNNN.............
...........NNNNNNNNNNNNNNNNNNNNNNNNNN...........
...........NNNNNNNNNNNNNNNNNNNNNNNNNN...........
.........NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN.........
.........NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN.........
.........NNNNNNWWWWWNNNNNNNNWWWWWNNNNNN.........
.........NNNNNNWWWWWNNNNNNNNWWWWWNNNNNN.........
.......NNNNNNWWWDDDWWWNNNNWWWDDDWWWNNNNNN.......
.......NNNNNNWWDWWDDWWNNNNWWDWWDDWWNNNNNN.......
.......NNNNNNWWDWWDDWWNNNNWWDWWDDWWNNNNNN.......
.......NNNNNNWWDDDDDWWNNNNWWDDDDDWWNNNNNN.......
.....NNNNNNNNWWDDDDDWWNNNNWWDDDDDWWNNNNNNNN.....
.....NNNNNNNNWWWDDDWWWNNNNWWWDDDWWWNNNNNNNN.....
....NNNNNNNNNWWWWWWWWWNNNNWWWWWWWWWNNNNNNNNN....
...NNNNNNNNNNNNWWWWWNNNNNNNNWWWWWNNNNNNNNNNNN...
..NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN..
..NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN..
NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN
NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN
NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN
NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN
.NNNNNN..NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN..NNNNNN.
..NNNNN..NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN..NNNNN..
.........NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN.........
.........NNNNNNNNNNNNNNNNNNNNNNNNNNNNNN.........
...........NNNNNNNNNNNNNNNNNNNNNNNNNN...........
...........NNNNNNNNNNNNNNNNNNNNNNNNNN...........
.............NNNNNNNNNNNNNNNNNNNNNN.............
.............NNNNNNN........NNNNNNN.............
.............NNNNNNN........NNNNNNN.............
.............NNNNNNN........NNNNNNN.............
"""


def parse(grid):
    rows = grid.strip("\n").split("\n")
    width = len(rows[0])
    if any(len(r) != width for r in rows):
        raise ValueError("all rows of a sprite must have the same width")
    return rows


def render(grid, canvas, background=None, dy=0):
    """Return the sprite centered on a canvas x canvas image, one pixel per cell, moved dy rows."""
    rows = parse(grid)
    fill = (*background, 255) if background else (0, 0, 0, 0)
    img = Image.new("RGBA", (canvas, canvas), fill)
    x0 = (canvas - len(rows[0])) // 2
    y0 = (canvas - len(rows)) // 2 + dy
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            color = PALETTE[ch]
            if color is not None:
                img.putpixel((x0 + x, y0 + y), (*color, 255))
    return img


def scale(img, size):
    return img.resize((size, size), Image.NEAREST)


# Poses of the large drawing. Each eye pattern covers a 9 x 10 box; the left
# eye box starts at column 13 and the right one at column 26, both at row 10.
EYE_ROW, LEFT_EYE, RIGHT_EYE = 10, 13, 26

EYES = {
    "open": """
NNWWWWWNN
NNWWWWWNN
WWWDDDWWW
WWDWWDDWW
WWDWWDDWW
WWDDDDDWW
WWDDDDDWW
WWWDDDWWW
WWWWWWWWW
NNWWWWWNN
""",
    "left": """
NNWWWWWNN
NNWWWWWNN
WWDDDWWWW
WDWWDDWWW
WDWWDDWWW
WDDDDDWWW
WDDDDDWWW
WWDDDWWWW
WWWWWWWWW
NNWWWWWNN
""",
    "right": """
NNWWWWWNN
NNWWWWWNN
WWWWDDDWW
WWWDWWDDW
WWWDWWDDW
WWWDDDDDW
WWWDDDDDW
WWWWDDDWW
WWWWWWWWW
NNWWWWWNN
""",
    "closed": """
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NWWWWWWWN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
""",
    "happy": """
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNWWWNNN
NNWWWWWNN
NWWWNWWWN
WWWNNNWWW
WWNNNNNWW
NNNNNNNNN
NNNNNNNNN
""",
    "worried": """
NNNNNWWNN
NNNNWWWNN
NNNWWWWWW
NWWWWWWWW
WWWDDDWWW
WWDWWDDWW
WWDDDDDWW
WWDDDDDWW
WWWDDDWWW
NNWWWWWNN
""",
    "squint": """
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
WWWWWWWWW
WWWWWWWWW
NNNNNNNNN
NNNNNNNNN
NNNNNNNNN
""",
}

SWEAT = """
.S.
.S.
SSS
SSS
.S.
"""

QUESTION = """
.QQQ.
QQ.QQ
...QQ
..QQ.
..Q..
.....
..Q..
"""


def mirror(pattern):
    return "\n".join(row[::-1] for row in pattern.strip("\n").split("\n"))


def stamp(rows, pattern, x, y):
    """Draw a pattern on a list of row strings; "." in the pattern leaves the pixel as it is."""
    for dy, line in enumerate(pattern.strip("\n").split("\n")):
        row = list(rows[y + dy])
        for dx, ch in enumerate(line):
            if ch != ".":
                row[x + dx] = ch
        rows[y + dy] = "".join(row)


def pose(left="open", right=None, extras=()):
    """The large drawing with the given eyes and extra patterns [(pattern, x, y)]."""
    right = right or left
    rows = parse(LARGE)
    stamp(rows, EYES[left], LEFT_EYE, EYE_ROW)
    # The worried brows slope towards the outside of the face on both sides.
    stamp(rows, mirror(EYES[right]) if right == "worried" else EYES[right], RIGHT_EYE, EYE_ROW)
    for pattern, x, y in extras:
        stamp(rows, pattern, x, y)
    return "\n".join(rows)


def magnifier(grid, cx=30, cy=14):
    """Hold a magnifying glass over the right eye: ring, tinted glass and handle."""
    rows = [list(r) for r in parse(grid)]
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
            if 5.6 <= d < 7.4:
                row[x] = "L"
            elif d < 5.6 and ch == "N":
                row[x] = "G"
    for i in range(7):
        for t in (0, 1):
            x, y = cx + 5 + i + t, cy + 5 + i
            if 0 <= y < len(rows) and 0 <= x < len(rows[0]):
                rows[y][x] = "H"
    return "\n".join("".join(r) for r in rows)


def animations():
    """Name -> list of (grid, vertical offset, frame duration in ms)."""
    question = [(QUESTION, 41, 0)]
    return {
        "idle": [(pose(), 0, 2400), (pose("closed"), 0, 140), (pose(), 0, 1600), (pose("closed"), 0, 140)],
        "scan": [
            (magnifier(pose("left")), 0, 260),
            (magnifier(pose()), -1, 260),
            (magnifier(pose("right")), 0, 260),
            (magnifier(pose()), -1, 260),
        ],
        "happy": [(pose("happy"), 0, 220), (pose("happy"), -3, 220)],
        "worried": [
            (pose("worried", extras=[(SWEAT, 36, 5)]), 0, 900),
            (pose("worried", extras=[(SWEAT, 36, 7)]), 0, 900),
        ],
        "confused": [
            (pose("open", "squint", question), 0, 700),
            (pose("left", "squint", question), 0, 700),
        ],
    }


def rounded(img, radius_ratio=0.22):
    """Clip an image to a rounded square, for the application icon."""
    size = img.width
    mask = Image.new("L", (size, size), 0)
    radius = int(size * radius_ratio)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius, fill=255)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


def icon(size):
    """The application icon at one size, from the drawing that fits that size best."""
    if size == 16:
        tile = render(TINY, 16, BACKGROUND)
    elif size == 24:
        tile = render(MEDIUM, 24, BACKGROUND)
    elif size <= 32:
        base = render(SMALL, 32, BACKGROUND)
        tile = base if size == 32 else base.resize((size, size), Image.LANCZOS)
    elif size % 64 == 0:
        tile = scale(render(LARGE, 64, BACKGROUND), size)
    else:
        # Sizes between the drawings are smoothed down from the next larger one.
        tile = render(LARGE, 64, BACKGROUND).resize((size, size), Image.LANCZOS)
    return rounded(tile)


def write_ico(path, images):
    """Write a Windows .ico holding the given square images as PNG entries."""
    blobs = []
    for img in images:
        buf = io.BytesIO()
        img.save(buf, "PNG")
        blobs.append(buf.getvalue())
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    for img, blob in zip(images, blobs):
        side = img.width if img.width < 256 else 0
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    Path(path).write_bytes(header + entries + b"".join(blobs))


def write_animation(out, name, frames):
    """Save a horizontal sprite sheet of 64-pixel frames and a 4x preview GIF."""
    images = [render(grid, 64, dy=dy) for grid, dy, _ in frames]
    sheet = Image.new("RGBA", (64 * len(images), 64), (0, 0, 0, 0))
    for i, img in enumerate(images):
        sheet.paste(img, (64 * i, 0))
    sheet.save(out / f"qubis-{name}.png")
    previews = []
    for img in images:
        bg = Image.new("RGBA", img.size, (0xF4, 0xF6, 0xFB, 255))
        bg.alpha_composite(img)
        previews.append(scale(bg.convert("RGB"), 256))
    durations = [ms for _, _, ms in frames]
    previews[0].save(
        out / f"preview-{name}.gif", save_all=True, append_images=previews[1:], duration=durations, loop=0
    )


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent / "out"
    out.mkdir(parents=True, exist_ok=True)
    render(TINY, 16).save(out / "qubis-tiny-16.png")
    render(MEDIUM, 24).save(out / "qubis-medium-24.png")
    render(SMALL, 32).save(out / "qubis-small-32.png")
    render(LARGE, 64).save(out / "qubis-large-64.png")
    scale(render(LARGE, 64), 512).save(out / "qubis-512.png")
    sizes = (16, 20, 24, 32, 40, 48, 64, 128, 256)
    for size in (*sizes, 1024):
        icon(size).save(out / f"icon-{size}.png")
    write_ico(out / "xtiger.ico", [icon(size) for size in sizes])
    for name, frames in animations().items():
        write_animation(out, name, frames)
    print(f"wrote sprites to {out}")


if __name__ == "__main__":
    main()
