"""Tier 3's case of the two font sets every build installs (ADR-1204 D2).

With no font named, vtium draws its default set `terminus`: bitmap Terminus
at nine strikes from 12 to 32 pixels, then GNU Unifont, then the built-in
font. The outline set `cullet-mono` draws Cullet Mono, then Noto Color Emoji
and Unifont. This case draws a row of probe characters with each set at
cells 16, 24 and 40 pixels high, one variant for each set and height, and
works out from the pixels which font drew each probe.

The font helper cannot say which of a set's sources drew a character: its
sources query names only the sources a machine lacks, and the specimen tells
the set's own sources apart only from the built-in font. So the case judges
the pixels instead.

- A bitmap source's glyph is read from the font file in the run's install
  tree, on the host, and drawn as the console draws it: 1 bit, at a whole
  factor. A probe passes when its ink matches the expected source's glyph
  exactly and matches no other source's glyph. The candidates are each
  Terminus strike and Unifont, at each whole factor that fits the cell.
- An outline glyph cannot be drawn on the host as the font helper draws it.
  So the cullet-mono variants switch the console, with `vtium-config
  use-frit`, to the reference set `cullet-alone`, which holds Cullet Mono
  Regular alone, and draw the same row again. The helper then draws Cullet
  Mono's own glyphs, and a probe that Cullet Mono draws in the set must be
  the same, pixel for pixel, in both captures, and match no bitmap
  source's glyph.
- A color emoji shows color, which no 1-bit source and no outline in the
  set can give, so a probe from Noto Color Emoji passes when its cells hold
  color.

vtium is judged in the cell the console draws in when each screen is
captured, from the display's top left corner, on row PROBE_ROW of the
screen that guest/screens/font-probe.sh prints. The case reads that cell
from vtium's log up to the capture (see `console_cell`). When the font
helper answers a set's cell query too late, vtium first draws in a
fallback cell and moves to the set's own cell later, so the last line that
names a cell before the capture is the one that counts. A capture drawn
while the set's cell was still not known fails the case.

References:
- ADR-1204 D2: the two sets, what draws each cell height, and the tier 3
  check this case runs.
- ADR-1200 D17: how a font size in points gives the cell's height.
"""

import gzip
import io
import os
import re
import tarfile

import t3img
from t3_qemu import wait_stable

CASE = "font-sources"
# The cell heights each set is drawn at, in pixels. Each variant's vtium.conf
# sets display-dpi=72, at which a font size in points is that many pixels.
SIZES = (16, 24, 40)
# The probes, with the column of the screen where font-probe.sh prints each.
# A wide character takes that column and the next.
PROBE_ROW = 1
LETTER, POWERLINE, HAN, PLANE2, EMOJI, ICON, LOGO = (
    "g", "", "中", "\U00020000", "\U0001f600", "", "")
COLUMNS = {LETTER: 2, POWERLINE: 5, HAN: 8, PLANE2: 12, EMOJI: 16, ICON: 20, LOGO: 23}
WIDE = {HAN, PLANE2, EMOJI}
NAMES = {LETTER: "the letter g", POWERLINE: "the Powerline separator U+E0B0",
         HAN: "the Han character U+4E2D", PLANE2: "U+20000", EMOJI: "the emoji U+1F600",
         ICON: "the Nerd Fonts icon U+F121", LOGO: "the Font Logos icon U+F303"}

# Where the install tree puts the fonts of both sets.
FONTS = "usr/share/vtium/fonts/"
UNIFONT = FONTS + "unifont_all-17.0.03.hex"
# The width of Unifont's narrow glyphs, in pixels; a wide glyph is twice it.
UNIFONT_WIDTH = 8
TERMINUS_STRIKES = (12, 14, 16, 18, 20, 22, 24, 28, 32)
# The reference set the cullet-mono variants switch to (guest/frits).
REFERENCE = "cullet-alone"

