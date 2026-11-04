#!/usr/bin/env python3
"""The showcase: screens of what vtium draws on an ordinary monitor, captured
by tier 3's variant showcase-1920x1080 for the documentation (ADR-1502 D6).

The variant's cases carry the status `capture`: they pass and judge
nothing, and no golden judges them. This module holds what the variant needs
beyond the harness:

- the fonts, each pinned by its URL and SHA-256 and fetched into a cache
  outside the tree, as the specimens pin theirs, and carried into the guest
  on the run's disc (`fetch`, `disc_fonts`);
- the sixel and kitty pictures the pictures scene sends, which this module
  computes from a few lines of arithmetic, so nothing is downloaded
  (`pictures`), and the gallery the shell scene shows, which
  t3_showcase_gallery.py makes (`gallery`);
- vtium's named palettes, read from vtium-term's table for the color scene
  (`palettes`);
- the pages: each scene's capture copied into docs/showcase/, the fonts
  scene's four captures cropped to their bands and stacked, the two
  small-machine goldens copied beside them, and the page that shows them,
  with the source of each font and each photograph (`compose`).

    t3_showcase.py fetch-fonts          fetch the pinned fonts into the cache
    t3_showcase.py compose [--out DIR]  write the showcase from the last run
"""

import argparse
import datetime
import hashlib
import io
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3_evaluate  # noqa: E402
import t3_showcase_gallery as gallery  # noqa: E402
import t3img  # noqa: E402
from t3_common import REPORT, ROOT, RUNDIR, rel  # noqa: E402

VARIANT = "showcase-1920x1080"
SPECIMEN_FONTS = os.path.join(ROOT, "tools", "specimens", "src", "fonts.rs")
PALETTE_RS = os.path.join(ROOT, "crates", "vtium-term", "src", "palette.rs")
# Where the guest finds the fonts: under /usr/local/share/fonts, where the
# glyph helper reads outline fonts (docs/managing-vtium.md, Fonts).
GUEST_FONTS = "/usr/local/share/fonts/vtium-showcase"
# Where the page and its images go, from the repository's root.
OUT = os.path.join(ROOT, "docs", "showcase")


def cache_dir():
    """The cache the pinned fonts are fetched into, beside the specimens'."""
    base = os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache")
    return os.path.join(base, "vtium", "showcase", "fonts")


def specimen_pin(file):
    """(url, sha256) of the font `file` that tools/specimens/src/fonts.rs
    pins, so a font the two sets share is pinned in one place."""
    with open(SPECIMEN_FONTS) as f:
        text = f.read()
    for block in re.findall(r"Pinned \{(.*?)\}", text, re.S):
        fields = dict(re.findall(r'(\w+): "([^"]*)"', block))
        if fields.get("file") == file:
            return fields["url"], fields["sha256"]
    raise ValueError("%s pins no font %s" % (rel(SPECIMEN_FONTS), file))


class Font:
    """One pinned font: its name, version and license as the page gives
    them, what the showcase draws with it, where it comes from in words,
    the download's URL and SHA-256, the file in that download when it is an
    archive (`member`, None when the download is the font), the font file's
    name in the cache and the guest, and that file's SHA-256."""

    def __init__(self, name, version, license, used, source, url, sha256, file,
                 file_sha256=None, member=None):
        self.name, self.version, self.license = name, version, license
        self.used, self.source = used, source
        self.url, self.sha256, self.member = url, sha256, member
        self.file, self.file_sha256 = file, file_sha256 or sha256

    def guest_path(self):
        return "%s/%s" % (GUEST_FONTS, self.file)


def _specimen(name, version, license, used, file):
    url, sha256 = specimen_pin(file)
    return Font(name, version, license, used,
                "the release's file, pinned by tools/specimens/src/fonts.rs", url, sha256, file)


