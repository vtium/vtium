"""Tier 3's cases and their verdicts: what each case shows, the text drawn
on its screen, and its capture compared with its golden."""

import os
import re
import subprocess

import t3_bench
import t3img
from t3_common import GOLDEN, GUEST, RUNDIR, rel

# Screens drawn by `t3 NAME` in the guest (fork/tier3/guest/screens/NAME.sh),
# in the local VT demo's order (fork/demo/sections), then sixel-bound and
# ADR-103's benchmark scenarios, which have no demo page, except that cjk
# stays last: vt-restore is compared with the last screen's golden, and its
# golden is the cjk one (test_t3.py holds the two together).
SCREENS = ["sgr", "colour-forms", "underline-styles", "underline-colour", "strike",
           "boxdraw", "blocks", "mosaic", "wide-margin", "combining", "emoji", "sync-hold", "sixel",
           "sixel-ramps", "sixel-article", "sixel-bound", "sixel-evict", "photos",
           "photo-scroll", "mpv-sixel", "tmux-2x2", "tmux-3x3", "kitty", "kitty-photo", "kitty-files", "kitty-layers", "s1-scroll", "flood", "cjk"]
# Cases captured after a screen is drawn, by pressing keys: each is (screen,
# case, the chords pressed before its capture), captured once the screen has
# changed, so a key that changes nothing fails its case. After the last, the
# chords in KEYS_AFTER put the terminal back as the screen left it. The
# rotate grab is Logo with plus or equal, zoom Ctrl with the same, and both
# match equal unshifted. photo-scroll's photograph has scrolled into
# kmscon's scrollback, which Shift+PageUp shows and Shift+PageDown leaves
# (ADR-103's A1).
KEYED = [
    ("sixel-evict", "sixel-evict-rotated", [("meta_l", "equal")]),
    ("sixel-evict", "sixel-evict-history", [("meta_l", "minus")] + [("shift", "pgup")] * 30),
    ("sixel-evict", "sixel-evict-zoomed", [("shift", "pgdn")] * 30 + [("ctrl", "equal")]),
    ("photo-scroll", "photo-scrollback", [("shift", "pgup")]),
    ("photo-scroll", "photo-scrollforward", [("shift", "pgdn")]),
]
KEYS_AFTER = {"sixel-evict": [("ctrl", "minus")]}
# The screens that are also ADR-103 benchmark scenarios: each reports its
# window on the event port, and the report carries its figures (t3_bench.py).
BENCHMARKS = ["mpv-sixel", "tmux-2x2", "tmux-3x3", "s1-scroll", "flood"]
# The benchmark screens judged by ADR-103's criterion for A2, that kmscon is
# not the bottleneck (t3_bench.bottleneck); the tmux grids report their
# figures only, since tmux redraws every pane's pictures itself.
RATE_CHECKED = ["mpv-sixel"]
# Variants where kmscon is known to be the bottleneck, by screen, each with
# the issue that tracks it, as {"mpv-sixel": {"2d-7680x2160": "owner/repo#N"}}.
# Such a case passes with a note while the bottleneck lasts and fails once
# it is gone, so the entry goes with the fix. None today: both 7680x2160
# variants carried aaronsb/kmscon#139 until #152 fixed it there.
KNOWN_BOTTLENECKS = {}
# A case compared with another case's golden: back from the scrollback, and
# back from tty3 (vt-restore, with the last screen's).
SAME_AS = {"photo-scrollforward": "photo-scroll", "vt-restore": SCREENS[-1]}
# The screens that draw sixel or kitty pictures. Every variant with images
# draws them: both renderers draw image placements, bbulk on the 2D variants
# and gltex on the GL one (ADR-103 steps 3f and 4e, ADR-108). A variant
# without images ("images": False, the minimal build) shows each screen's
# "not offered" page instead (t3_scenario.not_offered).
IMAGE_SCREENS = ["sixel", "sixel-ramps", "sixel-article", "sixel-bound", "sixel-evict",
                 "photos", "photo-scroll", "mpv-sixel", "tmux-2x2", "tmux-3x3", "kitty",
                 "kitty-photo", "kitty-files", "kitty-layers", "flood"]


def keyed(name):
    """The keyed cases after screen `name`."""
    return [k for s, k, _ in KEYED if s == name]


def cases():
    """Every case of a variant's report: each screen and the keyed cases after
    it, then the VT switch (a check without a golden) and the restore after
    it."""
    out = []
    for name in SCREENS:
        out += [name] + keyed(name)
    return out + ["vt-switch", "vt-restore"]