# What the case shows and what correct looks like, for the report.
CHECKS = {
    CASE: "A row of probe characters drawn with one of the two font sets every build installs, "
          "terminus with no font named or cullet-mono named, at a cell 16, 24 or 40 pixels "
          "high (ADR-1204 D2). The case works out from the pixels which font drew each probe. "
          "Correct for terminus: the letter and the Powerline separator from Terminus's strike "
          "of the cell's height, or its 32-pixel strike at 40; the Han character, U+20000 and "
          "the emoji from Unifont, in 1 bit; the Nerd Fonts icon as a hollow box. Correct for "
          "cullet-mono: the letter and the Nerd Fonts icon from Cullet Mono, the emoji in "
          "color from Noto Color Emoji, the Han character from Unifont, or a hollow box where "
          "the cell is narrower than Unifont's 8-pixel glyphs, and the Font Logos "
          "icon as a hollow box.",
}


def variants():
    """{name: variant} of the case: default-font-N with no font named and
    cullet-mono-N with font-name=cullet-mono, for each N of SIZES, on the
    minspec machine at 1366x768. Each logs in, draws no screen of the
    scenario and runs the check CASE. The cullet-mono variants run vtium
    built with the full package's features in place of the minspec
    binary, and install the reference set REFERENCE."""
    base = {
        "device": "virtio-vga,xres=1366,yres=768,edid=on",
        "display": "none",
        "renderer": "cpu",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "screens": [],
        "vt_switch": False,
        "checks": [CASE],
    }
    out = {}
    for size in SIZES:
        out["default-font-%d" % size] = dict(base, conf="vtium-default-font-%d.conf" % size,
                                             font_set="terminus", size=size)
        out["cullet-mono-%d" % size] = dict(base, conf="vtium-cullet-mono-%d.conf" % size,
                                            font_set="cullet-mono", size=size,
                                            features="bidi,gpu,images",
                                            deferred_frits=[REFERENCE + ".vtfrit"])
    return out


# --- the sources' glyphs ----------------------------------------------------

def parse_bdf(text, wanted):
    """{codepoint: (width, height, on)} of each codepoint of `wanted` that the
    BDF font `text` holds, where `on` is the set of (x, y) pixels of the
    glyph's bounding box that are set, from its top left."""
    out = {}
    code = None
    lines = iter(text.splitlines())
    for line in lines:
        words = line.split()
        if not words:
            continue
        if words[0] == "ENCODING":
            code = int(words[1])
        elif words[0] == "BBX":
            width, height = int(words[1]), int(words[2])
        elif words[0] == "BITMAP" and code in wanted:
            on = set()
            for y in range(height):
                digits = next(lines).strip()
                row, bits = int(digits, 16), 4 * len(digits)
                for x in range(width):
                    if row >> (bits - 1 - x) & 1:
                        on.add((x, y))
            out[code] = (width, height, on)
    return out


def parse_hex(lines, wanted):
    """{codepoint: (width, height, on)} of each codepoint of `wanted` in GNU
    Unifont's .hex format, `lines` of CODEPOINT:BITMAP: 16 rows, 8 or 16
    pixels wide."""
    out = {}
    for line in lines:
        head, _, bitmap = line.strip().partition(":")
        if not bitmap:
            continue
        code = int(head, 16)
        if code not in wanted:
            continue
        width = len(bitmap) * 4 // 16
        step = width // 4
        on = set()
        for y in range(16):
            row = int(bitmap[y * step:(y + 1) * step], 16)
            for x in range(width):
                if row >> (width - 1 - x) & 1:
                    on.add((x, y))
        out[code] = (width, 16, on)
    return out


def tree_member(tar, path):
    """The bytes of `path` in the install tree's tar, whose members may be
    named with or without a leading "./"."""
    for name in (path, "./" + path):
        try:
            return tar.extractfile(name).read()
        except KeyError:
            continue
    raise RuntimeError("the install tree holds no %s" % path)


