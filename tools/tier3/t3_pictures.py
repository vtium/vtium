#!/usr/bin/env python3
"""The test pictures of tier 3's image screens (guest/screens/img-*.sh).

This script makes them from a few lines of arithmetic, so nothing is
downloaded and the same script always writes the same pixels. The run writes
them into the disc's screens/pictures/ for a variant that names "pictures"
(t3_scenario.build_disc), and the screens send them to vtium as a program
would: as sixel strings, as kitty graphics commands with raw pixels, with
zlib-compressed pixels, and with PNG files that vtium's image helper decodes
(ADR-1101).

Every picture is a whole number of the variant's 8x16 cells, and every sixel
picture a whole number of sixel bands of 6 rows, so a picture drawn one cell
off, or cut short, moves whole cells of the capture. The colors are given as
sixel percentages, and the raw and PNG pictures carry the 8-bit values vtium
makes of them, so a sixel picture and its raw twin draw the same pixels.

    t3_pictures.py DIR    write every picture into DIR
"""

import hashlib
import os
import struct
import sys
import zlib

# The colors, as sixel percentages (r, g, b).
RED = (100, 0, 0)
GREEN = (0, 100, 0)
BLUE = (0, 0, 100)
YELLOW = (100, 100, 0)
ORANGE = (100, 50, 0)
VIOLET = (50, 0, 100)
CYAN = (0, 100, 100)
MAGENTA = (100, 0, 100)
WHITE = (100, 100, 100)


def eight(pct):
    """The 8-bit color vtium makes of a sixel percentage, rounded half up,
    as sixel-common.inc's `eight` gives it."""
    return tuple((c * 255 + 50) // 100 for c in pct)


class Picture:
    """A picture of `width` by `height` pixels, each a sixel color (r, g, b
    percentages) or None where the picture is transparent."""

    def __init__(self, width, height, pixel):
        self.width, self.height = width, height
        self.pixels = [[pixel(x, y) for x in range(width)] for y in range(height)]

    def rgba(self):
        """The raw pixels, 4 bytes each, row by row, as kitty's f=32 takes them."""
        out = bytearray()
        for row in self.pixels:
            for p in row:
                out += bytes(eight(p)) + b"\xff" if p else b"\0\0\0\0"
        return bytes(out)

    def png(self):
        """The picture as a PNG file, 8-bit RGBA, for the image helper to decode."""
        def chunk(kind, payload):
            return (struct.pack(">I", len(payload)) + kind + payload
                    + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF))

        stride = self.width * 4
        raw = self.rgba()
        rows = b"".join(b"\0" + raw[y * stride:(y + 1) * stride] for y in range(self.height))
        return (b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(rows, 9))
                + chunk(b"IEND", b""))

    def sixel(self):
        """The picture as a sixel string, DCS 0;1 q ... ST: square dots, and
        pixels no color sets left transparent (P2=1). Raster attributes give
        its size, each color its own register in the order it first appears,
        and a run of 4 or more of one sixel character is written with `!`."""
        colors = []
        for row in self.pixels:
            for p in row:
                if p and p not in colors:
                    colors.append(p)
        out = ["\x1bP0;1q\"1;1;%d;%d" % (self.width, self.height)]
        out += ["#%d;2;%d;%d;%d" % ((n + 1,) + c) for n, c in enumerate(colors)]
        for top in range(0, self.height, 6):
            band = []
            for n, c in enumerate(colors):
                chars = []
                for x in range(self.width):
                    bits = sum(1 << dy for dy in range(6)
                               if top + dy < self.height and self.pixels[top + dy][x] == c)
                    chars.append(chr(63 + bits))
                if any(ch != "?" for ch in chars):
                    band.append("#%d%s" % (n + 1, runs("".join(chars))))
            out.append("$".join(band) + "-")
        return ("".join(out) + "\x1b\\").encode()


