"""Tier 3's cases and their verdicts: what each case shows, the text drawn
on its screen, and its capture compared with its golden."""

import os
import re
import subprocess

import t3_bench
import t3_devices
import t3_heads
import t3_images
import t3_pictures
import t3_pointer
import t3_power
import t3img
from t3_common import GOLDEN, GUEST, HERE, ROOT, RUNDIR, rel

# Screens drawn by `t3 NAME` in the guest (guest/screens/NAME.sh), in the
# the order the guided demo shows them in, then sixel-bound and
# the benchmark scenarios, which have no demo page, except that cjk
# stays last: vt-restore is compared with the last screen's golden, and its
# golden is the cjk one (test_t3.py holds the two together).
SCREENS = ["sgr", "colour-forms", "underline-styles", "underline-colour", "strike",
           "boxdraw", "blocks", "mosaic", "wide-margin", "combining", "emoji", "sync-hold", "sixel",
           "sixel-ramps", "sixel-article", "sixel-bound", "sixel-evict", "photos",
           "photo-scroll", "mpv-sixel", "tmux-2x2", "tmux-3x3", "kitty", "kitty-photo", "kitty-files", "kitty-layers", "s1-scroll", "flood", "cjk"]
# The screens of the small displays (vtium/vtium#75): small-console, written
# for 40 columns, then screens of the list above that draw at any width,
# with the colored underlines and strikes at small cells (vtium/vtium#119),
# cjk last as there.
SMALL_SCREENS = ["small-console", "sgr", "boxdraw", "underline-styles", "underline-colour",
                 "strike", "wide-margin", "cjk"]
# The screens of vtium's images variant (guest/screens/img-*.sh), which send
# the test pictures t3_pictures.py writes: a sixel picture, kitty pictures
# sent directly, relative placements, text and an erase over pictures,
# deletes, pictures scrolled partly off the top, one pane of a tmux grid
# playing video, whose damage is measured, kitty animations stopped on a
# frame, iTerm2 pictures with an animated GIF, kitty pictures drawn through
# Unicode placeholders, and pictures the image helper reads and decodes.
# img-png is last, so vt-restore shows pictures redrawn after the VT switch
# (RESTORED).
PICTURE_SCREENS = ["img-sixel", "img-kitty", "img-relative", "img-overwrite", "img-delete",
                   "img-scroll", "img-tmux-pane", "img-animation", "img-iterm2",
                   "img-placeholder", "img-png"]
# Cases captured after a screen is drawn, by pressing keys: each is (screen,
# case, the chords pressed before its capture), captured once the screen has
# changed, so a key that changes nothing fails its case. After the last, the
# chords in KEYS_AFTER put the terminal back as the screen left it. The
# rotate grab is Logo with plus or equal, zoom Ctrl with the same, and both
# match equal unshifted. photo-scroll's photograph has scrolled into
# kmscon's scrollback, which Shift+PageUp shows and Shift+PageDown leaves
# (A1).
KEYED = [
    ("sixel-evict", "sixel-evict-rotated", [("meta_l", "equal")]),
    ("sixel-evict", "sixel-evict-history", [("meta_l", "minus")] + [("shift", "pgup")] * 30),
    ("sixel-evict", "sixel-evict-zoomed", [("shift", "pgdn")] * 30 + [("ctrl", "equal")]),
    ("photo-scroll", "photo-scrollback", [("shift", "pgup")]),
    ("photo-scroll", "photo-scrollforward", [("shift", "pgdn")]),
    ("img-scroll", "img-scrollback", [("shift", "pgup")]),
    ("img-scroll", "img-scrollforward", [("shift", "pgdn")]),
]
KEYS_AFTER = {"sixel-evict": [("ctrl", "minus")]}
# The screens that are also benchmark scenarios: each reports its
# window on the event port, and the report carries its figures (t3_bench.py).
BENCHMARKS = ["mpv-sixel", "tmux-2x2", "tmux-3x3", "s1-scroll", "flood", "img-tmux-pane"]
# The benchmark screens judged by A2's criterion, that the console is
# not the bottleneck (t3_bench.bottleneck); the tmux grids report their
# figures only, since tmux redraws every pane's pictures itself.
RATE_CHECKED = ["mpv-sixel"]
# Variants where kmscon is known to be the bottleneck, by screen, each with
# the issue that tracks it, as {"mpv-sixel": {"2d-7680x2160": "owner/repo#N"}}.
# Such a case passes with a note while the bottleneck lasts and fails once
# it is gone, so the entry goes with the fix. None today: both 7680x2160
# variants once waited on a console bottleneck that is fixed.
KNOWN_BOTTLENECKS = {}
# A case compared with another case's golden: back from the scrollback, and
# back from tty3 (vt-restore, with the last screen's).
SAME_AS = {"photo-scrollforward": "photo-scroll", "img-scrollforward": "img-scroll",
           "vt-restore": SCREENS[-1]}