def source_glyphs(tree_tar, probes):
    """{codepoint: {source name: glyph}} of each codepoint of `probes` in
    Terminus's regular strikes and Unifont, read from the install tree's tar
    at `tree_tar`. A source that lacks a codepoint is left out of its entry.
    The source names are "Terminus N px" and "Unifont"."""
    wanted = {ord(c) for c in probes}
    by_source = {}
    with tarfile.open(tree_tar) as tar:
        for strike in TERMINUS_STRIKES:
            data = gzip.decompress(tree_member(tar, FONTS + "ter-u%dn.bdf.gz" % strike))
            by_source["Terminus %d px" % strike] = parse_bdf(data.decode("latin-1"), wanted)
        text = io.TextIOWrapper(io.BytesIO(tree_member(tar, UNIFONT)), encoding="ascii")
        by_source["Unifont"] = parse_hex(text, wanted)
    return {code: {source: glyphs[code] for source, glyphs in by_source.items()
                   if code in glyphs} for code in wanted}


def candidates(glyphs, height):
    """[(name, height in pixels, on)] of every source glyph of `glyphs` (one
    codepoint's, {source: glyph}) at each whole factor whose glyph fits a
    cell `height` pixels high, named "SOURCE" at 1 and "SOURCE xK" at K."""
    out = []
    for source, glyph in glyphs.items():
        _, h, on = glyph
        for k in range(1, height // h + 1):
            scaled = {(x * k + i, y * k + j) for x, y in on for i in range(k) for j in range(k)}
            out.append((source if k == 1 else "%s x%d" % (source, k), scaled))
    return out


# --- the capture ------------------------------------------------------------

def pixel(img, x, y):
    i = (y * img.width + x) * 3
    return bytes(img.data[i:i + 3])


def probe_box(char, cell):
    """(x, y, width, height) of the cells the probe `char` takes in a grid
    of cells of `cell` (width, height) that starts at the display's top left
    corner."""
    cw, ch = cell
    cells = 2 if char in WIDE else 1
    return (COLUMNS[char] * cw, PROBE_ROW * ch, cells * cw, ch)


def region(char, cell):
    """(x, y, width, height) of the part of the screen judged for the probe
    `char`: its cells and a blank cell on each side, from the row above to
    the row below. vtium centres its grid on the display, so the grid starts
    up to a cell's width right of the display's left edge and up to a cell's
    height below its top. The probe's glyph lies inside this region wherever
    the grid starts, and font-probe.sh leaves every other cell of it blank."""
    x, y, w, h = probe_box(char, cell)
    return (x - cell[0], y - cell[1], w + 2 * cell[0], h + 2 * cell[1])


def paper(img, cell):
    """The color most pixels of the probe row have: the background."""
    counts = {}
    y0 = PROBE_ROW * cell[1]
    for y in range(y0, min(y0 + cell[1], img.height)):
        row = img.row(y)
        for i in range(0, len(row), 3):
            color = bytes(row[i:i + 3])
            counts[color] = counts.get(color, 0) + 1
    return max(counts, key=counts.get)


def ink(img, box, background):
    """{(x, y): color} of each pixel of `box` that is not `background`,
    from the box's top left."""
    x0, y0, w, h = box
    out = {}
    for y in range(h):
        for x in range(w):
            p = pixel(img, x0 + x, y0 + y)
            if p != background:
                out[(x, y)] = p
    return out


def same_shape(a, b):
    """Whether the pixel sets `a` and `b` hold the same shape at some
    offset."""
    if len(a) != len(b):
        return False
    if not a:
        return True
    ax, ay = min(x for x, _ in a), min(y for _, y in a)
    bx, by = min(x for x, _ in b), min(y for _, y in b)
    return {(x - ax, y - ay) for x, y in a} == {(x - bx, y - by) for x, y in b}


def matches(on, glyphs, height):
    """The names of the candidates (candidates) whose shape is `on`."""
    return [name for name, shape in candidates(glyphs, height) if same_shape(on, shape)]


def hollow_box(on):
    """Whether the pixel set `on` is a hollow box: its bounding box with a
    rectangle cut out of its inside, so each side is a solid line, though
    the sides may differ in thickness."""
    if not on:
        return False
    x0, x1 = min(x for x, _ in on), max(x for x, _ in on)
    y0, y1 = min(y for _, y in on), max(y for _, y in on)
    hole = {(x, y) for x in range(x0, x1 + 1) for y in range(y0, y1 + 1)} - on
    if not hole:
        return False
    hx0, hx1 = min(x for x, _ in hole), max(x for x, _ in hole)
    hy0, hy1 = min(y for _, y in hole), max(y for _, y in hole)
    inside = x0 < hx0 and hx1 < x1 and y0 < hy0 and hy1 < y1
    return inside and len(hole) == (hx1 - hx0 + 1) * (hy1 - hy0 + 1)


def colored(pixels):
    """Whether any pixel of `pixels` (colors) is not a shade of grey."""
    return any(max(p) - min(p) > 24 for p in pixels)


# --- the verdict ------------------------------------------------------------

def judge_terminus(img, cell, size, glyphs):
    """(found, faults) for the default set terminus drawn at `size` pixels in
    cells of `cell`: `found` names the sources that match each probe, and
    `faults` says where they are not the ones ADR-1204 D2 expects. The
    Nerd Fonts icon draws as a hollow box, because no source of the set
    holds it: Terminus lacks it, and the unifont layer leaves the
    private-use areas out."""
    strike = max(s for s in TERMINUS_STRIKES if s <= size)
    unifont = "Unifont" if size < 32 else "Unifont x%d" % (size // 16)
    want = {LETTER: "Terminus %d px" % strike, POWERLINE: "Terminus %d px" % strike,
            HAN: unifont, PLANE2: unifont, EMOJI: unifont}
    background = paper(img, cell)
    found, faults = {}, []
    for char, source in want.items():
        drawn = ink(img, region(char, cell), background)
        names = matches(set(drawn), glyphs[ord(char)], cell[1])
        found[char] = names
        if names != [source]:
            faults.append("%s matches %s, not %s alone" % (
                NAMES[char], " and ".join(names) or "no source's glyph", source))
        elif len(set(drawn.values())) != 1:
            faults.append("%s is drawn in %d colors, not in 1 bit"
                          % (NAMES[char], len(set(drawn.values()))))
    icon = set(ink(img, region(ICON, cell), background))
    found[ICON] = ["a hollow box"] if hollow_box(icon) else []
    if not found[ICON]:
        faults.append("%s is not a hollow box" % NAMES[ICON])
    return found, faults


def judge_cullet(img, reference, cell, size, glyphs):
    """(found, faults) for the outline set cullet-mono drawn at `size` pixels
    in cells of `cell`, with `reference` the same row drawn with the
    reference set REFERENCE, Cullet Mono alone."""
    unifont = "Unifont" if size < 32 else "Unifont x%d" % (size // 16)
    background = paper(img, cell)
    found, faults = {}, []
    drawn = {c: ink(img, region(c, cell), background) for c in COLUMNS}
    alone = {c: ink(reference, region(c, cell), paper(reference, cell))
             for c in COLUMNS}
    for char in (LETTER, ICON):
        names = matches(set(drawn[char]), glyphs[ord(char)], cell[1])
        if not drawn[char] or hollow_box(set(drawn[char])):
            found[char] = []
            faults.append("%s is %s, not Cullet Mono's glyph"
                          % (NAMES[char], "blank" if not drawn[char] else "a hollow box"))
        elif drawn[char] != alone[char]:
            found[char] = names
            faults.append("%s differs from Cullet Mono's own glyph in %s" % (NAMES[char],
                                                                             REFERENCE))
        elif names:
            # The two captures agree, but the glyph has a bitmap source's
            # shape, as when the console cannot read Cullet Mono and draws
            # both rows from the built-in font, whose 8x16 strike is
            # Terminus's, or from Unifont.
            found[char] = names
            faults.append("%s matches %s, not Cullet Mono's glyph"
                          % (NAMES[char], " and ".join(names)))
        else:
            found[char] = ["Cullet Mono"]
    if colored(drawn[EMOJI].values()):
        found[EMOJI] = ["Noto Color Emoji"]
    else:
        found[EMOJI] = matches(set(drawn[EMOJI]), glyphs[ord(EMOJI)], cell[1])
        faults.append("%s holds no color, so Noto Color Emoji did not draw it" % NAMES[EMOJI])
    # Cullet Mono's cell is 0.46 times as wide as it is high, so at 16 and 17
    # pixels it is 7 pixels wide, too narrow for Unifont's 8-pixel glyphs, and
    # no source draws the Han character: vtium draws a hollow box.
    #
    # References:
    # - ADR-1204 D2, amended 2026-11-18: cullet-mono draws CJK through
    #   Unifont from 18 pixels up.
    if cell[0] < UNIFONT_WIDTH:
        found[HAN] = ["a hollow box"] if hollow_box(set(drawn[HAN])) else []
        if not found[HAN]:
            faults.append("%s is not a hollow box, though Unifont's %d-pixel glyphs do not fit "
                          "a %d-pixel cell" % (NAMES[HAN], UNIFONT_WIDTH, cell[0]))
    else:
        names = matches(set(drawn[HAN]), glyphs[ord(HAN)], cell[1])
        found[HAN] = names
        if names != [unifont]:
            faults.append("%s matches %s, not %s alone" % (
                NAMES[HAN], " and ".join(names) or "no source's glyph", unifont))
    found[LOGO] = ["a hollow box"] if hollow_box(set(drawn[LOGO])) else []
    if not found[LOGO]:
        faults.append("%s is not a hollow box" % NAMES[LOGO])
    return found, faults


# The lines vtium logs about the cell a console draws in, quoted from
# crates/vtium/src/app/drawing.rs, fonts.rs and glyph_asks.rs. A display
# line names the cell of a display vtium opens. An outline font set names
# its cell when the font helper's answer comes in time, and again when a
# late answer moves the console to it. A bitmap set's lines name its fonts'
# own cell or its strike, which is not the display's cell: a 16x32 strike
# draws in a 16x40 cell at 40 pixels.
DISPLAY_CELL = re.compile(r"Display \S+ with backend \[[^]]*\] text renderer \[[^]]*\], "
                          r"cell size (\d+)x(\d+)")
OUTLINE_CELL = re.compile(r"the frit (\S+) draws in its outline font's (\d+)x(\d+) cell "
                          r"at \d+ pixels")
LATE_OUTLINE_CELL = re.compile(r"the frit ([^\s']+)'s (\d+)x(\d+) cell at \d+ pixels came "
                               r"after the wait: it draws in it from the next frame")
# Lines that say a set's cell is now known without naming the display's
# cell. vtium logs no display line when a late answer moves the console to
# a new cell, so after a bitmap set's late answer no line names the cell
# the console draws in until a display line comes.
BITMAP_CELL = (
    re.compile(r"the font set (\S+) draws its \d+x\d+ strike at \d+ pixels"),
    re.compile(r"the frit (\S+) draws in its fonts' own \d+x\d+ cell"),
    re.compile(r"the frit ([^\s']+)'s own \d+x\d+ cell came after the wait: it draws in it "
               r"from the next frame"),
)
# vtium's warning that a set's cell is not known in time: the console then
# draws in a fallback cell until a late answer comes.
NOT_KNOWN = re.compile(r"the frit ([^\s']+)'s (?:own cell|cell at \d+ pixels) is not known "
                       r"\(.*\): it draws (?:in the display's cell|as wide as the built-in font)")


def console_cell(log_text, font_set, screen):
    """The cell, as (W, H), that the console drawing `font_set` draws in at
    the end of `log_text`, the serial log up to the capture of `screen`.

    The cell is the one the last display line or the last of the set's
    outline cell lines names. Raises RuntimeError, quoting vtium's line,
    when the set's cell is still not known at the end of the log, since
    the capture then shows a fallback cell. It also raises when vtium's
    last line about the set's cell names a strike or a bitmap font's cell
    with no display line after it, since then no line names the cell the
    console draws in."""
    cell = warning = unnamed = None
    for line in log_text.splitlines():
        line = line.strip()
        m = DISPLAY_CELL.search(line)
        if m:
            cell, unnamed = (int(m.group(1)), int(m.group(2))), None
            continue
        m = OUTLINE_CELL.search(line) or LATE_OUTLINE_CELL.search(line)
        if m and m.group(1) == font_set:
            cell, warning, unnamed = (int(m.group(2)), int(m.group(3))), None, None
            continue
        m = next((m for m in (r.search(line) for r in BITMAP_CELL) if m), None)
        if m and m.group(1) == font_set:
            warning, unnamed = None, line
            continue
        m = NOT_KNOWN.search(line)
        if m and m.group(1) == font_set:
            warning = line
    if warning:
        raise RuntimeError("the console drew %s before vtium knew the font set %s's cell, so "
                           "the capture shows a fallback cell. vtium logged: %s"
                           % (screen, font_set, warning))
    if unnamed:
        raise RuntimeError("the console drew %s after the font set %s's cell changed, and no "
                           "line names the cell it draws in now. vtium logged: %s"
                           % (screen, font_set, unnamed))
    if cell is None:
        raise RuntimeError("the console logged no cell size")
    return cell


def draw(vm, work, screen, last, log):
    """Types `t3 SCREEN`, waits for the script to finish and returns the
    screen once it has settled and differs from `last`, saved in WORK,
    with the console's serial log up to the capture. The log ends at its
    last whole line, so a line vtium was still writing is left out."""
    vm.type("t3 %s\n" % screen)
    vm.wait_event("T3-DONE %s" % screen, timeout=60)
    path = os.path.join(work, screen + ".ppm")
    img = wait_stable(vm, path, settle=2.0, timeout=90,
                      want=lambda shot: shot.data != last.data)
    with open(vm.serial, "rb") as f:
        text = f.read()
    log("%s: captured %s" % (screen, path))
    return path, img, text[:text.rfind(b"\n") + 1].decode(errors="replace")


def run(v, vm, work, caps, errors, log):
    """The check CASE on a logged-in console: draws the probes, and for the
    cullet-mono set draws them again with the reference set, then judges
    the capture. As with every check without a golden, the case keeps its
    capture only when it passes, since a check with a capture passes; when
    it fails, `errors` says why, and the capture stays in WORK."""
    try:
        last = wait_stable(vm, os.path.join(work, "wait.ppm"))
        font_set = v["font_set"]
        path, img, serial = draw(vm, work, "font-probe", last, log)
        cell = console_cell(serial, font_set, "font-probe")
        if cell[1] != v["size"]:
            raise RuntimeError("the console's cell is %dx%d, not %d pixels high"
                               % (cell[0], cell[1], v["size"]))
        glyphs = source_glyphs(os.path.join(work, "disc", "tree.tar"), COLUMNS)
        if font_set == "terminus":
            found, faults = judge_terminus(img, cell, v["size"], glyphs)
        else:
            _, reference, serial = draw(vm, work, "font-probe-alone", img, log)
            with open(vm.events, errors="replace") as f:
                events = [e for e in f.read().splitlines() if e.startswith("T3-USE-FRIT ")]
            if not events or events[-1].split()[1:] != ["set", REFERENCE]:
                raise RuntimeError("vtium-config use-frit %s did not switch the console: %s"
                                   % (REFERENCE, events[-1] if events else "no answer"))
            # Both captures are compared cell by cell, so they must be
            # drawn in one cell.
            alone = console_cell(serial, REFERENCE, "font-probe-alone")
            if alone != cell:
                raise RuntimeError("the console drew font-probe in a %dx%d cell and "
                                   "font-probe-alone in a %dx%d cell" % (cell + alone))
            found, faults = judge_cullet(img, reference, cell, v["size"], glyphs)
    except Exception as e:  # any failure fails the case
        log("%s: %s: %s" % (CASE, type(e).__name__, e))
        errors[CASE] = "%s: %s" % (type(e).__name__, e)
        return
    for char, names in found.items():
        log("%s: %s: %s" % (CASE, NAMES[char], " and ".join(names) or "no known source"))
    if faults:
        errors[CASE] = "in a %dx%d cell, %s; the capture is %s" % (
            cell[0], cell[1], "; ".join(faults), path)
    else:
        caps[CASE] = path