# Every font the showcase draws with that vtium does not carry in its own
# tree. GNU Unifont is vtium's built-in font, so it needs no pin (BUILTIN).
FONTS = [
    Font("Iosevka Term", "34.9.1", "SIL Open Font License 1.1",
         "every scene's text, and a band of the fonts scene",
         "the project's release on GitHub, PkgTTF-IosevkaTerm-34.9.1.zip",
         "https://github.com/be5invis/Iosevka/releases/download/v34.9.1/"
         "PkgTTF-IosevkaTerm-34.9.1.zip",
         "3bd57a66a7f1e429a131d3bc35f4d0f3a8eb722f2fe8f59f595e424bda9a33c9",
         "IosevkaTerm-Regular.ttf",
         "430ad4f56c6cd82e4c954efb6dd4b3e2b4cc2b2b1bd4f8143f3d3800096f32b4",
         member="IosevkaTerm-Regular.ttf"),
    Font("Terminus", "4.49.1", "SIL Open Font License 1.1",
         "the bitmap band of the fonts scene, ter-v32n (16x32 cells)",
         "Arch Linux's terminus-font 4.49.1-8 in the archive snapshot 2026/10/20 that tier 3 "
         "pins, which builds the PSF files from the release's BDF sources",
         "https://archive.archlinux.org/repos/2026/10/20/extra/os/x86_64/"
         "terminus-font-4.49.1-8-any.pkg.tar.zst",
         "131b20ba0fcdaa98b7b26ae07939d67108b80770ed6ebcb746c4885f9642ea9f",
         "ter-v32n.psf.gz",
         "5ce141e1d3964bb25e11345e60a221f22ab12dd1ca49bee02bbae3cb035d7e63",
         member="usr/share/kbd/consolefonts/ter-v32n.psf.gz"),
    _specimen("JetBrains Mono", "2.304", "SIL Open Font License 1.1",
              "the second outline band of the fonts scene", "JetBrainsMono-Regular.ttf"),
    Font("Noto Sans CJK SC", "2.004", "SIL Open Font License 1.1",
         "Chinese, Japanese and Korean, which Iosevka Term lacks",
         "the Noto project's release Sans2.004 on GitHub",
         "https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/OTF/SimplifiedChinese/"
         "NotoSansCJKsc-Regular.otf",
         "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b",
         "NotoSansCJKsc-Regular.otf"),
    Font("Noto Sans Thai", "2.002", "SIL Open Font License 1.1",
         "Thai, which Iosevka Term lacks",
         "the Noto project's release NotoSansThai-v2.002 on GitHub",
         "https://github.com/notofonts/thai/releases/download/NotoSansThai-v2.002/"
         "NotoSansThai-v2.002.zip",
         "af889cc673fc714060ce5e4e088fbad32aa4c0571a19958efeaff128a22da485",
         "NotoSansThai-Regular.ttf",
         "61cf814eec46b294d6ea4401ac295d0cecd5207bd2331dcc5a15e7301d30ee44",
         member="NotoSansThai/hinted/ttf/NotoSansThai-Regular.ttf"),
    _specimen("Noto Color Emoji", "2.051", "SIL Open Font License 1.1", "color emoji",
              "NotoColorEmoji.ttf"),
    _specimen("Symbols Nerd Font Mono", "3.4.0", "MIT", "the prompt's symbols",
              "SymbolsNerdFontMono-Regular.ttf"),
]
# vtium's built-in font, which the fonts scene draws at twice its size.
BUILTIN = {"name": "GNU Unifont", "version": "17.0.03, built into vtium",
           "license": "SIL Open Font License 1.1 (COPYING)",
           "used": "a band of the fonts scene, at twice its 8x16 size",
           "source": "vtium's own tree, crates/vtium-font/data/unifont.hex"}


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def fault(directory, font):
    """Why `font`'s file in `directory` cannot be used: absent, or not the
    pinned bytes. None when it is the pinned file."""
    path = os.path.join(directory, font.file)
    if not os.path.isfile(path):
        return "%s is not in %s" % (font.file, directory)
    got = sha256_file(path)
    if got != font.file_sha256:
        return "%s has SHA-256 %s, not %s" % (font.file, got, font.file_sha256)
    return None


