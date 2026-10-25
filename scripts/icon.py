#!/usr/bin/env python3
"""Draws Compressy's app icon: the little guy from the home screen on a
paper tile, with a sparse Bayer-dithered field around him.

Writes the 1024 px master as PNG with the standard library alone; the
bundle script scales it with sips and packs it with iconutil.

    python3 scripts/icon.py assets/icon-1024.png
"""
import struct
import sys
import zlib

# Kept in step with src/ui/theme.rs and the sprite in src/ui/dither.rs.
PAPER = (243, 242, 236)
INK = (13, 13, 12)
ACCENT = (217, 82, 31)
GUY = [
    "########    ",
    "#......##   ",
    "#......#.#  ",
    "#......#..# ",
    "#......#####",
    "#..........#",
    "#..#....#..#",
    "#..#....#..#",
    "#.o......o.#",
    "#..#....#..#",
    "#...####...#",
    "#..........#",
    "#..........#",
    "############",
    "  #     #   ",
    " ##    ##   ",
]

SIZE = 1024
# The macOS icon grid: an 824 px tile, 100 px in, with continuous-ish corners.
TILE, INSET, RADIUS, RULE = 824, 100, 185, 14
BIT = 26

BAYER = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
]


def tile_sdf(x, y):
    """Signed distance to the rounded tile, negative inside."""
    half = TILE / 2
    cx, cy = x - (INSET + half), y - (INSET + half)
    qx, qy = abs(cx) - (half - RADIUS), abs(cy) - (half - RADIUS)
    outside = (max(qx, 0) ** 2 + max(qy, 0) ** 2) ** 0.5
    return outside + min(max(qx, qy), 0) - RADIUS


def guy_cell(bx, by):
    if 0 <= by < len(GUY) and 0 <= bx < len(GUY[by]):
        return GUY[by][bx]
    return " "


def guy_color(x, y):
    """The sprite's colour at a pixel, or None where it's clear."""
    ox = SIZE // 2 - len(GUY[0]) * BIT // 2
    oy = SIZE // 2 - len(GUY) * BIT // 2
    bx, by = (x - ox) // BIT, (y - oy) // BIT
    if x < ox or y < oy:
        return None
    c = guy_cell(bx, by)
    if c == "#":
        return INK
    if c == ".":
        return PAPER
    if c == "o":
        return ACCENT
    # His shadow, down and right of the body (not the feet): a fine 50%
    # checker, which at this size reads as his dithered shadow in the app.
    if (
        bx > 0
        and 1 <= by < len(GUY) - 1
        and guy_cell(bx - 1, by - 1) != " "
        and (x // 4 + y // 4) % 2 == 0
    ):
        return INK
    return None


def field(x, y):
    """Ink where the dither falls: none by the guy, thicker to the edges."""
    cell = 8
    dx, dy = (x - SIZE / 2) / (TILE / 2), (y - SIZE / 2) / (TILE / 2)
    t = min(1.0, (dx * dx + dy * dy) ** 0.5)
    density = max(0.0, (t - 0.55) / 0.45) ** 1.5 * 0.3
    return BAYER[(y // cell) % 8][(x // cell) % 8] / 64 < density


def pixel(x, y):
    d = tile_sdf(x + 0.5, y + 0.5)
    cover = min(1.0, max(0.0, 0.5 - d))
    if cover == 0:
        return (0, 0, 0, 0)
    if d > -RULE:
        rgb = INK
    else:
        rgb = guy_color(x, y) or (INK if field(x, y) else PAPER)
    return (*rgb, round(cover * 255))


def write_png(path, width, height, rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\x00" + bytes(v for px in row for v in px) for row in rows)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "assets/icon-1024.png"
    rows = [[pixel(x, y) for x in range(SIZE)] for y in range(SIZE)]
    write_png(out, SIZE, SIZE, rows)
    print(out)
