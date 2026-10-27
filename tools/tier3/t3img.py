#!/usr/bin/env python3
"""Image I/O and masked comparison with tolerances for the tier 3 harness.

Stdlib only. Reads binary PPM (P6, what QEMU's screendump writes) and 8-bit
RGB/RGBA PNG; writes PNG through zlib, so goldens of very large screens stay
small enough to commit.

A golden may have a tolerance sidecar beside it, `<screen>.tol`, of key=value
lines (`#` starts a comment):

    max_delta=1          largest per-channel difference allowed
    max_pixels=200       how many pixels may differ
    mask=0,780,1280,800  a rectangle x0,y0,x1,y1 (end-exclusive) left out of
                         the comparison; repeat for more

No sidecar means exact. An absent max_delta or max_pixels is 0. The verdict is
"pass" when nothing differs outside the masks, "near" when something does but
within both limits, and "fail" otherwise.

Tier 2 is building its own compare tool, pixcmp, in parallel. The two should
be merged once both exist.

Usage:
    t3img.py compare ACTUAL GOLDEN [--tol FILE] [--diff OUT.png]
    t3img.py topng IN OUT.png
"""

import argparse
import json
import struct
import sys
import zlib

MAX_REGIONS = 16
THUMB_WIDTH = 480


class Image:
    """An RGB image: width, height and packed 8-bit RGB rows."""

    def __init__(self, width, height, data):
        if len(data) != width * height * 3:
            raise ValueError("pixel data does not match %dx%d" % (width, height))
        self.width = width
        self.height = height
        self.data = data

    def row(self, y):
        stride = self.width * 3
        return self.data[y * stride:(y + 1) * stride]


# --- reading and writing ----------------------------------------------------

def _ppm_tokens(buf, count):
    """Return `count` header tokens of a PPM and the offset after them."""
    tokens, i = [], 0
    while len(tokens) < count:
        while buf[i:i + 1].isspace():
            i += 1
        if buf[i:i + 1] == b"#":
            while buf[i:i + 1] not in (b"\n", b""):
                i += 1
            continue
        j = i
        while not buf[j:j + 1].isspace():
            j += 1
        tokens.append(buf[i:j])
        i = j
    return tokens, i + 1


def read_ppm(buf):
    """A binary PPM. QEMU's screendump (11.1) pads each row to a multiple of
    4 bytes while the header gives the width, so a width whose rows are not
    a multiple of 4 bytes (1366 pixels: 4098 bytes) carries 2 bytes more per
    row; those are dropped. A body of any other size is refused, so a
    capture still being written is never read as a whole one."""
    (magic, w, h, maxval), off = _ppm_tokens(buf, 4)
    if magic != b"P6" or maxval != b"255":
        raise ValueError("only 8-bit binary PPM (P6) is supported")
    w, h = int(w), int(h)
    row, body = w * 3, len(buf) - off
    padded = (row + 3) & ~3
    if body == h * row:
        return Image(w, h, bytes(buf[off:off + h * row]))
    if body == h * padded:
        return Image(w, h, b"".join(buf[off + y * padded:off + y * padded + row] for y in range(h)))
    raise ValueError("a %dx%d PPM holds %d bytes of pixels, not %d or %d"
                     % (w, h, body, h * row, h * padded))


def _paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def read_png(buf):
    if buf[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    off, idat, hdr = 8, [], None
    while off < len(buf):
        length, kind = struct.unpack(">I4s", buf[off:off + 8])
        chunk = buf[off + 8:off + 8 + length]
        off += 12 + length
        if kind == b"IHDR":
            hdr = struct.unpack(">IIBBBBB", chunk)
        elif kind == b"IDAT":
            idat.append(chunk)
        elif kind == b"IEND":
            break
    w, h, depth, ctype, _, _, interlace = hdr
    if depth != 8 or ctype not in (2, 6) or interlace:
        raise ValueError("only 8-bit non-interlaced RGB/RGBA PNG is supported")
    bpp = 3 if ctype == 2 else 4
    raw = zlib.decompress(b"".join(idat))
    stride = w * bpp
    prev = bytearray(stride)
    out = bytearray()
    pos = 0
    for _ in range(h):
        ftype = raw[pos]
        line = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if ftype == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 0xFF
        elif ftype == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ftype == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ftype == 4:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                upleft = prev[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + _paeth(left, prev[i], upleft)) & 0xFF
        elif ftype != 0:
            raise ValueError("bad PNG filter %d" % ftype)
        prev = line
        if bpp == 4:
            del line[3::4]
        out += line
    return Image(w, h, bytes(out))


def read(path):
    with open(path, "rb") as f:
        buf = f.read()
    return read_png(buf) if buf[:4] == b"\x89PNG" else read_ppm(buf)


def write_png(path, img):
    def chunk(kind, payload):
        return (struct.pack(">I", len(payload)) + kind + payload +
                struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF))

    # Filter type 0 on every row: screens are mostly flat colour, which zlib
    # compresses well without the row filters.
    raw = b"".join(b"\x00" + img.row(y) for y in range(img.height))
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", img.width, img.height,
                                           8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


