"""The gallery that tier 3's showcase shell scene shows
(guest/screens/showcase-shell.sh, t3_showcase.py): six pictures, four
computed here from a few lines of arithmetic and two photographs in the
public domain, committed in tools/tier3/showcase-photos/ with their
provenance in PHOTOS, and vtium's gallery tool, tools/gallery/gallery, which
the scene runs to show them. t3_showcase.disc_screens puts the pictures and
the tool on the run's disc (`disc`), and t3_showcase.page lists each
photograph's source on the showcase's page."""

import hashlib
import math
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3img  # noqa: E402
from t3_common import ROOT, rel  # noqa: E402


# Each picture is 256x160 pixels, and the gallery tool shows it four rows
# tall in the shell scene's cell, Iosevka Term's 15x38, in a box of 16 cells
# by 4 rows.
GALLERY_SIZE = (256, 160)
# The shell scene's cell in pixels, which its frit, showcase-shell, states.
CELL = (15, 38)
PHOTO_DIR = os.path.join(ROOT, "tools", "tier3", "showcase-photos")
TOOL = os.path.join(ROOT, "tools", "gallery", "gallery")


def render(shade, width, height, samples):
    """A t3img.Image of `width` by `height` pixels, each the average of
    `samples` by `samples` points of `shade(u, v)`, which gives the (r, g, b)
    color at the point (u, v) of the unit square, u across and v down."""
    out = bytearray()
    n = samples * samples
    for y in range(height):
        for x in range(width):
            acc = [0.0, 0.0, 0.0]
            for sy in range(samples):
                for sx in range(samples):
                    c = shade((x + (sx + 0.5) / samples) / width,
                              (y + (sy + 0.5) / samples) / height)
                    acc = [a + b for a, b in zip(acc, c)]
            out += bytes(min(255, max(0, int(a / n + 0.5))) for a in acc)
    return t3img.Image(width, height, bytes(out))


def mix(a, b, t):
    """The color `t` of the way from `a` to `b`."""
    return tuple(p + (q - p) * t for p, q in zip(a, b))


def ramp(stops, t):
    """The color at `t`, clamped to 0 to 1, of a gradient through `stops`,
    each (t, color) in rising order."""
    t = min(1.0, max(0.0, t))
    for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
        if t <= t1:
            return mix(c0, c1, (t - t0) / (t1 - t0))
    return stops[-1][1]


def hex_tiles(u, v):
    """Hexagonal tiles in teal, gold, coral and deep blue, each shaded
    darker toward its edge and lit from above, with dark grout between."""
    size = 0.085
    x, y = (u - 0.5) * 1.6 / size, (v - 0.5) / size
    # The tile holding (x, y), in axial coordinates rounded to the nearest
    # hexagon's center.
    q, r = math.sqrt(3) / 3 * x - y / 3, 2 / 3 * y
    s = -q - r
    rq, rr, rs = round(q), round(r), round(s)
    dq, dr, ds = abs(rq - q), abs(rr - r), abs(rs - s)
    if dq > dr and dq > ds:
        rq = -rr - rs
    elif dr > ds:
        rr = -rq - rs
    px, py = x - math.sqrt(3) * (rq + rr / 2), y - 1.5 * rr
    edge = max(abs(px) * 2 / math.sqrt(3), abs(px) / math.sqrt(3) + abs(py))
    if edge > 0.9:
        return (25, 22, 30)
    color = [(30, 130, 140), (235, 180, 60), (230, 100, 85), (20, 80, 110)][(rq * 2 + rr * 3) % 4]
    light = 1.1 - 0.35 * edge - 0.15 * py
    return tuple(min(255.0, c * light) for c in color)


def julia_set(u, v):
    """The Julia set of c = -0.7269 + 0.1889i, deep violet, edged in a glow
    that runs from purple through pink and orange to cream; each point
    outside it is colored by how fast it escapes."""
    zr, zi = (u - 0.5) * 3.2, (v - 0.5) * 2.0
    for n in range(160):
        zr2, zi2 = zr * zr, zi * zi
        if zr2 + zi2 > 64:
            mu = n + 1 - math.log(math.log(math.sqrt(zr2 + zi2))) / math.log(2)
            return ramp([(0, (10, 6, 40)), (0.25, (90, 20, 140)), (0.5, (220, 50, 140)),
                         (0.75, (255, 160, 60)), (1, (255, 245, 200))], math.sqrt(mu / 60))
        zi = 2 * zr * zi + 0.1889
        zr = zr2 - zi2 - 0.7269
    return (40, 10, 60)


def plasma(u, v):
    """A plasma: four sine waves summed, the sum mapped around a rainbow."""
    x, y = u * 16, v * 10
    s = (math.sin(x * 0.9) + math.sin(y * 1.1 + 1) + math.sin((x + y) * 0.6)
         + math.sin(math.hypot(x - 8, y - 5) * 1.3)) / 4
    return tuple(255 * (0.5 + 0.45 * math.cos(2 * math.pi * (s + k))) for k in (0, 1 / 3, 2 / 3))