# The variants whose last screen is not SCREENS[-1], by variant: the screen
# their vt-restore is compared with, the last they draw.
RESTORED = {"images-640x480": PICTURE_SCREENS[-1]}
# The variants judged by another variant's goldens, by variant: the
# directory under goldens/ their cases compare with. gl-1280x800 draws on the
# GPU path, which draws the CPU path's bytes, so 2d-1280x800's goldens judge
# it exactly (ADR-1303 D1, D11).
GOLDENS_OF = {"gl-1280x800": "2d-1280x800"}
# The directories under goldens/ kept as a record, which no case compares
# with: the first GL goldens, drawn by a renderer vtium does not use
# (ADR-1303 D11).
RECORDS = {"gl-1280x800": "what the first GL renderer drew"}


def same_as(case, variant=None):
    """The case whose golden `case` of `variant` is compared with (SAME_AS,
    RESTORED), on the same output (t3_devices.split)."""
    screen, output = t3_devices.split(case)
    if screen == "vt-restore" and variant in RESTORED:
        return RESTORED[variant] + output
    return SAME_AS.get(screen, screen) + output

# The screens that draw sixel or kitty pictures. Every variant with images
# draws them: both renderers draw image placements, bbulk on the 2D variants
# and gltex on the GL one. A variant
# without images ("images": False, the minimal build) shows each screen's
# "not offered" page instead (t3_scenario.not_offered).
IMAGE_SCREENS = ["sixel", "sixel-ramps", "sixel-article", "sixel-bound", "sixel-evict",
                 "photos", "photo-scroll", "mpv-sixel", "tmux-2x2", "tmux-3x3", "kitty",
                 "kitty-photo", "kitty-files", "kitty-layers", "flood"]
# Every screen that draws pictures: those above and the images variant's.
DRAWS_PICTURES = IMAGE_SCREENS + PICTURE_SCREENS
# What the picture screens that ask for replies must report on the event
# port (T3-REPLY, img-common.inc), in order: the image helper read and
# decoded each picture, removed the temporary file and refused a file the
# session's user cannot read (img-png); a delete kept one image's data and
# freed another's (img-delete); vtium took every animation frame and control
# (img-animation) and both virtual placements (img-placeholder); and each
# iTerm2 picture left the cursor on the row of its last pixel row, at its
# left column, where a dropped picture leaves it at row 3 (img-iterm2). A
# screen that reports anything else fails, whether or not its golden is
# reviewed yet.
REPLIES = {
    "img-png": ["i=41;OK", "i=42;OK", "i=43;OK", "removed", "i=44;OK",
                "i=45;EBADF:Failed to read image file", "rpc_pipefs readable",
                "i=46;EBADF:Failed to read image file"],
    "img-delete": ["i=32;OK", "i=33;ENOENT:image not found"],
    "img-animation": ["i=50;OK"] * 6 + ["i=51;OK"] * 4,
    "img-iterm2": ["png at row 5, column 3", "gif at row 5, column 15",
                   "parts at row 8, column 27", "pixels at row 4, column 47"],
    "img-placeholder": ["i=60;OK", "i=16909060,p=7;OK"],
}