def thumbnail(img, width=THUMB_WIDTH):
    """Downscale by an integer step, keeping one pixel per step (nearest)."""
    step = max(1, -(-img.width // width))
    if step == 1:
        return img
    rows = []
    for y in range(0, img.height, step):
        row = img.row(y)
        rows.append(b"".join(row[x:x + 3] for x in range(0, len(row), step * 3)))
    return Image(len(rows[0]) // 3, len(rows), b"".join(rows))


def diff_thumbnail(actual, points, width=THUMB_WIDTH):
    """Thumbnail of a diff: the actual screen dimmed, with every block that
    holds a differing pixel in red, so a one-pixel difference stays visible."""
    step = max(1, -(-actual.width // width))
    thumb = thumbnail(actual, width)
    out = bytearray(b // 3 for b in thumb.data)
    for x, y in points:
        i = ((y // step) * thumb.width + x // step) * 3
        out[i:i + 3] = b"\xff\x00\x00"
    return Image(thumb.width, thumb.height, bytes(out))


# --- tolerance sidecars -----------------------------------------------------

def load_tol(path):
    """The tolerance in a sidecar, or None (exact) when there is none.

    Returns {"max_delta": int, "max_pixels": int, "masks": [[x0,y0,x1,y1]]}.
    """
    if not path:
        return None
    try:
        with open(path) as f:
            lines = f.read().splitlines()
    except FileNotFoundError:
        return None
    tol = {"max_delta": 0, "max_pixels": 0, "masks": []}
    for n, line in enumerate(lines, 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        key, sep, value = (p.strip() for p in line.partition("="))
        try:
            if not sep:
                raise ValueError("expected key=value")
            if key in ("max_delta", "max_pixels"):
                tol[key] = int(value)
            elif key == "mask":
                rect = [int(v) for v in value.split(",")]
                if len(rect) != 4 or rect[0] >= rect[2] or rect[1] >= rect[3]:
                    raise ValueError("mask needs x0,y0,x1,y1 with x0<x1, y0<y1")
                tol["masks"].append(rect)
            else:
                raise ValueError("unknown key %r" % key)
        except ValueError as e:
            raise ValueError("%s:%d: %s" % (path, n, e))
    return tol


def verdict(stats, tol):
    """pass, near or fail for comparison stats under a tolerance (or None)."""
    if not stats["pixels"]:
        return "pass"
    if (tol and "size_mismatch" not in stats and stats["pixels"] <= tol["max_pixels"]
            and stats["max_delta"] <= tol["max_delta"]):
        return "near"
    return "fail"


# --- comparison -------------------------------------------------------------

def _gap(a, b):
    """Chebyshev distance between two boxes; 0 when they touch or overlap."""
    dx = max(0, max(a[0], b[0]) - min(a[2], b[2]))
    dy = max(0, max(a[1], b[1]) - min(a[3], b[3]))
    return max(dx, dy)


def _union(a, b):
    return [min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3])]


def _cluster(points, cell):
    """Boxes of 8-connected components of the cell-sized tiles holding points."""
    tiles = {}
    for x, y in points:
        key = (x // cell, y // cell)
        b = tiles.get(key)
        if b is None:
            tiles[key] = [x, y, x + 1, y + 1]
        else:
            tiles[key] = _union(b, [x, y, x + 1, y + 1])
    seen, boxes = set(), []
    for start in tiles:
        if start in seen:
            continue
        seen.add(start)
        stack, box = [start], tiles[start]
        while stack:
            tx, ty = stack.pop()
            box = _union(box, tiles[(tx, ty)])
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    n = (tx + dx, ty + dy)
                    if n in tiles and n not in seen:
                        seen.add(n)
                        stack.append(n)
        boxes.append(box)
    return boxes


def regions(points, cell=16, limit=MAX_REGIONS):
    """At most `limit` boxes around the differing pixels, for a contact sheet
    to outline. Pixels in touching cell-sized tiles share a box; coarser cells
    are used while there are many clusters, then the closest boxes merge."""
    if not points:
        return []
    boxes = _cluster(points, cell)
    while len(boxes) > 4 * limit:
        cell *= 2
        boxes = _cluster(points, cell)
    while len(boxes) > limit:
        _, i, j = min((_gap(boxes[i], boxes[j]), i, j)
                      for i in range(len(boxes)) for j in range(i + 1, len(boxes)))
        boxes[i] = _union(boxes[i], boxes[j])
        del boxes[j]
    return sorted(boxes, key=lambda b: (b[1], b[0]))


def regions_merged(points, cell=16, limit=MAX_REGIONS):
    """Whether regions() joins separate clusters to stay within `limit`
    boxes, so that a box can hold pixels that did not differ."""
    return len(_cluster(points, cell)) > limit


def compare(actual, golden, masks=()):
    """Compare two images exactly outside the mask rectangles.

    Returns (stats, diff_image_or_None, points). stats has pixels, bbox (the
    clustered regions) and max_delta; a size mismatch counts every pixel and
    boxes the whole larger frame.
    """
    if (actual.width, actual.height) != (golden.width, golden.height):
        w, h = max(actual.width, golden.width), max(actual.height, golden.height)
        return {"pixels": w * h, "bbox": [[0, 0, w, h]], "max_delta": 255,
                "size_mismatch": [[actual.width, actual.height],
                                  [golden.width, golden.height]]}, None, []
    w = actual.width
    points, max_delta = [], 0
    for y in range(actual.height):
        ra, rg = actual.row(y), golden.row(y)
        if ra == rg:
            continue
        for x in range(w):
            i = x * 3
            pa, pg = ra[i:i + 3], rg[i:i + 3]
            if pa == pg:
                continue
            if any(x0 <= x < x1 and y0 <= y < y1 for x0, y0, x1, y1 in masks):
                continue
            points.append((x, y))
            max_delta = max(max_delta, *(abs(a - b) for a, b in zip(pa, pg)))
    stats = {"pixels": len(points), "bbox": regions(points), "max_delta": max_delta}
    if not points:
        return stats, None, points
    # The diff shows the actual screen dimmed, differing pixels in red and
    # mask rectangles tinted blue.
    out = bytearray(b // 3 for b in actual.data)
    for x0, y0, x1, y1 in masks:
        for y in range(max(0, y0), min(actual.height, y1)):
            for x in range(max(0, x0), min(w, x1)):
                out[(y * w + x) * 3 + 2] = min(255, out[(y * w + x) * 3 + 2] + 96)
    for x, y in points:
        i = (y * w + x) * 3
        out[i:i + 3] = b"\xff\x00\x00"
    return stats, Image(w, actual.height, bytes(out)), points


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("compare", help="compare ACTUAL with GOLDEN; exit 1 on fail")
    c.add_argument("actual")
    c.add_argument("golden")
    c.add_argument("--tol", help="tolerance sidecar (default: none, exact)")
    c.add_argument("--diff")
    t = sub.add_parser("topng", help="convert a PPM or PNG to PNG")
    t.add_argument("src")
    t.add_argument("dst")
    args = ap.parse_args(argv)
    if args.cmd == "topng":
        write_png(args.dst, read(args.src))
        return 0
    tol = load_tol(args.tol)
    stats, diff, _ = compare(read(args.actual), read(args.golden),
                             tol["masks"] if tol else ())
    if diff is not None and args.diff:
        write_png(args.diff, diff)
    stats["status"] = verdict(stats, tol)
    json.dump(stats, sys.stdout)
    print()
    return 1 if stats["status"] == "fail" else 0


if __name__ == "__main__":
    sys.exit(main())