def sunset_hills(u, v):
    """A pale sun setting in a sky that fades from indigo through pink to
    orange, behind three ridges of hills, each nearer one darker."""
    color = None
    for base, amp, freq, phase, c in [(0.62, 0.05, 3.0, 0.4, (120, 60, 110)),
                                      (0.72, 0.06, 5.0, 1.7, (80, 35, 80)),
                                      (0.84, 0.05, 7.0, 3.1, (40, 18, 45))]:
        if v > base + amp * (math.sin(u * freq + phase) + 0.4 * math.sin(u * freq * 2.7 + phase)):
            color = c
    if color:
        return color
    d = math.hypot((u - 0.62) * 1.6, v - 0.52)
    if d < 0.13:
        return (255, 236, 170)
    sky = ramp([(0, (40, 28, 90)), (0.35, (150, 50, 120)), (0.6, (245, 110, 80)),
                (0.75, (255, 180, 90))], v)
    return mix(sky, (255, 220, 140), 0.8 * max(0.0, 1 - d / 0.45) ** 2)


# The computed pictures: each file's name, the function that shades it, and
# the samples a side each pixel averages, so edges are smooth.
COMPUTED = [("hex-tiles.png", hex_tiles, 3), ("julia-set.png", julia_set, 2),
            ("plasma.png", plasma, 2), ("sunset-hills.png", sunset_hills, 3)]


class Photo:
    """One photograph of the gallery: its file in PHOTO_DIR, what it shows,
    who made it, its license, the page that describes the original, the
    original's URL and SHA-256, how the original was made into the file,
    and the file's SHA-256."""

    def __init__(self, file, title, author, license, page, url, sha256, made, file_sha256):
        self.file, self.title, self.author, self.license = file, title, author, license
        self.page, self.url, self.sha256 = page, url, sha256
        self.made, self.file_sha256 = made, file_sha256


PHOTOS = [
    Photo("earthrise.png", "Earthrise, the Earth rising over the Moon's horizon, photograph "
          "AS08-14-2383, 24 December 1968",
          "Bill Anders, Apollo 8, NASA", "public domain, a work of NASA",
          "https://commons.wikimedia.org/wiki/File:NASA-Apollo8-Dec24-Earthrise.jpg",
          "https://upload.wikimedia.org/wikipedia/commons/a/a8/NASA-Apollo8-Dec24-Earthrise.jpg",
          "24fd2a4f833534ca6741ed96d03b028ac1d8fda2cd129ded4d208535f541d4da",
          "the 2400x2400 original cropped to 1760x1100 at 480,1000 and scaled to 256x160 with "
          "ImageMagick 7.1.2's Lanczos filter",
          "b2474765e226e3ba5d2b305d56e7a60e2ee2eca13e4bc9eb1d8f78ebf76c6438"),
    Photo("whole-earth.png", "The Blue Marble, the whole Earth, photograph AS17-148-22727, "
          "7 December 1972",
          "the crew of Apollo 17, NASA", "public domain, a work of NASA",
          "https://commons.wikimedia.org/wiki/File:The_Earth_seen_from_Apollo_17.jpg",
          "https://upload.wikimedia.org/wikipedia/commons/9/97/The_Earth_seen_from_Apollo_17.jpg",
          "51049ed4e34514880bfbe39d34e2dd2c56e31f55919e7f4f0b34ef1698cb9f50",
          "the 3000x3002 original scaled to 160x160 with ImageMagick 7.1.2's Lanczos filter and "
          "set on black, 48 pixels each side, to 256x160",
          "7bcd13a03aa710cf37f95e02895465c7de8125e57a6ab35665eb060729a7c98f"),
]
# The gallery's files in name order, the order the gallery tool shows a
# directory in.
GALLERY = sorted([name for name, _, _ in COMPUTED] + [p.file for p in PHOTOS])


def gallery(directory, size=GALLERY_SIZE):
    """Writes the gallery into `directory`: each computed picture at `size`,
    and each photograph copied from PHOTO_DIR once it is found to be the
    pinned file. Returns the names, in GALLERY's order."""
    os.makedirs(directory, exist_ok=True)
    for photo in PHOTOS:
        src = os.path.join(PHOTO_DIR, photo.file)
        with open(src, "rb") as f:
            got = hashlib.sha256(f.read()).hexdigest()
        if got != photo.file_sha256:
            raise RuntimeError("%s has SHA-256 %s, not %s" % (rel(src), got, photo.file_sha256))
        shutil.copyfile(src, os.path.join(directory, photo.file))
    for name, shade, samples in COMPUTED:
        t3img.write_png(os.path.join(directory, name), render(shade, *size, samples))
    return GALLERY


def disc(showcase):
    """Puts the gallery on the disc, in the directory `showcase` the scenes
    read: the pictures in pictures/ and the gallery tool as bin/gallery."""
    gallery(os.path.join(showcase, "pictures"))
    os.makedirs(os.path.join(showcase, "bin"), exist_ok=True)
    shutil.copy(TOOL, os.path.join(showcase, "bin", "gallery"))