CASES = cases()
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
                           "1 is evicted from scrollback (ADR-103's Q1 note); gltex's copy is a "
                           "texture in a budget of its own and evicts nothing. Correct: the last "
                           "picture drawn, turned, under its caption.",
    "sixel-evict-history": "Turned back with Logo+-, then Shift+PageUp to the top of the "
                           "scrollback. Correct: picture 1's caption over blank cells, the "
                           "pictures after it drawn under theirs; on gltex (the GL variant) "
                           "picture 1 drawn too, as its turned copy evicted nothing.",
    "sixel-evict-zoomed": "Scrolled down, then zoomed with Ctrl+=: a scaled copy of the last "
                          "picture for the larger cells. Correct: the last picture drawn, "
                          "scaled to the larger cells, under its caption.",
    "vt-switch": "After Ctrl+Alt+F3, the second kmscon instance on tty3 with its login "
                 "prompt. Correct: a drawn screen that is neither blank nor the tty2 "
                 "screen just left.",
    "vt-restore": "Back on tty2 after Ctrl+Alt+F2, the %s screen redrawn by the first "
                  "instance. Correct: identical to the %s golden." % (SCREENS[-1], SCREENS[-1]),
}

# Seconds a screen script may run on the host before its text is given up.
SCREEN_TIMEOUT = 10


def header(case):
    """The leading comment lines of a screen's script, by field: "" for the
    caption, then the "# look-for:", "# wrong-looks-like:", "# screen-text:"
    and "# kit:" lines (the local VT demo's review kit)."""
    fields = {"": [], "look-for": [], "wrong-looks-like": [], "screen-text": [], "kit": []}
    with open(os.path.join(GUEST, "screens", case + ".sh")) as f:
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
    the second part, or None for a script without them.
    """
    if case in CHECKS:
        return CHECKS[case], None
    fields = header(case)
    caption = " ".join(fields[""])
    if fields["look-for"]:
        caption += " Correct: " + " ".join(fields["look-for"])
    return caption, " ".join(fields["wrong-looks-like"]) or None


def screen_text(case, log):
    """The lines a case's screen script prints, colours removed, so a region
    of the screen can be named by the text drawn there. The guest clears the
    screen first, so line N is the screen's row N. Any other control in the
    output (a cursor move, a tab, a character-set switch) would break that
    mapping, so such output gives None, as do a check that draws no script
    and a script that fails, hangs or reads its input: the text only
    captions a region, and losing it must never cost a case its verdict.
    A script whose "# screen-text:" line begins "none" is not run: its
    printed lines are not its rows, as when it scrolls."""
    base = SAME_AS.get(case, case)
    if base not in SCREENS:
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
        log("%s: no screen text: the script prints controls other than colours" % case)
        return None
    return text.rstrip("\n").split("\n")


def golden_for(variant, case):
    """(golden, tolerance sidecar) paths for a case, or (None, None) for
    vt-switch, a check without a golden."""
    if case == "vt-switch":
        return None, None
    base = SAME_AS.get(case, case)
    return (os.path.join(GOLDEN, variant, base + ".png"),
            os.path.join(GOLDEN, variant, base + ".tol"))


def write_thumb(img, path):
    t3img.write_png(path, img)
    return rel(path)


def evaluate(variant, case, ppm, meta, log):
    """The report entry for one case, from its capture (or None)."""
    golden, tolpath = golden_for(variant, case)
    stem = os.path.join(RUNDIR, variant, case)
    description, wrong = describe(case)
    entry = {"name": "%s/%s" % (variant, case), "description": description,
             "wrong": wrong, "text": screen_text(case, log), "golden": rel(golden),
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
    if ppm is None:
        entry["status"] = "fail"
        entry["error"] = meta.get("errors", {}).get(case) or meta.get("error", "no capture")
        return entry
    img = t3img.read(ppm)
    t3img.write_png(stem + ".png", img)
    entry["actual"] = rel(stem + ".png")
    entry["thumb_actual"] = write_thumb(t3img.thumbnail(img), stem + ".thumb.png")
    if golden is None:  # a check, passed when the capture was taken
        entry["status"] = "pass"
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
    figures = meta.get("bench", {}).get(case, {})
    if figures.get("error"):  # drawn, but not measured
        entry["status"], entry["error"] = "fail", figures["error"]
    elif case in RATE_CHECKED and entry["status"] in ("pass", "near"):
        judge_rate(entry, variant, case, figures)
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