# The pictures a picture screen must show before its capture is taken, each
# (picture of t3_pictures.PICTURES, column, row), its top left corner at
# that cell: the frame an animation stops on, which a capture taken while
# it still played, or of an animation that never played, does not show, and
# the pictures drawn through placeholder cells, which a console that drew
# the cells as glyphs does not show. The driver captures such a screen once
# it has settled showing them, and the case fails, whatever its golden says,
# when it never does (t3_scenario.capture_screen).
SHOWS = {
    "img-animation": [("anim-stopped", 5, 3), ("anim-ended", 20, 3)],
    "img-iterm2": [("quad-turned-twice", 15, 3)],
    "img-placeholder": [("quad", 35, 3), ("child", 5, 9)],
}


def not_shown(name, img):
    """Why capture `img` of screen `name` does not show the pictures SHOWS
    lists for it, or None when it shows every one."""
    missing = []
    for picture, col, row in SHOWS.get(name, []):
        wrong = t3_pictures.shows(img, picture, col, row)
        if wrong:
            missing.append("%s at column %d, row %d (%d pixels differ)"
                           % (picture, col, row, len(wrong)))
    if not missing:
        return None
    return "the screen settled without showing " + "; ".join(missing)


def keyed(name):
    """The keyed cases after screen `name`."""
    return [k for s, k, _ in KEYED if s == name]


def cases(screens=None):
    """Every case of a variant's report: each of `screens` (default SCREENS)
    and the keyed cases after it, then the VT switch (a check without a
    golden) and the restore after it."""
    out = []
    for name in SCREENS if screens is None else screens:
        out += [name] + keyed(name)
    return out + ["vt-switch", "vt-restore"]


CASES = cases()


def restore_caption(screen):
    """What the vt-restore case shows when `screen` was the last screen drawn."""
    return ("Back on tty2 after Ctrl+Alt+F2, the %s screen redrawn by the first instance. "
            "Correct: identical to the %s golden." % (screen, screen))

# What each check shows and what correct looks like, for the conformance run
# page; a screen's comes from the header comment of its script (describe).
CHECKS = {
    "photo-scrollback": "After Shift+PageUp on the photo-scroll screen, kmscon's view of its "
                        "scrollback: the caption and the day photograph above line 1, the "
                        "picture travelling with its lines. Correct: the photograph whole, "
                        "at the rows it was drawn on, nothing over the lines below it.",
    "photo-scrollforward": "After Shift+PageDown, back from the scrollback. Correct: identical "
                           "to the photo-scroll golden, nothing of the photograph left on "
                           "the lines.",
    "sixel-evict-rotated": "The sixel-evict screen turned with Logo+=: a scaled copy of the last "
                           "picture, which on bbulk fits the full image budget only once picture "
                           "1 is evicted from scrollback (the Q1 note); gltex's copy is a "
                           "texture in a budget of its own and evicts nothing. Correct: the last "
                           "picture drawn, turned, under its caption.",
    "sixel-evict-history": "Turned back with Logo+-, then Shift+PageUp to the top of the "
                           "scrollback. Correct: picture 1's caption over blank cells, the "
                           "pictures after it drawn under theirs; on gltex (the GL variant) "
                           "picture 1 drawn too, as its turned copy evicted nothing.",
    "sixel-evict-zoomed": "Scrolled down, then zoomed with Ctrl+=: a scaled copy of the last "
                          "picture for the larger cells. Correct: the last picture drawn, "
                          "scaled to the larger cells, under its caption.",
    "img-scrollback": "After Shift+PageUp on the img-scroll screen, vtium's view of its "
                      "scrollback: the caption and both pictures of six bands whole, from red "
                      "at row 2 to violet at row 7, each band beside its label, the pictures "
                      "having travelled into the scrollback with their lines. Correct: both "
                      "pictures whole and at the rows they were drawn on.",
    "img-scrollforward": "After Shift+PageDown, back from the scrollback. Correct: identical "
                         "to the img-scroll golden, the top two bands of each picture out of "
                         "view again.",
    "vt-switch": "After Ctrl+Alt+F3, the second kmscon instance on tty3 with its login "
                 "prompt. Correct: a drawn screen that is neither blank nor the tty2 "
                 "screen just left.",
    "vt-restore": restore_caption(SCREENS[-1]),
    **t3_power.CHECKS,
    **t3_pointer.CHECKS,
    **t3_devices.CHECKS,
    **t3_heads.CHECKS,
    **t3_images.CHECKS,
}
# The checks without a golden: each passes when its capture was taken, or,
# on a guest with no display to capture, when the scenario reports it
# passed (the variant's metadata, "passed").
NO_GOLDEN = {"vt-switch", *t3_power.CHECKS, *t3_pointer.CHECKS, *t3_devices.CHECKS,
             *t3_images.CHECKS,
             *t3_heads.CHECKS}