def runs(chars):
    """`chars` with each run of 4 or more of one character written `!N` and
    the character, sixel's repeat introducer."""
    out, i = [], 0
    while i < len(chars):
        j = i
        while j < len(chars) and chars[j] == chars[i]:
            j += 1
        n = j - i
        out.append("!%d%s" % (n, chars[i]) if n >= 4 else chars[i] * n)
        i = j
    return "".join(out)


def quad(width, height, tl, tr, bl, br):
    """A picture in four quarters of colors `tl`, `tr`, `bl` and `br`, inside
    a white frame one pixel wide, so its edges and its turn show."""
    def pixel(x, y):
        if x in (0, width - 1) or y in (0, height - 1):
            return WHITE
        top, left = y < height // 2, x < width // 2
        return (tl if left else tr) if top else (bl if left else br)
    return Picture(width, height, pixel)


def bands(width, height, colors):
    """A picture of horizontal bands, one in each of `colors` from the top,
    each height / len(colors) pixels tall, inside a white frame."""
    each = height // len(colors)

    def pixel(x, y):
        if x in (0, width - 1) or y in (0, height - 1):
            return WHITE
        return colors[y // each]
    return Picture(width, height, pixel)


def disc(width, height, outer, inner):
    """A disc as wide as the picture's shorter side, in `outer` around a
    disc half as wide in `inner`, transparent outside it. A pixel is inside
    a disc of diameter d when (2x + 1 - w)^2 + (2y + 1 - h)^2 < d^2, as
    sixel-common.inc's circles draw them."""
    d = min(width, height)

    def pixel(x, y):
        r2 = (2 * x + 1 - width) ** 2 + (2 * y + 1 - height) ** 2
        if r2 < (d // 2) ** 2:
            return inner
        return outer if r2 < d * d else None
    return Picture(width, height, pixel)


# The pictures, by name. quad is 8 cells by 3 rows at 8x16, bands 8 by 6 with
# one band to a row, parent 6 by 2, child 4 by 2 and disc 6 by 3.
PICTURES = {
    "quad": quad(64, 48, RED, GREEN, BLUE, YELLOW),
    "bands": bands(64, 96, [RED, ORANGE, YELLOW, GREEN, BLUE, VIOLET]),
    "parent": quad(48, 32, ORANGE, VIOLET, VIOLET, ORANGE),
    "child": quad(32, 32, CYAN, MAGENTA, MAGENTA, CYAN),
    "disc": disc(48, 48, ORANGE, BLUE),
}


def files():
    """{file name: bytes} of every file the screens read: each sixel string
    (.six), raw pixels (.rgba), raw pixels compressed with zlib (.rgba.z)
    and PNG file (.png) they send."""
    p = PICTURES
    return {
        "quad.six": p["quad"].sixel(),
        "quad.rgba": p["quad"].rgba(),
        "quad.rgba.z": zlib.compress(p["quad"].rgba(), 9),
        "quad.png": p["quad"].png(),
        "bands.six": p["bands"].sixel(),
        "bands.rgba": p["bands"].rgba(),
        "parent.rgba": p["parent"].rgba(),
        "child.rgba": p["child"].rgba(),
        "disc.six": p["disc"].sixel(),
        "disc.png": p["disc"].png(),
    }


def write(directory):
    """Write every file of `files` into `directory`, made if missing; returns
    their names."""
    os.makedirs(directory, exist_ok=True)
    out = files()
    for name, data in sorted(out.items()):
        with open(os.path.join(directory, name), "wb") as f:
            f.write(data)
    return sorted(out)


def digest():
    """One SHA-256 over every picture's size and pixels, which the
    self-tests pin: the goldens of the images variant hold these pixels."""
    h = hashlib.sha256()
    for name, pic in sorted(PICTURES.items()):
        h.update(("%s %dx%d\n" % (name, pic.width, pic.height)).encode())
        h.update(pic.rgba())
    return h.hexdigest()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: t3_pictures.py DIR")
    for name in write(sys.argv[1]):
        print(os.path.join(sys.argv[1], name))