def extract(font, download):
    """The bytes of `font`'s file in the archive `download`: a zip archive,
    or a pacman package (a tar archive compressed with zstd, which the
    zstd program decompresses)."""
    if download.endswith(".zip"):
        with zipfile.ZipFile(download) as z:
            return z.read(font.member)
    if download.endswith(".tar.zst"):
        tar = subprocess.run(["zstd", "-dc", download], check=True,
                             capture_output=True).stdout
        with tarfile.open(fileobj=io.BytesIO(tar)) as t:
            return t.extractfile(font.member).read()
    raise ValueError("%s: an archive of no kind this module reads" % download)


def fetch(directory=None, fonts=FONTS, curl=("curl", "-fsSL", "--retry", "3")):
    """Fetches each font of `fonts` the cache `directory` lacks with curl,
    checking the download's SHA-256 and then the font file's; a download or
    a file that does not match is removed and the fetch fails. Returns the
    directory."""
    directory = directory or cache_dir()
    downloads = os.path.join(directory, "downloads")
    os.makedirs(downloads, exist_ok=True)
    for font in fonts:
        if fault(directory, font) is None:
            continue
        download = os.path.join(downloads, os.path.basename(font.url))
        if not os.path.isfile(download) or sha256_file(download) != font.sha256:
            part = download + ".part"
            subprocess.run(list(curl) + ["-o", part, font.url], check=True)
            got = sha256_file(part)
            if got != font.sha256:
                os.unlink(part)
                raise RuntimeError("%s has SHA-256 %s, not %s" % (font.url, got, font.sha256))
            os.replace(part, download)
        path = os.path.join(directory, font.file)
        if font.member is None:
            shutil.copyfile(download, path + ".part")
        else:
            with open(path + ".part", "wb") as f:
                f.write(extract(font, download))
        os.replace(path + ".part", path)
        why = fault(directory, font)
        if why:
            os.unlink(path)
            raise RuntimeError(why)
    return directory


def disc_fonts(disc, directory=None):
    """Copies every pinned font from the cache into the disc's
    showcase-fonts/, which guest/setup installs into GUEST_FONTS. Fails,
    naming the command that fetches them, when one is missing or not the
    pinned file."""
    directory = directory or cache_dir()
    for font in FONTS:
        why = fault(directory, font)
        if why:
            raise RuntimeError("%s; tools/tier3/showcase.sh fetches the pinned fonts (or "
                               "python3 tools/tier3/t3_showcase.py fetch-fonts)" % why)
    out = os.path.join(disc, "showcase-fonts")
    os.makedirs(out, exist_ok=True)
    for font in FONTS:
        shutil.copyfile(os.path.join(directory, font.file), os.path.join(out, font.file))


# The scenes, in the order the variant draws them. Each is a script,
# guest/screens/NAME.sh. The fonts scene is four screens, one in each font,
# whose captures compose crops and stacks into one image (FONT_BANDS).
SCENES = ["showcase-text", "showcase-emoji", "showcase-scripts", "showcase-color",
          "showcase-boxes", "showcase-pictures", "showcase-shell"]
FONT_BANDS = ["showcase-fonts-iosevka", "showcase-fonts-terminus", "showcase-fonts-unifont",
              "showcase-fonts-jetbrains"]