PROVISIONAL_FILE = os.path.join(HERE, "provisional.tsv")


def read_provisional(path=PROVISIONAL_FILE):
    """provisional.tsv as {golden "VARIANT/SCREEN": why}: vtium's goldens the
    operator has not reviewed. A malformed line, or a golden listed twice, is
    refused."""
    out = {}
    with open(path) as f:
        for n, line in enumerate(f, 1):
            if not line.strip() or line.startswith("#"):
                continue
            where = "%s:%d" % (rel(path), n)
            fields = line.rstrip("\n").split("\t")
            if len(fields) != 2 or not all(fields) or fields[0].count("/") != 1:
                raise SystemExit("%s: want VARIANT/SCREEN and WHY, separated by a tab" % where)
            if fields[0] in out:
                raise SystemExit("%s: %s is listed twice" % (where, fields[0]))
            out[fields[0]] = fields[1]
    return out


PROVISIONAL = read_provisional()


def check_goldens(cases_of, root, provisional=()):
    """Every reason the goldens under `root` cannot judge a run: a golden or
    sidecar no case of a variant they judge compares with (`cases_of` maps
    each such variant to its cases, and GOLDENS_OF names the directory a
    variant compares with); a file in a record (RECORDS) that is none of
    its variant's screens, though no case compares with the record; a
    `provisional` row (vtium's provisional.tsv) for a golden in a record,
    which judges nothing; and a provisional row for a golden that does not
    exist."""
    used = {}
    for variant, cases_of_variant in cases_of.items():
        used.setdefault(GOLDENS_OF.get(variant, variant), set()).update(
            same_as(c, variant) for c in cases_of_variant)
    for record in RECORDS:
        used[record] = {same_as(c, record) for c in cases_of.get(record, ())}
    errors = []
    for dirpath, _, names in os.walk(root):
        for name in names:
            path = os.path.join(dirpath, name)
            stem, ext = os.path.splitext(os.path.relpath(path, root))
            if ext not in (".png", ".tol"):
                continue
            directory, _, screen = stem.partition(os.sep)
            if screen not in used.get(directory, ()):
                errors.append("%s is a golden no case draws" % rel(path))
    for golden in provisional:
        if golden.partition("/")[0] in RECORDS:
            errors.append("provisional.tsv lists %s, which is in a record that judges nothing "
                          "(RECORDS)" % golden)
        elif not os.path.exists(os.path.join(root, golden + ".png")):
            errors.append("provisional.tsv lists %s, which has no golden" % golden)
    return sorted(errors)

# Seconds a screen script may run on the host before its text is given up.
SCREEN_TIMEOUT = 10


def header(case, script=None):
    """The leading comment lines of a screen's script, or of `script` in the
    screens' directory, by field: "" for the caption, then the "# look-for:",
    "# wrong-looks-like:", "# screen-text:" and "# kit:" lines (the local VT
    demo's review kit), and the "# alt:" lines, a showcase scene's alt text
    (t3_showcase)."""
    fields = {"": [], "look-for": [], "wrong-looks-like": [], "screen-text": [], "kit": [],
              "alt": []}
    with open(os.path.join(GUEST, "screens", script or case + ".sh")) as f:
        for line in f:
            if line.startswith("#!"):  # the script's interpreter
                continue
            if not line.startswith("#"):
                break
            key, sep, text = line[1:].strip().partition(": ")
            if sep and key in fields:
                fields[key].append(text.strip())
            else:
                fields[""].append(line[1:].strip())
    return fields


