#!/usr/bin/env python3
"""The test pictures of tier 3's image screens (guest/screens/img-*.sh).

This script makes them from a few lines of arithmetic, so nothing is
downloaded and the same script always writes the same pixels. The run writes
them into the disc's screens/pictures/ for a variant that names "pictures"
(t3_scenario.build_disc), and the screens send them to vtium as a program
would: as sixel strings, as kitty graphics commands with raw pixels, with
zlib-compressed pixels, and with PNG files that vtium's image helper decodes
(ADR-1101), as the frames of kitty animations (ADR-1102), and as iTerm2
pictures, among them an animated GIF (ADR-1103).

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


def shows(img, name, col, row, rows=None, part=None, cell=(8, 16)):
    """Where `img`, a capture or golden (t3img.Image), fails to show picture
    `name` at its own size with its top left corner at cell (`col`, `row`),
    counted from 1, in cells of `cell` pixels: the (x, y) of each opaque
    pixel of the picture that is not its color there, so an empty list
    means the picture is shown. With `rows`, a range, only those pixel rows
    of the picture count; with `part`, a pixel box (x0, y0, x1, y1), only
    that part of it, drawn at the cell. Clear pixels are not checked."""
    pic = PICTURES[name]
    x0, y0, x1, y1 = part or (0, 0, pic.width, pic.height)
    left, top = (col - 1) * cell[0], (row - 1) * cell[1]
    wrong = []
    for y in rows or range(y0, y1):
        for x in range(x0, x1):
            p = pic.pixels[y][x]
            if not p:
                continue
            ix, iy = left + x - x0, top + y - y0
            i = (iy * img.width + ix) * 3
            if not (0 <= ix < img.width and 0 <= iy < img.height) \
                    or tuple(img.data[i:i + 3]) != eight(p):
                wrong.append((x, y))
    return wrong


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


def fill(width, height, color):
    """A picture of one color, `width` by `height` pixels."""
    return Picture(width, height, lambda x, y: color)


# The pictures, by name. quad is 8 cells by 3 rows at 8x16, bands 8 by 6 with
# one band to a row, parent 6 by 2, child 4 by 2 and disc 6 by 3.
#
# The quad-turned pictures are the quarters turned a quarter clockwise, once
# and twice: with quad they are the three frames of quad.gif, an animated
# GIF sent as an iTerm2 picture (img-iterm2.sh), which plays once and stops
# on its last frame. anim-stopped and anim-ended are the frames the kitty
# animations of img-animation.sh show once they stop: the quarters with
# their top left and bottom right made cyan and magenta by PATCHES, and the
# child with its top left and bottom right made yellow. No file holds them;
# the self-tests read the goldens for them.
PICTURES = {
    "quad": quad(64, 48, RED, GREEN, BLUE, YELLOW),
    "bands": bands(64, 96, [RED, ORANGE, YELLOW, GREEN, BLUE, VIOLET]),
    "parent": quad(48, 32, ORANGE, VIOLET, VIOLET, ORANGE),
    "child": quad(32, 32, CYAN, MAGENTA, MAGENTA, CYAN),
    "disc": disc(48, 48, ORANGE, BLUE),
    "quad-turned": quad(64, 48, BLUE, RED, YELLOW, GREEN),
    "quad-turned-twice": quad(64, 48, YELLOW, BLUE, GREEN, RED),
    "anim-stopped": quad(64, 48, CYAN, GREEN, BLUE, MAGENTA),
    "anim-ended": quad(32, 32, YELLOW, MAGENTA, MAGENTA, YELLOW),
}
# The pictures that existed before the animation, iTerm2 and placeholder
# screens; the goldens of the screens before them hold these alone.
FIRST_PICTURES = ("quad", "bands", "parent", "child", "disc")

# The frames of quad.gif, in order, and each frame's delay in hundredths of
# a second, which vtium shows for 10 ms a hundredth.
GIF_FRAMES = ("quad", "quad-turned", "quad-turned-twice")
GIF_DELAY = 10

# The pixels img-animation.sh lays on its images as kitty animation frames
# (a=f), each the inside of one quarter, so a frame made from it keeps the
# picture's white frame: 31 by 23 for a quarter of quad, 15 by 15 for one
# of child. They are not whole cells, as frame data need not be.
PATCHES = {
    "patch-cyan": fill(31, 23, CYAN),
    "patch-magenta": fill(31, 23, MAGENTA),
    "patch-violet": fill(31, 23, VIOLET),
    "patch-yellow": fill(15, 15, YELLOW),
}


def gif(frames, delay):
    """An animated GIF of `frames`, Pictures of one size with no clear
    pixel, each shown for `delay` hundredths of a second and left in place
    for the next (disposal method 1). It has no NETSCAPE2.0 extension, so
    it plays its frames once (ADR-1103 D2). One global color table holds
    every color the frames use, in the order they first appear.

    The image data is LZW codes of a fixed width: each code is a color's
    index, and a clear code comes before the decoder's table could grow
    past that width, so the file needs no compression at all and any GIF
    decoder reads it."""
    width, height = frames[0].width, frames[0].height
    colors = []
    for f in frames:
        for row in f.pixels:
            for p in row:
                if p not in colors:
                    colors.append(p)
    bits = max(1, (len(colors) - 1).bit_length())
    table = 1 << bits
    minimum = max(2, bits)
    clear, end = 1 << minimum, (1 << minimum) + 1
    # After a clear, the first literal adds no entry to the decoder's table
    # and each later one adds one, from clear + 2. The decoder widens its
    # codes once its next entry would be 1 << (minimum + 1), which the
    # (1 << minimum) - 1th literal reaches, so (1 << minimum) - 2 literals
    # fit. The writer sends one fewer before each clear, so a decoder that
    # widens one code early reads the same width too.
    run = (1 << minimum) - 3
    out = bytearray(b"GIF89a")
    out += struct.pack("<HHBBB", width, height, 0x80 | ((bits - 1) << 4) | (bits - 1), 0, 0)
    for n in range(table):
        out += bytes(eight(colors[n])) if n < len(colors) else b"\0\0\0"
    for f in frames:
        # the graphic control extension: disposal 1, the delay, no clear color
        out += b"\x21\xf9\x04" + struct.pack("<BHB", 1 << 2, delay, 0) + b"\0"
        out += b"\x2c" + struct.pack("<HHHHB", 0, 0, width, height, 0)
        indices = [colors.index(p) for row in f.pixels for p in row]
        codes = []
        for i in range(0, len(indices), run):
            codes.append(clear)
            codes += indices[i:i + run]
        codes.append(end)
        width_bits = minimum + 1
        acc = nacc = 0
        data = bytearray()
        for c in codes:
            acc |= c << nacc
            nacc += width_bits
            while nacc >= 8:
                data.append(acc & 0xFF)
                acc >>= 8
                nacc -= 8
        if nacc:
            data.append(acc)
        out.append(minimum)
        for i in range(0, len(data), 255):
            out.append(len(data[i:i + 255]))
            out += data[i:i + 255]
        out.append(0)
    return bytes(out + b"\x3b")


def files():
    """{file name: bytes} of every file the screens read: each sixel string
    (.six), raw pixels (.rgba), raw pixels compressed with zlib (.rgba.z),
    PNG file (.png) and animated GIF (.gif) they send."""
    p = PICTURES
    out = {name + ".rgba": patch.rgba() for name, patch in PATCHES.items()}
    out.update({
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
        "quad.gif": gif([p[name] for name in GIF_FRAMES], GIF_DELAY),
    })
    return out


def write(directory):
    """Write every file of `files` into `directory`, made if missing; returns
    their names."""
    os.makedirs(directory, exist_ok=True)
    out = files()
    for name, data in sorted(out.items()):
        with open(os.path.join(directory, name), "wb") as f:
            f.write(data)
    return sorted(out)


def digest(names=None):
    """One SHA-256 over the size and pixels of each picture `names` names,
    or of every picture and patch, which the self-tests pin: the goldens of
    the images variant hold these pixels."""
    every = dict(PICTURES, **PATCHES)
    h = hashlib.sha256()
    for name, pic in sorted((n, every[n]) for n in (names or every)):
        h.update(("%s %dx%d\n" % (name, pic.width, pic.height)).encode())
        h.update(pic.rgba())
    return h.hexdigest()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: t3_pictures.py DIR")
    for name in write(sys.argv[1]):
        print(os.path.join(sys.argv[1], name))