SCREENS = SCENES + FONT_BANDS
# Each image the page shows: its title on the page, its file in
# docs/showcase, the script whose header gives its caption and alt text
# (t3_evaluate.header), and the cases it is made of. The hero, the shell
# scene, comes first.
IMAGES = [
    ("A shell session", "hero.png", "showcase-shell.sh", ["showcase-shell"]),
    ("Outline text", "text.png", "showcase-text.sh", ["showcase-text"]),
    ("Emoji", "emoji.png", "showcase-emoji.sh", ["showcase-emoji"]),
    ("Scripts", "scripts.png", "showcase-scripts.sh", ["showcase-scripts"]),
    ("Color", "color.png", "showcase-color.sh", ["showcase-color"]),
    ("Box drawing", "boxes.png", "showcase-boxes.sh", ["showcase-boxes"]),
    ("Pictures", "pictures.png", "showcase-pictures.sh", ["showcase-pictures"]),
    ("Fonts", "fonts.png", "showcase-fonts.inc", FONT_BANDS),
]
# The two small-machine goldens the page shows below the scenes, copied into
# docs/showcase/small/ so the README and the site share one copy: each its
# golden, the copy's name, its alt text and its caption.
SMALL = [
    ("tools/tier3/goldens/2d-640x480-psf16/sgr.png", "small/sgr.png",
     "vtium on a 640x480 display: the labels fg30 to fg97 in their own colors and bg40 to "
     "bg107 on their own backgrounds, each set wrapped over two rows, with fg30 and bg40 "
     "drawn black on black; the words bold, underline, reverse, bold-underline-red, faint "
     "and italic in their styles, the 256-color cube as six bands, a 24-bit ramp from green "
     "to red, and a shell prompt with a block cursor",
     "vtium's minspec build draws this on a VT of tier 3's guest, which has 1 vCPU and "
     "512 MB, on a virtio-vga display at 640x480, in vtium's 8x16 PSF test font made from "
     "Unifont's glyphs, 80 by 30 cells. It is a tier 3 golden the maintainer reviewed and "
     "accepted. The font has no italic face, so `italic` draws upright."),
    ("tools/tier3/goldens/2d-320x240-psf8/small-console.png", "small/small-console.png",
     "vtium on a 320x240 display: a ruler of 40 columns, a colored directory listing, the 16 "
     "background colors, six color names, bold, faint, underlined, reversed and struck "
     "words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, "
     "four underline styles, a progress bar at 50%, and a prompt",
     "The same build draws this in the same guest on a 320x240 display, the smallest "
     "vtium supports, in vtium's 8x8 PSF test font made from Unifont's glyphs folded to "
     "half height, 40 by 30 cells. It is also a tier 3 golden the maintainer reviewed and "
     "accepted."),
]


# The pictures. Both are computed here and sent by showcase-pictures.sh:
# a sixel picture of the Mandelbrot set in at most 256 colors, and a kitty
# picture of a lit sphere over a 24-bit gradient, sent as raw pixels
# compressed with zlib (f=32, o=z).

def mandelbrot(width, height, iterations=200):
    """Rows of (r, g, b) pixels of the Mandelbrot set seen whole, each point
    outside it colored by how many iterations it took to escape, through a
    gradient of deep blue, sky blue, white, gold and black, and the set
    itself black: at most iterations + 1 colors. The points of the main
    cardioid and the bulb left of it are known to be in the set and skip
    the iteration."""
    stops = [(0.0, (0, 7, 100)), (0.16, (32, 107, 203)), (0.42, (237, 255, 255)),
             (0.6425, (255, 170, 0)), (0.8575, (0, 2, 0)), (1.0, (0, 7, 100))]

    def gradient(t):
        for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
            if t <= t1:
                f = (t - t0) / (t1 - t0)
                return tuple(int(a + (b - a) * f + 0.5) for a, b in zip(c0, c1))
        return stops[-1][1]

    palette = [gradient(math.sqrt(n / iterations) * 1.6 % 1.0) for n in range(iterations)]
    palette.append((0, 0, 0))
    scale = 2.5 / height
    rows = []
    for y in range(height):
        ci = (y - height / 2 + 0.5) * scale
        row = []
        for x in range(width):
            cr = -0.75 + (x - width / 2 + 0.5) * scale
            q = (cr - 0.25) ** 2 + ci * ci
            if q * (q + cr - 0.25) <= 0.25 * ci * ci or (cr + 1) ** 2 + ci * ci <= 0.0625:
                row.append(palette[iterations])
                continue
            zr = zi = 0.0
            n = 0
            while n < iterations:
                zr2, zi2 = zr * zr, zi * zi
                if zr2 + zi2 > 4.0:
                    break
                zi = 2 * zr * zi + ci
                zr = zr2 - zi2 + cr
                n += 1
            row.append(palette[n])
        rows.append(row)
    return rows