def describe(case):
    """(caption, what a defect looks like) of a case.

    A check's caption is its entry in CHECKS. A screen's is the leading
    comment lines of its script; the "# look-for:" lines the local VT demo
    shows join it as "Correct: ...", and its "# wrong-looks-like:" lines are
    the second part, or None for a script without them. A case drawn on an
    output after the first (t3_devices.split) is its screen's case,
    captured there.
    """
    screen, output = t3_devices.split(case)
    if output:
        caption, wrong = describe(screen)
        return "%s Captured from the output %s." % (caption, output[1:]), wrong
    if case in CHECKS:
        return CHECKS[case], None
    if case in t3_heads.FIRST:
        return t3_heads.FIRST[case], None
    fields = header(case)
    caption = " ".join(fields[""])
    if fields["look-for"]:
        caption += " Correct: " + " ".join(fields["look-for"])
    return caption, " ".join(fields["wrong-looks-like"]) or None


def screen_text(case, log, variant=None):
    """The lines a case's screen script prints, colors removed, so a region
    of the screen can be named by the text drawn there. The guest clears the
    screen first, so line N is the screen's row N. Any other control in the
    output (a cursor move, a tab, a character-set switch) would break that
    mapping, so such output gives None, as do a check that draws no script
    and a script that fails, hangs or reads its input: the text only
    captions a region, and losing it must never cost a case its verdict.
    A script whose "# screen-text:" line begins "none" is not run: its
    printed lines are not its rows, as when it scrolls."""
    base = same_as(t3_devices.split(case)[0], variant)
    if base not in SCREENS + SMALL_SCREENS:
        return None
    reason = " ".join(header(base)["screen-text"])
    if reason.startswith("none"):
        log("%s: no screen text: %s" % (case, reason))
        return None
    try:
        out = subprocess.run(["bash", os.path.join(GUEST, "screens", base + ".sh")],
                             stdin=subprocess.DEVNULL, capture_output=True, check=True,
                             timeout=SCREEN_TIMEOUT,
                             env=dict(os.environ, LC_ALL="C.UTF-8")).stdout.decode()
    except Exception as e:
        log("%s: no screen text: %s" % (case, e))
        return None
    text = re.sub(r"\x1b\[[0-9;:]*m", "", out)
    if re.search(r"[\x00-\x09\x0b-\x1f\x7f-\x9f]", text):
        log("%s: no screen text: the script prints controls other than colors" % case)
        return None
    return text.rstrip("\n").split("\n")


def golden_for(variant, case, root=None):
    """(golden, tolerance sidecar) paths for a case under `root` (default
    GOLDEN), in the variant's directory or the one GOLDENS_OF names, or
    (None, None) for a check without a golden (NO_GOLDEN)."""
    if case in NO_GOLDEN:
        return None, None
    base = same_as(case, variant)
    directory = os.path.join(root or GOLDEN, GOLDENS_OF.get(variant, variant))
    return (os.path.join(directory, base + ".png"),
            os.path.join(directory, base + ".tol"))


def write_thumb(img, path):
    t3img.write_png(path, img)
    return rel(path)