def sixel(rows):
    """`rows` of (r, g, b) pixels as a sixel string, DCS 0;1 q ... ST, its
    size in raster attributes and each color its own register, in sixel's
    percentages. At most 256 colors."""
    pct = [[tuple((c * 100 + 127) // 255 for c in p) for p in row] for row in rows]
    colors = sorted({p for row in pct for p in row})
    if len(colors) > 256:
        raise ValueError("a sixel picture of %d colors; at most 256" % len(colors))
    index = {c: n for n, c in enumerate(colors)}
    height, width = len(pct), len(pct[0])
    out = ["\x1bP0;1q\"1;1;%d;%d" % (width, height)]
    out += ["#%d;2;%d;%d;%d" % ((n,) + c) for n, c in enumerate(colors)]
    for top in range(0, height, 6):
        bits = {}
        for dy in range(min(6, height - top)):
            for x, p in enumerate(pct[top + dy]):
                bits.setdefault(index[p], [0] * width)[x] |= 1 << dy
        band = []
        for n in sorted(bits):
            chars = "".join(chr(63 + b) for b in bits[n]).rstrip("?")
            band.append("#%d%s" % (n, runs(chars)))
        out.append("$".join(band) + "-")
    return ("".join(out) + "\x1b\\").encode()


def runs(chars):
    """`chars` with each run of 4 or more of one character written `!N` and
    the character, sixel's repeat introducer."""
    return re.sub(r"(.)\1{3,}", lambda m: "!%d%s" % (len(m.group(0)), m.group(1)), chars)


def sphere(width, height):
    """Raw RGBA pixels, row by row, of a sphere lit from the upper left over
    a background that fades from violet at the top to teal at the bottom."""
    out = bytearray()
    cx, cy, radius = width / 2, height / 2, min(width, height) * 0.42
    light = (-0.5, -0.6, 0.62)
    norm = math.sqrt(sum(c * c for c in light))
    light = tuple(c / norm for c in light)
    for y in range(height):
        t = y / (height - 1)
        bg = (int(70 * (1 - t) + 10 * t), int(30 * (1 - t) + 110 * t),
              int(120 * (1 - t) + 120 * t))
        for x in range(width):
            dx, dy = (x + 0.5 - cx) / radius, (y + 0.5 - cy) / radius
            d2 = dx * dx + dy * dy
            if d2 > 1:
                out += bytes(bg) + b"\xff"
                continue
            dz = math.sqrt(1 - d2)
            diffuse = max(0.0, dx * light[0] + dy * light[1] + dz * light[2])
            spec = max(0.0, 2 * diffuse * dz - light[2]) ** 24
            base = (240, 140, 40)
            px = [min(255, int(c * (0.15 + 0.85 * diffuse) + 255 * spec)) for c in base]
            out += bytes(px) + b"\xff"
    return bytes(out)


def pictures(directory, sixel_size=(800, 600), kitty_size=(800, 600)):
    """Writes the pictures scene's pictures into `directory`: mandelbrot.six
    and sphere.rgba.z with its size in sphere.size. Returns their names."""
    os.makedirs(directory, exist_ok=True)
    with open(os.path.join(directory, "mandelbrot.six"), "wb") as f:
        f.write(sixel(mandelbrot(*sixel_size)))
    with open(os.path.join(directory, "sphere.rgba.z"), "wb") as f:
        f.write(zlib.compress(sphere(*kitty_size), 9))
    with open(os.path.join(directory, "sphere.size"), "w") as f:
        f.write("%d %d\n" % kitty_size)
    return ["mandelbrot.six", "sphere.rgba.z", "sphere.size"]


def palettes(path=PALETTE_RS):
    """vtium's named palettes from vtium-term's table, as (name, [16 colors],
    foreground, background), each color (r, g, b)."""
    with open(path) as f:
        text = f.read()
    out = []
    for block in re.findall(r"Named \{(.*?)\n    \}", text, re.S):
        name = re.search(r'name: "([^"]+)"', block).group(1)
        num = r"\s*(0x[0-9a-fA-F]+|\d+)\s*"
        rgb = [tuple(int(v, 0) for v in m) for m in
               re.findall(r"Rgb::new\(%s,%s,%s\)" % (num, num, num), block)]
        if len(rgb) != 18:
            raise ValueError("the palette %s has %d colors, not 16 and the default two"
                             % (name, len(rgb)))
        out.append((name, rgb[:16], rgb[16], rgb[17]))
    if not out:
        raise ValueError("%s holds no named palette" % rel(path))
    return out


def disc_screens(screens):
    """What the scenes read beside the screen scripts on the disc, in
    `screens`/showcase: the pictures, the gallery in gallery/, and
    palettes.txt, one named palette a line, its name then its 16 colors as
    r;g;b."""
    out = os.path.join(screens, "showcase")
    pictures(out)
    gallery.gallery(os.path.join(out, "gallery"))
    with open(os.path.join(out, "palettes.txt"), "w") as f:
        for name, colors, _, _ in palettes():
            f.write("%s %s\n" % (name, " ".join("%d;%d;%d" % c for c in colors)))


def faults(events, name):
    """What showcase screen `name` reported wrong on the event port, after
    the screen before it ended and before it ended itself: a font switch
    vtium did not take (T3-FONT-REFUSED FRIT NAMED, showcase.inc) and
    pictures missing from the disc (T3-NO-PICTURES). The scenario fails the
    case on any of them."""
    seen = []
    for e in events:
        if e == "T3-DONE %s" % name:
            return seen
        if e.startswith("T3-DONE "):
            seen = []
        elif e.startswith("T3-FONT-REFUSED "):
            frit, named = (e.split()[1:] + ["none"])[:2]
            seen.append("vtium did not switch to the font %s: OSC 50 named %s" % (frit, named))
        elif e == "T3-NO-PICTURES":
            seen.append("the showcase's pictures are not on the disc")
    return []


# The page: the scenes' captures, the fonts scene's bands stacked, the
# small-machine goldens, and docs/showcase/README.md.

def bands(events):
    """{frit: (rows, cell height)} from the event port's lines `T3-BAND FRIT
    rows=N cell=WxH`, which each band of the fonts scene reports."""
    out = {}
    for line in events:
        m = re.match(r"T3-BAND (\S+) rows=(\d+) cell=(\d+)x(\d+)$", line.strip())
        if m:
            out[m.group(1)] = (int(m.group(2)), int(m.group(4)))
    return out


def top(img, height):
    """The top `height` pixel rows of `img`."""
    if not 0 < height <= img.height:
        raise ValueError("a band of %d rows in a capture of %d" % (height, img.height))
    return t3img.Image(img.width, height, img.data[:img.width * 3 * height])


def stack(images):
    """`images` one above the other, each at its own size, unscaled; all are
    as wide as the display."""
    if len({img.width for img in images}) != 1:
        raise ValueError("bands of different widths: %s" % [img.width for img in images])
    return t3img.Image(images[0].width, sum(img.height for img in images),
                       b"".join(img.data for img in images))


def write_if_changed(path, img):
    """Writes `img` to `path` as PNG unless the file there already holds the
    same pixels; True when it wrote."""
    if os.path.exists(path):
        old = t3img.read(path)
        if (old.width, old.height, old.data) == (img.width, img.height, img.data):
            return False
    os.makedirs(os.path.dirname(path), exist_ok=True)
    t3img.write_png(path, img)
    return True


def copy_if_changed(src, dst):
    """Copies the file `src` to `dst` unless `dst` holds the same bytes;
    True when it copied."""
    if os.path.exists(dst) and sha256_file(dst) == sha256_file(src):
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copyfile(src, dst)
    return True


def caption(script):
    """(caption, alt text) from the header comment of `script` in the
    screens' directory (t3_evaluate.header)."""
    fields = t3_evaluate.header(None, script)
    return " ".join(fields[""]), " ".join(fields["alt"])


def image_of(name, cases, run, events):
    """The image `name` of IMAGES, made of the captures of `cases` in the run
    directory `run`: a scene's capture as it is, or, for the fonts scene,
    each band's capture cropped to the rows its band reported and the bands
    stacked."""
    captures = [t3img.read(os.path.join(run, case + ".png")) for case in cases]
    if len(cases) == 1:
        return captures[0]
    reported = bands(events)
    cropped = []
    for case, img in zip(cases, captures):
        frit = "showcase-" + case[len("showcase-fonts-"):]
        if frit not in reported:
            raise RuntimeError("%s: the band in %s reported no height on the event port"
                               % (name, frit))
        rows, height = reported[frit]
        cropped.append(top(img, rows * height))
    return stack(cropped)


def font_rows():
    """The page's font table rows: name, version, license, use, source and
    the file's SHA-256, the pinned fonts and the built-in one."""
    rows = [(f.name, f.version, f.license, f.used, "[%s](%s)" % (f.source, f.url),
             "`%s`" % f.file_sha256) for f in FONTS]
    rows.insert(2, (BUILTIN["name"], BUILTIN["version"], BUILTIN["license"], BUILTIN["used"],
                    BUILTIN["source"], "built into vtium"))
    return rows


def photo_rows():
    """The page's photograph table rows: the gallery's file, what it shows,
    its author, its license, the original with its SHA-256, how the file was
    made from it, and the file's SHA-256."""
    return [("`%s`" % p.file, p.title, p.author, p.license,
             "[the original](%s) ([its page](%s)), SHA-256 `%s`" % (p.url, p.page, p.sha256),
             p.made, "`%s`" % p.file_sha256) for p in gallery.PHOTOS]


def page(report, date):
    """docs/showcase/README.md: each image with its title, alt text and
    caption, the small-machine goldens, and how the screens were made: the
    build's commit, the date, the machine, each font, each photograph and
    each copied golden."""
    v = report["variants"][VARIANT]
    lines = ["<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. "
             "Change the scenes in tools/tier3/guest/screens/ and run it again; "
             "an edit here is lost. -->", "", "# Showcase", "",
             "These screens are vtium's own drawing on a Linux VT at 1920x1080. Tier 3, "
             "vtium's test harness in a virtual machine, ran vtium on a VT of its guest and "
             "captured each screen from the guest's display, and nothing in them is mocked "
             "up or edited. Each scene is a shell script that writes fixed bytes, so every "
             "run draws the same screen. The screens are made again for each release, and "
             "they judge nothing: [Concepts](../concepts.md) explains how vtium's output is "
             "judged.", ""]
    for title, name, script, _ in IMAGES:
        text, alt = caption(script)
        lines += ["## %s" % title, "", "![%s](%s)" % (alt, name), "", text, ""]
    lines += ["## Small machines", "",
              "The same console runs on a machine with 512 MB of memory and no GPU. These "
              "two screens are tier 3 goldens of vtium's smallest build, minspec.", ""]
    for _, name, alt, text in SMALL:
        lines += ["![%s](%s)" % (alt, name), "", text, ""]
    commit = report["tools"]["vtium"]
    console = v.get("console") or {}
    lines += ["## How these screens are made", "",
              "`tools/tier3/showcase.sh` built vtium at commit `%s`, whose console logged "
              "itself as vtium %s, ran tier 3's variant `%s` on %s, and wrote this page. "
              "The variant's machine has %d virtual CPUs and %d MB of memory, a virtio-vga "
              "display at 1920x1080 and Linux %s, and runs vtium built with the `images` "
              "feature, drawing on its CPU renderer in a cell of %s pixels. "
              "[tools/tier3/README.md](https://github.com/vtium/vtium/blob/main/tools/tier3/"
              "README.md#the-showcase) describes the variant."
              % (commit, console.get("revision", "of no revision"), VARIANT, date,
                 v["cpus"], v["memory_mb"], v["kernel"],
                 "x".join(str(n) for n in v["cell"] or ()) or "unknown"), "",
              "Every font may be redistributed. vtium carries no font file but GNU Unifont, "
              "so `t3_showcase.py` pins each other font by its address and SHA-256 and "
              "fetches it into a cache, and the run carries it into the guest.", "",
              "| Font | Version | License | Used for | Source | SHA-256 of the file |",
              "|---|---|---|---|---|---|"]
    lines += ["| %s |" % " | ".join(row) for row in font_rows()]
    computed = [name for name, _, _ in gallery.COMPUTED]
    lines += ["", "The hero's gallery shows six pictures. `t3_showcase_gallery.py` computes "
              "%s from a few lines of arithmetic. The other two are photographs in the "
              "public domain, shrunk once to %dx%d pixels and committed in "
              "`tools/tier3/showcase-photos/`:"
              % (", ".join("`%s`" % n for n in computed[:-1]) + " and `%s`" % computed[-1],
                 *gallery.GALLERY_SIZE), "",
              "| File | What it shows | Author | License | Source | Made by | "
              "SHA-256 of the file |", "|---|---|---|---|---|---|---|"]
    lines += ["| %s |" % " | ".join(row) for row in photo_rows()]
    lines += ["", "The small-machine screens are copies of these goldens:", "",
              "| Image | Copied from | SHA-256 |", "|---|---|---|"]
    for src, name, _, _ in SMALL:
        lines.append("| `%s` | [`%s`](https://github.com/vtium/vtium/blob/main/%s) | `%s` |"
                     % (name, src, src, sha256_file(os.path.join(ROOT, src))))
    return "\n".join(lines) + "\n"


def compose(run=None, report=REPORT, out=OUT):
    """Writes the showcase into `out` from the last run of the variant: each
    image of IMAGES, the small-machine goldens in small/, and README.md.
    Refuses a run in which any case of the variant is not a capture. Returns
    the files it wrote; an image whose pixels did not change is left as it
    is."""
    run = run or os.path.join(RUNDIR, VARIANT)
    with open(report) as f:
        rep = json.load(f)
    cases = {c["name"].split("/", 1)[1]: c for c in rep["cases"]
             if c["name"].startswith(VARIANT + "/")}
    wrong = ["%s is %s%s" % (case, c["status"], ": " + c["error"] if c.get("error") else "")
             for case, c in sorted(cases.items()) if c["status"] != "capture"]
    missing = sorted(set(SCREENS) - set(cases))
    if wrong or missing or VARIANT not in rep.get("variants", {}):
        raise RuntimeError("the last tier 3 run did not capture the showcase: %s"
                           % "; ".join(wrong + ["%s did not run" % m for m in missing]))
    with open(os.path.join(run, "events.log"), errors="replace") as f:
        events = f.read().splitlines()
    wrote = []
    for _, name, _, image_cases in IMAGES:
        path = os.path.join(out, name)
        if write_if_changed(path, image_of(name, image_cases, run, events)):
            wrote.append(path)
    for src, name, _, _ in SMALL:
        path = os.path.join(out, name)
        if copy_if_changed(os.path.join(ROOT, src), path):
            wrote.append(path)
    date = datetime.datetime.fromtimestamp(os.path.getmtime(report),
                                           datetime.timezone.utc).date().isoformat()
    readme = os.path.join(out, "README.md")
    with open(readme, "w") as f:
        f.write(page(rep, date))
    wrote.append(readme)
    return wrote


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("fetch-fonts", help="fetch the pinned fonts into the cache")
    comp = sub.add_parser("compose", help="write the showcase from the last tier 3 run")
    comp.add_argument("--out", default=OUT, help="the directory to write (docs/showcase)")
    args = parser.parse_args(argv)
    if args.command == "fetch-fonts":
        print("the showcase's fonts are in %s" % fetch())
    else:
        for path in compose(out=args.out):
            print("wrote %s" % rel(path))
    return 0


if __name__ == "__main__":
    sys.exit(main())