def evaluate(variant, case, ppm, meta, log, root=None, verdict=None):
    """The report entry for one case, from its capture (or None), judged by
    its golden under `root` (golden_for). A case of a variant whose
    `verdict` is "capture", the showcase's, has no golden: it carries the
    status capture once captured, which passes and judges nothing, and
    fails only when it has no capture or the scenario found it wrong."""
    golden, tolpath = (None, None) if verdict == "capture" else golden_for(variant, case, root)
    stem = os.path.join(RUNDIR, variant, case)
    description, wrong = describe(case)
    if t3_devices.split(case)[0] == "vt-restore" and variant in RESTORED:
        description = restore_caption(RESTORED[variant])
    entry = {"name": "%s/%s" % (variant, case), "description": description,
             "wrong": wrong, "text": screen_text(case, log, variant), "golden": rel(golden),
             "actual": None, "diff": None, "tolerance": None,
             "thumb_actual": None, "thumb_golden": None, "thumb_diff": None,
             "stats": {"pixels": 0, "bbox": [], "max_delta": 0}}
    # The golden's thumbnail comes first, so a case that fails without a
    # capture still shows on the contact sheet what it should have drawn.
    gimg = None
    if golden is not None and os.path.exists(golden):
        gimg = t3img.read(golden)
        os.makedirs(os.path.dirname(stem), exist_ok=True)
        entry["thumb_golden"] = write_thumb(t3img.thumbnail(gimg), stem + ".golden.thumb.png")
    try:
        tol = entry["tolerance"] = t3img.load_tol(tolpath)
    except ValueError as e:  # a malformed sidecar fails its case, loudly
        entry["status"], entry["error"] = "fail", str(e)
        return entry
    if ppm is None and golden is None and case in meta.get("passed", ()):
        entry["status"] = "pass"
        entry["note"] = "passed on the guest's state; the guest has no display to capture"
        return entry
    if ppm is None:
        entry["status"] = "fail"
        entry["error"] = meta.get("errors", {}).get(case) or meta.get("error", "no capture")
        return entry
    img = t3img.read(ppm)
    os.makedirs(os.path.dirname(stem), exist_ok=True)
    t3img.write_png(stem + ".png", img)
    entry["actual"] = rel(stem + ".png")
    entry["thumb_actual"] = write_thumb(t3img.thumbnail(img), stem + ".thumb.png")
    if verdict == "capture":
        entry["status"] = "capture"
        error = meta.get("errors", {}).get(case)
        if error:
            entry["status"], entry["error"] = "fail", error
        return entry
    if golden is None:  # a check, passed when the capture was taken
        entry["status"] = "pass"
        if meta.get("notes", {}).get(case):
            entry["note"] = meta["notes"][case]
        return entry
    if gimg is None:
        entry["status"] = "missing"
        return entry
    stats, diff, points = t3img.compare(img, gimg, tol["masks"] if tol else ())
    entry["stats"] = stats
    if diff is not None:
        t3img.write_png(stem + ".diff.png", diff)
        entry["diff"] = rel(stem + ".diff.png")
        entry["thumb_diff"] = write_thumb(t3img.diff_thumbnail(img, points),
                                          stem + ".diff.thumb.png")
    entry["status"] = t3img.verdict(stats, tol)
    reviewed = "%s/%s" % (GOLDENS_OF.get(variant, variant), same_as(case, variant))
    if reviewed in PROVISIONAL:
        entry["note"] = ("provisional: not yet reviewed, so not judged; judged, it would be %s"
                         % entry["status"])
        entry["status"] = "provisional"
    figures = meta.get("bench", {}).get(case, {})
    if figures.get("error"):  # drawn, but not measured
        entry["status"], entry["error"] = "fail", figures["error"]
    elif case in RATE_CHECKED:
        # judged whatever the pixels said, a provisional golden's included:
        # judge_rate only fails the case or adds a note
        judge_rate(entry, variant, case, figures)
    error = meta.get("errors", {}).get(case)
    if error:  # captured, but the scenario found the case wrong
        entry["status"], entry["error"] = "fail", error
    return entry


def judge_rate(entry, variant, case, figures):
    """Fails a drawn case whose figures show kmscon as the bottleneck,
    unless the variant's bottleneck is known (KNOWN_BOTTLENECKS); a known
    one that is gone fails too, and so does a video that showed no frame,
    known bottleneck or not."""
    why = t3_bench.bottleneck(figures)
    known = KNOWN_BOTTLENECKS.get(case, {}).get(variant)
    if "mpv_frames" in figures and not figures["mpv_frames"]:
        # A known bottleneck is slowness; a video with no frame is broken.
        entry["status"] = "fail"
        entry["error"] = "; ".join(why)
    elif why and known:
        entry["note"] = "known bottleneck, %s: %s" % (known, "; ".join(why))
    elif why:
        entry["status"] = "fail"
        entry["error"] = "kmscon is the bottleneck: " + "; ".join(why)
    elif known:
        entry["status"] = "fail"
        entry["error"] = ("kmscon kept up, so %s is fixed here: remove its entry in "
                          "KNOWN_BOTTLENECKS" % known)
