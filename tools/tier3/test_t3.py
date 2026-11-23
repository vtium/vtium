#!/usr/bin/env python3
"""Self-test for the parts of tier 3 that run on the host without a guest:
the caption and screen text a case's report carries (t3_evaluate.py), the
colors and cells of the sixel goldens, the tree a run installs and the
guest's account of it (t3_impl.py, t3_scenario.py), the machine each variant
boots, the console lines the driver waits for and the QEMU command line
(t3_qemu.py), the power cases' verdicts (t3_power.py), the pointer case's
reading of the cursor plane (t3_pointer.py), the outputs, keyboards and
udev examples of the device variants (t3_devices.py), a benchmark screen's
figures (t3_bench.py), the checks a run makes before it starts, and the
image cache and the logger a worker process receives (t3_common.py).

    python3 -I -B tools/tier3/test_t3.py
"""

import argparse
import base64
import contextlib
import csv
import gzip
import hashlib
import io
import json
import multiprocessing
import os
import pickle
import pwd
import re
import shlex
import shutil
import socket
import struct
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import unittest
import zipfile
import zlib
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3  # noqa: E402
import t3_batch  # noqa: E402
import t3_bench  # noqa: E402
import t3_apply  # noqa: E402
import t3_builtin  # noqa: E402
import t3_devices  # noqa: E402
import t3_fonts  # noqa: E402
import t3_evaluate  # noqa: E402
import t3_heads  # noqa: E402
import t3_images  # noqa: E402
import t3_impl  # noqa: E402
import t3_pictures  # noqa: E402
import t3_pointer  # noqa: E402
import t3_power  # noqa: E402
import t3_rootchecks  # noqa: E402
import t3_seat  # noqa: E402
import t3_handover  # noqa: E402
import t3_keyecho  # noqa: E402
import t3_qemu  # noqa: E402
import t3_runner  # noqa: E402
import t3_scenario  # noqa: E402
import t3_showcase  # noqa: E402
import t3_showcase_gallery  # noqa: E402
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "gallery",
                                "tests"))
import terminal as gallery_terminal  # noqa: E402
import t3img  # noqa: E402
import t3_common  # noqa: E402
from t3_common import Logger  # noqa: E402

LOG = Logger()

# What a run writes, the tests write into a directory of their own, never
# into the checkout's target/tier3/run, which `make tier3` writes. Each
# module that took RUNDIR from t3_common when it was imported reads this
# directory instead, and the directory is removed when the tests end.
_RUN = tempfile.TemporaryDirectory(prefix="vtium-test-t3-run-")
for _module in (t3_common, t3_evaluate, t3_impl, t3_runner, t3_scenario, t3_showcase):
    _module.RUNDIR = _RUN.name
t3_common.TREE_RECORD = os.path.join(_RUN.name, "tree.json")

SCRIPTS = {
    "colors": r"printf '\033[1;31mred\033[0m plain\nnext\n'",
    "fails": "echo partial; exit 3",
    "hangs": "sleep 5; echo late",
    "reads": 'read -r x; echo "got:$x"',
    "moves": r"printf 'a\033[2;5Hb\n'",
    "tab": r"printf 'a\tb\n'",
    "charset": r"printf '\033(0q\033(B\n'",
    "title": r"printf '\033]0;title\007x\n'",
    "scrolls": "# A screen.\n# screen-text: none, it scrolls.\necho never-run",
}


class ScreenTextTest(unittest.TestCase):
    def test_real_screens(self):
        self.assertTrue(t3_evaluate.screen_text("sgr", LOG)[2]
                        .startswith("bold underline reverse "))
        self.assertEqual(t3_evaluate.screen_text("cjk", LOG)[2], "|一|二|三|四|五|")
        self.assertEqual(t3_evaluate.screen_text("vt-restore", LOG),
                         t3_evaluate.screen_text(t3_evaluate.SCREENS[-1], LOG))
        self.assertIsNone(t3_evaluate.screen_text("vt-switch", LOG))
        # wide-margin moves the cursor to reach the screen's real edge,
        # s1-scroll prints 500 000 lines of which the screen shows the last,
        # the sixel and kitty screens draw pictures, flood clears what it
        # streamed, and sync-hold redraws its frames in place and prints the
        # terminal's replies, so they have no text on purpose; every other
        # screen has its rows.
        textless = ("wide-margin", "s1-scroll", "sixel", "sixel-ramps", "sixel-article",
                    "sixel-bound", "sixel-evict", "photos", "photo-scroll", "mpv-sixel",
                    "tmux-2x2", "tmux-3x3", "kitty", "kitty-photo", "kitty-files", "kitty-layers",
                    "flood", "sync-hold", "sync-clock")
        with contextlib.redirect_stderr(io.StringIO()):
            for name in textless:
                self.assertIsNone(t3_evaluate.screen_text(name, LOG))
        for name in t3_evaluate.SCREENS:
            if name not in textless:
                with self.subTest(name):
                    self.assertTrue(t3_evaluate.screen_text(name, LOG))
        # small-console fits 40 columns, its ruler ending at the 40th
        small = t3_evaluate.screen_text("small-console", LOG)
        self.assertEqual(len(small[0]), 40)
        self.assertTrue(small[0].endswith("40"))
        self.assertLessEqual(max(len(line) for line in small), 40)
        # e and U+0301 between bars: one cell built from two code points.
        self.assertEqual(t3_evaluate.screen_text("combining", LOG)[1][27:31], "|e\u0301|")

    def test_cjk_last(self):
        # vt-restore compares the screen left on tty2 with the last screen's
        # golden; its caption and the tier 3 README name cjk. A screen added
        # after cjk would move that check without a word.
        self.assertEqual(t3_evaluate.SCREENS[-1], "cjk")
        self.assertIn("the cjk golden", t3_evaluate.describe("vt-restore")[0])

    def text(self, name):
        with tempfile.TemporaryDirectory() as guest, contextlib.redirect_stderr(io.StringIO()):
            os.mkdir(os.path.join(guest, "screens"))
            with open(os.path.join(guest, "screens", name + ".sh"), "w") as f:
                f.write(SCRIPTS[name] + "\n")
            saved = t3_evaluate.GUEST, t3_evaluate.SCREENS, t3_evaluate.SCREEN_TIMEOUT
            t3_evaluate.GUEST, t3_evaluate.SCREENS, t3_evaluate.SCREEN_TIMEOUT = guest, [name], 1
            try:
                return t3_evaluate.screen_text(name, LOG)
            finally:
                t3_evaluate.GUEST, t3_evaluate.SCREENS, t3_evaluate.SCREEN_TIMEOUT = saved

    def test_colors_removed(self):
        self.assertEqual(self.text("colors"), ["red plain", "next"])

    def test_failure_gives_none(self):
        self.assertIsNone(self.text("fails"))

    def test_hang_gives_none(self):
        self.assertIsNone(self.text("hangs"))

    def test_input_not_inherited(self):
        # The run's own stdin carries a line the script must not see.
        r, w = os.pipe()
        os.write(w, b"secret\n")
        os.close(w)
        saved = os.dup(0)
        os.dup2(r, 0)
        try:
            self.assertEqual(self.text("reads"), ["got:"])
        finally:
            os.dup2(saved, 0)
            os.close(saved)
            os.close(r)

    def test_marked_none_not_run(self):
        # The script would print a line; its header says its lines are not
        # its rows, so it is not run.
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            self.assertIsNone(self.text("scrolls"))
        self.assertNotIn("never-run", err.getvalue())

    def test_other_controls_give_none(self):
        # Each would make line N of the output something other than row N.
        for name in ("moves", "tab", "charset", "title"):
            with self.subTest(name):
                self.assertIsNone(self.text(name))


class DescribeTest(unittest.TestCase):
    def describe(self, header):
        with tempfile.TemporaryDirectory() as guest:
            os.mkdir(os.path.join(guest, "screens"))
            with open(os.path.join(guest, "screens", "s.sh"), "w") as f:
                f.write(header + "echo body\n# not header\n")
            saved = t3_evaluate.GUEST
            t3_evaluate.GUEST = guest
            try:
                return t3_evaluate.describe("s")
            finally:
                t3_evaluate.GUEST = saved

    def test_guidance(self):
        # The demo's guidance lines, in any order among the caption's, after
        # the interpreter line.
        self.assertEqual(self.describe("#!/bin/bash\n# look-for: Lines\n# look-for: joined.\n"
                                       "# wrong-looks-like: A gap.\n# A screen.\n"),
                         ("A screen. Correct: Lines joined.", "A gap."))

    def test_plain_caption(self):
        self.assertEqual(self.describe("# A screen. Correct: x: y.\n"),
                         ("A screen. Correct: x: y.", None))

    def test_check(self):
        self.assertEqual(t3_evaluate.describe("vt-switch"),
                         (t3_evaluate.CHECKS["vt-switch"], None))

    def test_screen_text_not_in_caption(self):
        self.assertEqual(self.describe("# A screen.\n# screen-text: none, it scrolls.\n"),
                         ("A screen.", None))
        caption, _ = t3_evaluate.describe("s1-scroll")
        self.assertTrue(caption.startswith("S1 (ADR-103): text scrolling"))  # adr-cite-ignore: kmscon's record
        self.assertNotIn("screen-text", caption)
        self.assertIn("Correct: Numbered lines", caption)

    def test_benchmarks_are_screens(self):
        # A benchmark is drawn and compared like any screen, before cjk.
        for name in t3_evaluate.BENCHMARKS:
            self.assertIn(name, t3_evaluate.SCREENS[:-1] + t3_evaluate.PICTURE_SCREENS[:-1])

    def test_kit_not_in_caption(self):
        # The demo's kit line names the review kit a page belongs to.
        self.assertEqual(self.describe("# A screen.\n# kit: sixel\n# look-for: Lines.\n"),
                         ("A screen. Correct: Lines.", None))


class AcceptanceTest(unittest.TestCase):
    """The sixel acceptance scenarios as tier 3 cases: A1 (photos,
    photo-scroll and its two key steps), A2 (mpv-sixel) and A4 (the tmux
    grids)."""

    def test_screens(self):
        for name in ("photos", "photo-scroll", "mpv-sixel", "tmux-2x2", "tmux-3x3"):
            self.assertIn(name, t3_evaluate.SCREENS[:-1])
        for name in ("mpv-sixel", "tmux-2x2", "tmux-3x3"):
            self.assertIn(name, t3_evaluate.BENCHMARKS)

    def test_key_steps(self):
        # After photo-scroll, Shift+PageUp shows the picture again from the
        # scrollback and Shift+PageDown must give back photo-scroll's screen.
        self.assertEqual([(case, chords) for screen, case, chords in t3_evaluate.KEYED
                          if screen == "photo-scroll"],
                         [("photo-scrollback", [("shift", "pgup")]),
                          ("photo-scrollforward", [("shift", "pgdn")])])
        i = t3_evaluate.CASES.index("photo-scroll")
        self.assertEqual(t3_evaluate.CASES[i:i + 3],
                         ["photo-scroll", "photo-scrollback", "photo-scrollforward"])
        self.assertEqual(t3_evaluate.CASES[-2:], ["vt-switch", "vt-restore"])
        self.assertEqual(t3_evaluate.golden_for("v", "photo-scrollforward"),
                         t3_evaluate.golden_for("v", "photo-scroll"))
        self.assertTrue(t3_evaluate.golden_for("v", "photo-scrollback")[0]
                        .endswith(os.path.join("v", "photo-scrollback.png")))
        for step in ("photo-scrollback", "photo-scrollforward"):
            self.assertIn(step, t3_evaluate.CHECKS)
            self.assertEqual(t3_evaluate.describe(step), (t3_evaluate.CHECKS[step], None))
        self.assertEqual(t3_evaluate.screen_text("photo-scrollforward", LOG), None)


class SixelGoldenTest(unittest.TestCase):
    # The sixel page's colors (sixel.sh), as kmscon draws them: the bars,
    # the seven discs and swatches, each beside its text twin, the ring
    # around its blue disc, and the grey band under the swatches.
    BARS = [(255, 0, 0), (255, 255, 0), (0, 255, 0), (0, 255, 255), (0, 0, 255),
            (255, 0, 255), (255, 255, 255), (128, 128, 128)]
    DISCS = {"red": (255, 0, 0), "green": (0, 255, 0), "blue": (0, 0, 255),
             "cyan": (0, 255, 255), "magenta": (255, 0, 255), "yellow": (255, 255, 0),
             "orange": (255, 128, 0)}
    GREY = (128, 128, 128)

    @classmethod
    def grid(cls, variant):
        """The cell size and the screen's top-left cell in a variant's sixel
        golden: the red bar is the first red pixel and one cell, at column
        2, row 2 of a grid kmscon may centre on the display."""
        img = t3img.read(t3_evaluate.golden_for(variant, "sixel")[0])
        x0, y0 = next((x, y) for y in range(200) for x in range(200)
                      if cls.pixel(img, x, y) == cls.BARS[0])
        cw = next(n for n in range(1, 200) if cls.pixel(img, x0 + n, y0) != cls.BARS[0])
        ch = next(n for n in range(1, 200) if cls.pixel(img, x0, y0 + n) != cls.BARS[0])
        return cw, ch, x0 - 2 * cw, y0 - 2 * ch

    @staticmethod
    def pixel(img, x, y):
        i = (y * img.width + x) * 3
        return tuple(img.data[i:i + 3])

    def screen(self, variant, case):
        """The golden of a sixel screen, read at (column, row) in cells."""
        cw, ch, left, top = self.grid(variant)
        img = t3img.read(t3_evaluate.golden_for(variant, case)[0])
        return (lambda col, row: self.pixel(img, left + int(col * cw), top + int(row * ch)),
                img.width // cw, cw, ch)

    def test_colors_in_cells(self):
        # Every default variant's golden holds each picture in the cells the
        # page puts it in, in its colors.
        for variant in t3_scenario.DEFAULT_VARIANTS:
            with self.subTest(variant):
                at, _, cw, ch = self.screen(variant, "sixel")
                self.assertEqual([at(2.5 + i, 2.5) for i in range(8)], self.BARS)
                # each slot is 10 columns from column 2: the disc and its
                # text square on rows 9 and 10, the swatch and its text
                # square on rows 15 to 17 of the grey band
                for k, (name, rgb) in enumerate(self.DISCS.items()):
                    c = 2 + 10 * k
                    self.assertEqual(at(c + 1.5, 10), rgb, name + " disc")
                    self.assertEqual(at(c + 5.5, 10), rgb, name + " text square")
                    self.assertEqual(at(c + 2, 16.5), rgb, name + " swatch")
                    self.assertEqual(at(c + 6, 16.5), rgb, name + " text swatch")
                    self.assertEqual(at(c + 9, 16.5), self.GREY, name + " band")
                d = min(4 * cw, 2 * ch)
                self.assertEqual(at(42 - 3 * d / 8 / cw, 4), self.DISCS["orange"])
                self.assertEqual(at(42, 4), self.DISCS["blue"])

    def test_ramps_match_their_text(self):
        # sixel-ramps.sh: each ramp on rows 4, 7, 10, 13 and 16 from column
        # 2, one cell a step, over its text strip on the next row; the
        # swatches on rows 19 and 20, 7 columns apart, over their text twins
        # on rows 21 and 22. Every step and swatch equals its text twin.
        ends = {4: ((0, 0, 0), (255, 0, 0)), 7: ((0, 0, 0), (0, 255, 0)),
                10: ((0, 0, 0), (0, 0, 255)), 13: ((0, 0, 0), (255, 255, 255)),
                16: ((255, 0, 0), None)}
        for variant in t3_scenario.DEFAULT_VARIANTS:
            with self.subTest(variant):
                at, cols, _, _ = self.screen(variant, "sixel-ramps")
                # the steps the page chose: the most that divide 100% evenly
                # and fit the columns (registers are 1024 here)
                n = next(n for n in (101, 51, 26, 21, 11, 6, 5, 3, 2) if n <= cols - 4)
                for row, (first, last) in ends.items():
                    steps = [at(2.5 + i, row + 0.5) for i in range(n)]
                    self.assertEqual(steps, [at(2.5 + i, row + 1.5) for i in range(n)],
                                     "ramp on row %d" % row)
                    self.assertEqual(steps[0], first, "ramp on row %d" % row)
                    if last:
                        self.assertEqual(steps[-1], last, "ramp on row %d" % row)
                    self.assertEqual(len(set(steps)), n, "ramp on row %d" % row)
                for k in range(10):
                    self.assertEqual(at(5 + 7 * k, 20), at(5 + 7 * k, 22), "swatch %d" % k)


def write_tree(root, files):
    """Write {relative path: text} under `root`."""
    for name, text in files.items():
        os.makedirs(os.path.join(root, os.path.dirname(name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as f:
            f.write(text)


class ImplementationTest(unittest.TestCase):
    """The tree under test (t3_impl.py, `t3.py run`): vtium's install tree,
    its size on disk and its binary's against the budget, and the guest's
    account of what it installed."""

    UNIT = t3_impl.VT_UNIT
    TREE = {"usr/bin/vtium": "12345678", UNIT: "[Service]\n", "usr/share/doc/x": "abc"}
    # what make install-tree adds to it with images in FEATURES (ADR-1101 D1)
    IMAGES = {f: "x" for f in t3_impl.IMAGE_FILES}
    LINE = "[  12.3] TIER3-INSTALL-OK packages: ; kernel: 6.17.2-arch1-1; tree: %s"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.tree = os.path.join(self.dir.name, "tree")
        write_tree(self.tree, self.TREE)
        self.tar = os.path.join(self.dir.name, "out", "tree.tar")

    def test_a_tree_installs_no_udev_file(self):
        # vtium's udev material is examples alone (ADR-1401 D4): a tree with
        # anything under a directory udev reads is refused, a link included.
        for path in ("etc/udev/hwdb.d/vtium.hwdb", "usr/lib/udev/rules.d/70-vtium.rules",
                     "lib/udev/hwdb.d/vtium.hwdb"):
            with self.subTest(path), tempfile.TemporaryDirectory() as d:
                tree = os.path.join(d, "tree")
                write_tree(tree, self.TREE)
                os.makedirs(os.path.dirname(os.path.join(tree, path)))
                with open(os.path.join(tree, path), "w") as f:
                    f.write("x")
                udev = "/" + "/".join(path.split("/")[:3 if path.startswith("usr") else 2])
                with self.assertRaisesRegex(SystemExit, "installs %s, where udev reads" % udev):
                    t3_impl.tree(tree, os.path.join(d, "tree.tar"))
        with tempfile.TemporaryDirectory() as d:
            tree = os.path.join(d, "tree")
            write_tree(tree, self.TREE)
            os.makedirs(os.path.join(tree, "etc"))
            os.symlink("/tmp", os.path.join(tree, "etc", "udev"))
            with self.assertRaisesRegex(SystemExit, "installs /etc/udev, where udev reads"):
                t3_impl.tree(tree, os.path.join(d, "tree.tar"))
        # the examples under the documentation are fine
        write_tree(self.tree, {"usr/share/doc/vtium/examples/vtium.hwdb": "x"})
        t3_impl.tree(self.tree, self.tar)

    def test_tree(self):
        # The bytes of the regular files a tree installs; a link counts nothing.
        os.symlink("vtium", os.path.join(self.tree, "usr/bin/kmscon"))
        impl = t3_impl.tree(self.tree, self.tar)
        self.assertEqual((impl["binary_bytes"], impl["installed_bytes"]), (8, 8 + 10 + 3))
        # packed for the disc owned by root, without the tree's own directory
        with tarfile.open(impl["tar"]) as t:
            members = t.getmembers()
        self.assertEqual({m.name for m in members},
                         {"usr", "usr/bin", "usr/bin/vtium", "usr/bin/kmscon", "usr/lib",
                          "usr/lib/systemd", "usr/lib/systemd/system", self.UNIT, "usr/share",
                          "usr/share/doc", "usr/share/doc/x"})
        self.assertEqual({(m.uid, m.gid, m.uname, m.gname) for m in members},
                         {(0, 0, "root", "root")})
        # the same tree gives the same archive, so the digest names the tree
        again = t3_impl.tree(self.tree, self.tar + "2")
        self.assertEqual(again["tree_sha256"], impl["tree_sha256"])
        self.assertRegex(impl["tree_sha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(t3_impl.describe(impl), {
            "tree": t3_common.rel(self.tree), "tree_sha256": impl["tree_sha256"],
            "features": [], "binary_bytes": 8, "installed_bytes": 21,
            "budget": {"binary_bytes": t3_impl.BINARY_BUDGET, "met": True}})

    def test_budget(self):
        # The operator's budget is the minspec binary's bytes, 4 MiB + 192 KiB
        # (ADR-1200 D19): at it the run passes, one byte past it fails. The
        # rest of the tree does not count.
        budget = t3_impl.BINARY_BUDGET
        self.assertEqual(budget, 4390912)
        impl = t3_impl.tree(self.tree, self.tar)
        for size, met in ((budget, True), (budget + 1, False)):
            impl["binary_bytes"] = size
            self.assertEqual(t3_impl.within_budget(impl), met, size)
            self.assertEqual(t3_impl.describe(impl)["budget"],
                             {"binary_bytes": budget, "met": met})
        # A tree whose console is built with features is no minspec tree:
        # the budget does not judge it, however large its binary.
        write_tree(self.tree, self.IMAGES)
        impl = t3_impl.tree(self.tree, self.tar, features=["logind", "images"])
        impl["binary_bytes"] = budget + 1
        self.assertFalse(t3_impl.judged_by_budget(impl))
        self.assertTrue(t3_impl.within_budget(impl))
        self.assertEqual((t3_impl.describe(impl)["features"], t3_impl.describe(impl)["budget"]),
                         (["images", "logind"], None))

    def test_install_tree_features(self):
        # make install-tree builds the console with the features its command
        # line names, and with none from a FEATURES exported in the shell. An
        # enclosing make's command line, which reaches these through
        # MAKEFLAGS, is kept out.
        bare = {k: v for k, v in os.environ.items() if k not in ("MAKEFLAGS", "MFLAGS")}

        def rustc(argv, env):
            r = subprocess.run(["make", "-n", "--no-print-directory", "install-tree",
                                "DESTDIR=/D"] + argv, cwd=t3_common.ROOT, capture_output=True,
                               text=True, check=True, env=dict(bare, **env))
            return [l for l in r.stdout.splitlines() if "cargo rustc" in l]

        for argv, env, want in ((["FEATURES=images logind"], {}, " --features 'images logind' "),
                                (["FEATURES=images"], {}, " --features 'images' ")):
            with self.subTest(argv):
                lines = rustc(argv, env)
                self.assertEqual(len(lines), 1)
                self.assertIn(want, lines[0])
        for argv, env in (([], {}), ([], {"FEATURES": "images"}), (["FEATURES="], {})):
            with self.subTest(argv=argv, env=env):
                lines = rustc(argv, env)
                self.assertEqual(len(lines), 1)
                self.assertNotIn("--features", lines[0])
        # the tier 3 targets hand the same list to t3.py, which builds the
        # tree with it and judges the budget by it, and hand none when make's
        # command line names none, so tier3-run reads the last run's record
        for goal in ("tier3-run", "tier3", "tier3-minimal", "tier3-nosystemd"):
            for argv, env, want in ((["FEATURES=images"], {}, r" --features 'images'( |$)"),
                                    (["FEATURES="], {}, r" --features ''( |$)"),
                                    ([], {"FEATURES": "images"}, None), ([], {}, None)):
                with self.subTest(goal=goal, argv=argv, env=env):
                    r = subprocess.run(["make", "-n", "--no-print-directory", goal] + argv,
                                       cwd=t3_common.ROOT, capture_output=True, text=True,
                                       check=True, env=dict(bare, **env))
                    lines = r.stdout.replace("\\\n", " ").splitlines()
                    runs = [l for l in lines if "/t3.py run " in l]
                    self.assertEqual(len(runs), 1)
                    if want:
                        self.assertRegex(runs[0], want)
                    else:
                        self.assertNotIn("--features", runs[0])

    def test_refused(self):
        # No tree, a file in its place, or a tree without the binary or the
        # VT unit the guest starts.
        binary = os.path.join(self.tree, "usr/bin/vtium")
        for path, why in ((os.path.join(self.dir.name, "missing"), "no install tree at"),
                          (binary, "no install tree at")):
            with self.subTest(why):
                with self.assertRaisesRegex(SystemExit, why):
                    t3_impl.tree(path, self.tar)
        for name in (self.UNIT, "usr/bin/vtium"):
            with self.subTest(name):
                write_tree(self.tree, self.TREE)
                os.unlink(os.path.join(self.tree, name))
                with self.assertRaisesRegex(SystemExit, "holds no /%s" % re.escape(name)):
                    t3_impl.tree(self.tree, self.tar)

    def test_install_checked(self):
        # No kmscon, libtsm or vtium package in the guest, and the tree the
        # run packed; anything else, or no account, fails the variant.
        impl = t3_impl.tree(self.tree, self.tar)
        sha = impl["tree_sha256"]
        self.assertEqual(t3_scenario.check_install(self.LINE % sha, impl),
                         {"kernel": "6.17.2-arch1-1", "tree": sha})
        # a guest that names no kernel leaves the result without the kernel
        # it ran on, so the variant fails
        for line in ((self.LINE % sha).replace(" kernel: 6.17.2-arch1-1;", ""),
                     (self.LINE % sha).replace("6.17.2-arch1-1", "")):
            with self.assertRaisesRegex(RuntimeError, "the guest reported no kernel version"):
                t3_scenario.check_install(line, impl)
        with self.assertRaisesRegex(RuntimeError, "the guest installed tree 0+, the run's is"):
            t3_scenario.check_install(self.LINE % ("0" * 64), impl)
        with self.assertRaisesRegex(RuntimeError, "packages installed that are not the run's "
                                                  "tree: kmscon-aaronsb 10-1"):
            t3_scenario.check_install((self.LINE % sha).replace(
                "packages: ;", "packages: kmscon-aaronsb 10-1 ;"), impl)
        with self.assertRaisesRegex(RuntimeError, "cannot read"):
            t3_scenario.check_install("TIER3-INSTALL-OK", impl)

    def test_setup_reports_install(self):
        # guest/setup's report line is the one check_install reads.
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            text = f.read()
        self.assertIn('"TIER3-INSTALL-OK packages: ', text)
        self.assertIn('"kernel: $(uname -r);"', text)
        self.assertIn('"tree: $tree"', text)
        self.assertNotIn("pacman -U", text)

    def test_disc(self):
        # The tree goes on the disc as tree.tar, with the variant's config,
        # the pointer variant's carries the `pointer` file, on which
        # guest/setup lets the screens read the DRM state, and a PSF
        # variant's its frits, which guest/setup builds into the store, and
        # the batch file that activates its font (t3_batch).
        impl = t3_impl.tree(self.tree, self.tar)
        disc = os.path.join(self.dir.name, "disc")
        for variant, extra in (("2d-potato-min", ["pointer"]),
                               ("2d-potato-min-psf", ["frits", "batch.conf"])):
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(variant, self.dir.name, impl)
                self.assertEqual(sorted(os.listdir(disc)),
                                 sorted(["vtium.conf", "screens", "setup", "t3", "tree.tar"]
                                        + extra))
                with open(os.path.join(disc, "vtium.conf")) as f:
                    self.assertIn("render-timer=log", f.read())
                if "frits" in extra:
                    self.assertEqual(os.listdir(os.path.join(disc, "frits")),
                                     ["default8x16.vtfrit"])
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            self.assertIn('if [ -f "$disc"/pointer ]; then', f.read())
        # Each guest installs the disc's configuration where vtium reads it,
        # /etc/vtium/vtium.conf (ADR-1500 D4), as root's, which the start's
        # ownership check trusts (ADR-1403 D3), and nothing into /etc/kmscon.
        for guest in (t3_common.GUEST, t3_common.GUEST_SYSVINIT):
            with self.subTest(guest), open(os.path.join(guest, "setup")) as f:
                setup = f.read()
                self.assertIn('install -Dm644 -o root -g root "$disc"/vtium.conf '
                              '/etc/vtium/vtium.conf\n', setup)
                self.assertNotIn("/etc/kmscon", setup)

    def test_frit_variants(self):
        # A configured frit generated at run time (ADR-1301 D2): two 640x480
        # variants on the minspec machine, in tier3-minimal, judged by
        # vtium's goldens; the disc carries the frit to be installed with no
        # set built, and the held variant's `helper-held`, on which
        # guest/setup puts a helper that never answers in place.
        small = t3_scenario.VARIANTS["2d-640x480-psf16"]
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            minimal = re.search(r"^tier3-minimal:.*\n\t(.*)", f.read(), re.M).group(1)
        impl = t3_impl.tree(self.tree, self.tar)
        disc = os.path.join(self.dir.name, "disc")
        for name, held in (("2d-640x480-frit", False), ("2d-640x480-frit-held", True)):
            with self.subTest(name), mock.patch.object(t3_scenario, "make_iso"):
                v = t3_scenario.VARIANTS[name]
                for key in ("device", "cpus", "memory_mb", "renderer", "images"):
                    self.assertEqual(v[key], small[key])
                self.assertIn(name, minimal)
                self.assertNotIn("frits", v)
                self.assertEqual(t3_scenario.screens_of(name), ["small-console", "cjk"])
                with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
                    font = re.search(r"^font-name=(.*)$", f.read(), re.M).group(1)
                self.assertEqual(v["deferred_frits"], [font + ".vtfrit"])
                with open(os.path.join(t3_common.GUEST, "frits", font + ".vtfrit")) as f:
                    self.assertIn("source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf\n",
                                  f.read())
                t3_scenario.build_disc(name, self.dir.name, impl)
                self.assertEqual(sorted(os.listdir(disc)),
                                 sorted(["vtium.conf", "screens", "setup", "t3", "tree.tar",
                                         "deferred-frits", "batch.conf"]
                                        + (["helper-held"] if held else [])))
                self.assertEqual(os.listdir(os.path.join(disc, "deferred-frits")),
                                 [font + ".vtfrit"])
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        self.assertIn('if [ -d "$disc"/deferred-frits ]; then', setup)
        self.assertIn('if [ -f "$disc"/helper-held ]; then', setup)

    def test_fonts_disc(self):
        # A variant drawing with the test fonts carries them, which
        # guest/setup installs where its config names them; others carry none.
        impl = t3_impl.tree(self.tree, self.tar)
        fonts = os.path.join(self.dir.name, "fonts")
        os.mkdir(fonts)
        for name in ("vtium-test-8x8.psf", "vtium-test-8x16.psf"):
            with open(os.path.join(fonts, name), "w") as f:
                f.write(name)
        impl["fonts"] = fonts
        disc = os.path.join(self.dir.name, "disc")
        for variant, conf in (("2d-320x240-psf8", "vtium-test-8x8.psf"),
                              ("2d-640x480-psf16", "vtium-test-8x16.psf")):
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(variant, self.dir.name, impl)
                self.assertEqual(sorted(os.listdir(os.path.join(disc, "fonts"))),
                                 ["vtium-test-8x16.psf", "vtium-test-8x8.psf"])
                with open(os.path.join(disc, "vtium.conf")) as f:
                    self.assertIn("font-name=/usr/local/share/tier3-fonts/%s\n" % conf,
                                  f.read())
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("2d-potato-min", self.dir.name, impl)
        self.assertNotIn("fonts", os.listdir(disc))
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            self.assertIn('install -Dm644 -t /usr/local/share/tier3-fonts "$disc"/fonts/*',
                          f.read())

    def test_text_font_set(self):
        # Tier 3's default 2D variants and its GPU variants draw from its
        # text font set, the frit tier3-text (ADR-1503 D9): each one's
        # configuration names the frit, which the disc carries to be
        # installed with no set built, beside Noto Sans CJK SC, the set's font
        # the guest image lacks, which guest/setup installs where the frit
        # names it.
        cache = os.path.join(self.dir.name, "cache")
        os.makedirs(cache)
        for font in t3_scenario.TEXT_FONTS:
            with open(os.path.join(cache, font.file), "w") as f:
                f.write(font.name)
        impl = dict(t3_impl.tree(self.tree, self.tar), text_fonts=cache)
        disc = os.path.join(self.dir.name, "disc")
        drawn = ["2d-1280x800", "2d-1366x768-potato", "2d-7680x2160", "2d-7680x2160-font32",
                 "gl-1280x800", "gpu-1280x800"]
        self.assertEqual(sorted(v for v, conf in t3_scenario.VARIANTS.items()
                                if conf.get("text_fonts")), sorted(drawn))
        self.assertLessEqual(set(t3_scenario.DEFAULT_VARIANTS), set(drawn))
        for variant in drawn:
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                v = t3_scenario.VARIANTS[variant]
                self.assertEqual(v["deferred_frits"], ["tier3-text.vtfrit"])
                with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
                    conf = f.read()
                self.assertIn("\nfont-name=tier3-text\n", conf)
                self.assertNotIn("font-engine", conf)
                t3_scenario.build_disc(variant, self.dir.name, impl)
                self.assertEqual(os.listdir(os.path.join(disc, "deferred-frits")),
                                 ["tier3-text.vtfrit"])
                self.assertEqual(os.listdir(os.path.join(disc, "text-fonts")),
                                 ["NotoSansCJKsc-Regular.otf"])
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("2d-potato-min", self.dir.name, impl)
        self.assertNotIn("text-fonts", os.listdir(disc))
        # The frit names DejaVu Sans Mono's four faces, each style by
        # style=, then the CJK font where guest/setup installs it, then Noto
        # Color Emoji, and states no size or cell, so each configuration's
        # font-size sets the cell by the W rule.
        with open(os.path.join(t3_common.GUEST, "frits", "tier3-text.vtfrit")) as f:
            frit = [line for line in f.read().splitlines() if line and not line.startswith("#")]
        cjk = "%s/NotoSansCJKsc-Regular.otf" % t3_scenario.TEXT_FONTS_DIR
        self.assertEqual([line for line in frit if not line.startswith("license=")], [
            "name=tier3-text",
            "source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "source=outline:/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf", "style=bold",
            "source=outline:/usr/share/fonts/TTF/DejaVuSansMono-Oblique.ttf", "style=italic",
            "source=outline:/usr/share/fonts/TTF/DejaVuSansMono-BoldOblique.ttf",
            "style=bold-italic",
            "source=outline:" + cjk,
            "source=outline:/usr/share/fonts/noto/NotoColorEmoji.ttf"])
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            self.assertIn('install -Dm644 -t %s "$disc"/text-fonts/*' % t3_scenario.TEXT_FONTS_DIR,
                          f.read())

    def test_text_fonts_fetch(self):
        # A run fetches the text font set's pinned fonts into the cache
        # before any variant boots, and only when one of its variants draws
        # from the set; a fetch that fails stops the run with the reason.
        fetched = []
        fetch = lambda fonts: fetched.append(fonts) or "/cache"
        self.assertIsNone(t3.text_fonts(["2d-potato-min"], LOG, fetch))
        self.assertEqual(fetched, [])
        self.assertEqual(t3.text_fonts(["2d-potato-min", "2d-1280x800"], LOG, fetch), "/cache")
        self.assertEqual(fetched, [t3_scenario.TEXT_FONTS])
        self.assertEqual([f.sha256 for f in t3_scenario.TEXT_FONTS],
                         ["2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b"])

        def fail(fonts):
            raise subprocess.CalledProcessError(6, ["curl"])

        with self.assertRaisesRegex(SystemExit, "fetching the text font set's fonts failed"):
            t3.text_fonts(["gl-1280x800"], LOG, fail)
        with self.assertRaisesRegex(ValueError, "pins no font X.ttf"):
            t3_showcase.pinned("X.ttf")

    def test_power_disc(self):
        # The hard reboot's disc carries its sysctl settings, which
        # guest/setup applies; the other power variants' carry none.
        impl = t3_impl.tree(self.tree, self.tar)
        disc = os.path.join(self.dir.name, "disc")
        built = os.path.join(self.dir.name, "vtium-logind")
        with open(built, "w") as f:
            f.write("logind build")
        impl["features"] = {"logind": built}
        verbose = "kernel.printk = 7 4 1 7\n"
        for variant, sysctl, binary in (
                ("power-reboot-hard", "kernel.ctrl-alt-del = 1\n", None),
                ("power-reboot", None, None), ("power-off", None, None),
                ("power-suspend-off", verbose, None),
                ("power-suspend-logind", verbose, "logind build")):
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(variant, self.dir.name, impl)
                path = os.path.join(disc, "sysctl.conf")
                if sysctl is None:
                    self.assertFalse(os.path.exists(path))
                else:
                    with open(path) as f:
                        self.assertEqual(f.read(), sysctl)
                path = os.path.join(disc, "vtium")
                if binary is None:
                    self.assertFalse(os.path.exists(path))
                else:
                    with open(path) as f:
                        self.assertEqual(f.read(), binary)
                with open(os.path.join(disc, "vtium.conf")) as f:
                    self.assertIn("key-reboot=", f.read())
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            text = f.read()
        self.assertIn('sysctl -q -p "$disc"/sysctl.conf', text)
        self.assertIn('install -m755 "$disc"/vtium /usr/bin/vtium', text)

    def test_setup_keeps_directory_links(self):
        # Arch links /bin, /lib and /sbin to directories under /usr; a tree
        # holding bin/ or lib/ must land through those links, never replace
        # them. Runs guest/setup's own tar command over a stand-in root.
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            line = next(x for x in f if x.strip().startswith("tar ") and "tree.tar" in x)
        args = shlex.split(line.replace('"$disc"/tree.tar', "TREE"))
        root = os.path.join(self.dir.name, "root")
        os.makedirs(os.path.join(root, "usr/bin"))
        os.symlink("usr/bin", os.path.join(root, "bin"))
        write_tree(self.tree, {"bin/vtium-helper": "x"})
        tar = t3_impl.pack_tree(self.tree, self.tar)["tar"]
        args = [tar if a == "TREE" else root if a == "/" else a for a in args]
        subprocess.run(args, check=True)
        self.assertTrue(os.path.islink(os.path.join(root, "bin")))
        self.assertTrue(os.path.isfile(os.path.join(root, "usr/bin/vtium-helper")))

    def test_resident(self):
        # The minspec figures: after the first screen, and the peak with the
        # screen it followed.
        got = t3_scenario.resident({"sgr": 10547, "s1-scroll": 16077, "cjk": 15900})
        self.assertEqual(got, {"first_screen": "sgr", "first_kib": 10547,
                               "peak_kib": 16077, "peak_screen": "s1-scroll"})
        self.assertEqual(t3_scenario.resident({}), {"first_screen": "sgr", "first_kib": None,
                                                    "peak_kib": None, "peak_screen": None})
        self.assertEqual(t3_scenario.failure_meta("x")["resident"], t3_scenario.resident({}))
        # a variant with screens of its own counts from its first
        self.assertEqual(t3_scenario.resident({"small-console": 9000}, ["small-console", "cjk"]),
                         {"first_screen": "small-console", "first_kib": 9000,
                          "peak_kib": 9000, "peak_screen": "small-console"})
        # the pointer case's readings, after the VT switch, are no screen
        self.assertEqual(t3_scenario.resident({"sgr": 10547, "pointer": 20000})["peak_screen"],
                         "sgr")

    def test_variant_errors(self):
        # The scenario's own error comes first and the renderer's after it, so
        # a console that never started reports why. A drawn screen without a
        # resident-memory reading fails the variant, as does a console that
        # does not log itself as vtium. A failure drops the captures, so
        # every case fails.
        bbulk = ["Display x with backend [drm2d] text renderer [bbulk]"]
        caps = {"sgr": "a", "cjk": "b", "vt-switch": "c"}
        waited = "no 'vt: using tty /dev/tty2' within 60 s"
        vtium = {"name": "vtium", "revision": "0.1"}
        kmscon = {"name": "kmscon", "revision": "v10"}
        none = "the console logged no Revision line, so nothing shows vtium drew"

        def errors(error, displays, caps, rss, console=vtium):
            return t3_scenario.variant_errors(error, displays, "bbulk", caps, rss, console)

        self.assertEqual(errors(waited, [], {}, {}, None),
                         ([waited, "the console did not draw with bbulk: []", none], True))
        self.assertEqual(errors(None, bbulk, caps, {"sgr": 1}),
                         (["no resident memory reading (T3-RSS) after cjk"], True))
        rss = {"sgr": 1, "cjk": 2}
        self.assertEqual(errors(None, bbulk, caps, rss), ([], False))
        # a scenario that stopped part way keeps the captures it took
        self.assertEqual(errors(waited, bbulk, {"sgr": "a"}, {"sgr": 1}), ([waited], False))
        self.assertEqual(errors(None, bbulk, caps, rss, kmscon),
                         (["the console that drew logged itself as kmscon v10, not vtium"],
                          True))
        self.assertEqual(errors(None, bbulk, caps, rss, None), ([none], True))
        # a variant's own screens are the ones read
        small = {"small-console": "a", "cjk": "b"}
        self.assertEqual(t3_scenario.variant_errors(None, bbulk, "bbulk", small, {"cjk": 1},
                                                    vtium, ["small-console", "cjk"]),
                         (["no resident memory reading (T3-RSS) after small-console"], True))

    def test_console_revision(self):
        # The guest's own account of the console that ran: its log's
        # "NAME Revision" line, the first one on the console.
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    9.1] vtium[502]: log: vtium Revision 0.1.0\n"
                    "[    9.6] kmscon[459]: log: kmscon Revision v10.1.0-890-gb1f7a7c5\n")
        try:
            self.assertEqual(t3_scenario.console_revision(f.name),
                             {"name": "vtium", "revision": "0.1.0"})
            with open(f.name, "w") as g:
                g.write("[    9.1] vtium[502]: vt: using tty /dev/tty2\n")
            self.assertIsNone(t3_scenario.console_revision(f.name))
        finally:
            os.unlink(f.name)
        # the report names every console its variants logged
        metas = {"a": {"console": {"name": "vtium", "revision": "0.1"}},
                 "b": {"console": {"name": "vtium", "revision": "0.1"}}, "c": {"console": None}}
        self.assertEqual(t3_scenario.consoles(metas), ["vtium 0.1"])

    def test_summary(self):
        budget = t3_impl.BINARY_BUDGET
        impl = t3_impl.tree(self.tree, self.tar)
        impl["binary_bytes"], impl["installed_bytes"] = budget + 1, 2630443
        metas = {"2d-potato-min": {"resident": t3_scenario.resident({"sgr": 10547,
                                                                     "s1-scroll": 16077}),
                                   "console": {"name": "vtium", "revision": "0.1"},
                                   "kernel": "6.17.2-arch1-1"},
                 "2d-potato-min-psf": {"resident": t3_scenario.resident({}), "console": None,
                                       "kernel": None}}
        metas["heads-policy-typo"] = {"resident": t3_scenario.resident({}, []),
                                      "console": None, "kernel": "7.2.11-arch1-1"}
        self.assertEqual(t3.summary(impl, metas).splitlines(), [
            "tree %s: binary %d B, over the budget of %d B by 1 B; 2.51 MiB "
            "installed; the console logged vtium 0.1"
            % (t3_common.rel(self.tree), budget + 1, budget),
            "2d-potato-min: kernel 6.17.2-arch1-1, resident 10.30 MiB after sgr, peak "
            "15.70 MiB after s1-scroll",
            "2d-potato-min-psf: kernel not reported, resident none after sgr, peak none "
            "after none",
            "heads-policy-typo: kernel 7.2.11-arch1-1, resident none after none, peak none "
            "after none"])
        impl["binary_bytes"] = 2368120
        self.assertTrue(t3.summary(impl, {}).startswith(
            "tree %s: binary 2368120 B, within the budget of %d B; "
            % (t3_common.rel(self.tree), budget)))
        write_tree(self.tree, self.IMAGES)
        impl = t3_impl.tree(self.tree, self.tar, features=["images"])
        impl["binary_bytes"] = budget + 1
        self.assertTrue(t3.summary(impl, {}).startswith(
            "tree %s: built with images: binary %d B, which the budget, the minspec tree's "
            "alone, does not judge; " % (t3_common.rel(self.tree), budget + 1)))

    def test_vtium_process(self):
        # vtium's process logs as vtium[pid]; its render-timer and startup
        # lines are read like the console's.
        line = timer_line(100.5, 60, 60).replace("kmscon[505]", "vtium[505]")
        self.assertEqual(t3_bench.timer_lines(line)[0]["pid"], 505)
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] vtium[459]: log: vtium Revision 0.1\n"
                    "[    6.700000] kmscon[459]: terminal: Display a with backend [x]\n"
                    "[    6.739898] vtium[459]: terminal: Display b with backend [drm2d] "
                    "text renderer [bbulk]\n")
        try:
            self.assertEqual(t3_scenario.console_startup_ms(f.name), 76.7)
        finally:
            os.unlink(f.name)
        # vtium logs its Display line without the "terminal:" the earlier console wrote
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] vtium[459]: log: vtium Revision 0.1\n"
                    "[    6.700000] vtium[459]: vt: using tty /dev/tty2\n"
                    "[    6.739898] vtium[459]: Display Virtual-1 with backend [drm2d] "
                    "text renderer [bbulk], cell size 8x16\n")
        try:
            self.assertEqual(t3_scenario.console_startup_ms(f.name), 76.7)
        finally:
            os.unlink(f.name)
        with open(os.path.join(t3_common.GUEST, "t3")) as f:
            self.assertIn("pidof kmscon vtium", f.read())

    def test_tools(self):
        # The report names vtium's commit and the guest's pins.
        env = {"T3_VTIUM_REV": "84eec92+dirty", "T3_ARCHIVE": "2026/10/20"}
        with mock.patch.dict(os.environ, env, clear=True):
            tools = t3_runner.run_tools()
        self.assertEqual(tools, {"vtium": "84eec92+dirty", "guest_image": None,
                                 "guest_image_sha256": None, "guest_archive": "2026/10/20"})

    def test_option(self):
        # --tree names a tree built already; --tree-target builds the default
        # one, so the two are refused together, before run is called.
        with mock.patch.object(t3, "run", side_effect=lambda args, log: args) as run:
            args = t3.main(["run", "--tree", "a", "2d-potato-min"])
        self.assertEqual((args.tree, args.tree_target, args.features, args.variants),
                         ("a", None, None, ["2d-potato-min"]))
        self.assertTrue(run.called)
        # --features names them as cargo takes them, by commas or spaces
        for text, want in (("", []), ("images", ["images"]), ("logind,images", ["images", "logind"]),
                           (" images  logind images ", ["images", "logind"])):
            self.assertEqual(t3.feature_list(text), want, text)
        for argv, why in ((["--tree", "a", "--tree-target", "install-tree"],
                           "--tree names a tree built already"),
                          (["2d-potato-min", "2d-potato-min"], "a variant is named twice"),
                          (["no-such-variant"], "unknown variant(s): no-such-variant"),
                          (["--jobs", "0"], "--jobs must be at least 1")):
            with self.subTest(argv), mock.patch.object(t3, "run") as run:
                err = io.StringIO()
                with contextlib.redirect_stderr(err), self.assertRaises(SystemExit):
                    t3.main(["run"] + argv)
                self.assertIn(why, err.getvalue())
                self.assertFalse(run.called)

    def test_build_tree(self):
        # The tree is staged afresh: what an earlier build left is gone
        # before make runs, and make is given the tree as DESTDIR.
        tree = os.path.join(self.dir.name, "install")
        write_tree(tree, {"stale": "from an earlier build"})
        seen = []

        def make(args, cwd):
            seen.append((args, cwd, os.path.exists(tree)))
            return subprocess.CompletedProcess(args, 0)

        # build_tree removes the tree's record first; the test's record is
        # its own, so the last real run's stays for make tier3-run
        record = os.path.join(self.dir.name, "tree.json")
        self.addCleanup(mock.patch.object(t3_common, "TREE_RECORD", record).stop)
        mock.patch.object(t3_common, "TREE_RECORD", record).start()
        write_tree(self.dir.name, {"tree.json": "{}"})
        with mock.patch.object(t3.subprocess, "run", side_effect=make):
            t3.build_tree("install-tree", tree, [], lambda msg: None)
            self.assertFalse(os.path.exists(record))
            write_tree(tree, {"stale": "from an earlier build"})
            t3.build_tree("install-tree", tree, ["images", "logind"], lambda msg: None)
        # FEATURES is always given, empty for the minspec tree, so make's
        # own command line never adds features to it.
        self.assertEqual(seen, [(["make", "--no-print-directory", "install-tree",
                                  "DESTDIR=" + tree, "FEATURES="], t3_common.ROOT, False),
                                (["make", "--no-print-directory", "install-tree",
                                  "DESTDIR=" + tree, "FEATURES=images logind"], t3_common.ROOT,
                                 False)])
        with mock.patch.object(t3.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 2)):
            with self.assertRaisesRegex(SystemExit, "make install-tree failed with exit code 2"):
                t3.build_tree("install-tree", tree, [], lambda msg: None)



class BatchTest(unittest.TestCase):
    """The batch file a variant installs (t3_batch.py): none when the list
    of a fresh install holds every font it draws, else that list followed
    by its own fonts (ADR-1205 D4)."""

    # The list of a fresh install, spelled out: Terminus in each size and
    # weight, GNU Unifont and Cullet Mono's two faces in the package's font
    # folder, then the tree's color schemes, then its pointer file.
    FRESH = (["/usr/share/vtium/fonts/ter-u%d%s.bdf.gz" % (size, weight)
              for size in (12, 14, 16, 18, 20, 22, 24, 28, 32) for weight in "nb"]
             + ["/usr/share/vtium/fonts/unifont_all-17.0.03.hex",
                "/usr/share/vtium/fonts/CulletMono-Regular.ttf",
                "/usr/share/vtium/fonts/CulletMono-Bold.ttf",
                "/usr/share/vtium/palettes/amber.palette",
                "/usr/share/vtium/palettes/zenith.palette",
                "/usr/share/vtium/pointers.svg"])
    # The fonts beyond that list that each variant draws, as its
    # configuration and its frits name them.
    EXTRA = {
        "2d-potato-min-psf": ["/usr/share/kbd/consolefonts/default8x16.psfu.gz"],
        "2d-320x240-psf8": ["/usr/local/share/tier3-fonts/vtium-test-8x8.psf"],
        "2d-640x480-psf16": ["/usr/local/share/tier3-fonts/vtium-test-8x16.psf"],
        "2d-640x480-frit": ["/usr/share/fonts/TTF/DejaVuSansMono.ttf"],
        "2d-640x480-frit-held": ["/usr/share/fonts/TTF/DejaVuSansMono.ttf"],
        "cullet-mono-16": ["/usr/share/fonts/noto/NotoColorEmoji.ttf"],
        "cullet-mono-24": ["/usr/share/fonts/noto/NotoColorEmoji.ttf"],
        "cullet-mono-40": ["/usr/share/fonts/noto/NotoColorEmoji.ttf"],
        "2d-1366x768-potato": ["/usr/share/fonts/TTF/DejaVuSansMono.ttf",
                               "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
                               "/usr/share/fonts/TTF/DejaVuSansMono-Oblique.ttf",
                               "/usr/share/fonts/TTF/DejaVuSansMono-BoldOblique.ttf",
                               "/usr/local/share/fonts/vtium-tier3/NotoSansCJKsc-Regular.otf",
                               "/usr/share/fonts/noto/NotoColorEmoji.ttf"],
        "heads-two-heads": ["/usr/local/share/tier3-fonts/vtium-test-8x16.psf",
                            "/usr/local/share/tier3-fonts/vtium-test-8x8.psf"],
        "sysvinit-640x480": ["/usr/local/share/tier3-fonts/vtium-test-8x16.psf"],
    }
    # Variants that draw only fonts of the fresh-install list: the default
    # set, the built-in font, or Cullet Mono's reference set.
    NONE = ["2d-potato-min", "default-font-16", "default-font-40", "builtin-font", "power-off",
            "seat-seatd", "boot-claim", "heads-open-nodisplay"]

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        tree = os.path.join(self.dir.name, "tree")
        write_tree(tree, dict(ImplementationTest.TREE, **{
            "usr/share/vtium/palettes/zenith.palette": "p",
            "usr/share/vtium/palettes/amber.palette": "p",
            # a license beside the schemes is no scheme of the folder's own
            "usr/share/vtium/palettes/licenses/amber.txt": "l",
            "usr/share/vtium/pointers.svg": "<svg/>",
            "usr/share/vtium/fonts/ter-u16n.bdf.gz": "f"}))
        self.impl = t3_impl.tree(tree, os.path.join(self.dir.name, "out", "tree.tar"))
        self.disc = os.path.join(self.dir.name, "disc")

    def batch(self, variant):
        """The batch file `variant` installs, or None."""
        v = t3_scenario.VARIANTS[variant]
        return t3_batch.batch(v, t3_heads.guest_conf(v), self.impl["tar"])

    def test_fresh_list(self):
        # The list of a fresh install as the run's tree holds it: the font
        # names of vtium-settings's FRESH_FONTS, every color scheme directly
        # in the tree's palette folder, and the pointer file.
        self.assertEqual(t3_batch.fresh_list(self.impl["tar"]), self.FRESH)

    def test_a_variant_of_the_fresh_fonts_writes_no_batch(self):
        # tier 3 keeps checking how vtium starts with no batch file.
        for variant in self.NONE:
            with self.subTest(variant):
                self.assertIsNone(self.batch(variant))

    def test_a_variant_lists_its_own_fonts_after_the_fresh_list(self):
        for variant, extra in self.EXTRA.items():
            with self.subTest(variant):
                text = self.batch(variant)
                self.assertIsNotNone(text)
                lines = [line for line in text.splitlines() if not line.startswith("#")]
                self.assertEqual(lines, self.FRESH + extra)
        # the disc carries the file guest/setup installs
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("2d-640x480-frit", self.dir.name, self.impl)
        with open(os.path.join(self.disc, "batch.conf")) as f:
            self.assertEqual(f.read(), self.batch("2d-640x480-frit"))

    def test_a_frit_the_disc_carries_counts_too(self):
        # The showcase's configuration names showcase-iosevka, and its scenes
        # switch to the other frits its disc carries with OSC 50, so their
        # fonts are in its batch: JetBrains Mono and Terminus's PSF only
        # there. cullet-alone, Cullet Mono's reference set, names only a
        # fresh-install font.
        lines = self.batch("showcase-1920x1080").splitlines()
        for font in ("JetBrainsMono-Regular.ttf", "ter-v32n.psf.gz"):
            self.assertIn("/usr/local/share/fonts/vtium-showcase/" + font, lines)
        self.assertIn("cullet-alone.vtfrit",
                      t3_scenario.VARIANTS["cullet-mono-16"]["deferred_frits"])

    def test_every_variant_is_placed(self):
        # Each variant's fonts can be placed in the guest, so none stops the
        # run, and each batch file starts with the whole fresh-install list.
        for variant in t3_scenario.VARIANTS:
            with self.subTest(variant):
                text = self.batch(variant)
                if text is not None:
                    lines = [line for line in text.splitlines() if not line.startswith("#")]
                    self.assertEqual(lines[:len(self.FRESH)], self.FRESH)
                    self.assertGreater(len(lines), len(self.FRESH))

    def test_a_font_tier_3_cannot_place_stops_the_run(self):
        # A family the guest's table lacks, a kbd font found by name, a frit
        # nobody ships and a relative path each stop the run, so a new
        # variant cannot leave its font out of the batch.
        frits = os.path.join(self.dir.name, "frits")
        cases = {
            "family.vtfrit": ("source=font:Some Family\n", "names the family Some Family"),
            "kbd.vtfrit": ("source=kbd:ter-v16n\n", "cannot place in the guest"),
            "relative.vtfrit": ("source=outline:fonts/a.ttf\n", "cannot place in the guest"),
            "layer.vtfrit": ("layer=no-such-layer\n", "no layer no-such-layer"),
        }
        for name, (text, error) in cases.items():
            write_tree(frits, {name: text})
            with self.subTest(name), mock.patch.object(t3_batch, "GUEST", self.dir.name):
                with self.assertRaisesRegex(SystemExit, error):
                    t3_batch.variant_fonts({"frits": [name]}, "")
        with self.assertRaisesRegex(SystemExit, "no frit no-such-frit"):
            t3_batch.variant_fonts({}, "font-name=no-such-frit\n")
        # a family named under the freetype engine is looked up the same way
        with self.assertRaisesRegex(SystemExit, "names the family Monoid"):
            t3_batch.variant_fonts({}, "font-engine=freetype\nfont-name = Monoid  # mine\n")
        self.assertEqual(t3_batch.variant_fonts({}, "font-engine=unifont\nfont-name=x\n"), [])

    def test_setup_installs_the_batch(self):
        # Each guest installs the disc's batch file where vtium reads it, as
        # root's, which vtium's owner and mode check trusts, and refuses a
        # guest whose console font the batch file would leave out.
        for guest in (t3_common.GUEST, t3_common.GUEST_SYSVINIT):
            with self.subTest(guest), open(os.path.join(guest, "setup")) as f:
                setup = f.read()
                self.assertIn('if [ -f "$disc"/batch.conf ]; then', setup)
                self.assertIn('install -Dm644 -o root -g root "$disc"/batch.conf '
                              '/etc/vtium/batch.conf\n', setup)
                self.assertIn("grep -qs '^[[:space:]]*FONT=' /etc/vconsole.conf", setup)
                self.assertIn("TIER3-INSTALL-FAIL the guest names a console font", setup)

class PreflightTest(unittest.TestCase):
    """The checks a run makes before it builds or boots anything, the
    goldens in the tree, and the tree a run installs."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.root = self.dir.name
        self.copied = os.path.join(self.root, "c")

    def test_the_goldens(self):
        # Every golden a case compares with is in the tree, and every one in
        # the tree is one a case compares with, in the one directory of
        # goldens.
        cases_of = t3_scenario.golden_cases()
        self.assertEqual(t3_evaluate.check_goldens(cases_of, t3_common.GOLDEN), [])
        # a screen whose golden depends on the tree's build has a golden
        # for each build (BY_FEATURE)
        builds = [()] + [[f] for f in sorted(set(t3_evaluate.BY_FEATURE.values()))]
        wanted = {t3_evaluate.golden_for(v, c, features=b)[0] for v, cases in cases_of.items()
                  for c in cases for b in builds} - {None}
        self.assertTrue(wanted)
        self.assertEqual(sorted(t3_common.rel(p) for p in wanted if not os.path.exists(p)), [])
        self.assertEqual(os.path.basename(t3_common.GOLDEN), "goldens")

    def planted(self):
        """A planted goldens directory c holding two goldens 2d-potato-min's
        cases compare with."""
        write_tree(self.root, {"c/2d-potato-min/sgr.png": "sgr",
                               "c/2d-potato-min/cjk.png": "cjk"})

    def test_preflight(self):
        # Clean on the planted goldens, judged as 2d-potato-min's two cases
        # alone; then a stray golden no case draws, and a provisional row
        # for a golden that does not exist. A golden a case compares with
        # that is missing does not refuse the run: that case is "missing".
        self.planted()
        cases = mock.patch.object(t3_scenario, "golden_cases",
                                  lambda: {"2d-potato-min": ["sgr", "cjk"]})
        with cases, mock.patch.object(t3_evaluate, "PROVISIONAL", {}):
            self.assertEqual(t3.preflight(LOG, self.copied), [])
            os.unlink(os.path.join(self.copied, "2d-potato-min", "cjk.png"))
            self.assertEqual(t3.preflight(LOG, self.copied), [])
            write_tree(self.root, {"c/2d-potato-min/stray.png": "no case"})
            golden = lambda name: t3_common.rel(os.path.join(self.copied, "2d-potato-min", name))
            with mock.patch.object(t3_evaluate, "PROVISIONAL", {"2d-potato-min/cjk": "why"}):
                self.assertEqual(sorted(t3.preflight(LOG, self.copied)),
                                 sorted(["%s is a golden no case draws" % golden("stray.png"),
                                         "provisional.tsv lists 2d-potato-min/cjk, which has "
                                         "no golden"]))

    def test_run_refuses_a_bad_tree(self):
        # run() makes those checks under the run lock and refuses before the
        # tree is built.
        self.planted()
        write_tree(self.root, {"c/2d-potato-min/stray.png": "no case"})
        args = argparse.Namespace(variants=["2d-potato-min"], tree=None, features="",
                                  tree_target="install-tree", jobs=1, deadline=1)
        base = os.path.join(self.root, "base.qcow2")
        write_tree(self.root, {"base.qcow2": ""})
        with mock.patch.dict(t3_scenario.GUESTS["arch"], base=base), \
                mock.patch.object(t3_runner, "raise_on_sigterm", lambda log: None), \
                mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                mock.patch.object(t3_common, "GOLDEN", self.copied), \
                mock.patch.object(t3, "build_tree", side_effect=AssertionError("built")):
            with self.assertRaisesRegex(SystemExit, "stray.png is a golden no case draws"):
                t3.run(args, LOG)

    class Stop(Exception):
        """Raised where the run would start its variants."""

    def run_tree(self, **given):
        """t3.run on 2d-potato-min with `given` arguments, up to the point it
        starts the variants, in a scratch target/tier3: the tree is TREE,
        make writes it, and the variants are not run. Returns the impl the
        variants would get, or raises what run raised."""
        out = os.path.join(self.root, "tier3")
        args = argparse.Namespace(variants=["2d-potato-min"], tree=None, features=None,
                                  tree_target=None, jobs=1, deadline=1)
        vars(args).update(given)
        base = os.path.join(self.root, "base.qcow2")
        write_tree(self.root, {"base.qcow2": ""})
        tree = os.path.join(out, "install")
        seen = {}

        def make(argv, cwd):
            destdir = [a for a in argv if a.startswith("DESTDIR=")][0][len("DESTDIR="):]
            features = [a for a in argv if a.startswith("FEATURES=")][0][len("FEATURES="):]
            write_tree(destdir, dict(ImplementationTest.TREE, **(
                ImplementationTest.IMAGES if "images" in features.split() else {})))
            return subprocess.CompletedProcess(argv, 0)

        def variants(names, jobs, impl, deadline, log):
            seen["impl"] = impl
            raise self.Stop

        with mock.patch.dict(t3_scenario.GUESTS["arch"], base=base), \
                mock.patch.object(t3_runner, "raise_on_sigterm", lambda log: None), \
                mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                mock.patch.object(t3, "preflight", lambda log: []), \
                mock.patch.object(t3, "TREE", tree), \
                mock.patch.object(t3_common, "TREE_RECORD", os.path.join(out, "run", "tree.json")), \
                mock.patch.object(t3_impl, "RUNDIR", os.path.join(out, "run")), \
                mock.patch.object(t3.subprocess, "run", side_effect=make), \
                mock.patch.object(t3, "build_tree", wraps=t3.build_tree) as build, \
                mock.patch.object(t3, "build_features", lambda variants, log: {}), \
                mock.patch.object(t3, "build_fonts", lambda variants, log: None), \
                mock.patch.object(t3_runner, "run_variants", side_effect=variants):
            try:
                t3.run(args, LOG)
            except self.Stop:
                pass
        self.built = [c.args[2] for c in build.call_args_list]
        return seen["impl"]

    def test_run_builds_the_tree_with_its_features(self):
        # run() hands --features to the build, and the tree the variants get
        # is described with them, so the budget does not judge it.
        impl = self.run_tree(tree_target="install-tree", features="logind,images")
        self.assertEqual(self.built, [["images", "logind"]])
        self.assertEqual(t3_impl.describe(impl)["features"], ["images", "logind"])
        self.assertIsNone(t3_impl.describe(impl)["budget"])
        # without --features the tree is the minspec tree, which it judges
        impl = self.run_tree(tree_target="install-tree")
        self.assertEqual(self.built, [[]])
        self.assertEqual(t3_impl.describe(impl)["features"], [])
        self.assertIsNotNone(t3_impl.describe(impl)["budget"])

    def test_run_on_the_last_tree_reads_its_features(self):
        # tier3-run on the last run's tree takes the features that run
        # recorded and builds nothing; a --features that differs is refused,
        # either way, and one that agrees is accepted.
        self.run_tree(tree_target="install-tree", features="images")
        for given in ({}, {"features": "images"}):
            with self.subTest(given):
                impl = self.run_tree(**given)
                self.assertEqual(self.built, [])
                self.assertEqual(t3_impl.describe(impl)["features"], ["images"])
                self.assertIsNone(t3_impl.describe(impl)["budget"])
        with self.assertRaisesRegex(SystemExit, "built with images, and FEATURES names none"):
            self.run_tree(features="")
        self.run_tree(tree_target="install-tree")
        self.assertEqual(t3_impl.describe(self.run_tree())["features"], [])
        with self.assertRaisesRegex(SystemExit, "built with no features, and FEATURES names "
                                                "images"):
            self.run_tree(features="images")
        # a tree changed since its run, or one with no record, is refused
        write_tree(os.path.join(self.root, "tier3", "install"), {"usr/share/doc/y": "new"})
        with self.assertRaisesRegex(SystemExit, "is not the one the run that built it recorded"):
            self.run_tree()
        os.unlink(os.path.join(self.root, "tier3", "run", "tree.json"))
        with self.assertRaisesRegex(SystemExit, "no record of the features the tree at"):
            self.run_tree()
        # a build that fails leaves no record of the tree it replaced
        self.run_tree(tree_target="install-tree", features="images")
        with mock.patch.object(t3.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 2)), \
                mock.patch.object(t3_common, "TREE_RECORD",
                                  os.path.join(self.root, "tier3", "run", "tree.json")):
            with self.assertRaisesRegex(SystemExit, "failed with exit code 2"):
                t3.build_tree("install-tree", os.path.join(self.root, "tier3", "install"), [],
                              LOG)
        self.assertFalse(os.path.exists(os.path.join(self.root, "tier3", "run", "tree.json")))

    def test_run_on_a_named_tree_takes_its_features_as_declared(self):
        # TREE=DIR: a tree built elsewhere carries no record; its features
        # are the ones --features declares, none by default.
        named = os.path.join(self.root, "named")
        write_tree(named, ImplementationTest.TREE)
        self.assertEqual(t3_impl.describe(self.run_tree(tree=named))["features"], [])
        write_tree(named, ImplementationTest.IMAGES)
        impl = self.run_tree(tree=named, features="images")
        self.assertEqual((self.built, t3_impl.describe(impl)["features"]), ([], ["images"]))


class VerdictTest(unittest.TestCase):
    """The statuses that pass a run."""

    def test_the_statuses_that_pass_the_run(self):
        # The run's verdict counts a near case, a provisional case and a
        # showcase capture as a pass, and nothing else: a difference from a
        # golden is a fail.
        with open(os.path.join(t3_common.HERE, "t3.py")) as f:
            self.assertIn('c["status"] in ("pass", "near", "provisional", "capture")',
                          f.read())


class VtiumGoldenTest(unittest.TestCase):
    """vtium's goldens: the provisional list, and a golden no case draws."""

    VARIANT = "2d-320x240-psf8"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.root = os.path.join(self.dir.name, "goldens")
        os.makedirs(os.path.join(self.root, self.VARIANT))

    def golden(self, screen):
        t3img.write_png(os.path.join(self.root, self.VARIANT, screen + ".png"),
                        t3img.Image(8, 8, bytes(8 * 8 * 3)))

    def tsv(self, text):
        path = os.path.join(self.dir.name, "provisional.tsv")
        with open(path, "w") as f:
            f.write("# a comment\n" + text)
        return path

    def test_shipped(self):
        # The maintainer has reviewed every golden that tier 3 has listed as
        # provisional, so the list in the tree is empty. The reviews ran from
        # 2026-11-05 to 2026-11-15:
        # - 2026-11-05: every provisional golden at that date.
        # - 2026-11-06: the four notice goldens, recaptured with the reworded
        #   notices, and the ten screens of each default 2D variant,
        #   re-baselined on vtium's captures with the font set tier3-text
        #   (vtium/vtium#275).
        # - 2026-11-08: the three sgr goldens of the minimal variants,
        #   recaptured with the italic bands of 2026-11-07 (vtium/vtium#273).
        # - 2026-11-11: card 1b's kernel-message golden.
        # - 2026-11-13: the four panic goldens with the maintainer's drawing
        #   of Tux, and the img-delete golden with kitty 0.50.0's replies.
        # - 2026-11-15: the login picture's capture and the built-in font's
        #   two captures.
        # A golden that the list names must also be one that a case draws.
        #
        # References:
        # - ADR-1302 D4: the italic bands that the sgr goldens show.
        # - ADR-1401 D7: the panic screen that the panic goldens show.
        # - ADR-1100 D15: kitty's replies that the img-delete golden follows.
        # - ADR-1100 D17: the login picture.
        # - ADR-1204 D1: the built-in font.
        prov = t3_evaluate.read_provisional()
        self.assertEqual(sorted(prov), [])
        self.assertEqual(t3_evaluate.check_goldens(t3_scenario.golden_cases(),
                                                   t3_evaluate.GOLDEN, prov), [])
        with open(os.path.join(t3_common.HERE, "goldens", "README.md")) as f:
            self.assertIn("provisional.tsv", f.read())

    def test_refused(self):
        for text, why in (("a/x\n", "want VARIANT/SCREEN and WHY"),
                          ("ax\twhy\n", "want VARIANT/SCREEN and WHY"),
                          ("a/x\t\n", "want VARIANT/SCREEN and WHY"),
                          ("a/x\twhy\na/x\tagain\n", "a/x is listed twice")):
            with self.subTest(text):
                with self.assertRaisesRegex(SystemExit, re.escape(why)):
                    t3_evaluate.read_provisional(self.tsv(text))

    def test_stray_golden(self):
        # A golden or sidecar no case compares with is reported, as is a
        # provisional row without a golden; cjk serves vt-restore too.
        for screen in ("small-console", "cjk", "gone"):
            self.golden(screen)
        with open(os.path.join(self.root, self.VARIANT, "gone.tol"), "w") as f:
            f.write("max_delta=1\n")
        os.makedirs(os.path.join(self.root, "no-such"))
        t3img.write_png(os.path.join(self.root, "no-such", "sgr.png"),
                        t3img.Image(8, 8, bytes(8 * 8 * 3)))
        with open(os.path.join(self.root, "README.md"), "w") as f:
            f.write("not a golden\n")
        cases = {self.VARIANT: t3_scenario.variant_cases(self.VARIANT)}
        errors = t3_evaluate.check_goldens(
            cases, self.root,
            {"%s/small-console" % self.VARIANT: "x", "%s/sgr" % self.VARIANT: "x"})
        rels = [t3_common.rel(os.path.join(self.root, p)) for p in
                ("%s/gone.png" % self.VARIANT, "%s/gone.tol" % self.VARIANT, "no-such/sgr.png")]
        self.assertEqual(errors, sorted(["%s is a golden no case draws" % r for r in rels] +
                                        ["provisional.tsv lists %s/sgr, which has no golden"
                                         % self.VARIANT]))

    def judged(self, case, differs, provisional):
        """The entry for `case` of the variant, its golden black and its
        capture differing in one pixel when `differs`, with `provisional`
        as the provisional list."""
        self.golden("cjk")
        data = bytearray(8 * 8 * 3)
        if differs:
            data[0] = 200
        cap = os.path.join(self.dir.name, "cap.png")
        t3img.write_png(cap, t3img.Image(8, 8, bytes(data)))
        with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")), \
                mock.patch.object(t3_evaluate, "PROVISIONAL", provisional):
            return t3_evaluate.evaluate(self.VARIANT, case, cap, {}, LOG, self.root)

    def test_provisional(self):
        listed = {"%s/cjk" % self.VARIANT: "unreviewed"}
        # A drift on a provisional golden is noted and does not fail ...
        e = self.judged("cjk", True, listed)
        self.assertEqual(e["status"], "provisional")
        self.assertIn("judged, it would be fail", e["note"])
        # ... nor does it for vt-restore, which compares with the same golden.
        self.assertEqual(self.judged("vt-restore", True, listed)["status"], "provisional")
        e = self.judged("cjk", False, listed)
        self.assertEqual((e["status"], e["note"][-16:]), ("provisional", "it would be pass"))
        # With its row removed, the golden judges.
        self.assertEqual(self.judged("cjk", True, {})["status"], "fail")
        self.assertEqual(self.judged("cjk", False, {})["status"], "pass")
        # The row holds for the golden in the default directory, GOLDEN, too.
        with mock.patch.object(t3_evaluate, "GOLDEN", self.root):
            self.golden("cjk")
            cap = os.path.join(self.dir.name, "cap.png")
            t3img.write_png(cap, t3img.Image(8, 8, bytes([200]) + bytes(8 * 8 * 3 - 1)))
            with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")), \
                    mock.patch.object(t3_evaluate, "PROVISIONAL", listed):
                e = t3_evaluate.evaluate(self.VARIANT, "cjk", cap, {}, LOG)
        self.assertEqual(e["status"], "provisional")


class ProvisionTest(unittest.TestCase):
    """Where provisioning keeps its images: a vtium cache that
    target/tier3/vm/images links to, the pristine image fetched into its
    pristine/ and the base image built into its base/, each only when the
    cache lacks it."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.vm = os.path.join(self.dir.name, "vm")
        for name, value in (("VM", self.vm), ("BASE", os.path.join(self.vm, "base.qcow2")),
                            ("BASE_SYSVINIT", os.path.join(self.vm, "base-sysvinit.qcow2"))):
            patch = mock.patch.object(t3, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def path(self, *parts):
        return os.path.join(self.dir.name, *parts)

    def test_link_images(self):
        cache = self.path("cache", "images")
        t3.link_images(cache, LOG)
        self.assertTrue(os.path.isdir(cache))
        self.assertEqual(os.readlink(os.path.join(self.vm, "images")), cache)
        other = self.path("other")
        t3.link_images(other, LOG)
        self.assertEqual(os.readlink(os.path.join(self.vm, "images")), other)
        # images kept in a directory of their own are not replaced
        os.unlink(os.path.join(self.vm, "images"))
        os.makedirs(os.path.join(self.vm, "images"))
        with self.assertRaisesRegex(SystemExit, "is a directory; move its images to"):
            t3.link_images(cache, LOG)

    def args(self, cache, guest="arch"):
        return argparse.Namespace(guest=guest, image="img.qcow2", archive="2026/10/20", url="u",
                                  key="k" if guest == "arch" else None, sha256="0" * 64,
                                  images=cache)

    def provision(self, cache, guest="arch", base="x.qcow2"):
        """Provision `guest` into `cache`: [(what, detail)] for each fetch
        and build, which write the files they would."""
        done = []
        base = os.path.join(self.vm, "images", "base", base)

        def fetch(argv, check):
            done.append(("fetch", argv[2:]))
            write_tree(argv[2], {argv[4]: "pristine"})

        def build(args, log, path):
            done.append(("build", path))
            write_tree(os.path.dirname(path), {os.path.basename(path): "base"})

        with mock.patch.object(t3, "base_image", lambda i, a, *guest: base), \
                mock.patch.object(t3, "_provision", side_effect=build), \
                mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                mock.patch.object(t3.subprocess, "run", side_effect=fetch):
            self.assertEqual(t3.provision(self.args(cache, guest), LOG), 0)
        return done

    def test_fetches_and_builds_into_the_cache(self):
        # An empty cache: the pristine image is fetched into pristine/ and
        # the base image built into base/; base.qcow2 links to it.
        cache = self.path("cache")
        pristine = os.path.join(self.vm, "images", "pristine")
        base = os.path.join(self.vm, "images", "base", "x.qcow2")
        self.assertEqual(self.provision(cache),
                         [("fetch", [pristine, "u", "img.qcow2", "0" * 64, "k"]),
                          ("build", base)])
        self.assertEqual(t3_impl.contents(cache), {"pristine/img.qcow2": 8, "base/x.qcow2": 4,
                                                   "base/x.qcow2.lock": 0})
        with open(os.path.join(self.vm, "base.qcow2")) as f:
            self.assertEqual(f.read(), "base")
        # a cache holding both: nothing is fetched or built
        self.assertEqual(self.provision(cache), [])

    def test_the_guest_without_systemd(self):
        # Its image is fetched with fetch-debian.sh, which takes no key, and
        # its base image is linked from base-sysvinit.qcow2, beside the
        # Arch guest's, which stays as it was.
        cache = self.path("cache")
        self.provision(cache)
        pristine = os.path.join(self.vm, "images", "pristine")
        done = self.provision(cache, "sysvinit", "sysvinit-y.qcow2")
        self.assertEqual(done, [("build", os.path.join(self.vm, "images", "base",
                                                       "sysvinit-y.qcow2"))])
        os.unlink(os.path.join(cache, "pristine", "img.qcow2"))
        done = self.provision(cache, "sysvinit", "sysvinit-z.qcow2")
        self.assertEqual(done[0], ("fetch", [pristine, "u", "img.qcow2", "0" * 64]))
        self.assertEqual(os.readlink(os.path.join(self.vm, "base-sysvinit.qcow2")),
                         os.path.join("images", "base", "sysvinit-z.qcow2"))
        self.assertEqual(os.readlink(os.path.join(self.vm, "base.qcow2")),
                         os.path.join("images", "base", "x.qcow2"))
        self.assertEqual(t3.guest_of("sysvinit")["fetch"], "fetch-debian.sh")
        self.assertEqual(t3.guest_of("arch")["fetch"], "fetch-image.sh")

    def test_names_outside_pristine_and_base_are_not_read(self):
        # An image under the pristine image's or the base image's name at
        # the top of the cache, such as a link an earlier harness left there,
        # is neither read nor changed: the cache's pristine/ and base/ are
        # vtium's own.
        cache = self.path("cache")
        elsewhere = self.path("elsewhere")
        write_tree(elsewhere, {"img.qcow2": "theirs", "x.qcow2": "theirs"})
        os.makedirs(cache)
        for name in ("img.qcow2", "x.qcow2"):
            os.symlink(os.path.join(elsewhere, name), os.path.join(cache, name))
        self.assertEqual([what for what, _ in self.provision(cache)], ["fetch", "build"])
        for name in ("img.qcow2", "x.qcow2"):
            self.assertEqual(os.readlink(os.path.join(cache, name)),
                             os.path.join(elsewhere, name))
        self.assertEqual(os.path.realpath(os.path.join(self.vm, "base.qcow2")),
                         os.path.realpath(os.path.join(cache, "base", "x.qcow2")))


class BaseImageTest(unittest.TestCase):
    """The provisioned base image is named by what it was built from, in the
    cache's base/, which worktrees share, so every checkout with the same
    guest files, pristine image and archive date reuses one build, and a
    change to any of them builds a new one beside the old."""

    def key(self, guest, image="img.qcow2", archive="2026/10/20"):
        saved = t3_common.GUEST
        t3_common.GUEST = guest
        try:
            return t3_common.base_image(image, archive)
        finally:
            t3_common.GUEST = saved

    def test_keyed_by_inputs(self):
        with tempfile.TemporaryDirectory() as guest:
            for name in t3_common.BASE_INPUTS:
                with open(os.path.join(guest, name), "w") as f:
                    f.write(name + "\n")
            first = self.key(guest)
            self.assertEqual(os.path.dirname(first), os.path.join(t3_common.VM, "images", "base"))
            self.assertRegex(os.path.basename(first), r"^[0-9a-f]{16}\.qcow2$")
            self.assertEqual(self.key(guest), first)
            self.assertNotEqual(self.key(guest, archive="2026/10/21"), first)
            self.assertNotEqual(self.key(guest, image="other.qcow2"), first)
            for name in t3_common.BASE_INPUTS:
                with open(os.path.join(guest, name), "a") as f:
                    f.write("changed\n")
                self.assertNotEqual(self.key(guest), first, name)
        self.assertIn("user-data.in", t3_common.BASE_INPUTS)

    def test_pinned_base(self):
        # The pinned image, archive and guest files name one base image; a
        # change to any of them provisions a new one, about 330 MiB
        # downloaded and a file of about 1.1 GiB.
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            make = dict(re.findall(r"^(T3_IMAGE_VER|T3_ARCHIVE)\s*:=\s*(\S+)", f.read(), re.M))
        image = "Arch-Linux-x86_64-cloudimg-%s.qcow2" % make["T3_IMAGE_VER"]
        self.assertEqual(os.path.basename(t3_common.base_image(image, make["T3_ARCHIVE"])),
                         "0c9a695b265eaeb9.qcow2")


class VariantTest(unittest.TestCase):
    def test_every_variant_sizes_its_machine(self):
        for name, v in t3_scenario.VARIANTS.items():
            with self.subTest(name):
                self.assertIsInstance(v["cpus"], int)
                self.assertIsInstance(v["memory_mb"], int)

    def test_potato(self):
        # The minspec machine, in the default run.
        v = t3_scenario.VARIANTS["2d-1366x768-potato"]
        self.assertEqual((v["cpus"], v["memory_mb"], v["renderer"]), (1, 512, "cpu"))
        self.assertIn("xres=1366,yres=768", v["device"])
        self.assertFalse(t3_scenario.uses_virgl("2d-1366x768-potato"))
        self.assertIn("2d-1366x768-potato", t3_scenario.DEFAULT_VARIANTS)

    def test_potato_minimal(self):
        # The potato machine with the minimal configuration, unifont and a
        # PSF font, which make tier3-minimal runs, out of the default run.
        for name, conf in (("2d-potato-min", "vtium-minimal.conf"),
                           ("2d-potato-min-psf", "vtium-minimal-psf.conf")):
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                potato = t3_scenario.VARIANTS["2d-1366x768-potato"]
                for key in ("device", "cpus", "memory_mb", "renderer"):
                    self.assertEqual(v[key], potato[key])
                self.assertEqual(v["conf"], conf)
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                # no picture to key: the image screens' keyed cases are left out
                cases = t3_scenario.variant_cases(name)
                self.assertNotIn("photo-scrollback", cases)
                self.assertNotIn("sixel-evict-zoomed", cases)
                self.assertIn("photo-scroll", cases)
                self.assertIn("vt-restore", cases)
        for name in t3_scenario.DEFAULT_VARIANTS:
            self.assertEqual(t3_scenario.variant_cases(name), t3_evaluate.CASES)
        with open(os.path.join(t3_common.GUEST, "vtium-minimal-psf.conf")) as f:
            self.assertIn("font-engine=psf\n", f.read())

    def test_small_displays(self):
        # The small displays (vtium/vtium#75) on the minspec machine: 320x240
        # in the 8x8 test font, 640x480 in the 8x16 one, each drawing the
        # small screens, judged by vtium's goldens, and in tier3-minimal.
        potato = t3_scenario.VARIANTS["2d-potato-min-psf"]
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            minimal = re.search(r"^tier3-minimal:.*\n\t(.*)", f.read(), re.M).group(1)
        for name, size, font in (("2d-320x240-psf8", "xres=320,yres=240", "vtium-test-8x8.psf"),
                                 ("2d-640x480-psf16", "xres=640,yres=480",
                                  "vtium-test-8x16.psf")):
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                for key in ("cpus", "memory_mb", "renderer", "images"):
                    self.assertEqual(v[key], potato[key])
                self.assertIn(size, v["device"])
                self.assertTrue(v["fonts"])
                self.assertIn(name, t3_scenario.golden_cases())
                self.assertEqual(t3_scenario.screens_of(name), t3_evaluate.SMALL_SCREENS)
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                self.assertIn(name, minimal)
                with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
                    conf = f.read()
                self.assertIn("font-engine=psf\n", conf)
                self.assertIn("/" + font + "\n", conf)
                cases = t3_scenario.variant_cases(name)
                self.assertEqual(cases, t3_evaluate.SMALL_SCREENS + ["vt-switch", "vt-restore"])
                # every case judged by vtium's golden, the restore by cjk's
                meta = t3_scenario.failure_meta("no boot")
                entries = t3_scenario.evaluate_variant(name, {}, meta, LOG)
                goldens = {e["name"].split("/", 1)[1]: e["golden"] for e in entries}
                self.assertEqual(goldens["small-console"],
                                 "tools/tier3/goldens/%s/small-console.png" % name)
                self.assertEqual(goldens["vt-restore"], "tools/tier3/goldens/%s/cjk.png" % name)
                self.assertIsNone(goldens["vt-switch"])
        # no font size is set: at 320x240 the default size is the font's
        # own, 8 (vtium/vtium#118)
        with open(os.path.join(t3_common.GUEST, "vtium-small-8x8.conf")) as f:
            self.assertNotIn("font-size", f.read())
        # each PSF variant's frits are in guest/frits, and each names its
        # variant's font as its first source
        for variant, conf in t3_scenario.VARIANTS.items():
            if "frits" not in conf:
                continue
            with open(os.path.join(t3_common.GUEST, conf["conf"])) as f:
                font = re.search(r"^font-name=(.*)$", f.read(), re.M).group(1)
            with open(os.path.join(t3_common.GUEST, "frits", conf["frits"][0])) as f:
                self.assertIn("source=file:%s\n" % font, f.read(), variant)
        # vt-restore compares with the last screen, cjk, as in SCREENS
        self.assertEqual(t3_evaluate.SMALL_SCREENS[-1], t3_evaluate.SCREENS[-1])
        # every case of the minimal variants is judged by its golden in
        # goldens/
        entries = t3_scenario.evaluate_variant("2d-potato-min", {}, t3_scenario.failure_meta("x"),
                                               LOG)
        goldens = {e["name"].split("/", 1)[1]: e["golden"] for e in entries}
        self.assertEqual(goldens["sgr"], "tools/tier3/goldens/2d-potato-min/sgr.png")
        self.assertEqual(goldens["boxdraw"], "tools/tier3/goldens/2d-potato-min/boxdraw.png")

    def test_resident_memory(self):
        # guest/t3's T3-RSS lines: the largest VmRSS of the kmscon
        # processes after each screen; a line with none is left out.
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("T3-LOGIN /dev/tty2\nT3-RSS sgr 5120 \nT3-DONE sgr\n"
                    "T3-RSS vt-x 5200 9000 \nT3-RSS empty \n")
        try:
            vm = type("Vm", (), {"events": f.name})
            self.assertEqual(t3_scenario.resident_kb(vm), {"sgr": 5120, "vt-x": 9000})
        finally:
            os.unlink(f.name)

    def test_reported_not_offered(self):
        # A screen found images not offered when it reported so after the
        # screen before it ended and before it ended itself; a refusal of an
        # earlier screen, or one after this screen ended, is not its own.
        events = ["T3-NO-SIXEL", "T3-DONE sixel", "T3-DONE photos", "T3-NO-KITTY",
                  "T3-RSS kitty 5000", "T3-DONE kitty", "T3-NO-SIXEL"]
        self.assertEqual(t3_scenario.reported_not_offered(events, "sixel"), "T3-NO-SIXEL")
        self.assertIsNone(t3_scenario.reported_not_offered(events, "photos"))
        self.assertEqual(t3_scenario.reported_not_offered(events, "kitty"), "T3-NO-KITTY")
        self.assertIsNone(t3_scenario.reported_not_offered(events, "flood"))

    def test_image_benchmarks_without_images(self):
        # A variant without images measures no picture stream: each image
        # benchmark screen must have found images not offered since the
        # screen before it ended, and must report no window.
        def figures(events, images=False):
            with tempfile.TemporaryDirectory() as d:
                vm = type("Vm", (), {"serial": os.path.join(d, "serial"),
                                     "events": os.path.join(d, "events")})
                with open(vm.serial, "w") as f:
                    f.write("")
                with open(vm.events, "w") as f:
                    f.write(events)
                caps = {"mpv-sixel": "x", "tmux-2x2": "x"}
                return t3_scenario.bench_figures(vm, caps, "", images)
        got = figures("T3-NO-SIXEL\nT3-RSS mpv-sixel 5000 \nT3-DONE mpv-sixel\n"
                      "T3-DONE tmux-2x2\n")
        self.assertEqual(got["mpv-sixel"], {"not_offered": "T3-NO-SIXEL"})
        self.assertIn("not offered", got["tmux-2x2"]["error"])
        got = figures("T3-NO-SIXEL\nT3-BENCH mpv-sixel 1.0 2.0\nT3-DONE mpv-sixel\n")
        self.assertIn("measured a picture stream", got["mpv-sixel"]["error"])
        # a variant with images measures, as before
        got = figures("T3-DONE mpv-sixel\n", images=True)
        self.assertIn("no benchmark window", got["mpv-sixel"]["error"])
        # and an image screen in it that found images not offered fails
        # with the refusal; a refusal of the screen before it is not its own
        got = figures("T3-NO-SIXEL\nT3-DONE mpv-sixel\nT3-DONE tmux-2x2\n", images=True)
        self.assertIn("found images not offered (T3-NO-SIXEL)", got["mpv-sixel"]["error"])
        self.assertIn("no benchmark window", got["tmux-2x2"]["error"])

    def test_startup(self):
        # from the console's revision line to its display line, same process
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] kmscon[459]: log: kmscon Revision v10\n"
                    "[    6.700000] kmscon[460]: terminal: Display a with backend [x]\n"
                    "[    6.739898] kmscon[459]: terminal: Display pci-0000:00:01.0[Virtual-1] "
                    "with backend [drm2d] text renderer [bbulk]\n")
        try:
            self.assertEqual(t3_scenario.console_startup_ms(f.name), 76.7)
        finally:
            os.unlink(f.name)
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] kmscon[459]: log: kmscon Revision v10\n")
        try:
            self.assertIsNone(t3_scenario.console_startup_ms(f.name))
        finally:
            os.unlink(f.name)

    def test_gl_draws_images(self):
        # The GPU path draws image placements, so the GL variant runs every
        # case, the image screens and their keyed cases included, as the 2D
        # variants do; the no-images check it ran while the earlier
        # gltex renderer drew none is gone with its flag. 2d-1280x800's goldens judge
        # it, so it is in the default run.
        self.assertIn("gl-1280x800", t3_scenario.DEFAULT_VARIANTS)
        self.assertNotIn("draws_images", t3_scenario.VARIANTS["gl-1280x800"])
        self.assertNotIn("no-images", t3_evaluate.CASES)
        for name in t3_evaluate.IMAGE_SCREENS:
            self.assertIn(name, t3_evaluate.CASES)
        for screen, case, _ in t3_evaluate.KEYED:
            self.assertIn(case, t3_evaluate.CASES if screen in t3_evaluate.SCREENS
                          else t3_scenario.variant_cases("images-640x480"))

    def test_failed_variant_fails_every_case(self):
        # A variant that could not run fails every case, the image ones too.
        meta = t3_scenario.failure_meta("no boot")
        for variant in ("gl-1280x800", "2d-1280x800"):
            cases = {c["name"].split("/", 1)[1]: c
                     for c in t3_scenario.evaluate_variant(variant, {}, meta, LOG)}
            self.assertEqual(list(cases), t3_evaluate.CASES)
            for name, c in cases.items():
                with self.subTest(variant=variant, case=name):
                    self.assertEqual(c["status"], "fail")

    def test_image_screens(self):
        # A screen that sends a picture sources sixel-common.inc or
        # kitty-common.inc, directly or through acceptance.inc and
        # tmux-grid.inc, or writes a sixel DCS itself (sixel-bound); each is
        # listed.
        screens = os.path.join(t3_evaluate.GUEST, "screens")

        def sends_images(path, seen=()):
            with open(path) as f:
                text = f.read()
            if re.search(r"(\$esc'|\\033|\\e)P[\d;]*q", text):
                return True
            for inc in re.findall(r"([\w-]+\.inc)", text):
                if inc in ("sixel-common.inc", "kitty-common.inc"):
                    return True
                if inc not in seen and sends_images(os.path.join(screens, inc), seen + (inc,)):
                    return True
            return False

        for name in t3_evaluate.SCREENS:
            with self.subTest(name):
                self.assertEqual(sends_images(os.path.join(screens, name + ".sh")),
                                 name in t3_evaluate.IMAGE_SCREENS)

    def test_qemu_gets_the_machine(self):
        class Started(Exception):
            pass

        def popen(args, **kwargs):
            raise Started(args)

        saved = t3_qemu.subprocess.Popen
        t3_qemu.subprocess.Popen = popen
        try:
            with tempfile.TemporaryDirectory() as d:
                with self.assertRaises(Started) as cm:
                    t3_qemu.Qemu(d, [], "virtio-vga", lambda msg: None, cpus=1, memory_mb=512)
                args = cm.exception.args[0]
                self.assertEqual(args[args.index("-smp") + 1], "1")
                self.assertEqual(args[args.index("-m") + 1], "512")
                with self.assertRaises(Started) as cm:
                    t3_qemu.Qemu(d, [], "virtio-vga", lambda msg: None)
                args = cm.exception.args[0]
                self.assertEqual(args[args.index("-smp") + 1], "4")
                self.assertEqual(args[args.index("-m") + 1], "4096")
        finally:
            t3_qemu.subprocess.Popen = saved

    def test_qemu_keeps_its_sockets_out_of_a_deep_checkout(self):
        # A checkout with a long path, run as a variant with a second head
        # and the agent's port, gets every unix socket in a directory of its
        # own with a short path, under XDG_RUNTIME_DIR when the session has
        # one and under the system's temporary directory when it has none.
        # The directory is this user's alone, and it is gone once a start
        # fails. The files QEMU writes stay in the run directory.
        class Started(Exception):
            pass

        seen = []

        def popen(args, **kwargs):
            sockets = os.path.dirname(args[args.index("-qmp") + 1][len("unix:"):].split(",")[0])
            st = os.stat(sockets)
            seen.append((args, sockets, st.st_mode & 0o777, st.st_uid))
            raise Started()

        with mock.patch.object(t3_qemu.subprocess, "Popen", popen), \
                tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as runtime:
            deep = os.path.join(d, *(["a-checkout-path-far-down-a-tree"] * 6), "run", "heads")
            self.assertGreater(len(os.fsencode(deep)), t3_qemu.SUN_PATH_MAX)
            for env, parent in (({"XDG_RUNTIME_DIR": runtime}, runtime),
                                ({}, tempfile.gettempdir())):
                with self.subTest(env=env), mock.patch.dict(os.environ, env), \
                        self.assertRaises(Started):
                    if not env:
                        os.environ.pop("XDG_RUNTIME_DIR", None)
                    t3_qemu.Qemu(deep, [], "virtio-vga,id=video0,max_outputs=2",
                                 lambda msg: None, capture="vnc", agent=True,
                                 heads=[("video0", 1, (640, 480))])
                args, sockets, mode, uid = seen.pop()
                self.assertEqual(os.path.dirname(sockets), parent)
                self.assertTrue(os.path.basename(sockets).startswith(t3_qemu.SOCKET_DIR_PREFIX))
                self.assertEqual((mode, uid), (0o700, os.getuid()))
                self.assertFalse(os.path.exists(sockets))
                paths = [a[len("unix:"):].split(",")[0] for a in args if a.startswith("unix:")]
                self.assertEqual(sorted(os.path.basename(p) for p in paths),
                                 ["qmp.sock", "ttyS2", "vnc.sock", "vnc1.sock"])
                for path in paths:
                    self.assertEqual(os.path.dirname(path), sockets)
                    self.assertLessEqual(len(os.fsencode(path)), t3_qemu.SUN_PATH_MAX)
                self.assertIn("file:%s" % os.path.join(deep, "serial.log"), args)

    def test_a_left_qemu_is_killed_and_its_sockets_removed(self):
        # A QEMU its worker left running is the one that writes the
        # variant's serial log: the runner kills it and removes its socket
        # directory, and leaves alone a process that does not write that
        # log, socket directory included.
        def stand_in(serial, sockets):
            return subprocess.Popen(
                [sys.executable, "-c", "import time; time.sleep(600)",
                 "-qmp", "unix:%s,server=on,wait=off" % os.path.join(sockets, "qmp.sock"),
                 "-serial", "file:%s" % serial])

        job = t3_runner.Job.__new__(t3_runner.Job)
        job.variant, job.log, job.result = "deep", lambda msg: None, None
        job.proc = mock.Mock()
        job.conn, theirs = multiprocessing.Pipe(duplex=False)
        theirs.close()
        mine = os.path.join(t3_runner.RUNDIR, "deep", "serial.log")
        other = os.path.join(t3_runner.RUNDIR, "other", "serial.log")
        left, kept = t3_qemu.socket_dir(), t3_qemu.socket_dir()
        procs = [stand_in(mine, left), stand_in(other, kept)]
        try:
            job.qemu = {p.pid for p in procs}
            job.finish()
            self.assertEqual(procs[0].wait(30), -9)
            self.assertFalse(os.path.exists(left))
            self.assertIsNone(procs[1].poll())
            self.assertTrue(os.path.isdir(kept))
        finally:
            for p in procs:
                p.kill()
                p.wait()
            shutil.rmtree(left, ignore_errors=True)
            shutil.rmtree(kept, ignore_errors=True)


# The console after key-reboot on the guest without systemd, as sysvinit
# and its scripts left it.
REBOOT_SYSVINIT = ["[ 9.1] vtium[404]: key-reboot: asking init to reboot",
                   "INIT: Switching to runlevel: 6",
                   "Will now restart.",
                   "[ 12.0] reboot: Restarting system"]


class PowerTest(unittest.TestCase):
    """The power variants (t3_power.py): their machine, their cases and the
    verdict each case takes from the console log and QEMU's SHUTDOWN
    event."""

    # A console log as each action leaves it, systemd's status lines
    # colored as on the guest's serial console.
    OK = "[\x1b[0;32m  OK  \x1b[0m] "
    POWER_OFF = ["[ 9.1] vtium[404]: key-power-off: asking init to power off",
                 OK + "Reached target Unmount All Filesystems.",
                 OK + "Reached target System Power Off.",
                 "[ 12.0] reboot: Power down"]
    LOGS = {
        "reboot": ["[ 9.1] vtium[404]: key-reboot: asking init to reboot",
                   OK + "Stopped target Graphical Interface.",
                   OK + "Reached target Unmount All Filesystems.",
                   OK + "Reached target System Reboot.",
                   "[ 12.0] reboot: Restarting system"],
        "reboot-hard": ["[ 9.1] vtium[404]: key-reboot: restarting the kernel at once",
                        "[ 9.2] reboot: Restarting system"],
        "power-off": POWER_OFF,
        "power-off-after-s3": POWER_OFF,
        "suspend": ["[ 9.1] vtium[404]: key-suspend: suspending",
                    OK + "Reached target Sleep.",
                    "[ 9.1] vtium[404]: key-suspend: logind took the request",
                    "         Starting System Suspend...",
                    OK + "Finished System Suspend."],
        "suspend-sysfs": ["[ 9.1] vtium[404]: key-suspend: suspending",
                          "[ 9.1] vtium[404]: key-suspend: writing mem to /sys/power/state"],
        "reboot-sysvinit": REBOOT_SYSVINIT,
        "power-off-sysvinit": ["[ 9.1] vtium[404]: key-power-off: asking init to power off",
                               "INIT: Switching to runlevel: 0",
                               "Will now halt.",
                               "[ 9.9] reboot: Power down"],
        # The kernel's trace of the signal can reach the console before
        # vtium's line, which t3-console stamps and forwards.
        "power-off-busybox": ["           vtium-404     [000] d..2.     9.100500: "
                              "signal_generate: sig=38 errno=0 code=0 comm=busybox pid=1 "
                              "grp=1 res=1",
                              "[ 9.1] vtium[404]: key-power-off: asking init to power off"],
    }
    # vtium's warning at its start where PID 1 neither catches nor blocks
    # SIGRTMIN+4.
    WARNING = ("[ 3.2] vtium[404]: config: the key-power-off key will not work: the init "
               "system (PID 1) neither handles nor blocks SIGRTMIN+4 (signal 38, from SigCgt "
               "and SigBlk in /proc/1/status). Unbind the key, or use an init system that "
               "powers off on that signal, such as systemd or sysvinit.")
    # The console before the key, where a case needs lines there: on the
    # busybox boot, vtium's warning at its start.
    BEFORE = {"power-off-busybox": [WARNING]}

    # What a power-off after S3 ends with in QEMU: systemd-shutdown exits
    # when reboot(2) returns, ten seconds after the kernel's last line.
    PANIC = ["[ 22.0] Kernel panic - not syncing: Attempted to kill init! exitcode=0x00000000"]
    SHUTDOWN = {"reboot": "guest-reset", "reboot-hard": "guest-reset",
                "power-off": "guest-shutdown", "power-off-after-s3": "guest-shutdown",
                "suspend": None, "suspend-sysfs": None,
                "reboot-sysvinit": "guest-reset", "power-off-sysvinit": "guest-shutdown",
                "power-off-busybox": None}

    def before(self, case):
        """The console before the key in `case`'s log: BEFORE's lines."""
        lines = self.BEFORE.get(case, [])
        return ("\r\n".join(lines) + "\r\n").encode() if lines else b""

    def log(self, case, drop=None, extra=()):
        lines = [l for l in self.LOGS[case] if drop is None or drop not in l]
        return self.before(case) + ("\r\n".join(lines + list(extra)) + "\r\n").encode()

    def judge(self, case, serial, since=None):
        """t3_power.judge, with the key pressed after BEFORE's lines unless
        `since` says where."""
        since = len(self.before(case)) if since is None else since
        return t3_power.judge(case, serial, since, self.SHUTDOWN[case])

    def test_variants(self):
        # The minspec machine, out of the default run, each with its own
        # cases, all checks without a golden.
        potato = t3_scenario.VARIANTS["2d-potato-min"]
        seen = []
        for name in t3_scenario.POWER_VARIANTS + ["sysvinit-reboot", "sysvinit-restart",
                                                  "sysvinit-power-off", "busybox-power-off"]:
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                for key in ("device", "cpus", "memory_mb", "renderer"):
                    self.assertEqual(v[key], potato[key])
                self.assertEqual(v["conf"], "vtium-power.conf")
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                cases = t3_scenario.variant_cases(name)
                self.assertEqual(cases, v["power"])
                for case in cases:
                    self.assertIn(case, t3_power.CHORDS)
                    self.assertEqual(t3_evaluate.golden_for(name, case), (None, None))
                    self.assertEqual(t3_evaluate.describe(case),
                                     (t3_power.CHECKS[case], None))
                # Only the last case may end the VM.
                for case in cases[:-1]:
                    self.assertIn(case, t3_power.SUSPENDS)
                if set(cases) & set(t3_power.SUSPENDS):
                    self.assertIn("ICH9-LPC.disable_s3=0", v["qemu"])
                seen += cases
        self.assertEqual(sorted(set(seen)), sorted(t3_power.CHECKS))
        # The cases of the guest without systemd run on it alone, and the
        # hard reboot on both guests.
        for name in t3_scenario.POWER_VARIANTS:
            self.assertNotIn("guest", t3_scenario.VARIANTS[name])
            for case in ("reboot-sysvinit", "power-off-sysvinit"):
                self.assertNotIn(case, t3_scenario.VARIANTS[name]["power"])
        self.assertEqual(t3_scenario.VARIANTS["sysvinit-reboot"]["power"], ["reboot-sysvinit"])
        self.assertEqual(t3_scenario.VARIANTS["sysvinit-power-off"]["power"],
                         ["power-off-sysvinit"])
        self.assertEqual(t3_scenario.VARIANTS["sysvinit-restart"]["sysctl"],
                         t3_scenario.VARIANTS["power-reboot-hard"]["sysctl"])
        # power-off runs on a guest that has not slept.
        self.assertEqual(t3_scenario.VARIANTS["power-off"]["power"], ["power-off"])
        # Suspend through logind runs vtium built with it; the minspec
        # build suspends through sysfs. Each is followed by a power-off.
        self.assertEqual(t3_scenario.VARIANTS["power-suspend-logind"]["features"], "logind")
        self.assertNotIn("features", t3_scenario.VARIANTS["power-suspend-off"])
        for name in ("power-suspend-off", "power-suspend-logind"):
            self.assertEqual(t3_scenario.VARIANTS[name]["power"][-1], "power-off-after-s3")

    def test_chords_match_the_configuration(self):
        # Each qcode chord the driver presses is the chord vtium-power.conf
        # binds for its key.
        keys = {"reboot": "key-reboot", "reboot-hard": "key-reboot",
                "power-off": "key-power-off", "power-off-after-s3": "key-power-off",
                "suspend": "key-suspend", "suspend-sysfs": "key-suspend",
                "reboot-sysvinit": "key-reboot",
                "power-off-sysvinit": "key-power-off",
                "power-off-busybox": "key-power-off"}
        with open(os.path.join(t3_common.GUEST, "vtium-power.conf")) as f:
            conf = dict(line.strip().split("=", 1) for line in f if line.startswith("key-"))
        for case, chord in t3_power.CHORDS.items():
            with self.subTest(case):
                mods = re.findall(r"<(\w+)>", conf[keys[case]])
                key = conf[keys[case]].rsplit(">", 1)[1]
                self.assertEqual(chord, tuple(m.lower() for m in mods) + (key.lower(),))

    def test_passes(self):
        for case in t3_power.CHECKS:
            with self.subTest(case):
                self.assertIsNone(self.judge(case, self.log(case)))

    def test_each_required_line(self):
        for case, want in t3_power.EXPECT.items():
            for marker in want["require"] + want.get("anywhere", []):
                with self.subTest(case=case, missing=marker):
                    why = self.judge(case, self.log(case, drop=marker))
                    self.assertIn(repr(marker), why or "")

    def test_a_kernel_panic_fails_every_case(self):
        # The review's run: power-off "passed" on a guest that panicked
        # once its ACPI power-off did not take.
        for case in t3_power.CHECKS:
            with self.subTest(case):
                why = self.judge(case, self.log(case, extra=self.PANIC))
                self.assertIn("'Kernel panic' on the console", why or "")

    def test_qemus_shutdown_reason(self):
        # A power-off is QEMU's guest-shutdown and a reboot its
        # guest-reset; a guest still running, or ended another way, fails.
        for case, wrong in (("power-off", "guest-reset"), ("power-off", "guest-panic"),
                            ("power-off", "none: still running"), ("power-off", None),
                            ("reboot", "guest-shutdown"), ("reboot-hard", None)):
            with self.subTest(case=case, shutdown=wrong):
                why = t3_power.judge(case, self.log(case), 0, wrong)
                self.assertIn("SHUTDOWN reason", why or "")

    def test_order(self):
        # The kernel restarting before systemd reached its target is not a
        # clean reboot.
        lines = self.LOGS["reboot"]
        serial = ("\n".join(lines[:1] + lines[-1:] + lines[1:-1]) + "\n").encode()
        self.assertIn("'reboot: Restarting system'", self.judge("reboot", serial) or "")

    def test_the_warning_before_the_key(self):
        # Under systemd, vtium's warning at its start fails the power-off:
        # the warning is for a host where the key powers nothing off.
        for case in ("power-off", "power-off-after-s3"):
            with self.subTest(case):
                serial = (self.WARNING + "\r\n").encode()
                why = self.judge(case, serial + self.log(case), since=len(serial))
                self.assertIn("%r on the console before the key"
                              % t3_power.POWER_OFF_WARNING, why or "")

    def test_no_known_failure_on_the_sysvinit_guest(self):
        # sysvinit catches SIGRTMIN+4 with a handler, so vtium gives no
        # warning and power-off-sysvinit passes with no note; a warning
        # fails it.
        case = "power-off-sysvinit"
        self.assertNotIn(case, t3_power.KNOWN)
        self.assertEqual(t3_power.verdict(case, self.log(case), 0, "guest-shutdown"),
                         (None, None))
        warned = (self.WARNING + "\r\n").encode()
        why, _ = t3_power.verdict(case, warned + self.log(case), len(warned), "guest-shutdown")
        self.assertIn("on the console before the key", why or "")

    # The kernel's trace of a signal sent to PID 1, as tracefs's
    # signal_generate event prints it: res=1 when the kernel drops the
    # signal, res=0 when it delivers it.
    TRACE = ("           vtium-404     [000] d..2.     9.100500: signal_generate: sig=%d "
             "errno=0 code=0 comm=busybox pid=%d grp=1 res=%d")

    def test_busybox_needs_the_warning_before_the_key(self):
        # busybox init neither catches nor blocks SIGRTMIN+4, so vtium
        # warns at its start that the key will not work. No warning, or
        # the warning that vtium cannot tell, fails the case.
        case = "power-off-busybox"
        unwarned = self.log(case)[len(self.before(case)):]
        why = self.judge(case, unwarned, since=0)
        self.assertIn("no 'the key-power-off key will not work' on the console before the key",
                      why or "")
        unsure = ("[ 3.2] vtium[404]: config: the key-power-off key may not work: vtium cannot "
                  "tell whether the init system (PID 1) handles SIGRTMIN+4 (signal 38), "
                  "because /proc/1/status could not be read: denied\r\n").encode()
        why = self.judge(case, unsure + unwarned, since=len(unsure))
        self.assertIn("will not work' on the console before the key", why or "")
        # A warning after the key is not the warning at vtium's start.
        why = self.judge(case, self.log(case), since=0)
        self.assertIn("before the key", why or "")

    def test_busybox_needs_the_kernel_to_drop_the_signal(self):
        # The kernel's trace after the key shows PID 1 sent the signal the
        # warning names, and dropped: res=1.
        case = "power-off-busybox"
        self.assertIsNone(self.judge(case, self.log(case)))
        for name, lines, want in (
                ("no trace", [], "no trace of signal 38 sent to PID 1 after the key"),
                ("another signal", [self.TRACE % (12, 1, 0)],
                 "no trace of signal 38 sent to PID 1 after the key"),
                ("another process", [self.TRACE % (38, 404, 1)],
                 "no trace of signal 38 sent to PID 1 after the key"),
                ("delivered", [self.TRACE % (38, 1, 0)],
                 "the kernel delivered signal 38 to PID 1 (res=0), so PID 1 caught or "
                 "blocked it")):
            with self.subTest(name):
                serial = self.log(case, drop="signal_generate", extra=lines)
                self.assertEqual(self.judge(case, serial), want)
        # A trace from before the key does not count.
        trace = (self.TRACE % (38, 1, 1) + "\r\n").encode()
        serial = self.before(case) + trace + self.log(case, drop="signal_generate")[
            len(self.before(case)):]
        why = self.judge(case, serial, since=len(self.before(case)) + len(trace))
        self.assertEqual(why, "no trace of signal 38 sent to PID 1 after the key")

    def test_busybox_reads_the_signal_from_the_warning(self):
        # The signal is the number vtium's warning names, SIGRTMIN+4 as the
        # guest's libc numbers it, not a number the judge assumes.
        case = "power-off-busybox"
        warned = (self.WARNING.replace("(signal 38,", "(signal 40,") + "\r\n").encode()
        after = self.log(case)[len(self.before(case)):]
        self.assertEqual(t3_power.judge(case, warned + after, len(warned), None),
                         "no trace of signal 40 sent to PID 1 after the key")
        after = self.log(case, drop="signal_generate", extra=[self.TRACE % (40, 1, 1)])[
            len(self.before(case)):]
        self.assertIsNone(t3_power.judge(case, warned + after, len(warned), None))

    def test_busybox_fails_a_guest_that_went_down(self):
        # The guest stays up: busybox init's shutdown, the kernel's last
        # line, or QEMU's SHUTDOWN event each fail the case.
        case = "power-off-busybox"
        for extra in ("The system is going down NOW!", "Requesting system poweroff",
                      "[ 12.1] reboot: Power down", "[ 12.1] reboot: Restarting system"):
            with self.subTest(extra):
                why = self.judge(case, self.log(case, extra=[extra]))
                self.assertIn("on the console after the key", why or "")
        for shutdown in ("guest-shutdown", "guest-reset", "the guest is paused"):
            with self.subTest(shutdown):
                why = t3_power.judge(case, self.log(case), len(self.before(case)), shutdown)
                self.assertEqual(why, "QEMU's SHUTDOWN reason is %r, not None" % shutdown)
        why = self.judge(case, self.log(case, extra=self.PANIC))
        self.assertIn("'Kernel panic' on the console", why or "")
        # The tier 3 run with a vtium that sent SIGUSR2: busybox init shut
        # the guest down before t3-console forwarded vtium's line, and the
        # shutdown is the reason given.
        serial = self.log(case, drop="key-power-off: asking init",
                          extra=["The system is going down NOW!", "Requesting system poweroff",
                                 "[ 7.785749] reboot: Power down"])
        self.assertEqual(t3_power.judge(case, serial, len(self.before(case)), "guest-shutdown"),
                         "'The system is going down' on the console after the key")

    def test_busybox_case_stays_up(self):
        # The driver waits for the guest's own lines after the key, then
        # logs in, where every other case that does not suspend waits for
        # the VM to end; the disc has the kernel trace the signals to PID 1.
        self.assertEqual(t3_power.STAYS_UP, {
            "power-off-busybox": ["key-power-off: asking init to power off",
                                  "signal_generate: "]})
        for case in t3_power.CHECKS:
            with self.subTest(case):
                self.assertEqual(case in t3_power.STAYS_UP,
                                 t3_power.EXPECT[case]["shutdown"] is None
                                 and case not in t3_power.SUSPENDS)

    def test_forbidden_lines(self):
        for case, extra in (("reboot-hard", OK_TARGET_REBOOT),
                            ("reboot-hard", "INIT: Switching to runlevel: 6"),
                            ("reboot-sysvinit", OK_TARGET_REBOOT),
                            ("reboot-sysvinit", "[ 12.1] reboot: Power down"),
                            ("power-off-sysvinit", "[ 12.1] reboot: Restarting system"),
                            ("reboot", "[ 12.1] reboot: Power down"),
                            ("power-off", "[ 12.1] reboot: Restarting system"),
                            ("suspend", "[ 12.1] reboot: Restarting system"),
                            ("suspend", "[ 9.1] key-suspend: writing mem to /sys/power/state"),
                            ("suspend-sysfs", PowerTest.OK + "Reached target Sleep.")):
            with self.subTest(case):
                why = self.judge(case, self.log(case, extra=[extra]))
                self.assertIn("on the console after the key", why or "")

    def test_only_after_the_key(self):
        # Lines from before the key do not count: a boot's own lines never
        # pass a case, and an earlier line never fails one.
        serial = self.log("reboot")
        self.assertIsNotNone(self.judge("reboot", serial, since=len(serial)))
        earlier = (OK_TARGET_REBOOT + "\r\n").encode()
        self.assertIsNone(self.judge("reboot-hard", earlier + self.log("reboot-hard"),
                                     since=len(earlier)))

    def test_a_known_failure_passes_while_it_shows(self):
        case = "power-off-after-s3"
        # The known failure: systemd shut down, the machine did not power
        # off, the kernel panicked, and QEMU went on running.
        why, note = t3_power.verdict(case, self.log(case, extra=self.PANIC), 0,
                                     "none: still running")
        self.assertIsNone(why)
        self.assertIn("known: after an S3 resume", note)
        # Gone: the entry goes with the fix.
        why, note = t3_power.verdict(case, self.log(case), 0, "guest-shutdown")
        self.assertIn("no longer fails as KNOWN says", why)
        # Failing another way: vtium's line missing.
        serial = self.log(case, drop="key-power-off", extra=self.PANIC)
        why, note = t3_power.verdict(case, serial, 0, "none: still running")
        self.assertIn("not as KNOWN says", why)
        # Only a KNOWN case passes so.
        why, note = t3_power.verdict("power-off", self.log("power-off", extra=self.PANIC), 0,
                                     "none: still running")
        self.assertIsNotNone(why)

    def test_failed_variant_fails_every_case(self):
        meta = t3_scenario.failure_meta("no boot")
        cases = t3_scenario.evaluate_variant("power-suspend-off", {}, meta, LOG)
        self.assertEqual([c["name"] for c in cases],
                         ["power-suspend-off/suspend-sysfs",
                          "power-suspend-off/power-off-after-s3"])
        self.assertEqual({c["status"] for c in cases}, {"fail"})


OK_TARGET_REBOOT = PowerTest.OK + "Reached target System Reboot."


def timer_line(t, flips, frames, interval_ms=1000.0, cpu=(1.0, 1.0, 2.0, 3.0), damage=(500, 1.5),
               pid=505, before="", skipped=(0, 0.0)):
    return ("%s[%12.6f] kmscon[%d]: terminal: render-timer: display=pci-0000:00:01.0[Virtual-1] "
            "interval_ms=%.3f presented_per_s=%.2f flips=%d frames=%d cpu_ms_mean=%.3f "
            "cpu_ms_median=%.3f cpu_ms_p95=%.3f cpu_ms_max=%.3f damage_px_mean=%d "
            "damage_pct=%.2f skipped=%d skipped_cpu_ms=%.3f"
            % (before, t, pid, interval_ms, flips * 1000 / interval_ms, flips, frames, cpu[0],
               cpu[1], cpu[2], cpu[3], damage[0], damage[1], skipped[0], skipped[1]))


class SysvinitTest(unittest.TestCase):
    """The guest without systemd (guest-sysvinit/): its variants, its disc,
    its account of PID 1 and udev, and its base image."""

    LINE = ("[  12.3] TIER3-INSTALL-OK packages: ; kernel: 6.12.48+deb13-amd64; "
            "init: init /usr/sbin/init; systemd: no; udev: %s; tree: %s")

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def test_variants(self):
        # The minspec machine, out of the default and minimal runs, in make
        # tier3-nosystemd's; the drawing ones at 640x480 in the 8x16 test
        # font, judged by vtium's goldens, the second without udev.
        small = t3_scenario.VARIANTS["2d-640x480-psf16"]
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            make = f.read()
        minimal = re.search(r"^tier3-minimal:.*\n\t(.*)", make, re.M).group(1)
        nosystemd = re.search(r"^tier3-nosystemd:.*\n(?:\t.*\n)*?\t.*--tree-target install-tree "
                              r"(.*)", make, re.M).group(1)
        self.assertEqual(nosystemd, "$(or $(VARIANTS),%s)" % " ".join(
            t3_scenario.SYSVINIT_VARIANTS))
        sysvinit = [n for n, v in t3_scenario.VARIANTS.items() if v.get("guest") == "sysvinit"]
        self.assertEqual(sorted(sysvinit), sorted(t3_scenario.SYSVINIT_VARIANTS))
        for name in sysvinit:
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                self.assertEqual(v["guest"], "sysvinit")
                self.assertEqual((v["cpus"], v["memory_mb"]), (1, 512))
                self.assertEqual(t3_scenario.base_of(name),
                                 (t3_common.BASE_SYSVINIT, "tier3-image-nosystemd"))
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                self.assertNotIn(name, minimal)
        for name in ("sysvinit-640x480", "sysvinit-no-udev"):
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                for key in ("device", "conf", "renderer", "images", "fonts"):
                    self.assertEqual(v[key], small[key])
                self.assertEqual(t3_scenario.variant_cases(name),
                                 ["sgr", "cjk", "vt-switch", "vt-restore"])
        self.assertTrue(t3_scenario.VARIANTS["sysvinit-640x480"].get("udev", True))
        self.assertFalse(t3_scenario.VARIANTS["sysvinit-no-udev"]["udev"])
        two = t3_scenario.VARIANTS["sysvinit-two-vts"]
        self.assertTrue(two["two_vts"])
        self.assertEqual(t3_scenario.variant_cases("sysvinit-two-vts"),
                         ["cjk", "vt-switch", "vt-restore"])
        for name in ("sysvinit-640x480", "sysvinit-no-udev"):
            self.assertNotIn("two_vts", t3_scenario.VARIANTS[name])
        self.assertEqual(t3_scenario.base_of("2d-potato-min"), (t3_common.BASE, "tier3-image"))
        self.assertEqual(t3_scenario.guest_of("sysvinit-reboot")["boot"],
                         ["vt: using tty /dev/tty2"])
        # What shows the switch to tty2 took: a line no boot logs. Under
        # systemd logind spawns the console at the switch; under sysvinit
        # the console logged its VT at the boot and sets its display up
        # only once the VT is shown.
        self.assertEqual(t3_scenario.GUESTS["arch"]["shown"], "vt: using tty /dev/tty2")
        self.assertEqual(t3_scenario.GUESTS["sysvinit"]["shown"], "text renderer [")
        for guest in t3_scenario.GUESTS.values():
            self.assertFalse(any(guest["shown"] in m for m in guest["boot"]))

    def test_disc(self):
        # The guest's own setup and t3-console, which its inittab runs, the
        # frit of the test font the drawing variants draw with the batch
        # file that activates the font, and the no-udev file only for the
        # variant without udev.
        tree = os.path.join(self.dir.name, "tree")
        write_tree(tree, ImplementationTest.TREE)
        impl = t3_impl.tree(tree, os.path.join(self.dir.name, "out", "tree.tar"))
        fonts = os.path.join(self.dir.name, "fonts")
        write_tree(fonts, {"vtium-test-8x16.psf": "font"})
        impl["fonts"] = fonts
        disc = os.path.join(self.dir.name, "disc")
        for variant, extra in (("sysvinit-640x480", ["fonts", "frits", "batch.conf"]),
                               ("sysvinit-no-udev", ["fonts", "frits", "no-udev", "batch.conf"]),
                               ("sysvinit-two-vts", ["fonts", "frits", "two-vts", "batch.conf"]),
                               ("sysvinit-reboot", [])):
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(variant, self.dir.name, impl)
                self.assertEqual(sorted(os.listdir(disc)),
                                 sorted(["vtium.conf", "screens", "setup", "t3", "t3-console",
                                         "tree.tar"] + extra))
                for name in ("setup", "t3-console"):
                    with open(os.path.join(disc, name)) as f, \
                            open(os.path.join(t3_common.GUEST_SYSVINIT, name)) as g:
                        self.assertEqual(f.read(), g.read(), name)
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("2d-potato-min", self.dir.name, impl)
        self.assertNotIn("t3-console", os.listdir(disc))
        self.assertNotIn("no-udev", os.listdir(disc))
        with open(os.path.join(disc, "setup")) as f, \
                open(os.path.join(t3_common.GUEST, "setup")) as g:
            self.assertEqual(f.read(), g.read())

    def test_setup_reports_the_guest(self):
        # guest-sysvinit/setup's report line carries what check_guest reads,
        # stops udev for a variant without it, and installs t3-console.
        with open(os.path.join(t3_common.GUEST_SYSVINIT, "setup")) as f:
            text = f.read()
        for field in ('"TIER3-INSTALL-OK packages: ', '"kernel: $(uname -r);"',
                      '"init: $(cat /proc/1/comm) ',
                      '"systemd: $systemd;"', '"udev: $(pidof systemd-udevd udevd || echo none);"',
                      '"tree: $tree"', 'if [ -f "$disc"/no-udev ]; then',
                      'if [ -f "$disc"/two-vts ]; then', "telinit q",
                      "install -m755 \"$disc\"/t3-console /usr/local/sbin/t3-console"):
            self.assertIn(field, text)

    def test_install_fields(self):
        impl = {"tree_sha256": "a" * 64}
        installed = t3_scenario.check_install(self.LINE % ("none", "a" * 64), impl)
        self.assertEqual(installed, {"kernel": "6.12.48+deb13-amd64",
                                     "init": "init /usr/sbin/init", "systemd": "no",
                                     "udev": "none", "tree": "a" * 64})
        with self.assertRaisesRegex(RuntimeError, "packages installed that are not the run's"):
            t3_scenario.check_install((self.LINE % ("none", "a" * 64)).replace(
                "packages: ;", "packages: vtium 1 ;"), impl)

    def test_login_on_a_pty(self):
        # vtium's session logs in on a pty; a login on a VT's own tty is a
        # getty's, drawn by the kernel's console.
        t3_scenario.check_login("T3-LOGIN /dev/pts/0\r\n")
        for line in ("T3-LOGIN /dev/tty1", "T3-LOGIN /dev/pts/", "T3-LOGIN not a tty"):
            with self.subTest(line):
                with self.assertRaisesRegex(RuntimeError, "the login was not vtium's"):
                    t3_scenario.check_login(line)

    def test_draw_screens_refuses_a_getty_login(self):
        # The driver's own path: a login the guest reports on a VT's tty
        # stops the variant before any screen is typed.
        class Vm:
            typed = []

            def type(self, text):
                self.typed.append(text)

            def wait_event(self, marker, timeout):
                return "T3-LOGIN /dev/tty1"

        vm = Vm()
        screen = t3img.Image(1, 1, b"\0\0\0")
        with mock.patch.object(t3_scenario, "wait_stable", lambda *a, **k: screen):
            with self.assertRaisesRegex(RuntimeError, "the login was not vtium's"):
                t3_scenario.draw_screens("sysvinit-640x480", vm, self.dir.name, screen, {}, {},
                                         LOG, lambda name: None)
        self.assertEqual(vm.typed, ["tester\n", "tester\n"])

    def test_check_guest(self):
        good = {"init": "init /usr/sbin/init", "systemd": "no", "udev": "212", "tree": "x"}
        t3_scenario.check_guest("sysvinit-640x480", good)
        t3_scenario.check_guest("sysvinit-no-udev", dict(good, udev="none"))
        # Another guest is held to nothing.
        t3_scenario.check_guest("2d-potato-min", {"tree": "x"})
        for variant, account, why in (
                ("sysvinit-640x480", dict(good, init="systemd /usr/lib/systemd/systemd"),
                 "PID 1 is"),
                ("sysvinit-640x480", dict(good, systemd="yes"), "directory exists"),
                ("sysvinit-640x480", {"tree": "x"}, "PID 1 is None"),
                ("sysvinit-640x480", dict(good, udev="none"), "the variant runs one"),
                ("sysvinit-no-udev", good, "the variant runs none"),
                ("sysvinit-reboot", {k: v for k, v in good.items() if k != "udev"},
                 "udev daemon is None")):
            with self.subTest(why):
                with self.assertRaisesRegex(RuntimeError, why):
                    t3_scenario.check_guest(variant, account)

    def test_base_image(self):
        # Keyed by the guest's own files, under a name of its own in the
        # cache's base/, beside the Arch guest's.
        arch = t3_common.base_image("img.qcow2", "x")
        mine = t3_common.base_image("img.qcow2", "x", t3_common.GUEST_SYSVINIT,
                                    t3_common.SYSVINIT_INPUTS, "sysvinit-")
        self.assertEqual(os.path.dirname(mine), os.path.dirname(arch))
        self.assertRegex(os.path.basename(mine), r"^sysvinit-[0-9a-f]{16}\.qcow2$")
        for name in t3_common.SYSVINIT_INPUTS:
            self.assertTrue(os.path.isfile(os.path.join(t3_common.GUEST_SYSVINIT, name)))
        self.assertEqual(t3.guest_of("sysvinit")["inputs"], t3_common.SYSVINIT_INPUTS)

    def test_nothing_upgrades_past_the_snapshot(self):
        # The image's own apt timers and unattended-upgrades would install
        # packages newer than the snapshot: they are masked at the boot's
        # start, and unattended-upgrades purged before the first fetch.
        with open(os.path.join(t3_common.GUEST_SYSVINIT, "user-data.in")) as f:
            text = f.read()
        boot = text[text.index("\nbootcmd:"):text.index("\nwrite_files:")]
        for unit in ("apt-daily.timer", "apt-daily-upgrade.timer", "apt-daily.service",
                     "apt-daily-upgrade.service", "unattended-upgrades.service"):
            self.assertIn(unit, boot)
        self.assertIn("systemctl, mask, --now", boot)
        self.assertLess(text.index("purge unattended-upgrades"), text.index("apt update"))

    def test_pinned_base(self):
        # The pinned image, snapshot and guest files name one base image; a
        # change to any of them provisions a new one, an overlay of about
        # 165 MiB on the 416 MiB pristine image.
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            make = dict(re.findall(r"^(T3SV_IMAGE_VER|T3SV_SNAPSHOT)\s*:=\s*(\S+)", f.read(),
                                   re.M))
        image = "debian-13-generic-amd64-%s.qcow2" % make["T3SV_IMAGE_VER"]
        base = t3_common.base_image(image, make["T3SV_SNAPSHOT"], t3_common.GUEST_SYSVINIT,
                                    t3_common.SYSVINIT_INPUTS, "sysvinit-")
        self.assertEqual(os.path.basename(base), "sysvinit-10a794a420c61f4a.qcow2")

    def test_run_needs_its_base(self):
        # A run naming a variant of the guest without systemd refuses
        # without that guest's base image, naming the target that builds it.
        args = argparse.Namespace(variants=["sysvinit-reboot"], tree=None, tree_target=None,
                                  jobs=1, deadline=1)
        missing = os.path.join(self.dir.name, "base-sysvinit.qcow2")
        with mock.patch.dict(t3_scenario.GUESTS["sysvinit"], base=missing):
            with self.assertRaisesRegex(SystemExit, "run `make tier3-image-nosystemd`"):
                t3.run(args, LOG)


class BusyboxTest(unittest.TestCase):
    """The busybox boot of the guest without systemd: grub's second entry,
    which a variant's disc picks, busybox init's inittab, the kernel's trace
    of the signals sent to PID 1, the account of PID 1, and the driver's
    path through a case the guest stays up after."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def read(self, name):
        with open(os.path.join(t3_common.GUEST_SYSVINIT, name)) as f:
            return f.read()

    def test_variant(self):
        # The power variants' machine and configuration, on the guest
        # without systemd booted with busybox init, in make
        # tier3-nosystemd's run (SysvinitTest.test_variants).
        v = t3_scenario.VARIANTS["busybox-power-off"]
        self.assertEqual(v, dict(t3_scenario.POWER, guest="sysvinit", init="busybox",
                                 power=["power-off-busybox"]))
        self.assertEqual(t3_scenario.base_of("busybox-power-off"),
                         (t3_common.BASE_SYSVINIT, "tier3-image-nosystemd"))
        self.assertIn("busybox-power-off", t3_scenario.SYSVINIT_VARIANTS)

    def test_disc(self):
        # The busybox variant's disc carries the grub.cfg that picks the
        # busybox boot and the file that has the guest trace the signals
        # sent to PID 1; the sysvinit variants' discs carry neither.
        tree = os.path.join(self.dir.name, "tree")
        write_tree(tree, ImplementationTest.TREE)
        impl = t3_impl.tree(tree, os.path.join(self.dir.name, "out", "tree.tar"))
        disc = os.path.join(self.dir.name, "disc")
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("busybox-power-off", self.dir.name, impl)
            self.assertEqual(sorted(os.listdir(disc)),
                             sorted(["vtium.conf", "screens", "setup", "t3", "t3-console",
                                     "tree.tar", "grub.cfg", "trace-pid1-signals"]))
            with open(os.path.join(disc, "grub.cfg")) as f:
                self.assertEqual(f.read(), self.read("grub-busybox.cfg"))
            t3_scenario.build_disc("sysvinit-power-off", self.dir.name, impl)
            self.assertNotIn("grub.cfg", os.listdir(disc))
            self.assertNotIn("trace-pid1-signals", os.listdir(disc))

    def test_grub_picks_the_busybox_entry(self):
        # The disc's grub.cfg names the entry provisioning writes to
        # custom.cfg, whose kernel line runs t3-busybox-init as PID 1, and
        # custom.cfg reads the disc's grub.cfg only after that entry, from
        # the disc labelled T3PKG.
        cfg = [l for l in self.read("grub-busybox.cfg").splitlines()
               if l and not l.startswith("#")]
        self.assertEqual(cfg, ["set default=t3-busybox"])
        data = self.read("user-data.in")
        custom = data[data.index("cat > /boot/grub/custom.cfg"):]
        custom = custom[:custom.index("\n      EOF\n")]
        entry = custom.index("menuentry 'Debian GNU/Linux, with busybox init' --id t3-busybox {")
        self.assertIn(" init=/usr/local/sbin/t3-busybox-init\n", custom[entry:])
        search = custom.index("search --no-floppy --label --set=t3pkg T3PKG")
        self.assertLess(entry, search)
        self.assertIn("source ($t3pkg)/grub.cfg", custom[search:].replace("\\$", "$"))
        self.assertIn("insmod iso9660", custom)

    def test_busybox_init_reads_its_own_inittab(self):
        # t3-busybox-init binds busybox init's inittab over the sysvinit
        # one, whose runlevel lines busybox would run one after another,
        # then becomes busybox init.
        script = [l for l in self.read("t3-busybox-init").splitlines()
                  if l and not l.startswith("#")]
        self.assertEqual(script[-1], "exec /bin/busybox init")
        self.assertIn("if ! /bin/busybox mount -n -o bind /etc/inittab.busybox /etc/inittab; then",
                      script)
        data = self.read("user-data.in")
        for path, placeholder in (("/etc/inittab.busybox", "@BUSYBOX_INITTAB@"),
                                  ("/usr/local/sbin/t3-busybox-init", "@BUSYBOX_INIT@")):
            with self.subTest(path):
                at = data.index("  - path: %s\n" % path)
                self.assertIn("content: %s\n" % placeholder, data[at:at + 200])
        self.assertEqual(t3.guest_of("sysvinit")["embed"],
                         {"@INSTALL_SCRIPT@": "tier3-install",
                          "@BUSYBOX_INITTAB@": "inittab-busybox",
                          "@BUSYBOX_INIT@": "t3-busybox-init"})
        for name in ("inittab-busybox", "t3-busybox-init"):
            self.assertIn(name, t3_common.SYSVINIT_INPUTS)
        install = data[data.index("apt install --no-install-recommends"):]
        self.assertIn("busybox", install[:install.index("\n      echo LANG")].split())

    def test_inittab(self):
        # Each line is one busybox init reads: four fields, an action it
        # knows (init/init.c, parse_inittab), and a command. It runs the
        # boot's scripts, then the boot hook, then the terminals: vtium on
        # tty2 with init's console as its standard input, as under
        # sysvinit, and gettys on tty1 and tty3.
        actions = {"sysinit", "wait", "once", "respawn", "askfirst", "ctrlaltdel", "shutdown",
                   "restart"}
        lines = [l for l in self.read("inittab-busybox").splitlines()
                 if l and not l.startswith("#")]
        for line in lines:
            with self.subTest(line):
                fields = line.split(":", 3)
                self.assertEqual(len(fields), 4)
                self.assertIn(fields[2], actions)
                self.assertTrue(fields[3])
        self.assertEqual(lines, ["::sysinit:/etc/init.d/rcS",
                                 "::wait:/usr/local/sbin/tier3-install",
                                 "tty1::respawn:/sbin/getty --noclear 38400 tty1",
                                 "::respawn:/usr/local/sbin/t3-console tty2",
                                 "tty3::respawn:/sbin/getty 38400 tty3"])

    def test_setup_traces_the_signals_to_pid1(self):
        # Only on a disc that asks: the kernel's signal_generate event for
        # each signal sent to PID 1 but SIGCHLD, read onto the console.
        setup = self.read("setup")
        block = setup[setup.index('if [ -f "$disc"/trace-pid1-signals ]; then'):]
        block = block[:block.index("\nfi\n")]
        for part in ("mount -t tracefs tracefs",
                     "echo 'pid == 1 && sig != 17' > \"$t\"/events/signal/signal_generate/filter",
                     "echo 1 > \"$t\"/events/signal/signal_generate/enable",
                     "cat \"$t\"/trace_pipe > /dev/ttyS0 &"):
            self.assertIn(part, block)

    def test_check_guest(self):
        # PID 1 is the init the variant boots.
        good = {"init": "busybox /usr/bin/busybox", "systemd": "no", "udev": "212", "tree": "x"}
        t3_scenario.check_guest("busybox-power-off", good)
        with self.assertRaisesRegex(RuntimeError, "the guest's PID 1 is 'init /usr/sbin/init' "
                                    "and systemd's directory is absent; the variant runs "
                                    "busybox init"):
            t3_scenario.check_guest("busybox-power-off", dict(good, init="init /usr/sbin/init"))
        with self.assertRaisesRegex(RuntimeError, "the variant runs sysvinit"):
            t3_scenario.check_guest("sysvinit-power-off", good)
        with self.assertRaisesRegex(RuntimeError, "directory exists"):
            t3_scenario.check_guest("busybox-power-off", dict(good, systemd="yes"))

    class Vm:
        """A guest the driver presses a stays-up case on: its console log
        gets `after` once the key is pressed, its event port reports
        `login`, and QEMU reports `shutdown`."""

        def __init__(self, serial, before, after, login="T3-LOGIN /dev/pts/0",
                     shutdown=None):
            self.serial, self.after, self.login, self.shutdown = serial, after, login, shutdown
            self.did = []
            with open(serial, "wb") as f:
                f.write(before)

        def keys(self, *qcodes):
            self.did.append(("keys", qcodes))
            with open(self.serial, "ab") as f:
                f.write(self.after)

        def wait_serial(self, marker, timeout, since=0):
            self.did.append(("wait", marker, since))
            with open(self.serial, "rb") as f:
                if marker not in f.read()[since:].decode():
                    raise RuntimeError("no %r within %d s" % (marker, timeout))

        def type(self, text):
            self.did.append(("type", text))

        def wait_event(self, marker, timeout):
            self.did.append(("event", marker))
            if self.login is None:
                raise RuntimeError("no %r within %d s" % (marker, timeout))
            return self.login

        def shutdown_so_far(self):
            return self.shutdown

    def press(self, vm):
        caps, errors, notes = {}, {}, {}
        screen = t3img.Image(1, 1, b"\0\0\0")
        with mock.patch.object(t3_scenario, "wait_stable", lambda *a, **k: screen):
            t3_scenario.press_power_keys(vm, ["power-off-busybox"], self.dir.name, caps, errors,
                                         notes, LOG, lambda name: None)
        return caps, errors

    def test_the_driver_logs_in_after_the_key(self):
        # It waits for vtium's line and the kernel's trace written after
        # the key, then logs in on vtium's console, and the case passes.
        before = (PowerTest.WARNING + "\r\n").encode()
        after = ("\r\n".join(PowerTest.LOGS["power-off-busybox"]) + "\r\n").encode()
        vm = self.Vm(os.path.join(self.dir.name, "serial.log"), before, after)
        caps, errors = self.press(vm)
        self.assertEqual(errors, {})
        self.assertEqual(list(caps), ["power-off-busybox"])
        self.assertEqual(vm.did, [("keys", ("ctrl", "alt", "end")),
                                  ("wait", "key-power-off: asking init to power off", len(before)),
                                  ("wait", "signal_generate: ", len(before)),
                                  ("type", "tester\n"), ("type", "tester\n"),
                                  ("event", "T3-LOGIN")])

    def test_the_judges_reason_comes_first(self):
        # A guest that powered off fails with the judge's reason, not with
        # the login that could not happen.
        before = (PowerTest.WARNING + "\r\n").encode()
        after = ("\r\n".join(PowerTest.LOGS["power-off-busybox"]
                             + ["Requesting system poweroff", "[ 12.0] reboot: Power down"])
                 + "\r\n").encode()
        vm = self.Vm(os.path.join(self.dir.name, "serial.log"), before, after, login=None,
                     shutdown="guest-shutdown")
        caps, errors = self.press(vm)
        self.assertEqual(caps, {})
        self.assertEqual(errors, {"power-off-busybox":
                                  "'Requesting system' on the console after the key"})
        # A judge that passes leaves the login's failure as the reason.
        after = ("\r\n".join(PowerTest.LOGS["power-off-busybox"]) + "\r\n").encode()
        vm = self.Vm(os.path.join(self.dir.name, "serial.log"), before, after, login=None)
        caps, errors = self.press(vm)
        self.assertEqual(errors, {"power-off-busybox": "no 'T3-LOGIN' within 60 s"})
        # No trace of the signal: the wait for it fails, and the judge says why.
        after = ("\r\n".join(PowerTest.LOGS["power-off-busybox"][1:]) + "\r\n").encode()
        vm = self.Vm(os.path.join(self.dir.name, "serial.log"), before, after)
        caps, errors = self.press(vm)
        self.assertEqual(errors, {"power-off-busybox":
                                  "no trace of signal 38 sent to PID 1 after the key"})


class PointerTest(unittest.TestCase):
    """The pointer case (t3_pointer.py): where the cursor plane goes, and
    the guest's DRM state read and judged."""

    # A DRM atomic state dump as the kernel prints it: the primary plane,
    # the cursor plane showing a framebuffer, and a CRTC.
    STATE = ("plane[31]: plane-0\n\tcrtc=crtc-0\n\tfb=40\n\t\tallocated by = vtium\n"
             "\t\tsize=1366x768\n\tcrtc-pos=1366x768+0+0\n"
             "\tsrc-pos=1366.000000x768.000000+0.000000+0.000000\n\trotation=1\n"
             "plane[34]: plane-1\n\tcrtc=crtc-0\n\tfb=41\n\t\tsize=64x64\n"
             "\tcrtc-pos=64x64+342-1\n"
             "crtc[36]: crtc-0\n\tenable=1\n\tmode: \"1366x768\": 60\n")
    HIDDEN = ("plane[31]: plane-0\n\tcrtc=crtc-0\n\tfb=40\n\tcrtc-pos=1366x768+0+0\n"
              "plane[34]: plane-1\n\tcrtc=(null)\n\tfb=0\n\tcrtc-pos=0x0+0+0\n")
    DISPLAY = (1366, 768)

    def test_the_hotspot_follows_the_cell(self):
        for cell_h, hot in ((1, 1), (8, 1), (16, 1), (31, 1), (32, 2), (64, 3), (200, 3)):
            self.assertEqual(t3_pointer.hotspot(cell_h), hot, cell_h)

    def test_the_corner_is_the_hotspot_from_the_pixel_pointed_at(self):
        # 1366x768 in 8x16 cells: a grid of 1360x768 at 3,0.
        cell = (8, 16)
        self.assertEqual(t3_pointer.corner((0, 0), self.DISPLAY, cell), (2, -1))
        # the far end of the axis is the grid's last pixel
        self.assertEqual(t3_pointer.corner((t3_pointer.ABS_MAX,) * 2, self.DISPLAY, cell),
                         (3 + 1359 - 1, 767 - 1))
        # a quarter across and down, doubled by the mouse: 16384 * 1360 //
        # 65535 is 340; and 3 of 65535 is the first pixel's, 49 the second's
        self.assertEqual(t3_pointer.corner((0x2000, 0x2000), self.DISPLAY, cell),
                         (3 + 340 - 1, 192 - 1))
        self.assertEqual(t3_pointer.corner((24, 24), self.DISPLAY, cell), (3 + 0 - 1, 0 - 1))
        self.assertEqual(t3_pointer.corner((25, 49), self.DISPLAY, cell), (3 + 1 - 1, 1 - 1))
        # a 64-pixel cell: a hotspot of 3, the grid 1344x768 at 11,0
        self.assertEqual(t3_pointer.corner((0, 0), self.DISPLAY, (32, 64)), (11 - 3, -3))

    def test_the_planes_are_read_from_the_dump(self):
        planes = t3_pointer.planes(self.STATE)
        self.assertEqual([(p["name"], p["crtc"], p["fb"], p["pos"]) for p in planes],
                         [("plane-0", "crtc-0", 40, (1366, 768, 0, 0)),
                          ("plane-1", "crtc-0", 41, (64, 64, 342, -1))])
        self.assertEqual([p["name"] for p in t3_pointer.shown(self.STATE, self.DISPLAY)],
                         ["plane-1"])
        self.assertEqual(t3_pointer.shown(self.HIDDEN, self.DISPLAY), [])

    def test_a_reading_is_judged(self):
        judge = t3_pointer.judge
        self.assertIsNone(judge(self.STATE, self.DISPLAY, (342, -1)))
        self.assertIn("is at (342, -1), not (343, -1)", judge(self.STATE, self.DISPLAY, (343, -1)))
        self.assertIn("0 planes besides the primary", judge(self.HIDDEN, self.DISPLAY, (342, -1)))
        two = self.STATE + "plane[35]: plane-2\n\tcrtc=crtc-0\n\tfb=42\n\tcrtc-pos=8x8+1+1\n"
        self.assertIn("2 planes besides the primary", judge(two, self.DISPLAY, (342, -1)))
        # hidden: no plane besides the primary shows a framebuffer
        self.assertIsNone(judge(self.HIDDEN, self.DISPLAY, None))
        self.assertIn("still shows framebuffer 41", judge(self.STATE, self.DISPLAY, None))

    def test_the_dump_is_read_from_the_event_port_after_its_offset(self):
        events = ("T3-PLANE-BEGIN\nold\nT3-PLANE-END\nT3-DONE pointer\n"
                  "T3-PLANE-BEGIN\n" + self.STATE + "T3-PLANE-END\nT3-DONE pointer\n")
        since = events.index("T3-DONE pointer\n") + 1
        self.assertEqual(t3_pointer.dump(events, since), (self.STATE, None))
        self.assertEqual(t3_pointer.dump(events, len(events)),
                         (None, "the guest printed no DRM state"))
        state, why = t3_pointer.dump("T3-PLANE-ERROR no state file\n")
        self.assertIsNone(state)
        self.assertIn("T3-PLANE-ERROR no state file", why)

    def test_the_tablet_moves_with_absolute_events(self):
        self.assertEqual(t3_pointer.move_events(1, 2),
                         [{"type": "abs", "data": {"axis": "x", "value": 1}},
                          {"type": "abs", "data": {"axis": "y", "value": 2}}])

    def test_aim_puts_the_pointer_on_the_middle_of_the_cell(self):
        # aim runs corner's scaling backwards: the pointer lands on the
        # cell's middle pixel, whose corner is the hotspot up and left of it
        for display, cell, target in (((1920, 1080), (16, 40), (24, 4)),
                                      ((1920, 1080), (16, 40), (119, 26)),
                                      (self.DISPLAY, (8, 16), (0, 0)),
                                      (self.DISPLAY, (8, 16), (100, 30))):
            with self.subTest(display=display, cell=cell, target=target):
                at = t3_pointer.aim(target, display, cell)
                self.assertTrue(all(0 <= v <= t3_pointer.ABS_MAX for v in at), at)
                hot = t3_pointer.hotspot(cell[1])
                off = [(side - side // c * c) // 2 for side, c in zip(display, cell)]
                want = tuple(o + n * c + c // 2 - hot
                             for o, n, c in zip(off, target, cell))
                self.assertEqual(t3_pointer.corner(at, display, cell), want)
                # each value is the smallest that reaches the pixel
                self.assertNotEqual(t3_pointer.corner((at[0] - 1, at[1]), display, cell)[0],
                                    want[0])
        self.assertEqual(t3_pointer.aim((24, 4), (1920, 1080), (16, 40)), (6691, 5462))

    def test_a_scene_asks_for_the_pointer_on_the_event_port(self):
        events = ["T3-POINT 1 2 cell=8x16", "T3-DONE showcase-shell",
                  "T3-RSS showcase-pointer 1", "T3-POINT 24 4 cell=16x40",
                  "T3-DONE showcase-pointer", "T3-DONE showcase-other",
                  "T3-POINT 3 x cell=16x40", "T3-DONE showcase-bad"]
        self.assertEqual(t3_pointer.pointed(events, "showcase-pointer"),
                         ((24, 4), (16, 40)))
        self.assertEqual(t3_pointer.pointed(events, "showcase-shell"), ((1, 2), (8, 16)))
        # a screen that asked for none, a malformed line, a screen not drawn
        self.assertIsNone(t3_pointer.pointed(events, "showcase-other"))
        self.assertIsNone(t3_pointer.pointed(events, "showcase-bad"))
        self.assertIsNone(t3_pointer.pointed(events, "showcase-missing"))

    def test_the_cursor_plane_is_the_one_plane_besides_the_primary(self):
        plane = t3_pointer.cursor_plane(self.STATE, self.DISPLAY)
        self.assertEqual((plane["name"], plane["fb"], plane["pos"]),
                         ("plane-1", 41, (64, 64, 342, -1)))
        self.assertIsNone(t3_pointer.cursor_plane(self.HIDDEN, self.DISPLAY))
        self.assertIsNone(t3_pointer.cursor_plane("no readable state", self.DISPLAY))
        two = self.STATE + "plane[38]: plane-2\n\tcrtc=crtc-0\n\tfb=42\n\tcrtc-pos=8x8+0+0\n"
        with self.assertRaisesRegex(RuntimeError, "2 planes besides the primary"):
            t3_pointer.cursor_plane(two, self.DISPLAY)

    def test_compose_lays_the_premultiplied_cursor_over_the_capture(self):
        # a 4x3 capture of gray 100, and a 2x2 cursor: opaque white, the
        # outline's black at 220, half-covered white, and transparent
        img = t3img.Image(4, 3, bytes([100]) * 36)
        px = [(255, 255, 255, 255), (0, 0, 0, 220), (128, 128, 128, 128), (0, 0, 0, 0)]
        cursor = (2, 2, b"".join(bytes(p) for p in px))
        out = t3_pointer.compose(img, cursor, (1, 1))

        def at(x, y):
            return tuple(out.data[(y * 4 + x) * 3:(y * 4 + x) * 3 + 3])
        self.assertEqual(at(1, 1), (255, 255, 255))
        # 0 + 100 * 35 / 255, rounded
        self.assertEqual(at(2, 1), (14, 14, 14))
        # 128 + 100 * 127 / 255, rounded
        self.assertEqual(at(1, 2), (178, 178, 178))
        self.assertEqual(at(2, 2), (100, 100, 100))
        self.assertEqual(at(0, 0), (100, 100, 100))
        # B, G, R, A: the blue byte comes first
        blue = t3_pointer.compose(img, (1, 1, bytes((255, 0, 0, 255))), (0, 0))
        self.assertEqual(tuple(blue.data[:3]), (0, 0, 255))
        # the parts off the display are left out, on every side
        out = t3_pointer.compose(img, cursor, (-1, -1))
        self.assertEqual(tuple(out.data[:3]), (100, 100, 100))
        self.assertEqual(out.data[3:], img.data[3:])
        out = t3_pointer.compose(img, cursor, (3, 2))
        self.assertEqual(tuple(out.data[-3:]), (255, 255, 255))
        self.assertEqual(out.data[:-3], img.data[:-3])
        with self.assertRaises(ValueError):
            t3_pointer.compose(img, (2, 2, bytes(12)), (0, 0))

    def test_the_cursor_image_comes_over_vnc_with_its_alpha(self):
        # A VNC server that sends a framebuffer rectangle, then the cursor
        # with alpha (RFB's -314), raw; the client asked for both encodings.
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "vnc.sock")
            srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            srv.bind(path)
            srv.listen(1)
            got = {}
            pixels = bytes(range(2 * 3 * 4))

            def recv(c, n):
                buf = b""
                while len(buf) < n:
                    buf += c.recv(n - len(buf))
                return buf

            def serve(update):
                c, _ = srv.accept()
                with c:
                    c.sendall(b"RFB 003.008\n")
                    recv(c, 12)
                    c.sendall(b"\x01\x01")
                    recv(c, 1)
                    c.sendall(struct.pack(">I", 0))
                    recv(c, 1)
                    c.sendall(struct.pack(">HH", 640, 480) + bytes(16)
                              + struct.pack(">I", 1) + b"q")
                    recv(c, 20)
                    _, n = struct.unpack(">BxH", recv(c, 4))
                    got["encodings"] = list(struct.unpack(">%di" % n, recv(c, 4 * n)))
                    c.sendall(update)
                    c.recv(1)  # until the client closes

            update = (b"\x02" + b"\x00\x00" + struct.pack(">H", 2)
                      + struct.pack(">HHHHi", 0, 0, 1, 1, 0) + bytes(4)
                      + struct.pack(">HHHHi", 12, 2, 2, 3, -314) + struct.pack(">i", 0)
                      + pixels)
            t = threading.Thread(target=serve, args=(update,))
            t.start()
            with t3_qemu.VncCursor(path) as vnc:
                self.assertEqual(vnc.read(), (2, 3, pixels))
            t.join()
            self.assertEqual(got["encodings"], [0, -314])
            # a server that sends no cursor: the read gives up
            t = threading.Thread(target=serve, args=(b"",))
            t.start()
            with t3_qemu.VncCursor(path) as vnc:
                vnc.sock.settimeout(0.2)
                with self.assertRaisesRegex(RuntimeError, "sent no cursor image"):
                    vnc.read()
            t.join()
            srv.close()

    def test_the_showcase_reads_the_pointer(self):
        # the showcase runs the agent, which reads the cursor plane, and a
        # VNC server beside its screendumps, which gives the plane's image
        v = t3_scenario.VARIANTS["showcase-1920x1080"]
        self.assertTrue(v["agent"])
        self.assertTrue(v["cursor"])
        self.assertEqual(v.get("capture", "screendump"), "screendump")
        self.assertEqual(t3_scenario.display_size("showcase-1920x1080"), (1920, 1080))

        class Started(Exception):
            pass

        def popen(args, **kwargs):
            raise Started(args)

        with mock.patch.object(t3_qemu.subprocess, "Popen", popen), \
                tempfile.TemporaryDirectory() as d:
            with self.assertRaises(Started) as cm:
                t3_qemu.Qemu(d, [], "virtio-vga", lambda msg: None, cursor=True)
            args = cm.exception.args[0]
            self.assertEqual(args.count("-vnc"), 1)
            self.assertTrue(args[args.index("-vnc") + 1].endswith("/vnc.sock"))
        # the pointer scene asks for the pointer on its second address
        with open(os.path.join(t3_common.GUEST, "screens", "showcase-pointer.sh")) as f:
            self.assertRegex(f.read(), r"(?m)^point_at 24 4$")

    def test_only_the_pointer_variant_has_the_case(self):
        for name in t3_scenario.VARIANTS:
            with self.subTest(name):
                cases = t3_scenario.variant_cases(name)
                if t3_scenario.VARIANTS[name].get("pointer"):
                    # after every screen's case, followed only by the
                    # variant's checks
                    checks = t3_scenario.VARIANTS[name].get("checks", [])
                    self.assertEqual(cases[len(cases) - len(checks) - 1:],
                                     [t3_pointer.CASE] + checks)
                else:
                    self.assertNotIn(t3_pointer.CASE, cases)
        self.assertEqual([n for n, v in t3_scenario.VARIANTS.items() if v.get("pointer")],
                         ["2d-potato-min"])
        self.assertIn(t3_pointer.CASE, t3_evaluate.NO_GOLDEN)
        self.assertEqual(t3_evaluate.golden_for("2d-potato-min", t3_pointer.CASE), (None, None))
        self.assertEqual(t3_evaluate.describe(t3_pointer.CASE),
                         (t3_pointer.CHECKS[t3_pointer.CASE], None))


class BenchTest(unittest.TestCase):
    EVENT = "T3-BENCH s1-scroll lines=20000 start=100.00 end=102.00"

    def test_pane_damage(self):
        # img-tmux-pane (vtium/vtium#30): the mean damage per frame over the
        # window must stay within the playing pane's pixels. Two panes'
        # worth of damage fails, as does a window in which nothing was drawn.
        event = ("T3-BENCH img-tmux-pane start=100.00 end=102.00 mpv_frames=60 "
                 "mpv_dropped=0 tmux_sixels=60 pane_px=69888")
        conf = ["render-timer=log", "display-buffers=2"]
        for damage, want in ((58101, None), (69888, None), (69889, "more than the playing pane"),
                             (139776, "139776 pixels a frame on the mean, more than the "
                                      "playing pane's 69888")):
            with self.subTest(damage):
                serial = "\n".join([timer_line(100.6, 30, 30, damage=(damage, 22.0)),
                                    timer_line(101.6, 30, 30, damage=(damage, 22.0))])
                got = t3_bench.measure(serial, event, conf)
                self.assertEqual((got["pane_px"], got["damage_px_mean"]), (69888, damage))
                if want:
                    self.assertIn(want, got["error"])
                else:
                    self.assertNotIn("error", got)
        got = t3_bench.measure(timer_line(100.6, 0, 0), event, conf)
        self.assertIn("no frame was drawn while the pane played", got["error"])
        # One second of full-screen redraws among the pane's, wholly inside
        # the window, 100.0 to 103.0 s: the window's mean stays within the
        # pane, the second's does not, and the case fails.
        wide = event.replace("end=102.00", "end=103.00")
        serial = "\n".join([timer_line(100.6, 60, 60, damage=(40000, 13.0)),
                            timer_line(102.0, 1, 1, damage=(307200, 100.0)),
                            timer_line(103.0, 60, 60, damage=(40000, 13.0))])
        got = t3_bench.measure(serial, wide, conf)
        self.assertLess(got["damage_px_mean"], 69888)
        self.assertEqual(got["damage_px_seconds"], [307200, 40000])
        self.assertEqual(got["error"], "in 1 second(s) of the window the console damaged more "
                                       "than the playing pane's 69888 pixels a frame on the "
                                       "mean, at most 307200")
        # The same second straddling the window's start (99.6 to 100.6 s) or
        # its end (102.6 to 103.6 s) also holds frames from outside it, so
        # it does not count for the rule of each second, and the case passes
        # on the window's mean.
        for edge in (100.6, 103.6):
            with self.subTest(edge=edge):
                serial = "\n".join([timer_line(edge, 1, 1, damage=(307200, 100.0)),
                                    timer_line(102.0, 60, 60, damage=(40000, 13.0))])
                got = t3_bench.measure(serial, wide, conf)
                self.assertLess(got["damage_px_mean"], 69888)
                self.assertEqual(got["damage_px_seconds"], [40000])
                self.assertNotIn("error", got)
        # the variant keeps two display buffers, so a frame's damage counts
        # two frames' changes
        with open(os.path.join(t3_common.GUEST, "vtium-images.conf")) as f:
            self.assertIn("display-buffers=2\n", f.read())
        self.assertEqual(t3_scenario.VARIANTS["images-640x480"]["conf"], "vtium-images.conf")
        self.assertIn("img-tmux-pane", t3_evaluate.BENCHMARKS)
    MPV = "T3-BENCH mpv-sixel start=100.00 end=102.00 mpv_frames=60 mpv_dropped=2 console_cpu_ms=500"
    CONF = ["font-size=16", "render-timer=log"]

    def test_timer_lines(self):
        text = "\n".join([
            "[  OK  ] Started something.",
            timer_line(99.5, 3, 3),
            # a console line after a prompt on the same line
            timer_line(100.5, 60, 60, before="tier3 login: "),
            "[  100.600000] kmscon[505]: terminal: Resized terminal to 128x42",
            # broken into by the getty's queries on the serial console at boot
            timer_line(101.5, 6, 6)[:150] + "\x1b]3008;start=568e;user=root;comm=(agetty)",
        ])
        got = t3_bench.timer_lines(text)
        self.assertEqual([x["t"] for x in got], [99.5, 100.5])
        self.assertEqual(got[1]["display"], "pci-0000:00:01.0[Virtual-1]")
        self.assertEqual((got[1]["flips"], got[1]["frames"], got[1]["pid"]), (60, 60, 505))
        self.assertEqual(got[1]["damage_pct"], 1.5)

    def test_window(self):
        self.assertEqual(t3_bench.bench_window("x " + self.EVENT),
                         ("s1-scroll", 100.0, 102.0, {"lines": 20000}))
        self.assertIsNone(t3_bench.bench_window("T3-DONE s1-scroll"))
        # A program's screen reports its own counts after the window.
        self.assertEqual(t3_bench.bench_window(self.MPV),
                         ("mpv-sixel", 100.0, 102.0,
                          {"mpv_frames": 60, "mpv_dropped": 2, "console_cpu_ms": 500}))
        self.assertIsNone(t3_bench.bench_window("T3-BENCH x start=1.0 end=2.0 frames=many"))

    def test_summarize(self):
        lines = t3_bench.timer_lines("\n".join([
            timer_line(99.9, 5, 5),                 # ends before the window: out
            timer_line(100.6, 30, 30, cpu=(2.0, 2.0, 4.0, 5.0), damage=(1000, 10.0)),
            timer_line(101.6, 60, 60, cpu=(1.0, 1.0, 3.0, 9.0), damage=(400, 4.0)),
            timer_line(102.6, 10, 10, cpu=(4.0, 4.0, 6.0, 6.0), damage=(100, 1.0),
                       skipped=(400, 500.0)),
            timer_line(103.6, 0, 0),                # starts after the window: out
        ]))
        got = t3_bench.summarize(lines, 20000, 100.0, 102.0)
        self.assertEqual(got["intervals"], 3)
        self.assertEqual((got["flips"], got["frames"]), (100, 100))
        self.assertEqual(got["presented_per_s"], 50.0)
        self.assertEqual(got["lines_per_s"], 10000)
        self.assertEqual(got["cpu_ms_mean"], 1.6)          # (60 + 60 + 40) / 100
        self.assertEqual(got["cpu_ms_median"], 2.0)        # the seconds' medians 2, 1, 4
        self.assertEqual((got["cpu_ms_p95"], got["cpu_ms_max"]), (6.0, 9.0))
        self.assertEqual(got["damage_px_mean"], 550)       # (30000 + 24000 + 1000) / 100
        self.assertEqual(got["damage_pct"], 5.5)
        # Redraws that found nothing to present still cost CPU time (#139).
        self.assertEqual((got["skipped"], got["skipped_cpu_ms"]), (400, 500.0))
        self.assertEqual(got["skipped_cpu_pct"], 25.0)     # 500 ms of the 2 s window
        self.assertEqual(got["displays"], ["pci-0000:00:01.0[Virtual-1]"])
        # only the second from 100.6 to 101.6 lies wholly inside the window
        self.assertEqual((got["intervals_inside"], got["flips_inside"]), (1, 60))
        self.assertEqual(got["skipped_per_s"], 200.0)      # 400 over the 2 s window
        # A window with no skipped redraws: a rate of zero.
        quiet = t3_bench.timer_lines("\n".join(
            timer_line(t, 0, 0) for t in (101.0, 102.0, 103.0, 104.0)))
        got = t3_bench.summarize(quiet, 20000, 100.0, 102.0)
        self.assertEqual((got["skipped"], got["skipped_per_s"]), (0, 0.0))

    FLOOD = "T3-BENCH flood lines=20 start=100.00 end=104.00"

    def flood(self, *lines):
        return t3_bench.measure("\n".join(lines), self.FLOOD, self.CONF)

    def test_flood_verdict(self):
        # Frames in every second of the stream, and a few passes that drew
        # nothing: in bounds.
        ok = [timer_line(t, 10, 10, skipped=(4, 0.0)) for t in (101.0, 102.0, 103.0, 104.0)]
        self.assertNotIn("error", self.flood(*ok))
        # The same screen as s1-scroll is not judged.
        lines = [timer_line(t, 0, 0) for t in (101.0, 102.0, 103.0, 104.0)]
        self.assertNotIn("error", t3_bench.measure("\n".join(lines), self.EVENT, self.CONF))
        # No frame while the stream flowed: only the second that ends after
        # the window presents.
        late = [timer_line(t, 0, 0) for t in (101.0, 102.0, 103.0, 104.0)] + \
            [timer_line(104.5, 30, 30)]
        self.assertIn("no frame was presented while the stream flowed",
                      self.flood(*late)["error"])
        # A pass after every read: 100 a second that drew nothing.
        busy = [timer_line(t, 10, 10, skipped=(100, 0.0)) for t in (101.0, 102.0, 103.0, 104.0)]
        self.assertIn("100.0 passes a second drew nothing, over %d"
                      % t3_bench.FLOOD_MAX_SKIPPED_PER_S, self.flood(*busy)["error"])
        # A window too short to hold a whole second cannot show the stream.
        short = "T3-BENCH flood lines=2 start=100.00 end=100.50"
        got = t3_bench.measure(timer_line(101.0, 10, 10), short, self.CONF)
        self.assertIn("no render-timer second lies wholly inside the window", got["error"])

    def test_measure(self):
        serial = timer_line(101.0, 60, 60)
        got = t3_bench.measure(serial, self.EVENT, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["frames"], 60)
        # No line in the window: a fault of a build that has the timer...
        self.assertIn("error", t3_bench.measure(timer_line(50.0, 1, 1), self.EVENT, self.CONF))
        self.assertIn("error", t3_bench.measure("", self.EVENT, ["render-timer=both"]))
        # ...and none for a build without it, or a variant not configured.
        got = t3_bench.measure("[ 3.1] kmscon[9]: config: unknown config option 'render-timer'",
                               self.EVENT, self.CONF)
        self.assertEqual(got["note"], "this build has no render timer")
        self.assertNotIn("error", got)
        got = t3_bench.measure("", self.EVENT, ["render-timer=none"])
        self.assertIn("not configured", got["note"])
        self.assertNotIn("error", got)
        # A screen that reported no window.
        self.assertIn("no benchmark window", t3_bench.measure(serial, None, self.CONF)["error"])

    def test_measure_tmux_sixel(self):
        # tmux draws a "SIXEL IMAGE" placeholder for a client whose features
        # lack sixel, which looks the same to every other figure.
        serial = timer_line(101.0, 60, 60)
        # kmscon presents as many frames either way, so the screen counts
        # the sixel pictures tmux sent to kmscon.
        line = ("T3-BENCH tmux-2x2 start=100.00 end=102.00 mpv_frames=600 mpv_dropped=0 "
                "console_cpu_ms=700 panes=4 tmux_sixels=%d")
        got = t3_bench.measure(serial, line % 412, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["tmux_sixels"], 412)
        got = t3_bench.measure(serial, line % 0, self.CONF)
        self.assertIn("placeholder", got["error"])

    def test_measure_tmux_reported(self):
        # A4 is compatibility through tmux: the console's CPU time per
        # picture tmux sent is reported, and no figure of it fails the grid.
        serial = timer_line(101.0, 20, 20)
        line = ("T3-BENCH tmux-3x3 start=100.00 end=102.00 mpv_frames=300 mpv_dropped=900 "
                "console_cpu_ms=%d panes=9 tmux_sixels=100")
        got = t3_bench.measure(serial, line % 1400, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["console_cpu_ms_per_tmux_sixel"], 14.0)
        got = t3_bench.measure(serial, line % 3400, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["console_cpu_ms_per_tmux_sixel"], 34.0)

    def test_measure_program(self):
        # mpv's counts beside the render timer's: its output rate over the
        # window, kmscon's CPU time over the window per frame mpv sent (its
        # parsing included) and its share of one CPU.
        serial = "\n".join([timer_line(101.0, 30, 30), timer_line(102.0, 30, 30)])
        got = t3_bench.measure(serial, self.MPV, self.CONF)
        self.assertNotIn("error", got)
        self.assertNotIn("lines", got)
        self.assertEqual((got["mpv_frames"], got["mpv_dropped"]), (60, 2))
        self.assertEqual(got["mpv_per_s"], 30.0)
        self.assertEqual(got["presented_per_s"], 30.0)
        self.assertEqual(got["console_cpu_ms"], 500)
        self.assertEqual(got["console_cpu_ms_per_mpv_frame"], 8.333)
        self.assertEqual(got["console_cpu_pct"], 25.0)

    def test_bench_error_fails_case(self):
        meta = {"bench": {"s1-scroll": {"error": "no render-timer line"}}}
        with tempfile.TemporaryDirectory() as d:
            ppm = os.path.join(d, "a.ppm")
            t3_qemu.write_ppm(ppm, t3img.Image(2, 1, b"\0" * 6))
            saved = t3_evaluate.GOLDEN, t3_evaluate.RUNDIR
            t3_evaluate.GOLDEN, t3_evaluate.RUNDIR = d, d
            try:
                os.makedirs(os.path.join(d, "v"))
                t3img.write_png(os.path.join(d, "v", "s1-scroll.png"), t3img.Image(2, 1, b"\0" * 6))
                with contextlib.redirect_stderr(io.StringIO()):
                    entry = t3_evaluate.evaluate("v", "s1-scroll", ppm, meta, LOG)
                self.assertEqual((entry["status"], entry["error"]), ("fail", "no render-timer line"))
                with contextlib.redirect_stderr(io.StringIO()):
                    entry = t3_evaluate.evaluate("v", "s1-scroll", ppm, {}, LOG)
                self.assertEqual(entry["status"], "pass")
                # an image screen a build without images found not offered
                # is judged by its capture alone, its rate unjudged
                t3img.write_png(os.path.join(d, "v", "mpv-sixel.png"), t3img.Image(2, 1, b"\0" * 6))
                meta = {"bench": {"mpv-sixel": {"not_offered": "T3-NO-SIXEL"}}}
                with contextlib.redirect_stderr(io.StringIO()):
                    entry = t3_evaluate.evaluate("v", "mpv-sixel", ppm, meta, LOG)
                self.assertEqual(entry["status"], "pass")
            finally:
                t3_evaluate.GOLDEN, t3_evaluate.RUNDIR = saved


class BottleneckTest(unittest.TestCase):
    """A2's criterion: the console is not the bottleneck. Its CPU
    time per frame mpv sent stays under the clip's frame interval, and it
    presents at least as many frames as mpv sends."""
    FINE = {"mpv_frames": 150, "mpv_per_s": 29.1, "presented_per_s": 59.0,
            "console_cpu_ms_per_mpv_frame": 6.733}
    SLOW = {"mpv_frames": 69, "mpv_per_s": 2.1, "presented_per_s": 4.2,
            "console_cpu_ms_per_mpv_frame": 375.797}

    def test_bottleneck(self):
        self.assertEqual(t3_bench.bottleneck(self.FINE), [])
        self.assertEqual(len(t3_bench.bottleneck(self.SLOW)), 1)
        self.assertIn("33.3 ms", t3_bench.bottleneck(self.SLOW)[0])
        few = dict(self.FINE, presented_per_s=20.0)
        self.assertEqual(len(t3_bench.bottleneck(few)), 1)
        self.assertIn("fewer frames", t3_bench.bottleneck(few)[0])
        # No program: nothing to judge. A program that showed no frame
        # played nothing, which is no pass.
        self.assertEqual(t3_bench.bottleneck({"lines": 5}), [])
        self.assertIn("no frame", t3_bench.bottleneck(dict(self.FINE, mpv_frames=0))[0])

    def evaluate(self, variant, figures):
        meta = {"bench": {"mpv-sixel": figures}}
        with tempfile.TemporaryDirectory() as d:
            ppm = os.path.join(d, "a.ppm")
            t3_qemu.write_ppm(ppm, t3img.Image(2, 1, b"\0" * 6))
            saved = t3_evaluate.GOLDEN, t3_evaluate.RUNDIR
            t3_evaluate.GOLDEN, t3_evaluate.RUNDIR = d, d
            try:
                os.makedirs(os.path.join(d, variant))
                t3img.write_png(os.path.join(d, variant, "mpv-sixel.png"),
                                t3img.Image(2, 1, b"\0" * 6))
                with contextlib.redirect_stderr(io.StringIO()):
                    return t3_evaluate.evaluate(variant, "mpv-sixel", ppm, meta, LOG)
            finally:
                t3_evaluate.GOLDEN, t3_evaluate.RUNDIR = saved

    def test_case(self):
        self.assertEqual(self.evaluate("v", self.FINE)["status"], "pass")
        slow = self.evaluate("v", self.SLOW)
        self.assertEqual(slow["status"], "fail")
        self.assertIn("bottleneck", slow["error"])

    def test_known(self):
        # A known bottleneck, named by its issue, passes with a note, and
        # fails once it is gone, so its entry is removed with the fix. The
        # entry is the test's own, since none is known today.
        variant, issue = "v-known", "vtium/vtium#0"
        with mock.patch.dict(t3_evaluate.KNOWN_BOTTLENECKS, {"mpv-sixel": {variant: issue}}):
            self.known(variant, issue)
        # Without the entry the same variant is judged like any other.
        self.assertEqual(self.evaluate(variant, self.SLOW)["status"], "fail")
        # #139 at both 7680x2160 variants was fixed by #152, so A2 is
        # judged at every variant.
        self.assertEqual(t3_evaluate.KNOWN_BOTTLENECKS, {})
        self.assertEqual(t3_evaluate.RATE_CHECKED, ["mpv-sixel"])

    def known(self, variant, issue):
        known = self.evaluate(variant, self.SLOW)
        self.assertEqual(known["status"], "pass")
        self.assertIn(issue, known["note"])
        gone = self.evaluate(variant, self.FINE)
        self.assertEqual(gone["status"], "fail")
        self.assertIn(issue, gone["error"])
        # Only slowness is known: a variant whose mpv showed no frame at all
        # (a broken mpv or sixel path) fails like any other.
        dead = self.evaluate(variant, dict(self.SLOW, mpv_frames=0))
        self.assertEqual(dead["status"], "fail")
        self.assertIn("no frame", dead["error"])


class SerialTest(unittest.TestCase):
    """The guest writes its console a few bytes at a time, so the driver can
    read a line before its end has arrived (#61)."""
    LINE = (b"TIER3-INSTALL-OK packages: ; tree: "
            b"2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae")

    class Clock:
        """Stands in for t3_qemu's time: each sleep advances the clock and
        lets the guest write its next chunk."""
        def __init__(self, path, chunks):
            self.path, self.chunks, self.now = path, list(chunks), 0.0

        def time(self):
            return self.now

        def sleep(self, seconds):
            self.now += seconds
            if self.chunks:
                with open(self.path, "ab") as f:
                    f.write(self.chunks.pop(0))

    class Proc:
        def poll(self):
            return None

    def wait(self, chunks, marker="TIER3-INSTALL-OK", timeout=180, how=None):
        """Runs `how(vm)`, by default the wait for `marker`, while the guest
        writes `chunks`, and returns its result and the stand-in clock's
        time when it returned or raised."""
        with tempfile.TemporaryDirectory() as d:
            vm = t3_qemu.Qemu.__new__(t3_qemu.Qemu)
            vm.serial, vm.proc = os.path.join(d, "serial.log"), self.Proc()
            with open(vm.serial, "wb") as f:
                f.write(b"[  OK  ] Started \x1b[0;1;39mUser Login Management\x1b[0m.\r\r\n")
            saved = t3_qemu.time
            clock = t3_qemu.time = self.Clock(vm.serial, chunks)
            try:
                if how is None:
                    return vm.wait_serial(marker, timeout)
                return how(vm)
            finally:
                t3_qemu.time = saved
                self.ended = clock.now

    def test_install_failure_ends_the_wait(self):
        # guest/setup writes TIER3-INSTALL-FAIL and stops when vtium console
        # enable --all fails. The wait for the install returns at once with
        # that line's text, where waiting for TIER3-INSTALL-OK would time out
        # after 180 s.
        fail = b"[  20.1] TIER3-INSTALL-FAIL vtium console enable --all: VT 1 is held\r\r\n"
        with self.assertRaisesRegex(RuntimeError, "the guest reported a failure: .*"
                                    "TIER3-INSTALL-FAIL vtium console enable --all: VT 1 is held"):
            self.wait([b"[  19.0] setup: installing\r\r\n", fail], how=t3_scenario.wait_install)
        self.assertLess(self.ended, 1.0)

    def test_install_ok_is_returned(self):
        self.assertEqual(self.wait([self.LINE + b"\r\r\n"], how=t3_scenario.wait_install),
                         self.LINE.decode())

    def test_two_chunks(self):
        cut = self.LINE.index(b" tree: ")
        self.assertEqual(self.wait([self.LINE[:cut], self.LINE[cut:] + b"\r\r\n"]),
                         self.LINE.decode())

    def test_many_chunks(self):
        # One byte at a time, a marker split across chunks included.
        data = self.LINE + b"\r\r\n"
        self.assertEqual(self.wait([data[i:i + 1] for i in range(len(data))]),
                         self.LINE.decode())

    def test_partial_line_times_out(self):
        # A line whose end never arrives is never read, and the wait fails
        # at its deadline.
        with self.assertRaisesRegex(RuntimeError, "no 'TIER3-INSTALL-OK' within 5 s"):
            self.wait([self.LINE[:40], self.LINE[40:]], timeout=5)

    def test_split_utf8(self):
        # A character split between chunks is read whole once the line ends.
        line = "T3-DONE cjk \u4e2d".encode()
        self.assertEqual(self.wait([line[:-1], line[-1:] + b"\n"], marker="T3-DONE"),
                         "T3-DONE cjk \u4e2d")

    def test_since(self):
        # A wait from an offset reads only what the guest wrote after it:
        # the marker the boot already logged does not end the wait, and the
        # one written later does.
        start = len(b"[  OK  ] Started \x1b[0;1;39mUser Login Management\x1b[0m.\r\r\n")
        with self.assertRaisesRegex(RuntimeError, "no 'User Login Management' within 5 s"):
            self.wait([], marker="User Login Management", timeout=5,
                      how=lambda vm: vm.wait_serial("User Login Management", 5, since=start))
        self.assertEqual(self.wait([b"[  30.0] Stopped User Login Management.\r\n"],
                                   how=lambda vm: vm.wait_serial("User Login Management", 5,
                                                                 since=start)),
                         "[  30.0] Stopped User Login Management.")


class LoggerTest(unittest.TestCase):
    def line(self, log):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            log("hello")
        return err.getvalue()

    def test_labelled(self):
        log = Logger(t0=0)
        self.assertRegex(self.line(log), r"^\[t3 \+[0-9.]+s\] hello\n$")
        self.assertRegex(self.line(log.labelled("2d-1280x800")),
                         r"^\[t3 \+[0-9.]+s 2d-1280x800\] hello\n$")

    def test_pickles(self):
        # A worker process receives the run's logger by pickle.
        log = pickle.loads(pickle.dumps(Logger(t0=123.5).labelled("v")))
        self.assertEqual((log.t0, log.label), (123.5, "v"))


class DevicesTest(unittest.TestCase):
    """The device variants (t3_devices.py): their machines and cases, the
    outputs each captures, the second head connected over VNC, the mirror
    check, the guest's reading of the udev examples and the hotplugged
    keyboard."""

    def test_variants(self):
        # The minspec machine at 640x480 in the 8x16 test font, out of the
        # default and minimal runs, judged by vtium's goldens.
        cases = {
            "two-outputs-mirror": ["sgr", "sgr@1", "cjk", "cjk@1", "vt-switch", "vt-restore",
                                   "vt-restore@1", "mirror"],
            "two-cards-mirror": ["sgr", "sgr@card1", "cjk", "cjk@card1", "vt-switch",
                                 "vt-restore", "vt-restore@card1", "mirror"],
            "two-cards-mirror-hotplug": ["sgr", "sgr@card1", "cjk", "cjk@card1", "vt-switch",
                                         "vt-restore", "vt-restore@card1", "mirror",
                                         "display-unplug", "display-replug"],
            "usb-keyboards": ["cjk", "vt-switch", "vt-restore", "udev-examples",
                              "keyboard-hotplug"],
        }
        self.assertEqual(t3_scenario.DEVICE_VARIANTS, list(cases))
        for name, want in cases.items():
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                self.assertEqual((v["cpus"], v["memory_mb"], v["conf"]),
                                 (1, 512, "vtium-small-8x16.conf"))
                self.assertIn("id=video0", v["device"])
                self.assertIn("xres=640,yres=480", v["device"])
                self.assertIn(name, t3_scenario.golden_cases())
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                self.assertEqual(t3_scenario.variant_cases(name), want)
                self.assertEqual(t3_scenario.base_of(name)[0], t3_common.BASE)
        self.assertIn("max_outputs=2", t3_scenario.VARIANTS["two-outputs-mirror"]["device"])
        self.assertEqual(t3_scenario.VARIANTS["two-cards-mirror"]["qemu"],
                         ["-device", "virtio-gpu-pci,id=video1,xres=640,yres=480,edid=on"])
        self.assertEqual(t3_scenario.VARIANTS["two-cards-mirror-hotplug"]["qemu"],
                         ["-device", "pcie-root-port,id=rp1,bus=pcie.0,addr=0x10,chassis=1",
                          "-device", "bochs-display,id=video1,bus=rp1,xres=640,yres=480,edid=on"])
        # The hotplug variant is judged by two-cards-mirror's goldens and has
        # none of its own.
        root = t3_evaluate.GOLDEN
        for case in ("sgr@card1", "cjk", "vt-restore@card1"):
            self.assertEqual(t3_evaluate.golden_for("two-cards-mirror-hotplug", case, root)[0],
                             t3_evaluate.golden_for("two-cards-mirror", case, root)[0])
        self.assertFalse(os.path.exists(os.path.join(root, "two-cards-mirror-hotplug")))

    def test_second_card(self):
        # The card display-replug plugs back in is the card the guest boots
        # with: the same device, in the same port, as QMP device_add takes it.
        self.assertEqual(t3_devices.SECOND_CARD,
                         {"driver": "bochs-display", "id": "video1", "bus": "rp1",
                          "xres": 640, "yres": 480, "edid": True})
        self.assertEqual(t3_devices.device_option(t3_devices.SECOND_CARD),
                         "bochs-display,id=video1,bus=rp1,xres=640,yres=480,edid=on")
        self.assertEqual(t3_devices.device_option({"driver": "d", "edid": False}), "d,edid=off")
        # The port's id is the card's bus, and the card is the output the
        # variant captures after the first.
        self.assertIn("id=%s," % t3_devices.SECOND_CARD["bus"], t3_devices.SECOND_PORT)
        self.assertEqual(t3_scenario.VARIANTS["two-cards-mirror-hotplug"]["outputs"][0]
                         ["device"], t3_devices.SECOND_CARD["id"])
        self.assertEqual(set(t3_scenario.CHECK_RUNS) & set(t3_devices.CHECKS),
                         {"udev-examples", "keyboard-hotplug", "display-unplug",
                          "display-replug"})

    # The guest's account of its cards (screens/display-cards.sh).
    CARDS = ["T3-CARD card0 0000:00:01.0", "T3-CARD card1 0000:01:00.0", "T3-DONE display-cards"]

    def test_display_cards(self):
        self.assertEqual(t3_devices.cards(self.CARDS),
                         {"card0": "0000:00:01.0", "card1": "0000:01:00.0"})
        self.assertEqual(t3_devices.cards(["T3-CARD card0", "T3-DONE display-cards"]), {})
        want = ["0000:01:00.0", "0000:00:01.0"]
        self.assertIsNone(t3_devices.judge_cards(t3_devices.cards(self.CARDS), want))
        # The card came back under another name, at the same address.
        self.assertIsNone(t3_devices.judge_cards({"card0": "0000:00:01.0",
                                                  "card2": "0000:01:00.0"}, want))
        self.assertEqual(t3_devices.judge_cards({"card0": "0000:00:01.0"}, want),
                         "the guest lists the cards at 0000:00:01.0, not at 0000:00:01.0, "
                         "0000:01:00.0")
        self.assertEqual(t3_devices.judge_cards({}, want),
                         "the guest lists the cards at no address, not at 0000:00:01.0, "
                         "0000:01:00.0")
        # The guest's view goes into a failed replug's error.
        self.assertEqual(t3_devices.guest_view(self.CARDS[:2] + [
            "T3-SLOT 0 0000:01:00 1 1", "T3-KMSG pcieport: Slot(0): Card present",
            "T3-RSS display-cards 6524", "T3-DONE display-cards"]),
            "card0 0000:00:01.0; card1 0000:01:00.0; 0 0000:01:00 1 1; "
            "pcieport: Slot(0): Card present")
        self.assertEqual(t3_devices.guest_view(["T3-DONE display-cards", ""]), "nothing")
        m = t3_devices.DISPLAY_LINE.search(
            "[   27.753038] vtium[765]: Display Virtual-2 with backend [drm2d] text renderer "
            "[cpu], cell size 8x16")
        self.assertEqual(m.group(1), "Virtual-2")
        self.assertIsNone(t3_devices.DISPLAY_LINE.search(
            "[   27.748585] vtium[765]: display Virtual-2 at 640x480"))

    def test_outputs(self):
        outputs = t3_scenario.VARIANTS["two-outputs-mirror"]["outputs"]
        self.assertEqual(t3_devices.heads(outputs), [("video0", 1, (640, 480))])
        self.assertEqual(t3_devices.capture(outputs[0]), {"device": "video0", "head": 1})
        cards = t3_scenario.VARIANTS["two-cards-mirror"]["outputs"]
        self.assertEqual(t3_devices.heads(cards), [])
        self.assertEqual(t3_devices.capture(cards[0]), {"device": "video1"})
        self.assertEqual(t3_devices.split("cjk@card1"), ("cjk", "@card1"))
        self.assertEqual(t3_devices.split("cjk"), ("cjk", ""))

    def test_a_case_on_another_output_has_its_own_golden(self):
        root = t3_evaluate.GOLDEN
        self.assertEqual(t3_evaluate.golden_for("two-outputs-mirror", "cjk@1", root)[0],
                         os.path.join(root, "two-outputs-mirror", "cjk@1.png"))
        self.assertEqual(t3_evaluate.golden_for("two-outputs-mirror", "vt-restore@1", root)[0],
                         os.path.join(root, "two-outputs-mirror", "cjk@1.png"))
        self.assertEqual(t3_evaluate.same_as("vt-restore@card1"), "cjk@card1")
        self.assertEqual(t3_evaluate.same_as("vt-restore"), "cjk")
        caption, wrong = t3_evaluate.describe("cjk@1")
        self.assertEqual(caption, t3_evaluate.describe("cjk")[0] + " Captured from the output 1.")
        self.assertEqual(wrong, t3_evaluate.describe("cjk")[1])
        self.assertEqual(t3_evaluate.screen_text("cjk@card1", LOG),
                         t3_evaluate.screen_text("cjk", LOG))
        for check in t3_devices.CHECKS:
            self.assertEqual(t3_evaluate.golden_for("usb-keyboards", check), (None, None))
            self.assertEqual(t3_evaluate.describe(check), (t3_devices.CHECKS[check], None))

    def test_a_stray_golden_on_another_output(self):
        # A golden for an output the variant does not capture is refused.
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "two-outputs-mirror"))
            for name in ("cjk@1", "cjk@2"):
                t3img.write_png(os.path.join(d, "two-outputs-mirror", name + ".png"),
                                t3img.Image(1, 1, b"\0\0\0"))
            cases = {"two-outputs-mirror": t3_scenario.variant_cases("two-outputs-mirror")}
            self.assertEqual(t3_evaluate.check_goldens(cases, d),
                             ["%s is a golden no case draws"
                              % t3_common.rel(os.path.join(d, "two-outputs-mirror", "cjk@2.png"))])

    def test_qemu_connects_the_second_head(self):
        class Started(Exception):
            pass

        def popen(args, **kwargs):
            raise Started(args)

        with mock.patch.object(t3_qemu.subprocess, "Popen", popen), \
                tempfile.TemporaryDirectory() as d:
            with self.assertRaises(Started) as cm:
                t3_qemu.Qemu(d, [], "virtio-vga,id=video0,max_outputs=2", lambda msg: None,
                             heads=[("video0", 1, (640, 480))])
            args = cm.exception.args[0]
            sockets = os.path.dirname(args[args.index("-qmp") + 1][len("unix:"):].split(",")[0])
            self.assertIn("unix:%s,id=vnc-video0-1,display=video0,head=1"
                          % os.path.join(sockets, "vnc1.sock"), args)
            with self.assertRaises(Started) as cm:
                t3_qemu.Qemu(d, [], "virtio-vga", lambda msg: None)
            self.assertNotIn("-vnc", cm.exception.args[0])

    def test_screendump_names_the_output(self):
        seen = []
        vm = t3_qemu.Qemu.__new__(t3_qemu.Qemu)
        vm.capture = "screendump"

        def cmd(name, **arguments):
            seen.append((name, {k: v for k, v in arguments.items() if k != "filename"}))
            t3_qemu.write_ppm(arguments["filename"], t3img.Image(1, 1, b"\0\0\0"))

        vm.cmd = cmd
        with tempfile.TemporaryDirectory() as d:
            vm.screendump(os.path.join(d, "a.ppm"))
            vm.screendump(os.path.join(d, "b.ppm"), {"device": "video0", "head": 1})
        self.assertEqual(seen, [("screendump", {"format": "ppm"}),
                                ("screendump", {"format": "ppm", "device": "video0",
                                                "head": 1})])

    def rfb_server(self, path, status, got):
        """A VNC server on `path` that takes one SetDesktopSize and answers
        with ExtendedDesktopSize's `status`, putting what it read in `got`."""
        srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        srv.bind(path)
        srv.listen(1)

        def recv(c, n):
            buf = b""
            while len(buf) < n:
                buf += c.recv(n - len(buf))
            return buf

        def serve():
            c, _ = srv.accept()
            with c:
                c.sendall(b"RFB 003.008\n")
                recv(c, 12)
                c.sendall(b"\x01\x01")
                recv(c, 1)
                c.sendall(struct.pack(">I", 0))
                recv(c, 1)
                c.sendall(struct.pack(">HH", 640, 480) + bytes(16) + struct.pack(">I", 1) + b"q")
                recv(c, 20)  # SetPixelFormat
                _, n = struct.unpack(">BxH", recv(c, 4))
                got["encodings"] = list(struct.unpack(">%di" % n, recv(c, 4 * n)))
                # a notice first, then the answer to the request
                c.sendall(b"\x00\x00" + struct.pack(">H", 2)
                          + struct.pack(">HHHHi", 0, 0, 640, 480, -308) + b"\x01" + bytes(3)
                          + bytes(16)
                          + struct.pack(">HHHHi", 1, status, 640, 480, -308) + b"\x00" + bytes(3))
                kind, w, h, screens = struct.unpack(">BxHHBx", recv(c, 8))
                got["request"] = (kind, w, h, screens, struct.unpack(">IHHHHI", recv(c, 16)))
            srv.close()

        t = threading.Thread(target=serve)
        t.start()
        return t

    def test_vnc_asks_for_the_size(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "vnc1.sock")
            got = {}
            t = self.rfb_server(path, 4, got)
            t3_qemu.vnc_set_size(path, 320, 240)
            t.join()
            self.assertEqual(got["encodings"], [0, -308])
            self.assertEqual(got["request"], (251, 320, 240, 1, (0, 0, 0, 320, 240, 0)))
            os.unlink(path)
            t = self.rfb_server(path, 3, {})
            with self.assertRaisesRegex(RuntimeError, "refused 320x240: status 3"):
                t3_qemu.vnc_set_size(path, 320, 240)
            t.join()

    def test_each_output_is_captured_from_itself(self):
        # A fake VM whose every output shows its own steady screen, recording
        # what each screendump asked for: each capture of a case on another
        # output names that output's device and head.
        class Vm:
            def __init__(self):
                self.seen = []

            def screendump(self, path, output=None):
                self.seen.append((os.path.basename(path), output))
                key = repr(sorted((output or {}).items())).encode()
                img = t3img.Image(1, 1, hashlib.sha256(key).digest()[:3])
                t3_qemu.write_ppm(path, img)
                return img

        for variant, want in (("two-outputs-mirror", {"device": "video0", "head": 1}),
                              ("two-cards-mirror", {"device": "video1"})):
            with self.subTest(variant), tempfile.TemporaryDirectory() as d, \
                    mock.patch.object(t3_qemu.time, "sleep"):
                vm, caps = Vm(), {}
                outputs = t3_scenario.VARIANTS[variant]["outputs"]
                lasts = {o["suffix"]: t3img.Image(1, 1, b"\0\0\0") for o in outputs}
                t3_scenario.capture_outputs(vm, outputs, d, "cjk", caps, lasts)
                name = "cjk" + outputs[0]["suffix"]
                self.assertEqual(caps, {name: os.path.join(d, name + ".ppm")})
                self.assertEqual(sorted({(p, repr(o)) for p, o in vm.seen}),
                                 [(name + ".ppm", repr(want))])

    def test_a_display_for_each_output(self):
        # One Display line a display: a variant whose console set up fewer
        # displays than it has outputs fails, and its captures are dropped.
        line = "Display %s with backend [drm2d] text renderer [bbulk], cell size 8x16"
        outputs = t3_scenario.VARIANTS["two-outputs-mirror"]["outputs"]
        caps, rss = {"cjk": "a"}, {"cjk": 1}
        vtium = {"name": "vtium", "revision": "0.1"}
        both = [line % "Virtual-1", line % "Virtual-2"]
        self.assertEqual(t3_scenario.variant_errors(None, both, "bbulk", caps, rss, vtium,
                                                    ["cjk"], outputs), ([], False))
        self.assertEqual(t3_scenario.variant_errors(None, both[:1], "bbulk", caps, rss, vtium,
                                                    ["cjk"], outputs),
                         (["the console set up 1 display(s) (Virtual-1), not one for each of "
                           "the 2 outputs"], True))
        # the second console on tty3 logs the same displays again
        self.assertEqual(t3_scenario.variant_errors(None, both + both, "bbulk", caps, rss,
                                                    vtium, ["cjk"], outputs), ([], False))
        # a variant with one output is not counted
        self.assertEqual(t3_scenario.variant_errors(None, both, "bbulk", caps, rss, vtium,
                                                    ["cjk"]), ([], False))

    def ppm(self, d, name, data):
        path = os.path.join(d, name + ".ppm")
        t3_qemu.write_ppm(path, t3img.Image(1, 1, data))
        return path

    def test_mirror(self):
        variant = "two-outputs-mirror"
        cases = [c for c in t3_scenario.variant_cases(variant)
                 if c not in t3_evaluate.NO_GOLDEN]
        with tempfile.TemporaryDirectory() as d:
            caps = {c: self.ppm(d, c, b"\1\2\3") for c in cases}
            errors = {}
            t3_scenario.check_mirror(variant, caps, errors)
            self.assertEqual((errors, caps["mirror"]), ({}, caps["cjk@1"]))
            # one output drew one screen differently
            caps = {c: self.ppm(d, c, b"\0\0\0" if c == "sgr@1" else b"\1\2\3") for c in cases}
            t3_scenario.check_mirror(variant, caps, errors)
            self.assertEqual(errors, {"mirror": "differs from the first output's capture: sgr@1"})
            self.assertNotIn("mirror", caps)
            # a screen the other output never showed
            del caps["cjk@1"]
            errors = {}
            t3_scenario.check_mirror(variant, caps, errors)
            self.assertEqual(errors, {"mirror": "not captured on every output: cjk@1"})
            errors = {}
            t3_scenario.check_mirror(variant, {}, errors)
            self.assertEqual(errors, {"mirror": "no screen was captured"})

    # The guest's account of the examples (screens/udev-examples.sh), as it
    # prints it on a guest whose udev reads them as the record writes them.
    UDEV = ["T3-RULES-VERIFY ok",
            "T3-UDEV pci-0000:15:00.0-usb-0:3.2:1.0-event-kbd VTIUM_HEAD=panel",
            "T3-UDEV platform-i8042-serio-0-event-kbd VTIUM_ROLE=token",
            "T3-HWDB-BUILD ok",
            "T3-HWDB evdev:input:b0003vBBBBp0001e0110 VTIUM_ROLE=token",
            "T3-HWDB evdev:input:b0003vAAAAp0002e0110 VTIUM_ROLE=numpad",
            "T3-HWDB evdev:input:b0003vAAAAp0003e0110",
            "T3-DONE udev-examples"]

    def judge_udev(self, lines):
        return t3_devices.judge_udev(*t3_devices.parse_udev(lines))

    def test_udev_examples(self):
        self.assertIsNone(self.judge_udev(self.UDEV))
        wrong = {
            "the panel keyboard untouched": (
                lambda l: l.replace(" VTIUM_HEAD=panel", ""),
                "udevadm test gave pci-0000:15:00.0-usb-0:3.2:1.0-event-kbd nothing, not "
                "['VTIUM_HEAD=panel']"),
            # the installed fragment read by no rule of udev's, as a prefix
            # udev never looks up leaves it
            "the PS/2 keyboard unmarked": (
                lambda l: l.replace(" VTIUM_ROLE=token", "") if "i8042" in l else l,
                "udevadm test gave platform-i8042-serio-0-event-kbd nothing, not "
                "['VTIUM_ROLE=token']"),
            "another keyboard marked": (
                lambda l: l + " VTIUM_IGNORE=1" if "i8042" in l else l,
                "udevadm test gave platform-i8042-serio-0-event-kbd ['VTIUM_ROLE=token', "
                "'VTIUM_IGNORE=1'], not ['VTIUM_ROLE=token']"),
            "no PS/2 keyboard": (
                lambda l: None if "i8042" in l else l,
                "no PS/2 keyboard for the hwdb fragment to mark"),
            "no keyboard in the port": (
                lambda l: None if "usb-0:3.2" in l else l,
                "no keyboard in the port the example rule names"),
            "the token unmarked": (
                lambda l: l.replace(" VTIUM_ROLE=token", "") if "T3-HWDB " in l else l,
                "the hwdb gave evdev:input:b0003vBBBBp0001e0110 nothing, not "
                "['VTIUM_ROLE=token']"),
            "a device it does not name marked": (
                lambda l: l + " VTIUM_ROLE=numpad" if "p0003" in l else l,
                "the hwdb gave evdev:input:b0003vAAAAp0003e0110 ['VTIUM_ROLE=numpad'], not "
                "nothing"),
            "a query missing": (
                lambda l: None if "p0002" in l else l,
                "the guest did not query the hwdb for evdev:input:b0003vAAAAp0002e0110"),
            "the build complained": (
                lambda l: "T3-HWDB-BUILD vtium.hwdb:3: Property or empty line expected"
                if l.startswith("T3-HWDB-BUILD") else l,
                "building the hwdb fragment printed: vtium.hwdb:3: Property or empty line "
                "expected"),
            "no build": (
                lambda l: None if l.startswith("T3-HWDB-BUILD") else l,
                "the guest did not build the hwdb fragment"),
            # a misspelled key in a rule udevadm test never reaches
            "a rule udev refuses": (
                lambda l: "T3-RULES-VERIFY /usr/share/doc/vtium/examples/71-vtium-local.rules:17 "
                          "Invalid key 'ATTRZ'." if l.startswith("T3-RULES-VERIFY") else l,
                "udevadm verify on the example rules printed: "
                "/usr/share/doc/vtium/examples/71-vtium-local.rules:17 Invalid key 'ATTRZ'."),
            "verify failed silently": (
                lambda l: "T3-RULES-VERIFY" if l.startswith("T3-RULES-VERIFY") else l,
                "udevadm verify on the example rules printed: nothing, and failed"),
            "no verify": (
                lambda l: None if l.startswith("T3-RULES-VERIFY") else l,
                "the guest did not verify the example rules"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                lines = [m for m in (change(l) for l in self.UDEV) if m is not None]
                got = self.judge_udev(lines)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    def test_the_examples_name_the_ports_the_guest_has(self):
        # The keyboard the example rule names is the one tier 3 plugs into
        # that port, and the hotplugged one sits beside it on the same hub.
        examples = os.path.join(t3_common.ROOT, "crates", "vtium", "install", "examples")
        with open(os.path.join(examples, "71-vtium-local.rules")) as f:
            rule = next(l for l in f if "VTIUM_HEAD" in l and not l.startswith("#"))
        port = re.search(r'ENV\{ID_PATH\}=="([^"*]+)\*"', rule).group(1)
        self.assertEqual(port, "pci-0000:15:00.0-usb-0:3.2")
        self.assertIn("usb-kbd,id=kbd32,bus=%s,port=3.2" % t3_devices.USB_BUS, t3_devices.USB)
        self.assertIn("usb-hub,id=hub3,bus=%s,port=3" % t3_devices.USB_BUS, t3_devices.USB)
        self.assertIn(port + ":", "pci-0000:15:00.0" + t3_devices.PANEL_PORT)
        self.assertEqual(t3_devices.HOT_NAME, "pci-0000:15:00.0-usb-0:%s:1.0-event-kbd"
                         % t3_devices.HOT_PORT)
        # udev's own prefix, which its 60-evdev.rules looks up; the token's
        # entry is the one guest/setup changes to the PS/2 keyboard
        with open(os.path.join(examples, "vtium.hwdb")) as f:
            entries = [l.strip() for l in f if not l.startswith(("#", " ")) and l.strip()]
        self.assertEqual(entries, ["evdev:input:b0003vBBBBp0001*", "evdev:input:b0003vAAAAp0002*"])
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        self.assertIn("s/^evdev:input:b0003vBBBBp0001\\*$/evdev:input:b0011v0001p0001*/", setup)

    def test_the_examples_are_installed_as_documentation(self):
        # make install-tree puts both under /usr/share/doc/vtium/examples;
        # that the tree holds nothing where udev reads is t3_impl.tree's
        # check, on the tree every run builds (ImplementationTest).
        r = subprocess.run(["make", "-n", "--no-print-directory", "install-tree", "DESTDIR=/D"],
                           cwd=t3_common.ROOT, capture_output=True, text=True, check=True)
        lines = [l for l in r.stdout.splitlines() if l.startswith(("install ", "tic "))]
        self.assertIn("install -Dm644 -t /D/usr/share/doc/vtium/examples "
                      "crates/vtium/install/examples/71-vtium-local.rules "
                      "crates/vtium/install/examples/vtium.hwdb", lines)

    def test_the_tree_stages_the_frits_and_the_gallery_with_images(self):
        # make install-tree stages what make install does (ADR-1500 D8): the
        # frits, which change nothing until a look names one, and with images
        # the gallery tool. It stages no head file: the heads are sections
        # of vtium.conf, whose example the tree holds (ADR-1403 D6).
        def make_n(*args):
            r = subprocess.run(["make", "-n", "--no-print-directory", "install-tree", "DESTDIR=/D", *args],
                               cwd=t3_common.ROOT, capture_output=True, text=True, check=True)
            return r.stdout.splitlines()
        lines = make_n()
        frits = "crates/vtium-config/frits"
        self.assertIn(f"install -Dm644 -t /D/etc/vtium/fonts/console {frits}/console/terminus.vtfrit "
                      f"{frits}/console/cullet-mono.vtfrit && "
                      f"install -Dm644 -t /D/etc/vtium/fonts/code {frits}/code/jetbrains-mono.vtfrit && "
                      f"install -Dm644 -t /D/etc/vtium/fonts/small {frits}/small/unifont-icons.vtfrit "
                      f"{frits}/small/unscii-icons.vtfrit && "
                      f"install -Dm644 -t /D/etc/vtium/fonts/layers {frits}/layers/nerd-symbols.vtlayer "
                      f"{frits}/layers/nerd-symbols-small.vtlayer {frits}/layers/noto-emoji.vtlayer "
                      f"{frits}/layers/noto-emoji-small.vtlayer {frits}/layers/unifont.vtlayer", lines)
        self.assertFalse([l for l in lines if "heads.conf" in l], lines)
        self.assertIn("install -Dm644 crates/vtium/install/vtium.conf.example "
                      "/D/etc/vtium/vtium.conf.example", lines)
        self.assertFalse([l for l in lines if "vtium-gallery" in l], lines)
        self.assertIn("install -Dm755 tools/gallery/gallery /D/usr/bin/vtium-gallery", make_n("FEATURES=images"))

    def test_the_tree_installs_the_license_and_the_notices(self):
        # make install-tree installs LICENSE, COPYING and the three license
        # texts COPYING points to in vtium's license directory, as make
        # install does for a package. COPYING holds the notices for what
        # vtium builds in, such as Unicode's data tables, whose license asks
        # that its notice go with every copy, and Unifont, whose license, the
        # SIL Open Font License, asks that its text go with the font. The
        # font helper builds in tiny-skia, whose BSD-3-Clause license asks
        # that its text go with the binary. The license text of each color
        # scheme's family goes in its palettes folder (ADR-1205 D10).
        r = subprocess.run(["make", "-n", "--no-print-directory", "install-tree", "DESTDIR=/D"],
                           cwd=t3_common.ROOT, capture_output=True, text=True, check=True)
        lines = [l for l in r.stdout.splitlines() if "/D/usr/share/licenses" in l]
        notices = sorted(os.listdir(os.path.join(t3_common.ROOT, "tools", "cullet-mono", "notices")))
        self.assertEqual(lines, ["install -Dm644 -t /D/usr/share/licenses/vtium/cullet-mono "
                                 + " ".join(f"tools/cullet-mono/notices/{n}" for n in notices),
                                 "install -Dm644 -t /D/usr/share/licenses/vtium/palettes "
                                 + " ".join(f"crates/vtium/install/palettes/licenses/{n}-LICENSE"
                                            for n in ("catppuccin", "dracula", "gruvbox", "tokyo-night")),
                                 "install -Dm644 -t /D/usr/share/licenses/vtium LICENSE COPYING "
                                 "crates/vtium-font/data/OFL-1.1.txt "
                                 "crates/vtium-term/gen-color-names/src/rgb-license.txt "
                                 "crates/vtium-config/licenses/tiny-skia-LICENSE"])

    def test_the_tree_installs_vtiums_own_units(self):
        # make install-tree generates vtium@.service and vtium-getty@.service
        # with the generator and the changes make install uses (ADR-1500
        # D3), with the console at /usr/bin/vtium, and installs no unit
        # under kmscon's names.
        def make_n(target, *args):
            r = subprocess.run(["make", "-n", "--no-print-directory", target, *args],
                               cwd=t3_common.ROOT, capture_output=True, text=True, check=True)
            return r.stdout.splitlines()
        tree = make_n("install-tree", "DESTDIR=/D")
        units = [l for l in tree if "tools/vt/units.sh" in l]
        self.assertEqual(units, ["tools/vt/units.sh --name vtium --exec /usr/bin/vtium "
                                 "--by 'make install-tree' --conflicts 'kmsconvt@%i.service' "
                                 "/D/usr/lib/systemd/system"])
        package = [l for l in make_n("install", "DESTDIR=/D", "PREFIX=/usr")
                   if "tools/vt/units.sh" in l]
        self.assertEqual([l.replace("'make install'", "'make install-tree'") for l in package],
                         units)
        self.assertFalse([l for l in tree if "kmsconvt@.service" in l
                          or "kmsconvt-getty@" in l], tree)
        self.assertEqual(t3_impl.VT_UNIT, "usr/lib/systemd/system/vtium@.service")

    def test_the_guest_gives_vtium_its_vts_with_vtium_console(self):
        # The run's setup links the template back to the getty, as Arch's
        # systemd package has it, then gives vtium every VT as a user does,
        # with vtium console enable --direct --all, or --seat --all for a
        # variant in seat mode, and fails the install when that fails. No
        # vtium unit is named for kmscon.
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        enable = 'vtium console enable --"$mode" --all'
        getty = ("ln -sfn /usr/lib/systemd/system/getty@.service "
                 "/etc/systemd/system/autovt@.service\n")
        self.assertIn(getty, setup)
        self.assertLess(setup.index(getty), setup.index(enable))
        self.assertIn("if ! out=$(" + enable + " 2>&1); then\n"
                      "    printf 'TIER3-INSTALL-FAIL vtium console enable --%s --all: %s\\n' "
                      "\"$mode\" \"$out\" > /dev/ttyS0\n    exit 1\nfi\n", setup)
        self.assertLess(setup.index("tar -xpf"), setup.index(enable))
        self.assertIn("mode=direct\n", setup)
        self.assertIn('[ "$mode" != seat ] || unit=vtium-seat@.service\n', setup)
        self.assertIn("/etc/systemd/system/$unit.d/tier3.conf", setup)
        self.assertFalse([l for l in setup.splitlines()
                          if "kmsconvt" in l and not l.lstrip().startswith("#")])

    def test_keyboard_hotplug_helpers(self):
        self.assertEqual(t3_devices.key_events(["shift", "a"], True),
                         [{"type": "key", "data": {"down": True,
                                                   "key": {"type": "qcode", "data": "shift"}}},
                          {"type": "key", "data": {"down": True,
                                                   "key": {"type": "qcode", "data": "a"}}}])
        self.assertEqual(t3_devices.listed(["T3-KBD a-event-kbd /dev/input/event2",
                                            "T3-RSS x 1", "T3-KBD b /dev/input/event1"]),
                         {"a-event-kbd": "/dev/input/event2", "b": "/dev/input/event1"})
        self.assertEqual(t3_devices.i8042(["T3-KBD a /dev/input/event1", "T3-I8042 41"]), 41)
        self.assertIsNone(t3_devices.i8042(["T3-I8042 "]))
        m = t3_devices.KEYBOARD_LINE.search(
            "[ 6.4] vtium[411]: input: using /dev/input/event6 [QEMU QEMU USB Keyboard] as "
            "keyboard\n")
        self.assertEqual(m.groups(), ("/dev/input/event6", "QEMU QEMU USB Keyboard"))
        self.assertIsNone(t3_devices.KEYBOARD_LINE.search(
            "input: using /dev/input/event4 [VirtualPS/2 VMware VMMouse] as Virtual mouse"))
        # vtium names the vendor and product ids a device reports after its name.
        m = t3_devices.KEYBOARD_LINE.search(
            "[ 7.3] vtium[556]: input: using /dev/input/event2 [QEMU QEMU USB Keyboard] "
            "0627:0001 as keyboard\n")
        self.assertEqual(m.groups(), ("/dev/input/event2", "QEMU QEMU USB Keyboard"))
        self.assertIsNone(t3_devices.KEYBOARD_LINE.search(
            "input: using /dev/input/event4 [VirtualPS/2 VMware VMMouse] 0002:0013 as "
            "Virtual mouse"))

    # The guest's account of each keyboard-hotplug run (screens/keyboard-hotplug.sh).
    PS2 = "T3-KBD platform-i8042-serio-0-event-kbd /dev/input/event1"
    HOT = "T3-KBD %s /dev/input/event6" % t3_devices.HOT_NAME

    def hotplug(self, control=(40, 52), plugged=(HOT, 60), typed=(HOT, 60),
                pulled=(None, 72), took=("/dev/input/event6",)):
        run = lambda kbd, n: [self.PS2] + ([kbd] if kbd else []) + ["T3-I8042 %d" % n,
                                                                    "T3-DONE keyboard-hotplug"]
        return t3_devices.judge_hotplug([run(None, n) for n in control], run(*plugged),
                                        run(*typed), run(*pulled), list(took))

    def test_keyboard_hotplug(self):
        self.assertIsNone(self.hotplug())
        wrong = {
            # the console's keys never reached the PS/2 keyboard
            "no control": (dict(control=(40, 40)), "typing on the PS/2 keyboard did not raise "
                           "its interrupt count (40, then 40)"),
            # the keys sent to the plugged-in keyboard's console came from the PS/2 one
            "typed on the console": (dict(typed=(self.HOT, 64)), "typing on the plugged-in keyboard "
                                     "moved the PS/2 keyboard's interrupt count from 60 to 64"),
            "not listed": (dict(plugged=(None, 60)), "plugged in, the guest lists"),
            # vtium logged another keyboard after the plug, not the new one
            "another node": (dict(took=("/dev/input/event1",)), "vtium took /dev/input/event1 "
                             "after the plug, not /dev/input/event6"),
            "nothing taken": (dict(took=()), "vtium took no keyboard after the plug"),
            "still listed": (dict(pulled=(self.HOT, 72)), "pulled out, the guest still lists"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                got = self.hotplug(**change)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)


# Jobs the guest's boot leaves queued for good, and its timers
# (guest/t3-agent).
BOOT_JOBS = ["systemd-time-wait-sync.service", "multi-user.target"]
BOOT_TIMERS = ["systemd-tmpfiles-clean.timer"]


def agent_state(vtium=("active", "success"), fallback=("inactive", "success", "no", 0),
                getty="inactive", queued=(), marker=True, tty2="", logins=0, fg=2, i8042=15,
                pid=(410, 3.0), gettys=(), timers=(), sessions=(("1", "tty1"),),
                journal=("started",), restarts=0, systemd=262):
    """The agent's answer to `state` (guest/t3-agent), as lines. `gettys`
    gives a getty unit's (left inactive at, restarts) in place of (0, 0),
    or None for no reading of it; `journal` gives the words systemd logged
    of vtium's unit and its fallback, or None for no journal line."""
    readings = dict({u: (0, 0) for u in t3_heads.GETTY_UNITS}, **dict(gettys))
    lines = (["systemd %d" % systemd] if systemd else []) + [
             "unit vtium@tty2.service %s %s yes 6418546" % vtium,
             "restarts vtium@tty2.service running %d" % restarts,
             *(["journal " + " ".join(journal)] if journal is not None else []),
             "unit vtium-getty@tty2.service %s %s %s %d" % fallback,
             "unit getty@tty2.service %s success no 0" % getty,
             *("getty %s %d %d" % (u, *r) for u, r in readings.items() if r is not None),
             *("job %s start waiting" % u for u in BOOT_JOBS + list(queued)),
             *("timer %s waiting" % t for t in BOOT_TIMERS + list(timers)),
             *("session %s %s" % s for s in sessions),
             "marker %s" % ("yes" if marker else "no"), "tty2 %s" % tty2,
             "logins %d" % logins, "fgconsole %d" % fg, "i8042 %d" % i8042]
    return lines + (["vtium %d %.1f" % pid] if pid else [])


class HeadsTest(unittest.TestCase):
    """The head-file variants (t3_heads.py): their machines and cases, the
    disc that carries the head file and the agent, the agent's answers and
    each check's verdict on them, both ways."""

    CASES = {
        "heads-two-heads": ["cjk", "vt-switch", "vt-restore", "two-heads"],
        "heads-partly-open": ["partly-notice", "cjk", "vt-switch", "vt-restore",
                              "getty-fallback", "start-limit"],
        "heads-closed-broken": ["panic", "no-login", "getty-fallback", "start-limit"],
        "heads-policy-typo": ["policy-panic", "policy-line", "no-login", "getty-fallback"],
        "heads-closed-unmatched": ["no-login", "getty-fallback"],
        "heads-off": ["no-login", "getty-fallback"],
        "heads-kiosk": ["cjk", "kiosk"],
        "heads-closed-nodisplay": ["display-held", "getty-fallback"],
        "heads-open-nodisplay": ["display-exit"],
        "heads-open-away": ["display-away"],
        "heads-survive": ["session-survives"],
    }

    def test_variants(self):
        self.assertEqual(t3_scenario.HEAD_VARIANTS, list(self.CASES))
        for name, want in self.CASES.items():
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                self.assertEqual(t3_scenario.variant_cases(name), want)
                self.assertEqual((v["cpus"], v["memory_mb"]), (1, 512))
                self.assertIn(name, t3_scenario.golden_cases())
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                self.assertTrue(os.path.isfile(t3_heads.head_file(v)), v["heads"])
                with open(t3_heads.head_file(v)) as f:
                    text = f.read()
                if "policy_line" in v:
                    # a policy line vtium cannot read, alone among the
                    # file's policy lines, makes the file closed
                    self.assertEqual(v["policy"], "closed")
                    self.assertEqual(re.findall(r"(?mi)^\s*policy.*$", text),
                                     [v["policy_line"]])
                    self.assertNotRegex(v["policy_line"], r"^policy = (open|closed)\b")
                else:
                    self.assertRegex(text, r"(?m)^policy = %s\b" % v["policy"])
                # every check that fails vtium names its failures, each known
                if "getty-fallback" in want:
                    self.assertTrue(set(v["failures"]) <= set(t3_heads.FAILURES))
                # a console that gives no login is read through the agent
                if not v.get("login", True) or "getty-fallback" in want:
                    self.assertTrue(v["agent"])
        # the no-display guests have no display device and wait 5 s
        for name in ("heads-closed-nodisplay", "heads-open-nodisplay"):
            v = t3_scenario.VARIANTS[name]
            self.assertIsNone(v["device"])
            self.assertIsNone(v["renderer"])
            self.assertEqual(t3_heads.display_wait(v), 5)
        self.assertEqual(t3_scenario.VARIANTS["heads-open-nodisplay"]["heads"],
                         "docs/heads/default.conf")
        # the display-away guest has no display device either, so vtium's
        # first frame never comes, and waits 15 s under the shipped open file
        away = t3_scenario.VARIANTS["heads-open-away"]
        self.assertIsNone(away["device"])
        self.assertIsNone(away["renderer"])
        self.assertEqual(t3_heads.display_wait(away), 15)
        self.assertEqual((away["heads"], away["policy"]), ("docs/heads/default.conf", "open"))
        self.assertEqual(t3_scenario.VARIANTS["heads-off"]["heads"], "docs/heads/off.conf")

    def test_every_check_runs(self):
        # run_checks runs each check a variant names from CHECK_RUNS, so a
        # check missing there would end the variant's run in the VM
        for name, v in t3_scenario.VARIANTS.items():
            with self.subTest(name):
                self.assertLessEqual(set(v.get("checks", [])), set(t3_scenario.CHECK_RUNS))

    def test_policy_line(self):
        # The console's log shows the vtium.conf whose policy line vtium
        # cannot read as closed and broken: that line's error with the
        # installed file's path and the line's number in the whole file the
        # guest holds, and no outcome that runs any of the file.
        v = t3_scenario.VARIANTS["heads-policy-typo"]
        lines = t3_heads.guest_conf(v).splitlines()
        # the variant's configuration comes first, so the head file's policy
        # line is line 18 of the guest's vtium.conf, not line 9 of its own,
        # below the line of a version vtium does not know
        self.assertEqual(lines[16:18], ["version = 2", "policy = opne"])
        logged = ("[    9.1] vtium[502]: /etc/vtium/vtium.conf:18: `policy` cannot be "
                  "\"opne\", so vtium cannot read this line and counts the file as closed "
                  "and broken.\n"
                  "[    9.1] vtium[502]: /etc/vtium/vtium.conf:17: vtium knows only "
                  "`version = 1`, not version \"2\".\n"
                  "[    9.2] vtium[502]: vtium.conf has errors and counts as closed: all input is off.\n")
        self.assertIsNone(t3_heads.judge_policy_line(logged, lines, v["policy_line"]))
        # the error names another line, another file, or none is logged
        for wrong in (logged.replace("vtium.conf:18:", "vtium.conf:9:"),
                      logged.replace("cannot be \"opne\"", "cannot be \"open\""),
                      logged.replace("/etc/vtium/vtium.conf:18", "/etc/vtium/heads.conf:18"),
                      logged.replace("/etc/vtium/vtium.conf:18", "vtium.conf:18")):
            with self.subTest(wrong=wrong):
                self.assertRegex(t3_heads.judge_policy_line(wrong, lines, v["policy_line"]),
                                 "logged no error '/etc/vtium/vtium.conf:18: `policy` "
                                 "cannot be \"opne\"'")
        # the file run, partly broken under either policy, or the shipped
        # default in its place
        self.assertEqual(t3_heads.RUN_LINES, ("vtium.conf has errors: running the parts without errors.",
                                              "vtium.conf not used: running the built-in configuration."))
        for line in t3_heads.RUN_LINES:
            with self.subTest(line=line):
                self.assertRegex(t3_heads.judge_policy_line(
                    logged + "[    9.3] vtium[502]: %s\n" % line, lines, v["policy_line"]),
                    "an outcome that runs some of the file or the shipped default")
        with self.assertRaisesRegex(ValueError, "holds no line"):
            t3_heads.judge_policy_line(logged, lines, "policy = maybe")

    def test_broken_closed_is_tier_2s_file(self):
        # The guest's broken closed file is tier 2's BROKEN_CLOSED, so the
        # panic screen tier 3 captures reports the errors tier 2 draws, each
        # on its line of the guest's vtium.conf, below the variant's keys.
        with open(os.path.join(t3_common.ROOT, "tools", "tier2", "src", "notices.rs")) as f:
            m = re.search(r'const BROKEN_CLOSED: &str = "(.*)";', f.read())
        with open(os.path.join(t3_heads.HEADS, "broken-closed.conf")) as f:
            self.assertEqual(f.read(), m.group(1).replace("\\n", "\n"))

    def test_unmatched_leaves_its_display_off(self):
        # A valid closed file whose one head names a connector the card
        # lacks leaves the guest's display blank (ADR-1401 D3, D9): vtium
        # sets up no display and shows no first screen, and each vtium
        # started again must log that the display stays off.
        v = t3_scenario.VARIANTS["heads-closed-unmatched"]
        self.assertEqual((v["policy"], v["renderer"], v["off"]), ("closed", None, "Virtual-1"))
        self.assertNotIn("first", v)
        self.assertNotIn("No head has a connected display.", t3_heads.LINES.values())
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        vm = mock.Mock(serial=os.path.join(d.name, "serial.log"))
        old = ("[    7.7] vtium[774]: display Virtual-1 stays off: no rule in vtium.conf "
               "matches it; add the rule `display connector:pci-0000:00:01.0/Virtual-1 -> "
               "desk` to give it a head\n")
        new = old.replace("[    7.7] vtium[774]", "[   23.1] vtium[1043]")
        with open(vm.serial, "w") as f:
            f.write(old + new)
        t3_heads.wait_off(v, vm, 774, 0, "kill")
        # the line of the vtium that was killed, or of another display, is
        # not the new vtium's
        for serial in (old, new.replace("Virtual-1", "Virtual-2"),
                       new.replace("no rule in vtium.conf matches it",
                                   "the rule on line 4 of vtium.conf sends it to off")):
            with self.subTest(serial=serial):
                with open(vm.serial, "w") as f:
                    f.write(serial)
                with mock.patch.object(t3_heads, "STATE_TIMEOUT", 0), \
                        mock.patch.object(t3_heads.time, "sleep"):
                    with self.assertRaisesRegex(RuntimeError, "the vtium started again after the "
                                                "kill did not log 'display Virtual-1 stays off"):
                        t3_heads.wait_off(v, vm, 774, 0, "kill")

    def test_the_panic_screens_have_a_golden_for_each_build(self):
        # The panic screen shows the maintainer's drawing of Tux as ASCII
        # art in a console built without images and as a sprite in one
        # built with images (ADR-1401 D7). The head variants run on the
        # run's tree, so each panic case has one golden for each build, and
        # the run's tree picks the one that judges it.
        self.assertEqual(t3_evaluate.BY_FEATURE, {"panic": "images", "policy-panic": "images"})
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        root = d.name
        for variant, case in (("heads-closed-broken", "panic"),
                              ("heads-policy-typo", "policy-panic")):
            with self.subTest(case):
                for features, name in (((), case), (["libseat", "logind"], case),
                                       (["images"], case + "+images"),
                                       (["gpu", "images"], case + "+images")):
                    self.assertEqual(t3_evaluate.golden_for(variant, case, root, features),
                                     (os.path.join(root, variant, name + ".png"),
                                      os.path.join(root, variant, name + ".tol")))
        # a screen BY_FEATURE does not name keeps its one golden
        self.assertEqual(t3_evaluate.golden_for("heads-partly-open", "partly-notice", root,
                                                ["images"])[0],
                         os.path.join(root, "heads-partly-open", "partly-notice.png"))
        # both goldens are ones a case compares with, and no other feature's
        cases = {"heads-closed-broken": t3_scenario.variant_cases("heads-closed-broken")}
        os.makedirs(os.path.join(root, "heads-closed-broken"))
        for name in ("panic", "panic+images", "panic+gpu"):
            t3img.write_png(os.path.join(root, "heads-closed-broken", name + ".png"),
                            t3img.Image(8, 8, bytes(8 * 8 * 3)))
        self.assertEqual(t3_evaluate.check_goldens(cases, root), [
            "%s is a golden no case draws"
            % t3_common.rel(os.path.join(root, "heads-closed-broken", "panic+gpu.png"))])
        # a capture is judged by the golden of its tree's build
        t3img.write_png(os.path.join(root, "heads-closed-broken", "panic+images.png"),
                        t3img.Image(8, 8, bytes([200]) + bytes(8 * 8 * 3 - 1)))
        cap = os.path.join(root, "cap.png")
        t3img.write_png(cap, t3img.Image(8, 8, bytes([200]) + bytes(8 * 8 * 3 - 1)))
        with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(root, "run")), \
                mock.patch.object(t3_evaluate, "PROVISIONAL", {}):
            sprite = t3_evaluate.evaluate("heads-closed-broken", "panic", cap, {}, LOG, root,
                                          features=["images"])
            ascii_art = t3_evaluate.evaluate("heads-closed-broken", "panic", cap, {}, LOG, root)
        self.assertEqual((sprite["status"], sprite["golden"]),
                         ("pass", t3_common.rel(os.path.join(root, "heads-closed-broken",
                                                             "panic+images.png"))))
        self.assertEqual((ascii_art["status"], ascii_art["golden"]),
                         ("fail", t3_common.rel(os.path.join(root, "heads-closed-broken",
                                                             "panic.png"))))
        # the run's tree reaches the evaluation, a failed variant's included
        entries = t3_scenario.failed_variant("heads-closed-broken", "x", LOG, ["images"])[0]
        self.assertEqual(entries[0]["golden"],
                         t3_common.rel(os.path.join(t3_evaluate.GOLDEN, "heads-closed-broken",
                                                    "panic+images.png")))

    def test_first_screens_and_checks_are_described(self):
        for name in t3_scenario.HEAD_VARIANTS:
            for case in t3_scenario.variant_cases(name):
                with self.subTest(case=case):
                    caption, _ = t3_evaluate.describe(case)
                    self.assertTrue(caption)
        for case in t3_heads.CHECKS:
            self.assertEqual(t3_evaluate.golden_for("heads-off", case), (None, None))
        for case in t3_heads.FIRST:
            self.assertIn(case, t3_heads.LINES)
            self.assertEqual(t3_evaluate.golden_for("heads-closed-broken", case,
                                                    t3_evaluate.GOLDEN)[0],
                             os.path.join(t3_evaluate.GOLDEN, "heads-closed-broken",
                                          case + ".png"))

    def test_disc(self):
        # The disc carries one vtium.conf, the variant's configuration with
        # its head file below it, and the agent with its unit; guest/setup
        # installs the file as root's and the agent, and no head file of its
        # own (ADR-1403 D6).
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        tree = os.path.join(d.name, "tree")
        write_tree(tree, ImplementationTest.TREE)
        impl = t3_impl.tree(tree, os.path.join(d.name, "out", "tree.tar"))
        impl["fonts"] = os.path.join(d.name, "fonts")
        os.makedirs(impl["fonts"])
        disc = os.path.join(d.name, "disc")
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("heads-closed-broken", d.name, impl)
            self.assertTrue({"vtium.conf", "t3-agent", "t3-agent.service"}
                            <= set(os.listdir(disc)))
            self.assertNotIn("heads.conf", os.listdir(disc))
            v = t3_scenario.VARIANTS["heads-closed-broken"]
            with open(os.path.join(disc, "vtium.conf")) as f, \
                    open(os.path.join(t3_common.GUEST, v["conf"])) as g, \
                    open(os.path.join(t3_heads.HEADS, "broken-closed.conf")) as h:
                self.assertEqual(f.read(), g.read() + h.read())
            # vtium's own keys stay above the first section
            with open(os.path.join(disc, "vtium.conf")) as f:
                lines = f.read().splitlines()
            first = next(i for i, l in enumerate(lines) if l.startswith("["))
            self.assertIn("font-engine=psf", lines[:first])
            self.assertEqual(lines[first - 2:first], ["version = 1", "policy = closed"])
            # a variant with no head file gets its configuration alone
            t3_scenario.build_disc("2d-640x480-psf16", d.name, impl)
            self.assertFalse({"heads.conf", "t3-agent"} & set(os.listdir(disc)))
            with open(os.path.join(disc, "vtium.conf")) as f, \
                    open(os.path.join(t3_common.GUEST, "vtium-small-8x16.conf")) as g:
                self.assertEqual(f.read(), g.read())
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        self.assertIn('install -Dm644 -o root -g root "$disc"/vtium.conf /etc/vtium/vtium.conf\n',
                      setup)
        self.assertNotIn("heads.conf", setup)
        self.assertIn("systemctl start --no-block t3-agent.service", setup)
        # the units the agent reads are the ones the tree installs
        with open(os.path.join(t3_common.GUEST, "t3-agent")) as f:
            agent = f.read()
        # vtium's unit is the template link's: vtium@ in direct mode,
        # vtium-seat@ in seat mode.
        self.assertIn('units=("$vtium_unit" "vtium-getty@$vt.service" '
                      '"getty@$vt.service")', agent)
        self.assertIn("vtium_unit=vtium@$vt.service\n", agent)
        self.assertIn("*/vtium-seat@.service) vtium_unit=vtium-seat@$vt.service ;;\n", agent)
        self.assertEqual(t3_heads.unit({"units": {t3_heads.SEAT_UNIT: {"active": "active"}}},
                                       t3_heads.VTIUM_UNIT), {"active": "active"})
        self.assertEqual(t3_heads.unit({"units": {t3_heads.SEAT_UNIT: {"active": "active"}}},
                                       t3_heads.GETTY_UNIT), {})
        self.assertIn('gettys=("%s" "%s")' % tuple(u.replace("tty2", "$vt")
                                                   for u in t3_heads.GETTY_UNITS), agent)
        # the keyboard `watch` reads is the one the kiosk check plugs in
        self.assertIn("second=/dev/input/by-path/%s\n" % t3_heads.KIOSK_SECOND, agent)
        self.assertEqual((t3_heads.VTIUM_UNIT, t3_heads.FALLBACK_UNIT, t3_heads.GETTY_UNIT),
                         ("vtium@tty2.service", "vtium-getty@tty2.service",
                          "getty@tty2.service"))
        with open(os.path.join(t3_common.ROOT, "crates", "vtium", "install",
                               "vtium-getty@.service")) as f:
            self.assertIn("ConditionPathExists=!/run/vtium/%i.closed\n", f.read())

    def test_qemu_gets_the_agents_port_and_no_display(self):
        class Started(Exception):
            pass

        def popen(args, **kwargs):
            raise Started(args)

        with mock.patch.object(t3_qemu.subprocess, "Popen", popen), \
                tempfile.TemporaryDirectory() as d:
            with self.assertRaises(Started) as cm:
                t3_qemu.Qemu(d, [], None, lambda msg: None, agent=True)
            args = cm.exception.args[0]
            self.assertEqual(args[args.index("-vga") + 1], "none")
            self.assertNotIn("-device", args)
            serials = [args[i + 1] for i, a in enumerate(args) if a == "-serial"]
            sockets = os.path.dirname(args[args.index("-qmp") + 1][len("unix:"):].split(",")[0])
            self.assertEqual(serials[2],
                             "unix:%s,server=on,wait=off" % os.path.join(sockets, "ttyS2"))
            with self.assertRaises(Started) as cm:
                t3_qemu.Qemu(d, [], "virtio-vga", lambda msg: None)
            args = cm.exception.args[0]
            self.assertEqual(len([a for a in args if a == "-serial"]), 2)
            self.assertNotIn("-vga", args)

    def test_ask(self):
        # A request goes out numbered on the agent's socket, and its answer
        # is the lines of that number that come back on the socket, up to
        # its end line; another request's lines, and lines written
        # anywhere else, are not its answer.
        vm = t3_qemu.Qemu.__new__(t3_qemu.Qemu)
        vm.proc = mock.Mock(**{"poll.return_value": None})
        vm.asked, vm.agent_buf = 0, b""
        ours, theirs = socket.socketpair()
        self.addCleanup(ours.close)
        self.addCleanup(theirs.close)
        vm.agent_sock = ours
        theirs.sendall(b"T3-AGENT 9 marker no\nT3-AGENT-END 9\n")
        theirs.sendall(b"T3-AGENT 1 marker yes\nT3-AGENT 1 job x.service start running\n"
                       b"T3-AGENT-END 1\n")
        self.assertEqual(vm.ask("state"), ["marker yes", "job x.service start running"])
        self.assertEqual(theirs.recv(100), b"1 state\n")
        with self.assertRaises(RuntimeError):
            vm._ask("state", 1)

    def start_agent(self, cwd, env=None):
        """The agent on a pty in place of ttyS2, run in `cwd` with `env` added
        to the environment, and a function that sends it request N and
        returns its answer's lines."""
        import pty
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        # held open here too, so the pty stays up until the agent opens it
        self.addCleanup(os.close, slave)
        agent = subprocess.Popen(["bash", os.path.join(t3_common.GUEST, "t3-agent")],
                                 cwd=cwd, stdin=subprocess.DEVNULL,
                                 env=dict(os.environ, T3_AGENT_PORT=os.ttyname(slave),
                                          **(env or {})))
        self.addCleanup(agent.wait)
        self.addCleanup(agent.kill)

        def ask(n, request, timeout=10):
            os.write(master, b"%d %s\n" % (n, request.encode()))
            buf = b""
            deadline = time.time() + timeout
            while ("T3-AGENT-END %d" % n).encode() not in buf:
                self.assertLess(time.time(), deadline, buf)
                buf += os.read(master, 4096)
            return [l.strip()[len("T3-AGENT %d " % n):] for l in buf.decode().splitlines()
                    if l.startswith("T3-AGENT %d " % n)]
        return ask

    def test_the_agent_prints_the_drm_state(self):
        # planes prints the kernel's DRM state from debugfs, each line kept
        # with its indent, below T3_AGENT_ROOT as in the guest; a guest with
        # no state file says so
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        dri = os.path.join(d.name, "sys", "kernel", "debug", "dri")
        os.makedirs(os.path.join(dri, "0"))
        state = "plane[31]: plane-0\n\tcrtc=crtc-0\n\tfb=40\n\t\tsize=64x64\n"
        with open(os.path.join(dri, "0", "state"), "w") as f:
            f.write(state)
        ask = self.start_agent(d.name, {"T3_AGENT_ROOT": d.name})
        self.assertEqual(ask(1, "planes"),
                         ["plane[31]: plane-0", "\tcrtc=crtc-0", "\tfb=40", "\t\tsize=64x64"])
        os.unlink(os.path.join(dri, "0", "state"))
        self.assertEqual(ask(2, "planes"),
                         ["no readable %s/*/state" % dri])

    def test_the_agent_refuses_every_other_request(self):
        # The agent on a pty in place of ttyS2: each request it does not
        # know, spelled even one character differently, is refused and runs
        # nothing.
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        ask = self.start_agent(d.name)
        requests = ["kill TERM", "kill KILL; touch pwned", "$(touch %s/pwned-sub)" % d.name,
                    "`touch %s/pwned-tick`" % d.name, "state ", "start now", "Start",
                    "hangup && touch pwned-and", "watch /dev/input/event0", "Watch",
                    "limit 99", "Limit", "reset-failed getty@tty2.service", "reset-failed;id",
                    "prelogin now", "Prelogin", "prelogin;id", "restart now", "Restart",
                    "restart;id", "ptys tty2", "Ptys", "ptys;id", "stop now", "Stop", "stop;id",
                    "vts 7", "Vts", "vts;id", "seat-wait 4", "Seat-wait", "seat-wait;id",
                    "seat-hold 8", "seat-hold;id", "Seat-hold", "seat-release",
                    "seat-stop now", "Seat-stop", "seat-stop;id", "seat-stop && touch pwned-stop",
                    "chvt", "chvt 3", "chvt 12", "Chvt 1", "chvt 1;id", "chvt 2 now",
                    "chvt 1 && touch pwned-chvt", "chvt $(touch %s/pwned-chvt)" % d.name,
                    "bare now", "Bare", "bare;id", "bare && touch pwned-bare",
                    "planes now", "Planes", "planes;id", "planes && touch pwned-planes",
                    "apply-root now", "Apply-root", "apply-root;id", "apply-root /tmp/x",
                    "apply-root && touch pwned-apply"]
        for n, request in enumerate(requests, 1):
            with self.subTest(request):
                self.assertEqual(ask(n, request), ["refused %s" % request.strip()])
        self.assertEqual(os.listdir(d.name), [])

    # A vtium unit of Type=notify for the agent's limit request, as a fake
    # systemctl on PATH: each vtium the unit starts has its PID from its
    # fork, and the unit turns active, as systemd counts the start, only
    # from the fourth time the agent reads MainPID. The agent's kill reaches
    # the fake as `systemctl t3-kill`, which records each PID killed and the
    # unit's state then; after the sixth kill the unit is failed on its
    # start limit, and refuses a start.
    FAKE_SYSTEMCTL = """#!/bin/bash
d=$T3_FAKE_UNIT
read -r pid state polls kills < "$d/unit"
case "$1 $2 $3" in
"show -P MainPID")
    polls=$((polls + 1))
    if [ "$state" = activating ] && [ "$polls" -gt 3 ]; then state=active; fi
    echo "$pid $state $polls $kills" > "$d/unit"
    if [ "$state" = failed ]; then echo 0; else echo "$pid"; fi ;;
"show -P ActiveState") echo "$state" ;;
"show -P SubState")
    case $state in activating) echo start ;; active) echo running ;; *) echo failed ;; esac ;;
"show -P Result")
    if [ "$state" = failed ]; then echo start-limit-hit; else echo success; fi ;;
"start "*) [ "$state" != failed ] ;;
"t3-kill -s KILL")
    echo "$4 $state" >> "$d/kills"
    kills=$((kills + 1))
    if [ "$kills" -ge 6 ]; then state=failed; else pid=$((pid + 1)); state=activating; polls=0; fi
    echo "$pid $state $polls $kills" > "$d/unit" ;;
*) echo "fake systemctl: $*" >&2; exit 1 ;;
esac
"""

    def test_the_agent_kills_each_vtium_once_its_unit_is_active(self):
        # vtium's unit is Type=notify, so systemd counts a start, and logs
        # "Started vtium on tty2.", only once vtium has sent READY=1, while
        # MainPID names vtium from its fork. A kill before the unit is
        # active fails a start systemd never logged, so the agent waits for
        # the unit to be active before each kill.
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        bin_dir, unit = os.path.join(d.name, "bin"), os.path.join(d.name, "unit")
        os.makedirs(bin_dir)
        os.makedirs(unit)
        with open(os.path.join(bin_dir, "systemctl"), "w") as f:
            f.write(self.FAKE_SYSTEMCTL)
        os.chmod(os.path.join(bin_dir, "systemctl"), 0o755)
        with open(os.path.join(unit, "unit"), "w") as f:
            f.write("1001 activating 0 0\n")
        ask = self.start_agent(d.name, {
            "PATH": bin_dir + os.pathsep + os.environ["PATH"], "T3_FAKE_UNIT": unit,
            "T3_AGENT_TEMPLATE": os.path.join(d.name, "none"),
            # bash takes an exported function before its builtin kill
            "BASH_FUNC_kill%%": '() { systemctl t3-kill "$@"; }'})
        self.assertEqual(ask(1, "limit", timeout=60),
                         ["sent KILL to %d" % pid for pid in range(1001, 1007)]
                         + ["stopped failed failed start-limit-hit after 6 kills",
                            "start refused"])
        with open(os.path.join(unit, "kills")) as f:
            self.assertEqual(f.read().splitlines(),
                             ["%d active" % pid for pid in range(1001, 1007)])

    def test_the_boot_claim_is_judged_both_ways(self):
        # The agent's answer to vts on a guest where vtium holds VTs 1 to 6
        # from each VT's own unit, and logind holds VT 6 too.
        good = []
        for n in range(1, 7):
            good += ["vt %d %d vtium 9%d00 vtium@tty%d.service" % (n, 400 + n, n, n),
                     "unit %d vtium@tty%d.service active" % (n, n),
                     "unit %d vtium-seat@tty%d.service inactive" % (n, n)]
        good += ["vt 6 300 systemd-logind 7000 systemd-logind.service", "next-available 7"]
        held, units, free = t3_seat.parse_vts(good)
        self.assertEqual(held[6], [(406, "vtium", 9600, "vtium@tty6.service"),
                                   (300, "systemd-logind", 7000, "systemd-logind.service")])
        self.assertEqual(units[3], {"vtium@tty3.service": "active",
                                    "vtium-seat@tty3.service": "inactive"})
        self.assertEqual(free, 7)
        self.assertIsNone(t3_seat.boot_claim_verdict(held, units, free))

        def judged(lines):
            return t3_seat.boot_claim_verdict(*t3_seat.parse_vts(lines))
        # VT 2 under the template link alone: nothing holds it until a
        # switch, and the kernel offers it to a display manager.
        alone = [l for l in good if not l.startswith("vt 2 ")]
        alone[-1] = "next-available 2"
        self.assertEqual(judged(alone), "no vtium of its own active unit holds VT 2 open: VT 2: nothing")
        # logind's hold of VT 6 is no vtium's.
        six = [l for l in good if not l.startswith("vt 6 406 ")]
        self.assertEqual(judged(six), "no vtium of its own active unit holds VT 6 open: "
                                      "VT 6: systemd-logind 300 in systemd-logind.service")
        # A vtium of another unit, such as one a test ran by hand, is not
        # VT 3's claim.
        other = [l.replace("vtium@tty3.service", "run-u7.service") if l.startswith("vt 3 ") else l
                 for l in good]
        self.assertEqual(judged(other), "no vtium of its own active unit holds VT 3 open: "
                                        "VT 3: vtium 403 in run-u7.service")
        # A unit still activating, as one that never sent READY=1, holds
        # nothing yet.
        activating = [l.replace("vtium@tty4.service active", "vtium@tty4.service activating")
                      for l in good]
        self.assertIn("VT 4", judged(activating))
        # One vtium holding two VTs open is no claim of each.
        shared = [l.replace("vt 5 405 vtium", "vt 5 404 vtium") for l in good]
        self.assertEqual(judged(shared), "one vtium holds several VTs open: 401 402 403 404 404 406")
        unknown = good[:-1] + ["next-available fgconsole: command not found"]
        self.assertEqual(judged(unknown), "the first free VT is None, not VT 7")
        # vtium's log line once it has set up a display, as vtium 948 logged
        # it on tier 3.
        self.assertIn(t3_seat.DISPLAY_LINE % 948,
                      "[ 10.9] vtium[948]: Display Virtual-1 with backend [drm2d] text renderer [cpu]")
        v = t3_scenario.VARIANTS["boot-claim"]
        self.assertEqual(t3_scenario.variant_cases("boot-claim"), ["boot-claim"])
        self.assertIn("boot-claim", t3_scenario.SEAT_VARIANTS)
        self.assertTrue(v["agent"] and v["soft_reboot"])
        self.assertFalse(v["first_switch"])
        self.assertNotIn("mode", v)
        self.assertIs(t3_scenario.CHECK_RUNS["boot-claim"], t3_seat.check_boot_claim)
        self.assertIn("boot-claim", t3_seat.CHECKS)

    def test_the_session_survives_judged_both_ways(self):
        prompt = "pty 701 pts/1 80x30 login"
        before = t3_heads.parse_ptys(["pty 700 pts/0 80x30 bash", prompt, "masters 410 2"])
        good = t3_heads.parse_ptys(["pty 700 pts/0 80x30 bash", prompt, "masters 512 2"])
        self.assertEqual(t3_heads.shell_of(before), 700)
        self.assertIsNone(t3_heads.judge_survived(before, good, 512, True))
        for after, vtium, took, why in (
                (good, 512, False, "did not log"),
                (t3_heads.parse_ptys([prompt, "masters 512 1"]), 512, True, "shell 700 is gone"),
                (t3_heads.parse_ptys(["pty 700 pts/0 80x30 bash", "masters 512 1"]), 512, True,
                 "login prompt 701 is gone"),
                (t3_heads.parse_ptys(["pty 700 pts/2 80x30 bash", prompt, "masters 512 1"]), 512,
                 True, "not pts/0"),
                (t3_heads.parse_ptys(["pty 700 pts/0 10x5 bash", prompt, "masters 512 1"]), 512,
                 True, "not 80x30"),
                (t3_heads.parse_ptys(["pty 700 pts/0 80x30 bash", prompt, "masters 512 0"]), 512,
                 True, "holds no pty master"),
                (good, 513, True, "holds no pty master")):
            with self.subTest(why):
                self.assertIn(why, t3_heads.judge_survived(before, after, vtium, took))
        none = t3_heads.parse_ptys(["masters 410 0"])
        self.assertIn("no one shell", t3_heads.judge_survived(none, good, 512, True))
        self.assertEqual(t3_heads.parse_ptys(["answer restart 700"])["answers"],
                         {"restart": "700"})
        self.assertIsNone(t3_heads.judge_stopped(before, t3_heads.parse_ptys([])))
        self.assertIn("still run", t3_heads.judge_stopped(before, good))
        self.assertIn("still run", t3_heads.judge_stopped(
            before, t3_heads.parse_ptys([prompt])))

    def test_parse_state(self):
        st = t3_heads.parse_state(agent_state(tty2="agetty", fallback=("inactive", "success",
                                                                       "yes", 99)))
        self.assertEqual(st["units"]["vtium-getty@tty2.service"],
                         {"active": "inactive", "result": "success", "condition": "yes",
                          "checked": 99})
        self.assertEqual((len(st["queued"]), st["marker"], st["tty2"], st["logins"], st["fgconsole"],
                          st["i8042"], st["vtium"]), (2, True, ["agetty"], 0, 2, 15, (410, 3.0)))
        self.assertTrue(t3_heads.getty_on_tty2(st))
        self.assertEqual(st["gettys"], {u: (0, 0) for u in t3_heads.GETTY_UNITS})
        self.assertEqual((st["timers"], st["sessions"]),
                         ({"systemd-tmpfiles-clean.timer": "waiting"}, {"1": "tty1"}))
        st = t3_heads.parse_state(["xhci 7", "kbd a-event-kbd", "kbd b-event-kbd", "keys 7",
                                   "session 3", "getty getty@tty2.service 4100200 1"])
        self.assertEqual((st["xhci"], st["kbds"], st["keys"], st["sessions"], st["gettys"]),
                         (7, ["a-event-kbd", "b-event-kbd"], 7, {"3": ""},
                          {"getty@tty2.service": (4100200, 1)}))
        # the version, the unit's sub-state and restarts beside its unit
        # line, and the journal's words in the order logged
        st = t3_heads.parse_state(agent_state(restarts=3, journal=(
            "started", "failed:signal", "skipped", "restart:1", "started")))
        self.assertEqual(st["systemd"], 262)
        self.assertEqual((st["units"]["vtium@tty2.service"]["active"],
                          st["units"]["vtium@tty2.service"]["sub"],
                          st["units"]["vtium@tty2.service"]["restarts"]),
                         ("active", "running", 3))
        self.assertEqual(st["journal"], ["started", "failed:signal", "skipped", "restart:1",
                                         "started"])
        self.assertEqual(t3_heads.parse_state(["journal"])["journal"], [])
        empty = t3_heads.parse_state([])
        self.assertEqual((empty["units"], empty["gettys"], empty["marker"], empty["keys"],
                          empty["vtium"], empty["systemd"], empty["journal"]),
                         ({}, {}, None, None, None, None, None))

    def test_no_login(self):
        before = t3_heads.parse_state(agent_state())
        after = t3_heads.parse_state(agent_state(i8042=29))
        self.assertIsNone(t3_heads.judge_no_login(before, after, True, False))
        wrong = {
            "keys never came": (dict(after=agent_state(i8042=15)), "the PS/2 keyboard's"),
            "vtium gone": (dict(after=agent_state(i8042=29, vtium=("failed", "signal"),
                                                  pid=None)), "vtium does not run on tty2"),
            "another VT": (dict(after=agent_state(i8042=29, fg=1)), "the VT shown is 1"),
            "no marker": (dict(after=agent_state(i8042=29, marker=False)),
                          "the closed policy's marker"),
            "a getty": (dict(after=agent_state(i8042=29, tty2="agetty")), "a getty runs"),
            "a login reported": (dict(logged_in=True), "a login reported itself"),
            "the screen moved": (dict(same=False), "the screen changed"),
            "a login program": (dict(after=agent_state(i8042=29, logins=1)), "1 login program"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                a = t3_heads.parse_state(change.get("after", agent_state(i8042=29)))
                got = t3_heads.judge_no_login(before, a, change.get("same", True),
                                              change.get("logged_in", False))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    def test_fallback_under_closed(self):
        # Under closed each failure is logged with its result, the fallback
        # skips itself once, and systemd starts vtium again.
        before = t3_heads.parse_state(agent_state(fallback=("inactive", "success", "no", 50),
                                                  restarts=1, pid=(410, 3.0)))
        boot = list(before["journal"])
        for failure, result in (("kill", "signal"), ("crash", "core-dump"), ("crash", "signal"),
                                ("hangup", "exit-code")):
            for words in (["failed:" + result, "skipped", "restart:2", "started"],
                          ["failed:" + result, "restart:2", "skipped", "started"]):
                with self.subTest(failure, result=result, words=words):
                    after = t3_heads.parse_state(agent_state(
                        fallback=("inactive", "success", "no", 90), restarts=2,
                        pid=(700, 0.2), journal=boot + words))
                    self.assertTrue(t3_heads.restart_settled(after, before))
                    self.assertIsNone(t3_heads.judge_restart(failure, before, after))
        back = dict(fallback=("inactive", "success", "no", 90), restarts=2, pid=(700, 0.2),
                    journal=boot + ["failed:signal", "skipped", "restart:2", "started"])
        wrong = {
            # a unit with no Restart= stays failed
            "no restart": (dict(back, vtium=("failed", "signal"), pid=None,
                                journal=boot + ["failed:signal", "skipped"]),
                           "after the kill systemd logged failed:signal for vtium's unit, not "
                           "failed:signal restart:2 started"),
            "another result": (dict(back, journal=boot + ["failed:exit-code", "skipped",
                                                          "restart:2", "started"]),
                               "after the kill systemd logged failed:exit-code"),
            "the count": (dict(back, journal=boot + ["failed:signal", "skipped", "restart:1",
                                                     "started"]),
                          "after the kill systemd logged failed:signal restart:1 started"),
            "a getty": (dict(back, journal=boot + ["failed:signal", "fell-back", "stopped"],
                             vtium=("inactive", "success"), pid=None),
                        "after the kill the fallback started a getty"),
            "no skip": (dict(back, journal=boot + ["failed:signal", "restart:2", "started"]),
                        "after the kill the fallback skipped itself 0 times"),
            "two skips": (dict(back, journal=boot + ["failed:signal", "skipped", "skipped",
                                                     "restart:2", "started"]),
                          "after the kill the fallback skipped itself 2 times"),
            "journal moved": (dict(back, journal=["failed:signal", "skipped", "restart:2",
                                                  "started"]),
                              "the journal before the kill is not where"),
            # a getty's start still queued reads inactive until it runs
            "a job queued": (dict(back, queued=["t3-plant@tty2.service"]),
                             "after the kill systemd still has jobs queued: "
                             "t3-plant@tty2.service start waiting"),
            "not running": (dict(back, vtium=("activating", "success"), pid=None),
                            "after the kill vtium does not run on tty2"),
            "condition held": (dict(back, fallback=("inactive", "success", "yes", 90)),
                               "after the kill the fallback's condition held"),
            "marker gone": (dict(back, marker=False), "after the kill the closed policy's marker"),
            "a getty runs": (dict(back, getty="active", tty2="agetty"),
                             "after the kill a getty runs"),
            # a getty that failed, or started and ended between two reads
            "a getty failed": (dict(back, getty="failed"), "after the kill a getty runs"),
            "a getty came and went": (dict(back, gettys=[("getty@tty2.service", (9100000, 0))]),
                                      "after the kill getty@tty2.service started under the "
                                      "closed policy: it left its inactive state at 9.1 s"),
            "a serial getty": (dict(back, gettys=[("serial-getty@tty2.service", (0, 2))]),
                               "after the kill serial-getty@tty2.service started"),
            "no reading": (dict(back, gettys=[("getty@tty2.service", None)]),
                           "after the kill the agent gave no reading of getty@tty2.service"),
            "a session": (dict(back, sessions=[("1", "tty1"), ("4", "tty2")]),
                          "after the kill login session 4 is on tty2"),
            # a getty's start delayed by a timer waits in no job
            "a timer": (dict(back, timers=["run-u12.timer"]),
                        "after the kill a timer waits to start a unit: run-u12.timer"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_restart("kill", before,
                                             t3_heads.parse_state(agent_state(**change)))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    def test_a_failure_settles(self):
        # The wait after a failure ends once the fallback ran after it, no
        # new job is queued, and vtium's unit runs a new vtium it logged as
        # started, or waits to restart no longer; the boot's own jobs,
        # queued throughout, are no reason to wait.
        before = t3_heads.parse_state(agent_state())
        boot = list(before["journal"])

        def settled(**change):
            return t3_heads.restart_settled(t3_heads.parse_state(agent_state(**change)), before)

        self.assertTrue(settled(pid=(700, 0.2), journal=boot + ["failed:signal", "skipped",
                                                                "restart:1", "started"]))
        # still waiting out its 100 ms, or the new vtium not yet logged
        self.assertFalse(settled(vtium=("activating", "success"), pid=None,
                                 journal=boot + ["failed:signal", "skipped", "restart:1"]))
        self.assertFalse(settled(pid=(700, 0.2), journal=boot + ["failed:signal", "skipped",
                                                                 "restart:1"]))
        # the old vtium, before systemd saw it end
        self.assertFalse(settled(journal=boot))
        # the fallback not yet run
        self.assertFalse(settled(pid=(700, 0.2), journal=boot + ["failed:signal", "restart:1",
                                                                 "started"]))
        # a job still queued
        self.assertFalse(settled(pid=(700, 0.2), queued=["getty@tty2.service"],
                                 journal=boot + ["failed:signal", "skipped", "restart:1",
                                                 "started"]))
        # stopped by the getty, or failed for good: settled, to be judged
        self.assertTrue(settled(vtium=("inactive", "success"), pid=None,
                                journal=boot + ["failed:signal", "fell-back", "stopped"]))
        self.assertTrue(settled(vtium=("failed", "signal"), pid=None,
                                journal=boot + ["failed:signal", "skipped"]))
        # a journal that moved ends the wait, so the judge says so
        self.assertTrue(settled(journal=["failed:signal"]))

    def test_fallback_under_open(self):
        # Under open the fallback starts the getty, whose Conflicts= stops
        # vtium's unit, waiting to restart or restarted.
        before = t3_heads.parse_state(agent_state(marker=False))
        boot = list(before["journal"])
        ran = dict(vtium=("inactive", "success"), fallback=("inactive", "success", "yes", 90),
                   getty="active", tty2="agetty", marker=False, pid=None,
                   journal=boot + ["failed:signal", "fell-back", "stopped"])
        for words in (["failed:signal", "fell-back", "stopped"],
                      ["failed:signal", "stopped", "fell-back"],
                      ["failed:signal", "restart:1", "stopped", "fell-back"],
                      ["failed:signal", "restart:1", "started", "fell-back", "stopped"]):
            with self.subTest(words=words):
                self.assertIsNone(t3_heads.judge_fallback("kill", before, t3_heads.parse_state(
                    agent_state(**dict(ran, journal=boot + words)))))
        wrong = {
            "skipped": (dict(ran, fallback=("inactive", "success", "no", 90),
                             journal=boot + ["failed:signal", "skipped", "restart:1",
                                             "started"]),
                        "after the kill the fallback skipped the getty"),
            "another result": (dict(ran, journal=boot + ["failed:exit-code", "fell-back",
                                                         "stopped"]),
                               "after the kill systemd logged failed:exit-code stopped for "
                               "vtium's unit, not a failure with signal"),
            # with no Restart=, the getty finds vtium's unit failed already
            "not stopped": (dict(ran, vtium=("failed", "signal"),
                                 journal=boot + ["failed:signal", "fell-back"]),
                            "after the kill the getty's start did not stop vtium's unit"),
            "a marker": (dict(ran, marker=True), "the closed policy's marker exists"),
            "no getty": (dict(ran, getty="inactive", tty2=""), "after the kill no getty runs"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_fallback("kill", before,
                                              t3_heads.parse_state(agent_state(**change)))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    def test_start_limit(self):
        # From a cleared count, 6 kills: systemd starts vtium 5 times,
        # refuses the sixth start and the one that follows, and the fallback
        # skips itself after each failure.
        before = t3_heads.parse_state(agent_state())
        boot = list(before["journal"])
        burst = t3_heads.START_LIMIT_BURST
        self.assertEqual(burst, 5)
        words = sum((["failed:signal", "skipped", "restart:%d" % n, "started"]
                     for n in range(1, burst + 1)), [])
        words += ["failed:signal", "skipped", "restart:6", "failed:start-limit-hit", "skipped",
                  "failed:start-limit-hit"]
        answer = ["sent KILL to %d" % (900 + n) for n in range(burst + 1)] + [
            "stopped failed failed start-limit-hit after 6 kills", "start refused"]
        hit = dict(vtium=("failed", "start-limit-hit"), pid=None, journal=boot + words,
                   fallback=("inactive", "success", "no", 90))
        self.assertIsNone(t3_heads.judge_limit(True, answer, before, t3_heads.parse_state(
            agent_state(**hit))))
        # a unit with StartLimitBurst=3 stops after 4 kills
        four = sum((["failed:signal", "skipped", "restart:%d" % n, "started"]
                    for n in range(1, 4)), []) + ["failed:signal", "skipped", "restart:4",
                                                  "failed:start-limit-hit", "skipped",
                                                  "failed:start-limit-hit"]
        wrong = {
            "fewer kills": (answer[2:], dict(hit, journal=boot + four),
                            "vtium's unit stopped restarting after 4 kills, not the 6"),
            # with no Restart=, the first kill leaves the unit failed
            "no restart": (answer[:1] + ["stopped failed failed signal after 1 kills"],
                           dict(hit, vtium=("failed", "signal"),
                                journal=boot + ["failed:signal", "skipped"]),
                           "vtium's unit stopped restarting after 1 kills"),
            "the start ran": (answer[:-1] + ["start ran"], hit,
                              "systemd did not refuse the start"),
            "another word": (answer, dict(hit, journal=boot + words[:-1]),
                             "systemd logged failed:signal restart:1"),
            "running": (answer, dict(hit, vtium=("active", "success"), pid=(990, 0.1)),
                        "past the start limit vtium's unit is active"),
            "no skip at the limit": (answer, dict(hit, journal=boot + [
                w for i, w in enumerate(words) if i != len(words) - 2]),
                "past the start limit the fallback did not skip itself"),
            "condition held": (answer, dict(hit, fallback=("inactive", "success", "yes", 90)),
                               "past the start limit the fallback's condition held"),
            "marker gone": (answer, dict(hit, marker=False),
                            "past the start limit the closed policy's marker"),
            "a getty": (answer, dict(hit, gettys=[("getty@tty2.service", (9100000, 0))]),
                        "past the start limit getty@tty2.service started"),
        }
        for name, (got_answer, change, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_limit(True, got_answer, before,
                                           t3_heads.parse_state(agent_state(**change)))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # Under open the first kill hands the VT to the getty, and the
        # kills never reach the limit.
        before = t3_heads.parse_state(agent_state(marker=False))
        opened = dict(vtium=("inactive", "success"), fallback=("inactive", "success", "yes", 90),
                      getty="active", tty2="agetty", marker=False, pid=None,
                      journal=boot + ["failed:signal", "fell-back", "stopped"])
        one = ["sent KILL to 900", "stopped inactive dead success after 1 kills"]
        self.assertIsNone(t3_heads.judge_limit(False, one, before, t3_heads.parse_state(
            agent_state(**opened))))
        # a second kill, when the unit started vtium again before the
        # getty's start stopped it
        two = ["sent KILL to 900", "sent KILL to 901",
               "stopped inactive dead success after 2 kills"]
        self.assertIsNone(t3_heads.judge_limit(False, two, before, t3_heads.parse_state(
            agent_state(**dict(opened, journal=boot + [
                "failed:signal", "restart:1", "started", "failed:signal", "fell-back",
                "fell-back", "stopped"])))))
        three = ["sent KILL to %d" % n for n in (900, 901, 902)] + [
            "stopped inactive dead success after 3 kills"]
        got = t3_heads.judge_limit(False, three, before, t3_heads.parse_state(agent_state(
            **dict(opened, journal=boot + ["failed:signal", "restart:1", "started"] * 2 + [
                "failed:signal", "fell-back", "stopped"]))))
        self.assertTrue(got.startswith("under the open policy the getty did not take the VT "
                                       "at the first failure: 3 kills"), got)
        got = t3_heads.judge_limit(False, answer, before, t3_heads.parse_state(agent_state(
            **dict(opened, journal=boot + words))))
        self.assertTrue(got.startswith("under the open policy the getty did not take"), got)
        got = t3_heads.judge_limit(False, one, before, t3_heads.parse_state(agent_state(
            **dict(opened, getty="inactive", tty2=""))))
        self.assertTrue(got.startswith("after the kill no getty runs"), got)

    def test_an_older_systemd_is_refused(self):
        self.assertIsNone(t3_heads.judge_systemd(t3_heads.parse_state(agent_state())))
        self.assertIsNone(t3_heads.judge_systemd(t3_heads.parse_state(agent_state(
            systemd=254))))
        self.assertRegex(t3_heads.judge_systemd(t3_heads.parse_state(agent_state(
            systemd=253))), "systemd 253, which starts OnFailure= only once")
        self.assertRegex(t3_heads.judge_systemd(t3_heads.parse_state(agent_state(
            systemd=None))), "no version of systemd")

    def test_the_kernel_console_past_the_limit(self):
        # Blank but for the cursor in the kernel console's first cell.
        def screen(*lit):
            data = bytearray(640 * 480 * 3)
            for x, y in lit:
                data[(y * 640 + x) * 3:(y * 640 + x) * 3 + 3] = b"\xaa\xaa\xaa"
            return t3img.Image(640, 480, bytes(data))

        self.assertTrue(t3_heads.kernel_blank(screen()))
        self.assertTrue(t3_heads.kernel_blank(screen(*[(x, y) for x in range(8)
                                                         for y in (14, 15)])))
        for name, lit in {"a prompt": [(8, 15)], "below the cell": [(0, 16)],
                          "far off": [(639, 479)]}.items():
            with self.subTest(name):
                self.assertFalse(t3_heads.kernel_blank(screen(*lit)))

    def test_the_unit_keeps_systemds_restart_defaults(self):
        # vtium's unit restarts on failure with systemd's defaults
        # (ADR-1401 D9): no delay or start limit of its own.
        with open(os.path.join(t3_common.ROOT, "crates", "vtium", "install",
                               "vtium@.service")) as f:
            text = f.read()
        # systemd strips the space around each key and value, and reads a
        # line that starts with # or ; as a comment (systemd.syntax(7))
        settings = [tuple(part.strip() for part in line.split("=", 1))
                    for line in map(str.strip, text.splitlines())
                    if line and line[0] not in "#;" and "=" in line]
        self.assertEqual([v for k, v in settings if k == "Restart"], ["on-failure"])
        # No other key that changes when or how often systemd restarts it:
        # every Restart*= but Restart= itself (RestartSec=, RestartMode=,
        # RestartSteps=, RestartMaxDelaySec=, RestartPreventExitStatus=,
        # RestartForceExitStatus=), every StartLimit*= (StartLimitInterval=
        # is the older spelling of StartLimitIntervalSec=, which systemd
        # still reads), and every *ExitStatus=, as SuccessExitStatus= turns a
        # failure into a success that starts nothing.
        for key in (k for k, _ in settings):
            with self.subTest(key):
                self.assertFalse(key.startswith("Restart") and key != "Restart", key)
                self.assertFalse(key.startswith("StartLimit"), key)
                self.assertFalse(key.endswith("ExitStatus"), key)

    def test_held_and_exit(self):
        first = t3_heads.parse_state(agent_state(pid=(400, 1.0)))
        held = t3_heads.parse_state(agent_state(pid=(400, 7.5)))
        self.assertIsNone(t3_heads.judge_held(first, held, "", 5))
        wrong = {
            "restarted": (agent_state(pid=(500, 7.5)), "", "vtium's PID changed"),
            "too soon": (agent_state(pid=(400, 6.0)), "", "vtium was read 6.0 s"),
            "gave up": (agent_state(pid=(400, 7.5)), "vtium[400]: no display appeared within 5 s",
                        "vtium logged 'no display appeared within 5 s'"),
            "exited": (agent_state(vtium=("failed", "exit-code"), pid=None), "",
                       "vtium does not run"),
            "fallback ran": (agent_state(pid=(400, 7.5), fallback=("inactive", "success", "no",
                                                                  9)), "", "the getty fallback"),
            # a fallback that ran and was unloaded since reads no timestamp
            "fallback logged": (agent_state(pid=(400, 7.5), journal=("started", "skipped")), "",
                                "the getty fallback"),
        }
        for name, (lines, serial, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_held(first, t3_heads.parse_state(lines), serial, 5)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        boot = t3_heads.parse_state(agent_state())
        ran = dict(vtium=("inactive", "success"), fallback=("inactive", "success", "yes", 90),
                   getty="active", tty2="agetty", marker=False, pid=None,
                   journal=boot["journal"] + ["failed:exit-code", "fell-back", "stopped"])
        self.assertIsNone(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(**ran)),
                                              5, 5))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(**ran)), 5,
                                            10).startswith("vtium gave up after 10 s"))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, getty="inactive", tty2=""))), 5, 5).startswith("no getty runs"))
        # vtium back for a moment before the getty's start stops it
        self.assertIsNone(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, journal=boot["journal"] + ["failed:exit-code", "restart:1", "started",
                                                   "fell-back", "stopped"]))), 5, 5))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, journal=boot["journal"] + ["failed:exit-code", "skipped", "stopped"]))),
            5, 5).startswith("the fallback did not start the getty"))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, queued=["getty@tty2.service"]))), 5, 5)
            .startswith("systemd still has jobs queued: getty@tty2.service"))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, journal=boot["journal"] + ["failed:signal", "fell-back", "stopped"]))),
            5, 5).startswith("systemd logged failed:signal stopped for vtium's unit"))
        self.assertTrue(t3_heads.judge_exit(boot, t3_heads.parse_state(agent_state(
            **dict(ran, vtium=("failed", "exit-code"), journal=boot["journal"] + [
                "failed:exit-code", "fell-back"]))), 5, 5)
            .startswith("the getty's start did not stop vtium's unit"))
        # the boot's jobs were read once vtium had already ended
        self.assertTrue(t3_heads.judge_exit(t3_heads.parse_state(agent_state(pid=None)),
                                            t3_heads.parse_state(agent_state(**ran)), 5, 5)
                        .startswith("vtium was not running when the check read"))

    def test_display_away(self):
        # The agent's answers to chvt 1 and chvt 2 on a guest whose vtium
        # 400 started 3 s before the switch away, its state before, and its
        # state once vtium is older than the 15 s wait and 2 s more.
        first = t3_heads.parse_state(agent_state(pid=(400, 1.0)))
        away = t3_heads.parse_switch(["shown 1", "vtium 400 3.0"])
        self.assertEqual(away, {"shown": 1, "vtium": (400, 3.0)})
        self.assertIsNone(t3_heads.judge_switch(first, away, 1, 15))
        refused = t3_heads.parse_switch([
            "refused: chvt 1 failed or did not return within 10 s, so VT 1 may not be shown",
            "shown 2", "vtium 400 13.0"])
        wrong = {
            "refused": (refused, "the agent could not switch to VT 1: chvt 1 failed"),
            "not shown": ({"shown": 2, "vtium": (400, 3.0)},
                          "after the switch to VT 1 the VT shown is 2"),
            "vtium gone": ({"shown": 1, "vtium": None}, "after the switch to VT 1 vtium on"),
            "another vtium": ({"shown": 1, "vtium": (500, 0.5)},
                              "after the switch to VT 1 vtium on tty2 is (500, 0.5)"),
            "too late": ({"shown": 1, "vtium": (400, 15.2)}, "VT 1 was shown 15.2 s after"),
        }
        for name, (switch, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_switch(first, switch, 1, 15)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # the switch back has no wait to fall within
        self.assertIsNone(t3_heads.judge_switch(
            first, t3_heads.parse_switch(["shown 2", "vtium 400 30.0"]), 2))
        # VT 1 shown, vtium kept running past the wait
        kept = t3_heads.parse_state(agent_state(pid=(400, 17.5), fg=1))
        self.assertIsNone(t3_heads.judge_kept(first, kept, "", 15))
        wrong = {
            "exited": (agent_state(vtium=("failed", "exit-code"), pid=None, fg=1), "",
                       "vtium does not run on tty2 while VT 1 is shown"),
            "restarted": (agent_state(pid=(500, 1.2), fg=1), "", "vtium's PID changed"),
            "too soon": (agent_state(pid=(400, 16.0), fg=1), "", "vtium was read 16.0 s"),
            "back on 2": (agent_state(pid=(400, 17.5), fg=2), "", "the VT shown is 2, not 1"),
            # the defect the case covers: the wait ran out while VT 1 was shown
            "gave up": (agent_state(pid=(400, 17.5), fg=1),
                        "[   18.2] vtium[400]: no display appeared within 15 s "
                        "(display-wait), so vtium exits\n",
                        "vtium logged 'no display appeared within 15 s' while VT 1"),
        }
        for name, (lines, serial, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_kept(first, t3_heads.parse_state(lines), serial, 15)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # back on VT 2: vtium waits the full 15 s from the switch, by the
        # guest's monotonic clock, which the agent reads before chvt 2 and
        # the journal stamps vtium's line with, matched to vtium's PID. The
        # journal's stamp of the agent's own line, 0.5 s late here, is not
        # the time read.
        def log(back, gone, pid=400, wait=15):
            out = ["[   20.100000] systemd[1]: Started something else."]
            if back is not None:
                out.append("[ %10.6f] t3-agent[300]: switching to VT 2 at %.6f s" % (
                    back + 0.5, back))
            if gone is not None:
                out.append("[ %10.6f] vtium[%d]: no display appeared within %d s "
                           "(display-wait), so vtium exits" % (gone, pid, wait))
            return "\r\n".join(out) + "\r\n"
        self.assertEqual(t3_heads.judge_return(log(21.0, 36.004), 400, 15), (None, 15))
        wrong = {
            "no switch line": (log(None, 36.0), "the serial log has no line of the agent's"),
            "no line": (log(21.0, None), "vtium 400 logged no line that no display"),
            "another vtium": (log(21.0, 36.0, pid=500), "vtium 400 logged no line"),
            "another wait": (log(21.0, 31.0, wait=10), "vtium gave up after 10 s, not"),
            # a wait that kept the 3 s it ran before the switch away
            "not in full": (log(21.0, 33.0), "vtium gave up 12.000 s after the switch back"),
            "just short": (log(21.0, 35.999), "vtium gave up 14.999 s after"),
        }
        for name, (serial, why) in wrong.items():
            with self.subTest(name):
                got, _ = t3_heads.judge_return(serial, 400, 15)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # an earlier switch to VT 1 is not the switch back
        self.assertTrue(t3_heads.judge_return(
            "[    4.000000] t3-agent[300]: switching to VT 1 at 3.990000 s\n"
            + log(None, 19.0), 400, 15)[0]
            .startswith("the serial log has no line of the agent's switch to VT 2"))

    def test_kiosk(self):
        def st(xhci, keys=None, **kw):
            return t3_heads.parse_state(agent_state(**kw) + ["xhci %d" % xhci]
                                        + (["keys %d" % keys] if keys is not None else []))
        listed = t3_heads.parse_state(agent_state() + ["xhci 20",
                                                       "kbd " + t3_heads.KIOSK_SECOND])
        chord = (st(10), st(12))
        plugged = (listed, st(25, keys=len(t3_heads.KIOSK_LINE)))
        self.assertIsNone(t3_heads.judge_kiosk(chord, plugged, True))
        # the chord is judged alone first, before the second keyboard types
        self.assertIsNone(t3_heads.judge_kiosk(chord))
        self.assertTrue(t3_heads.judge_kiosk((st(10), st(12, fg=3)))
                        .startswith("after Ctrl+Alt+F3: the VT shown is 3"))
        wrong = {
            "chord never came": (dict(chord=(st(10), st(10))), "the USB controller's"),
            "the VT switched": (dict(chord=(st(10), st(12, fg=3))),
                                "after Ctrl+Alt+F3: the VT shown is 3"),
            # the controller, shared with the claimed keyboard, is no proof
            "keys never came": (dict(plugged=(listed, st(25, keys=0))),
                                "the second keyboard's evdev node counted 0 key presses"),
            "never watched": (dict(plugged=(listed, st(25))),
                              "the second keyboard's evdev node counted None"),
            "some keys lost": (dict(plugged=(listed, st(25, keys=5))),
                               "the second keyboard's evdev node counted 5 key presses, "
                               "not the 7"),
            "vtium gone": (dict(plugged=(listed, st(25, keys=7, pid=None,
                                                    vtium=("failed", "signal")))),
                           "after the second keyboard's line: vtium does not run"),
            "not listed": (dict(plugged=(st(20), st(25, keys=7))),
                           "plugged in, the guest lists"),
            "it typed": (dict(same=False), "the screen changed"),
        }
        for name, (change, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_kiosk(change.get("chord", chord),
                                           change.get("plugged", plugged),
                                           change.get("same", True))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    def test_two_heads(self):
        lines = ["Display Virtual-1 with backend [drm2d] text renderer [bbulk], cell size 8x16",
                 "Display Virtual-2 with backend [drm2d] text renderer [bbulk], cell size 8x8"]
        panel, desk = t3img.Image(1, 1, b"\0\0\0"), t3img.Image(1, 1, b"\1\1\1")
        self.assertIsNone(t3_heads.judge_two_heads(lines, panel, panel, desk))
        mirrored = [l.replace("8x8", "8x16") for l in lines]
        self.assertTrue(t3_heads.judge_two_heads(mirrored, panel, panel, desk)
                        .startswith("the displays and their cells"))
        self.assertTrue(t3_heads.judge_two_heads(lines[:1], panel, panel, desk)
                        .startswith("the displays and their cells"))
        self.assertTrue(t3_heads.judge_two_heads(lines, panel, desk, desk)
                        .startswith("the second output's screen changed"))
        self.assertTrue(t3_heads.judge_two_heads(lines, desk, desk, desk)
                        .startswith("the second output shows the first head's"))

    def test_a_check_with_no_display_passes_on_its_state(self):
        meta = {"passed": ["display-held"], "errors": {"getty-fallback": "after the kill"}}
        entries = {e["name"]: e for e in t3_scenario.evaluate_variant(
            "heads-closed-nodisplay", {}, meta, LOG)}
        self.assertEqual(entries["heads-closed-nodisplay/display-held"]["status"], "pass")
        self.assertEqual(entries["heads-closed-nodisplay/getty-fallback"]["status"], "fail")
        self.assertEqual(entries["heads-closed-nodisplay/getty-fallback"]["error"],
                         "after the kill")

    def test_no_display_variant_errors(self):
        # A guest with no display device fails when the console set one up.
        vtium = {"name": "vtium", "revision": "abc"}
        self.assertEqual(t3_scenario.variant_errors(None, [], None, {}, {}, vtium, []),
                         ([], False))
        why, drop = t3_scenario.variant_errors(
            None, ["Display Virtual-1 with backend [drm2d] text renderer [bbulk]"], None, {}, {},
            vtium, [])
        self.assertTrue(drop)
        self.assertTrue(why[0].startswith("the console set up a display on a guest with none"))
        self.assertEqual(t3_scenario.resident({}, [])["first_screen"], None)


class NovtTest(unittest.TestCase):
    """The head variants on a machine without kernel VTs (t3_heads.py): the
    stock guest, with /dev/tty0 hidden from vtium alone by guest/t3-novt.
    Their machines, cases and goldens, the disc and the guest's drop-in, and
    the no-vt and display-off checks' verdicts both ways. Tier 3's own runs
    of these variants test the script: it needs root, which makes the node
    /dev/ptmx."""

    CASES = {"novt-closed-unmatched": ["no-vt", "display-off"],
             "novt-closed-broken": ["panic", "no-vt"],
             "novt-open": ["cjk", "no-vt"]}
    # The variant on a VT whose head file, policy and configuration each
    # variant without VTs runs, by variant.
    ON_A_VT = {"novt-closed-unmatched": "heads-closed-unmatched",
               "novt-closed-broken": "heads-closed-broken",
               "novt-open": "2d-640x480-psf16"}

    def test_variants(self):
        self.assertEqual(t3_scenario.NOVT_VARIANTS, list(self.CASES))
        for name, want in self.CASES.items():
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                on_vt = t3_scenario.VARIANTS[self.ON_A_VT[name]]
                self.assertEqual(t3_scenario.variant_cases(name), want)
                self.assertIs(v["novt"], True)
                self.assertIs(v["agent"], True)
                self.assertIs(v["vt_switch"], False)
                self.assertEqual((v["cpus"], v["memory_mb"]), (1, 512))
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                self.assertIn(name, t3_scenario.golden_cases())
                # the machine and the configuration of the variant on a VT;
                # the id that names the card to QMP draws nothing
                for key in ("conf", "frits", "fonts", "images"):
                    self.assertEqual(v.get(key), on_vt.get(key), key)
                self.assertEqual(v["device"].replace("id=video0,", ""),
                                 on_vt["device"].replace("id=video0,", ""))
                with open(t3_heads.head_file(v)) as f:
                    self.assertRegex(f.read(), r"(?m)^policy = %s\b" % v["policy"])
        # the closed variants run the head files of their variants on a VT
        for name in ("novt-closed-unmatched", "novt-closed-broken"):
            v = t3_scenario.VARIANTS[name]
            on_vt = t3_scenario.VARIANTS[self.ON_A_VT[name]]
            self.assertEqual((v["heads"], v["policy"]), (on_vt["heads"], "closed"))
        unmatched = t3_scenario.VARIANTS["novt-closed-unmatched"]
        self.assertIsNone(unmatched["renderer"])
        self.assertIs(unmatched["login"], False)
        self.assertEqual(unmatched["off"], t3_scenario.VARIANTS["heads-closed-unmatched"]["off"])
        broken = t3_scenario.VARIANTS["novt-closed-broken"]
        self.assertEqual((broken["first"], broken["login"]), ("panic", False))
        opened = t3_scenario.VARIANTS["novt-open"]
        self.assertEqual((opened["heads"], opened["policy"]), ("docs/heads/default.conf", "open"))
        self.assertEqual(opened["renderer"], "cpu")

    def test_goldens(self):
        # The screens a variant without VTs draws are judged by the goldens
        # of the variant on a VT that draws the same screen, and its checks
        # by no golden.
        self.assertEqual(t3_evaluate.golden_for("novt-closed-broken", "panic")[0],
                         os.path.join(t3_evaluate.GOLDEN, "heads-closed-broken", "panic.png"))
        self.assertEqual(t3_evaluate.golden_for("novt-closed-broken", "panic",
                                                features=["images"])[0],
                         os.path.join(t3_evaluate.GOLDEN, "heads-closed-broken",
                                      "panic+images.png"))
        self.assertEqual(t3_evaluate.golden_for("novt-open", "cjk")[0],
                         os.path.join(t3_evaluate.GOLDEN, "2d-640x480-psf16", "cjk.png"))
        for case in ("no-vt", "display-off"):
            self.assertEqual(t3_evaluate.golden_for("novt-closed-unmatched", case), (None, None))
        self.assertFalse(os.path.exists(os.path.join(t3_evaluate.GOLDEN,
                                                     "novt-closed-unmatched")))

    def test_shown(self):
        # The line the driver waits for after its switch to tty2: on a VT,
        # the VT vtium uses; without VTs, the display vtium set up, or none
        # where vtium sets up no display.
        self.assertEqual(t3_scenario.shown_of("heads-closed-broken"), "vt: using tty /dev/tty2")
        self.assertEqual(t3_scenario.shown_of("sysvinit-640x480"), "text renderer [")
        self.assertEqual(t3_scenario.shown_of("novt-closed-broken"), "text renderer [")
        self.assertEqual(t3_scenario.shown_of("novt-open"), "text renderer [")
        self.assertIsNone(t3_scenario.shown_of("novt-closed-unmatched"))

    def test_disc_and_drop_in(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        tree = os.path.join(d.name, "tree")
        write_tree(tree, ImplementationTest.TREE)
        impl = t3_impl.tree(tree, os.path.join(d.name, "out", "tree.tar"))
        impl["fonts"] = os.path.join(d.name, "fonts")
        os.makedirs(impl["fonts"])
        disc = os.path.join(d.name, "disc")
        script = os.path.join(t3_common.GUEST, "t3-novt")
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("novt-closed-unmatched", d.name, impl)
            with open(os.path.join(disc, "t3-novt"), "rb") as f, open(script, "rb") as g:
                self.assertEqual(f.read(), g.read())
            t3_scenario.build_disc("heads-closed-unmatched", d.name, impl)
            self.assertNotIn("t3-novt", os.listdir(disc))
        # The drop-in starts vtium through the script with none of the
        # unit's options: --vt=%I names the VT, and --no-switchvt waits for
        # the VT to be shown, which nothing does to a fake VT. It keeps the
        # journal's name for vtium's lines, which the driver reads them by.
        with open(os.path.join(t3_common.ROOT, "crates", "vtium", "install",
                               "vtium@.service")) as f:
            start = re.findall(r"(?m)^ExecStart=(.*)$", f.read())
        self.assertEqual(start, ["/usr/bin/vtium --vt=%I --no-switchvt"])
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        self.assertIn("printf '[Service]\\nExecStart=\\nExecStart=/usr/local/sbin/t3-novt "
                      "/usr/bin/vtium\\nSyslogIdentifier=vtium\\n'", setup)
        self.assertIn('install -m755 "$disc"/t3-novt /usr/local/sbin/t3-novt', setup)

    def test_parse_state(self):
        st = t3_heads.parse_state(agent_state() + ["tty0 yes no"])
        self.assertEqual(st["tty0"], ("yes", "no"))
        self.assertIsNone(t3_heads.parse_state(agent_state())["tty0"])

    SERIAL = ("[   12.301] vtium[512]: log: vtium Revision abc\n"
              "[   12.302] vtium[512]: vt: no VTs: using a fake VT\n")

    def test_no_vt_is_judged_both_ways(self):
        good = t3_heads.parse_state(agent_state(pid=(512, 3.0)) + ["tty0 yes no"])
        self.assertIsNone(t3_heads.judge_no_vt(good, self.SERIAL))
        wrong = {
            "no reading": (agent_state(pid=(512, 3.0)), self.SERIAL,
                           "the agent gave no reading of /dev/tty0"),
            "no tty0 in the guest": (agent_state(pid=(512, 3.0)) + ["tty0 no no"], self.SERIAL,
                                     "the guest has no /dev/tty0"),
            "not hidden": (agent_state(pid=(512, 3.0)) + ["tty0 yes yes"], self.SERIAL,
                           "the /dev that vtium sees holds tty0"),
            "not running": (agent_state(pid=None) + ["tty0 yes none"], self.SERIAL,
                            "vtium does not run on tty2"),
            "no fake VT": (agent_state(pid=(512, 3.0)) + ["tty0 yes no"],
                           self.SERIAL.splitlines(True)[0],
                           "vtium on tty2 (PID 512) did not log"),
            "another's fake VT": (agent_state(pid=(513, 3.0)) + ["tty0 yes no"], self.SERIAL,
                                  "vtium on tty2 (PID 513) did not log"),
            "a kernel VT": (agent_state(pid=(512, 3.0)) + ["tty0 yes no"],
                            self.SERIAL + "[   14.1] vtium[530]: vt: using tty /dev/tty3\n",
                            "a vtium used a kernel VT"),
        }
        for name, (lines, serial, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_no_vt(t3_heads.parse_state(lines), serial)
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)

    OFF = ("[    7.4157] vtium[769]: display Virtual-1 at 640x480\n"
           "[    7.4182] vtium[769]: display Virtual-1 stays off: no rule in vtium.conf matches "
           "it; add the rule `display connector:pci-0000:00:01.0/Virtual-1 -> desk` to give it "
           "a head\n"
           "[    7.4208] vtium[769]: setting DPMS of Virtual-1 to Off\n")

    def test_display_off_is_judged_both_ways(self):
        self.assertIsNone(t3_heads.judge_display_off(self.OFF, "Virtual-1"))
        lines = self.OFF.splitlines(True)
        wrong = {
            "no off line": (lines[0] + lines[2], "vtium did not log 'display Virtual-1 stays off"),
            "no power state": ("".join(lines[:2]), "vtium did not leave the output of Virtual-1 "
                                                   "off: it set no power state"),
            "on again": (self.OFF + "[    7.43] vtium[769]: setting DPMS of Virtual-1 to On\n",
                         "vtium did not leave the output of Virtual-1 off: Off, On"),
            "drawn": (self.OFF + "[    7.46] vtium[769]: Display Virtual-1 with backend [drm2d] "
                                 "text renderer [cpu], cell size 8x16\n",
                      "vtium set up Virtual-1 for drawing"),
        }
        for name, (serial, why) in wrong.items():
            with self.subTest(name):
                got = t3_heads.judge_display_off(serial, "Virtual-1")
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # another display's lines say nothing of this one
        self.assertTrue(t3_heads.judge_display_off(self.OFF.replace("Virtual-1", "Virtual-2"),
                                                   "Virtual-1").startswith("vtium did not log"))


def sixel_pixels(data):
    """The pixels a sixel string sets, {(x, y): (r, g, b) percentages}: a
    reader of the plain forms t3_pictures.py writes, `#N;2;r;g;b` color
    definitions, `#N` selections, `!N` repeats, `$` and `-`, apart from the
    picture writer it checks."""
    body = data.decode()
    body = body[body.index("q") + 1:body.rindex("\x1b\\")]
    tokens = re.findall(r'"[\d;]+|#\d+(?:;\d+)*|!\d+.|[?-~]|\$|-', body)
    colors, out, color, x, top = {}, {}, None, 0, 0
    for t in tokens:
        if t.startswith('"'):
            continue
        if t.startswith("#"):
            parts = [int(v) for v in t[1:].split(";")]
            if len(parts) == 5:
                colors[parts[0]] = tuple(parts[2:])
            color = parts[0]
        elif t == "$":
            x = 0
        elif t == "-":
            x, top = 0, top + 6
        else:
            n, ch = (int(t[1:-1]), t[-1]) if t.startswith("!") else (1, t)
            for _ in range(n):
                bits = ord(ch) - 63
                for dy in range(6):
                    if bits >> dy & 1:
                        out[(x, top + dy)] = colors[color]
                x += 1
    return out


def gif_frames(data):
    """The frames of a GIF file, each [(r, g, b) per pixel, row by row], with
    its graphic control extension's delay and disposal method: a reader of
    the GIF89a form, its LZW codes read at any width, apart from the writer
    it checks. A frame covers the whole picture and uses the global color
    table, as t3_pictures.gif writes them."""
    assert data[:6] == b"GIF89a"
    width, height, packed = struct.unpack("<HHB", data[6:11])
    table_size = 2 << (packed & 7)
    table = [tuple(data[13 + 3 * i:16 + 3 * i]) for i in range(table_size)]
    i, frames, control = 13 + 3 * table_size, [], None
    while data[i] != 0x3B:
        if data[i] == 0x21:
            if data[i + 1] == 0xF9:
                flags, delay = struct.unpack("<BH", data[i + 3:i + 6])
                control = (delay, flags >> 2 & 7)
            i += 2
            while data[i]:
                i += data[i] + 1
            i += 1
            continue
        assert data[i] == 0x2C
        left, top, w, h, flags = struct.unpack("<HHHHB", data[i + 1:i + 10])
        assert (left, top, w, h, flags) == (0, 0, width, height, 0)
        minimum, i = data[i + 10], i + 11
        blob = bytearray()
        while data[i]:
            blob += data[i + 1:i + 1 + data[i]]
            i += data[i] + 1
        i += 1
        clear, end = 1 << minimum, (1 << minimum) + 1
        bits, pos, out, prev = minimum + 1, 0, [], None
        words = [[n] for n in range(clear)] + [None, None]
        while True:
            if pos + bits > len(blob) * 8:
                raise ValueError("a frame's codes end with no end code")
            code = int.from_bytes(blob[pos // 8:pos // 8 + 3], "little") >> pos % 8
            code &= (1 << bits) - 1
            pos += bits
            if code == clear:
                words, bits, prev = words[:end + 1], minimum + 1, None
                continue
            if code == end:
                break
            if code < len(words):
                word = words[code]
                if prev is not None:
                    words.append(prev + word[:1])
            else:
                word = prev + prev[:1]
                words.append(word)
            out += word
            prev = word
            if len(words) == 1 << bits and bits < 12:
                bits += 1
        frames.append(([table[n] for n in out], control))
    return frames


class PicturesTest(unittest.TestCase):
    """The test pictures of the images variant (t3_pictures.py): pinned, a
    whole number of cells, and each file a faithful copy of its picture."""

    def test_pinned(self):
        # The images variant's goldens hold these pixels, so a change to the
        # pictures moves them: this digest changes in the same commit as
        # the goldens.
        # the pictures of the screens before the animation, iTerm2 and
        # placeholder screens are as they were
        self.assertEqual(t3_pictures.digest(t3_pictures.FIRST_PICTURES),
                         "76eb38dd0e3d1b6dec0b7ede466c82ad2dc325704e8d732c351e8077846c1658")
        self.assertEqual(t3_pictures.digest(),
                         "052432683ed74a7f98165ab4167d64c08448770c2bbf2e81685d0a3b24833156")
        self.assertEqual(sorted(t3_pictures.files()), [
            "bands.rgba", "bands.six", "child.rgba", "disc.png", "disc.six", "parent.rgba",
            "patch-cyan.rgba", "patch-magenta.rgba", "patch-violet.rgba", "patch-yellow.rgba",
            "quad.gif", "quad.png", "quad.rgba", "quad.rgba.z", "quad.six"])

    def test_whole_cells(self):
        # Every picture is a whole number of the variant's 8x16 cells, and
        # every one sent as sixel a whole number of 6-row bands.
        sixels = {n.split(".")[0] for n in t3_pictures.files() if n.endswith(".six")}
        self.assertEqual(sixels, {"quad", "bands", "disc"})
        for name, pic in t3_pictures.PICTURES.items():
            with self.subTest(name):
                self.assertEqual((pic.width % 8, pic.height % 16), (0, 0))
                if name in sixels:
                    self.assertEqual(pic.height % 6, 0)

    def test_files(self):
        # Each file holds its picture: the sixel's set pixels in their
        # colors, the raw and inflated pixels byte for byte, the PNG's
        # colors where the picture is opaque and black where it is clear.
        files = t3_pictures.files()
        for name, data in files.items():
            pic = dict(t3_pictures.PICTURES, **t3_pictures.PATCHES)[name.split(".")[0]]
            with self.subTest(name):
                if name.endswith(".gif"):
                    # every frame whole, shown 100 ms and left in place
                    want = [([t3_pictures.eight(p) for row in t3_pictures.PICTURES[n].pixels
                              for p in row], (10, 1)) for n in t3_pictures.GIF_FRAMES]
                    self.assertEqual(gif_frames(data), want)
                    self.assertNotIn(b"NETSCAPE2.0", data)
                elif name.endswith(".six"):
                    want = {(x, y): p for y, row in enumerate(pic.pixels)
                            for x, p in enumerate(row) if p}
                    self.assertEqual(sixel_pixels(data), want)
                    self.assertIn(b'"1;1;%d;%d' % (pic.width, pic.height), data)
                elif name.endswith(".rgba"):
                    self.assertEqual(len(data), pic.width * pic.height * 4)
                    self.assertEqual(data, pic.rgba())
                elif name.endswith(".z"):
                    self.assertEqual(zlib.decompress(data), pic.rgba())
                else:
                    img = t3img.read_png(data)
                    self.assertEqual((img.width, img.height), (pic.width, pic.height))
                    rgba = pic.rgba()
                    self.assertEqual(img.data, bytes(b for i, b in enumerate(rgba) if i % 4 != 3))
        self.assertEqual(t3_pictures.eight(t3_pictures.ORANGE), (255, 128, 0))
        # a clear pixel is clear in the raw pixels and the PNG alike
        self.assertIsNone(t3_pictures.PICTURES["disc"].pixels[0][0])
        self.assertEqual(t3_pictures.PICTURES["disc"].rgba()[:4], b"\0\0\0\0")

    def test_write(self):
        with tempfile.TemporaryDirectory() as d:
            names = t3_pictures.write(os.path.join(d, "p"))
            self.assertEqual(names, sorted(t3_pictures.files()))
            for name in names:
                with open(os.path.join(d, "p", name), "rb") as f:
                    self.assertEqual(f.read(), t3_pictures.files()[name])


class ImagesVariantTest(unittest.TestCase):
    """The images variant (t3_scenario.VARIANTS["images-640x480"]): its
    cases, the tree it needs, the guest's account of the image helper, the
    pictures on its disc and the make target that runs it."""

    VARIANT = "images-640x480"
    HELPER = ("helper /usr/bin/vtium-image mode 755, account vtium-image uid 969 gid 969")

    def test_cases(self):
        self.assertEqual(t3_scenario.variant_cases(self.VARIANT), [
            "prelogin", "img-sixel", "img-kitty", "img-relative", "img-overwrite", "img-delete",
            "img-scroll", "img-scrollback", "img-scrollforward", "img-tmux-pane",
            "img-animation", "img-iterm2", "img-placeholder", "img-png",
            "vt-switch", "vt-restore"])
        v = t3_scenario.VARIANTS[self.VARIANT]
        self.assertEqual((v["device"], v["cpus"], v["memory_mb"], v["images"]),
                         ("virtio-vga,xres=640,yres=480,edid=on", 1, 512, True))
        self.assertIn(self.VARIANT, t3_scenario.golden_cases())
        self.assertEqual(v["tree_features"], ["images"])
        for screen in t3_evaluate.PICTURE_SCREENS:
            with self.subTest(screen):
                self.assertIn(screen, t3_evaluate.DRAWS_PICTURES)
                self.assertNotIn(screen, t3_evaluate.SCREENS)
                self.assertTrue(os.path.isfile(os.path.join(t3_common.GUEST, "screens",
                                                            screen + ".sh")))
                caption, wrong = t3_evaluate.describe(screen)
                self.assertIn("Correct: ", caption)
                self.assertTrue(wrong)

    def test_restore(self):
        # vt-restore is compared with the last screen a variant draws: the
        # images variant's img-png, every other variant's cjk.
        self.assertEqual(t3_evaluate.same_as("vt-restore", self.VARIANT), "img-png")
        self.assertEqual(t3_evaluate.same_as("img-scrollforward", self.VARIANT), "img-scroll")
        self.assertEqual(t3_evaluate.golden_for(self.VARIANT, "vt-restore",
                                                t3_evaluate.GOLDEN)[0],
                         os.path.join(t3_evaluate.GOLDEN, self.VARIANT, "img-png.png"))
        for variant in t3_scenario.VARIANTS:
            if "vt-restore" in t3_scenario.variant_cases(variant):
                with self.subTest(variant):
                    self.assertEqual(t3_evaluate.same_as("vt-restore", variant),
                                     t3_scenario.screens_of(variant)[-1])
        entry = t3_evaluate.evaluate(self.VARIANT, "vt-restore", None, {}, LOG,
                                     t3_evaluate.GOLDEN)
        self.assertIn("the img-png screen redrawn", entry["description"])

    def test_restore_must_match_before(self):
        # vt-restore fails, whatever its golden, when the screen after the
        # switch back is not the screen before it, as when a console draws
        # its text again and drops a picture.
        before = t3img.Image(4, 2, bytes(range(24)))
        dropped = t3img.Image(4, 2, bytes(range(21)) + b"\0\0\0")
        self.assertIsNone(t3_scenario.restore_differs(before, t3img.Image(4, 2, before.data)))
        self.assertEqual(t3_scenario.restore_differs(before, dropped),
                         "the screen after the switch back differs from the screen before "
                         "the switch in 1 pixels")
        self.assertIn("differs", t3_scenario.restore_differs(before, t3img.Image(2, 2,
                                                                                bytes(12))))
        tty3 = t3img.Image(4, 2, bytes(24))

        class VM:
            def keys(self, *chord):
                pass

            def wait_serial(self, line, timeout):
                pass

            def screendump(self, path, output=None):
                return tty3

        for after, want in ((before, None), (dropped, "in 1 pixels")):
            with self.subTest(want), tempfile.TemporaryDirectory() as d:
                shown = iter([tty3, after])
                caps, errors = {}, {}
                with mock.patch.object(t3_scenario, "wait_stable",
                                       lambda vm, path, **kw: next(shown)):
                    t3_scenario.switch_vts(self.VARIANT, VM(), d, before, caps, errors,
                                           lambda name: None)
                self.assertIn("vt-restore", caps)
                if want:
                    self.assertIn(want, errors["vt-restore"])
                else:
                    self.assertNotIn("vt-restore", errors)

    def test_scrollback_keys(self):
        self.assertIn(("img-scroll", "img-scrollback", [("shift", "pgup")]), t3_evaluate.KEYED)
        self.assertIn(("img-scroll", "img-scrollforward", [("shift", "pgdn")]),
                      t3_evaluate.KEYED)
        for case in ("img-scrollback", "img-scrollforward"):
            self.assertIn(case, t3_evaluate.CHECKS)
            self.assertNotIn(case, t3_evaluate.NO_GOLDEN)

    def test_tree_features(self):
        # The variant runs only on a tree whose console is built with
        # images; the other variants run on either tree.
        for features, want in (([], ["images-640x480 needs a tree whose console is built with "
                                     "images, and the tree's was built with no features; make "
                                     "tier3 and make tier3-images build one"]),
                               (["logind"], ["images-640x480 needs a tree whose console is "
                                             "built with images, and the tree's was built with "
                                             "logind; make tier3 and make tier3-images build "
                                             "one"]),
                               (["images"], []), (["images", "logind"], [])):
            with self.subTest(features):
                impl = {"tree_features": features}
                self.assertEqual(t3_scenario.check_tree_features([self.VARIANT], impl), want)
                self.assertEqual(t3_scenario.check_tree_features(
                    ["2d-potato-min", "2d-640x480-psf16"], impl), [])

    def test_run_refuses_a_tree_without_images(self):
        with tempfile.TemporaryDirectory() as d:
            tree = os.path.join(d, "tree")
            write_tree(tree, ImplementationTest.TREE)
            args = t3.main.__globals__["argparse"].Namespace(
                variants=[self.VARIANT], tree=tree, tree_target=None, features=None, jobs=1,
                deadline=60)
            with mock.patch.object(t3_scenario, "base_of", return_value=(tree, "x")), \
                    mock.patch.object(t3_impl, "RUNDIR", d), \
                    mock.patch.object(t3, "preflight", return_value=[]), \
                    mock.patch.object(t3_runner, "run_variants") as run, \
                    mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                    mock.patch.object(t3_runner, "raise_on_sigterm"):
                with self.assertRaisesRegex(SystemExit, "needs a tree whose console is built "
                                                        "with images"):
                    t3.run(args, LOG)
                run.assert_not_called()

    def test_image_files(self):
        # A tree built with images holds the helper and its sysusers file,
        # and a tree built without holds neither (ADR-1101 D1).
        base = dict(ImplementationTest.TREE)
        helper = {f: "x" for f in t3_impl.IMAGE_FILES}
        with tempfile.TemporaryDirectory() as d:
            for files, features, why in (
                    (base, ["images"], "holds no /usr/bin/vtium-image, "
                                       "/usr/lib/sysusers.d/vtium-image.conf"),
                    (dict(base, **{t3_impl.IMAGE_FILES[1]: "x"}), ["images"],
                     "holds no /usr/bin/vtium-image \\(ADR-1101"),
                    (dict(base, **helper), [], "built without images, installs "
                                               "/usr/bin/vtium-image, /usr/lib/sysusers.d"),
                    (dict(base, **{t3_impl.IMAGE_FILES[0]: "x"}), ["logind"],
                     "installs /usr/bin/vtium-image, which only")):
                with self.subTest(files=sorted(files), features=features):
                    tree = os.path.join(d, "t%d" % len(os.listdir(d)))
                    write_tree(tree, files)
                    with self.assertRaisesRegex(SystemExit, why):
                        t3_impl.tree(tree, os.path.join(d, "x.tar"), features=features)
            tree = os.path.join(d, "good")
            write_tree(tree, dict(base, **helper))
            impl = t3_impl.tree(tree, os.path.join(d, "x.tar"), features=["images"])
            self.assertEqual(impl["tree_features"], ["images"])

    def test_check_guest(self):
        t3_scenario.check_guest(self.VARIANT, {"images": self.HELPER})
        t3_scenario.check_guest("2d-640x480-psf16", {"images": "none"})
        for helper in (None, "none", self.HELPER.replace("969 gid", "0 gid"),
                       self.HELPER.replace("gid 969", "gid 0"),
                       self.HELPER.replace("755", "4755")):
            with self.subTest(helper):
                installed = {} if helper is None else {"images": helper}
                with self.assertRaisesRegex(RuntimeError, "the guest's image helper is"):
                    t3_scenario.check_guest(self.VARIANT, installed)
        # guest/setup's report line carries the field, and check_install
        # reads it
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            text = f.read()
        self.assertIn("systemd-sysusers /usr/lib/sysusers.d/vtium-image.conf", text)
        self.assertIn('"images: $images;"', text)
        impl = {"tree_sha256": "a" * 64}
        line = ("TIER3-INSTALL-OK packages: ; kernel: 7.2.11-arch1-1; images: %s; tree: %s"
                % (self.HELPER, "a" * 64))
        self.assertEqual(t3_scenario.check_install(line, impl)["images"], self.HELPER)

    def test_disc(self):
        # The disc carries the test pictures beside the screens; a variant
        # without "pictures" carries none.
        with tempfile.TemporaryDirectory() as d:
            tree = os.path.join(d, "tree")
            write_tree(tree, ImplementationTest.TREE)
            impl = t3_impl.tree(tree, os.path.join(d, "tree.tar"))
            impl["fonts"] = os.path.join(d, "fonts")
            os.makedirs(impl["fonts"])
            with mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(self.VARIANT, d, impl)
                pictures = os.path.join(d, "disc", "screens", "pictures")
                self.assertEqual(sorted(os.listdir(pictures)), sorted(t3_pictures.files()))
                with open(os.path.join(pictures, "quad.six"), "rb") as f:
                    self.assertEqual(f.read(), t3_pictures.files()["quad.six"])
                t3_scenario.build_disc("2d-640x480-psf16", d, impl)
                self.assertFalse(os.path.exists(pictures))

    def test_screens_name_written_pictures(self):
        # Every picture a screen sends is one t3_pictures.py writes; the
        # tmux screen alone plays a clip and sends none of them.
        for screen in t3_evaluate.PICTURE_SCREENS:
            with open(os.path.join(t3_common.GUEST, "screens", screen + ".sh")) as f:
                text = f.read()
            if screen == "img-tmux-pane":
                self.assertNotIn("img-common.inc", text)
                continue
            named = set(re.findall(r'\$pics/([\w.-]+)', text)
                        + re.findall(r'(?:pic64|iterm2_send "[^"]*"|iterm2_parts "[^"]*") '
                                     r'([\w.-]+)', text))
            with self.subTest(screen):
                self.assertTrue(named)
                self.assertLessEqual(named, set(t3_pictures.files()))

    def test_missing_pictures(self):
        # A screen whose pictures are missing reports it, and its keys are
        # not pressed.
        events = ["T3-DONE img-scroll-before", "T3-NO-PICTURES", "T3-RSS img-scroll 1",
                  "T3-DONE img-scroll"]
        self.assertEqual(t3_scenario.reported_not_offered(events, "img-scroll"),
                         "T3-NO-PICTURES")
        with open(os.path.join(t3_common.GUEST, "screens", "img-common.inc")) as f:
            self.assertIn("echo T3-NO-PICTURES", f.read())

    def test_prelogin(self):
        # Before login the file's picture is refused, the inline one drawn,
        # and the helper runs as the variant's pre-login account
        # (ADR-1101 D2, D10).
        self.assertEqual(t3_scenario.variant_cases("images-640x480-nobody"), ["prelogin"])
        v = t3_scenario.VARIANTS["images-640x480-nobody"]
        self.assertEqual((v["image_account"], v["login"], v["agent"], v["prelogin"]),
                         ("nobody", False, True, True))
        self.assertIn("prelogin", t3_evaluate.NO_GOLDEN)
        quad = t3_pictures.PICTURES["quad"]

        def screen(*at):
            data = bytearray(640 * 480 * 3)
            for col, row in at:
                for y, line in enumerate(quad.pixels):
                    for x, p in enumerate(line):
                        i = (((row - 1) * 16 + y) * 640 + (col - 1) * 8 + x) * 3
                        data[i:i + 3] = bytes(t3_pictures.eight(p))
            return t3img.Image(640, 480, bytes(data))

        good = ["pty /dev/pts/0 root", "helpers vtium-image"]
        right = screen(t3_images.INLINE_AT)
        self.assertIsNone(t3_images.judge(good, right, "vtium-image"))
        self.assertIsNone(t3_images.judge(["pty /dev/pts/0 root", "helpers nobody"], right,
                                          "nobody"))
        for answer, img, account, why in (
                (good, right, "nobody", "run as vtium-image; they must run as nobody"),
                (["pty /dev/pts/0 root", "helpers none"], right, "vtium-image",
                 "run as nobody at all"),
                (["pty /dev/pts/0 root", "helpers vtium-image tester"], right, "vtium-image",
                 "run as vtium-image tester"),
                (["pty /dev/pts/0 tester", "helpers vtium-image"], right, "vtium-image",
                 "not root's"),
                (["refused: no login prompt on a pty"], right, "vtium-image", "refused"),
                (good, screen(), "vtium-image", "the inline picture shows 0 of its 3072"),
                (good, screen(t3_images.INLINE_AT, t3_images.FILE_AT), "vtium-image",
                 "the picture read from a file shows 3072 pixels")):
            with self.subTest(why):
                self.assertIn(why, t3_images.judge(answer, img, account))
        # the agent writes the pictures where the judge looks, and setup
        # removes the account for a variant whose helper runs as nobody
        with open(os.path.join(t3_common.GUEST, "t3-agent")) as f:
            agent = f.read()
        self.assertIn("prelogin) prelogin ;;", agent)
        self.assertIn("\\e[%d;%dH\\033_Ga=T,i=91,q=2,C=1,f=100,t=f;" % t3_images.FILE_AT[::-1],
                      agent)
        self.assertIn("\\033[%d;%dH\\033_Ga=T,i=92,q=2,C=1,f=100;" % t3_images.INLINE_AT[::-1],
                      agent)
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            setup = f.read()
        self.assertIn('if [ -f "$disc"/no-image-account ]; then\n        userdel vtium-image',
                      setup)
        self.assertIn("mount -t rpc_pipefs rpc_pipefs /run/t3-rpc", setup)
        t3_scenario.check_guest("images-640x480-nobody", {
            "images": "helper /usr/bin/vtium-image mode 755, account vtium-image removed"})
        with self.assertRaisesRegex(RuntimeError, "account vtium-image removed"):
            t3_scenario.check_guest("images-640x480-nobody", {"images": self.HELPER})
        with self.assertRaisesRegex(RuntimeError, "image helper is"):
            t3_scenario.check_guest(self.VARIANT, {
                "images": "helper /usr/bin/vtium-image mode 755, account vtium-image removed"})
        # the discs carry the flags setup reads
        with tempfile.TemporaryDirectory() as d:
            tree = os.path.join(d, "tree")
            write_tree(tree, ImplementationTest.TREE)
            impl = t3_impl.tree(tree, os.path.join(d, "tree.tar"))
            impl["fonts"] = os.path.join(d, "fonts")
            os.makedirs(impl["fonts"])
            disc = os.path.join(d, "disc")
            with mock.patch.object(t3_scenario, "make_iso"):
                for variant, want in ((self.VARIANT, {"rpc-pipefs", "t3-agent"}),
                                      ("images-640x480-nobody",
                                       {"no-image-account", "t3-agent"})):
                    t3_scenario.build_disc(variant, d, impl)
                    names = set(os.listdir(disc)) & {"rpc-pipefs", "no-image-account",
                                                     "t3-agent"}
                    self.assertEqual(names, want, variant)

    def test_refusal_fails_a_provisional_case(self):
        # A provisional golden judges nothing, but a picture screen that
        # drew no picture in a variant with images fails all the same.
        why = "drew no picture (T3-NO-PICTURES) in a variant with images"
        with tempfile.TemporaryDirectory() as d, mock.patch.object(t3_evaluate, "RUNDIR", d), \
                mock.patch.dict(t3_evaluate.PROVISIONAL, {self.VARIANT + "/img-sixel": "x"}):
            golden = os.path.join(d, "goldens", self.VARIANT, "img-sixel.png")
            os.makedirs(os.path.dirname(golden))
            t3img.write_png(golden, t3img.Image(8, 8, bytes(8 * 8 * 3)))
            root = os.path.join(d, "goldens")
            entry = t3_evaluate.evaluate(self.VARIANT, "img-sixel", golden, {}, LOG, root)
            self.assertEqual(entry["status"], "provisional")
            entry = t3_evaluate.evaluate(self.VARIANT, "img-sixel", golden,
                                         {"errors": {"img-sixel": why}}, LOG, root)
            self.assertEqual((entry["status"], entry["error"]), ("fail", why))
        with open(os.path.join(t3_common.HERE, "t3_scenario.py")) as f:
            self.assertIn('errors[name] = "drew no picture (%s) in a variant with images"',
                          f.read())

    def test_replies(self):
        # The replies a screen reported after the screen before it ended,
        # in order, held to REPLIES: a picture the helper did not decode,
        # a temporary file it kept, a delete that freed the wrong image,
        # or a reply missing, each fails.
        want = t3_evaluate.REPLIES["img-png"]
        events = ["T3-REPLY i=1;OK", "T3-DONE img-delete", "T3-RSS img-png 1"] + [
            "T3-REPLY " + r for r in want] + ["T3-DONE img-png"]
        self.assertEqual(t3_scenario.screen_replies(events, "img-png"), want)
        self.assertEqual(t3_scenario.screen_replies(events, "img-delete"), ["i=1;OK"])
        self.assertEqual(t3_scenario.screen_replies(events[:-1], "img-png"), [])
        self.assertIsNone(t3_scenario.judge_replies("img-png", want))
        for got in ([want[0], "i=42;EINVAL:image data could not be decoded"] + want[2:],
                    want[:3] + ["kept"] + want[4:], want[:-1], []):
            with self.subTest(got):
                self.assertRegex(t3_scenario.judge_replies("img-png", got),
                                 "^the screen reported .*; it must report i=41;OK, ")
        self.assertEqual(t3_scenario.judge_replies("img-delete", []),
                         "the screen reported no reply; it must report i=32;OK, "
                         "i=33;ENOENT:Put command refers to non-existent image with id: 33 "
                         "and number: 0")
        # the replies kitty 0.50.0 gives (ADR-1100 D15): a put of a freed
        # image in kitty's own text, a frame's reply naming the frame it
        # made, and no reply to an animation control that succeeds
        self.assertIsNone(t3_scenario.judge_replies("img-delete", [
            "i=32;OK", "i=33;ENOENT:Put command refers to non-existent image with id: 33 and "
                       "number: 0"]))
        self.assertIsNotNone(t3_scenario.judge_replies("img-delete", [
            "i=32;OK", "i=33;ENOENT:image not found"]))
        self.assertIsNone(t3_scenario.judge_replies("img-animation", [
            "i=50,r=2;OK", "i=50,r=3;OK", "i=50,r=4;OK", "i=51,r=2;OK", "i=51,r=3;OK"]))
        self.assertIsNotNone(t3_scenario.judge_replies("img-animation", [
            "i=50;OK"] * 6 + ["i=51;OK"] * 4))
        with open(os.path.join(t3_common.GUEST, "screens", "img-animation.sh")) as f:
            self.assertNotRegex(f.read(), r'(?m)^ask_event "a=a,')
        # each screen reports as many times as REPLIES lists for it, each
        # report a line of its own that calls reply_event, ask_event or
        # cursor_event (img-common.inc)
        for screen, replies in t3_evaluate.REPLIES.items():
            with self.subTest(screen), open(os.path.join(t3_common.GUEST, "screens",
                                                         screen + ".sh")) as f:
                self.assertEqual(len(re.findall(r'^(?:reply|ask|cursor)_event "', f.read(),
                                                re.M)),
                                 len(replies))

    def bash(self, script, pics=""):
        """The bytes `script` prints in bash after it sources
        img-common.inc, with the test pictures read from `pics`."""
        inc = os.path.join(t3_common.GUEST, "screens", "img-common.inc")
        return subprocess.run(["bash", "-c", '. "$1"; pics=$2; ' + script, "bash", inc, pics],
                              capture_output=True, check=True).stdout

    def test_placeholder_bytes(self):
        # A placeholder cell is U+10EEEE and a diacritic for each argument,
        # the marks the first eight of vtium-term's table (ADR-1100 D13).
        self.assertEqual(self.bash("placeholder 2 3 1"),
                         "\U0010EEEE\u030E\u0310\u030D".encode())
        self.assertEqual(self.bash("placeholder"), "\U0010EEEE".encode())
        with open(os.path.join(t3_common.ROOT, "crates", "vtium-term", "src",
                               "placeholder.rs")) as f:
            table = f.read().split("DIACRITICS: [char; 297] = [", 1)[1]
        first = [chr(int(h, 16)) for h in re.findall(r"'\\u\{([0-9A-Fa-f]+)\}'", table)[:8]]
        self.assertEqual(len(first), 8)
        for n, mark in enumerate(first):
            with self.subTest(n):
                self.assertEqual(self.bash("placeholder %d" % n),
                                 ("\U0010EEEE" + mark).encode())

    def test_iterm2_senders(self):
        # iterm2_send sends one File= string; iterm2_parts sends
        # MultipartFile=, FileParts of the length asked for, which rejoin
        # to the file byte for byte, and FileEnd (ADR-1103 D1).
        png = t3_pictures.files()["quad.png"]
        with tempfile.TemporaryDirectory() as d:
            t3_pictures.write(d)
            one = self.bash('iterm2_send "inline=1;width=32px" quad.png', d)
            parts = self.bash('iterm2_parts "inline=1;width=16;height=6" quad.png 30', d)
        self.assertEqual(one, b"\x1b]1337;File=inline=1;width=32px:"
                         + base64.b64encode(png) + b"\x07")
        strings = parts.split(b"\x07")
        self.assertEqual(strings[-1], b"")
        self.assertEqual(strings[0], b"\x1b]1337;MultipartFile=inline=1;width=16;height=6")
        self.assertEqual(strings[-2], b"\x1b]1337;FileEnd")
        pieces = [p[len(b"\x1b]1337;FilePart="):] for p in strings[1:-2]]
        self.assertTrue(all(p.startswith(b"\x1b]1337;FilePart=") for p in strings[1:-2]))
        self.assertEqual([len(p) for p in pieces[:-1]], [30] * (len(pieces) - 1))
        self.assertLessEqual(len(pieces[-1]), 30)
        self.assertGreater(len(pieces), 2)
        self.assertEqual(base64.b64decode(b"".join(pieces)), png)

    def test_shows(self):
        # Each screen SHOWS names is shown by its golden, and not by another
        # screen's, so a capture of the wrong frame or of no picture fails.
        self.assertEqual(sorted(t3_evaluate.SHOWS),
                         ["img-animation", "img-iterm2", "img-placeholder"])
        for name in t3_evaluate.SHOWS:
            with self.subTest(name):
                golden = t3img.read(os.path.join(t3_evaluate.GOLDEN, self.VARIANT,
                                                 name + ".png"))
                self.assertIsNone(t3_evaluate.not_shown(name, golden))
                other = t3img.read(os.path.join(t3_evaluate.GOLDEN, self.VARIANT,
                                                "img-kitty.png"))
                self.assertRegex(t3_evaluate.not_shown(name, other),
                                 "^the screen settled without showing ")

    def test_capture_waits_for_the_pictures(self):
        # The driver captures a screen SHOWS names once it has settled
        # showing its pictures: a middle frame that holds still for a
        # second is not taken, a screen that never shows them fails its
        # case and the run goes on, and any other screen that never
        # settles stops the run as before.
        def paint(*pictures):
            data = bytearray(640 * 480 * 3)
            for name, col, row in pictures:
                for y, line in enumerate(t3_pictures.PICTURES[name].pixels):
                    for x, p in enumerate(line):
                        if p:
                            i = (((row - 1) * 16 + y) * 640 + (col - 1) * 8 + x) * 3
                            data[i:i + 3] = bytes(t3_pictures.eight(p))
            return t3img.Image(640, 480, bytes(data))

        before = paint(("quad", 3, 3))
        first, last = paint(("quad", 15, 3)), paint(("quad-turned-twice", 15, 3))

        class Clock:
            now = 0.0

            def time(self):
                return self.now

            def sleep(self, seconds):
                self.now += seconds

        class VM:
            def __init__(self, frames):
                self.frames = frames

            def screendump(self, path, output=None):
                img = self.frames.pop(0) if len(self.frames) > 1 else self.frames[0]
                t3_qemu.write_ppm(path, img)
                return img

        with tempfile.TemporaryDirectory() as d, \
                mock.patch.object(t3_qemu, "time", Clock()):
            path = os.path.join(d, "img-iterm2.ppm")
            img, why = t3_scenario.capture_screen(VM([first, first, last]), "img-iterm2", path,
                                                  before)
            self.assertEqual((img.data == last.data, why), (True, None))
            # the quarters show where the last frame should, all but its white frame
            img, why = t3_scenario.capture_screen(VM([first]), "img-iterm2", path, before)
            self.assertEqual(img.data, first.data)
            self.assertEqual(why, "the screen settled without showing quad-turned-twice at "
                                  "column 15, row 3 (2852 pixels differ)")
            with self.assertRaisesRegex(RuntimeError, "not in the expected state"):
                t3_scenario.capture_screen(VM([before]), "img-kitty", path, before)
        with open(os.path.join(t3_common.HERE, "t3_scenario.py")) as f:
            self.assertIn("last, why = capture_screen(vm, name, path, last)", f.read())

    def test_make_target(self):
        # make tier3-images builds the tree with images and any features
        # FEATURES adds, and runs the images variant unless VARIANTS names
        # others.
        bare = {k: v for k, v in os.environ.items() if k not in ("MAKEFLAGS", "MFLAGS")}
        both = ["images-640x480", "images-640x480-nobody"]
        for argv, features, variants in (
                ([], "images", both),
                (["FEATURES=logind"], "images logind", both),
                (["FEATURES=images,logind"], "images logind", both),
                (["VARIANTS=2d-potato-min"], "images", ["2d-potato-min"])):
            with self.subTest(argv):
                r = subprocess.run(["make", "-n", "--no-print-directory", "tier3-images"] + argv,
                                   cwd=t3_common.ROOT, capture_output=True, text=True,
                                   check=True, env=bare)
                runs = [l for l in r.stdout.replace("\\\n", " ").splitlines()
                        if "/t3.py run " in l]
                self.assertEqual(len(runs), 1)
                args = shlex.split(runs[0].split("/t3.py run ", 1)[1])
                last = len(args) - 1 - args[::-1].index("--features")
                self.assertEqual(args[last + 1], features)
                self.assertEqual(args[last + 2:], ["--tree-target", "install-tree"] + variants)


class PictureGoldenTest(unittest.TestCase):
    """The images variant's goldens, read where each screen places its
    pictures: the pixels of t3_pictures.py's pictures, cell for cell, at
    the 8x16 cells of 640x480. A golden that shows a picture a cell off, cut
    where nothing cut it, or missing fails here, whether or not the golden
    is still provisional."""

    VARIANT = "images-640x480"
    BLACK = (0, 0, 0)
    # The default foreground, which the text over the pictures is drawn in.
    GLYPH = (229, 229, 229)

    def golden(self, screen):
        return t3img.read(os.path.join(t3_evaluate.GOLDEN, self.VARIANT, screen + ".png"))

    @staticmethod
    def pixel(img, x, y):
        i = (y * img.width + x) * 3
        return tuple(img.data[i:i + 3])

    def placed(self, img, name, col, row, rows=None, part=None):
        """Assert picture `name` is drawn at its own size with its top left
        corner at cell (`col`, `row`), counted from 1, its opaque pixels in
        their colors: only its pixel rows `rows` (a range) when given, and
        only the pixel box `part` (x0, y0, x1, y1) of it, drawn at the cell,
        when given (t3_pictures.shows)."""
        wrong = t3_pictures.shows(img, name, col, row, rows=rows, part=part)
        self.assertEqual(wrong[:4], [], "%s at column %d, row %d: %d pixels differ"
                         % (name, col, row, len(wrong)))

    def shows_through(self, img, name, col, row):
        """Assert text drawn under picture `name`, placed at cell (`col`,
        `row`), shows where the picture is clear: some of its clear pixels
        are glyph pixels."""
        pic = t3_pictures.PICTURES[name]
        left, top = (col - 1) * 8, (row - 1) * 16
        lit = [(x, y) for y, line in enumerate(pic.pixels) for x, p in enumerate(line)
               if p is None and self.pixel(img, left + x, top + y) == self.GLYPH]
        self.assertTrue(lit, "no text shows through %s's clear pixels at column %d, row %d"
                        % (name, col, row))

    def blank(self, img, col, row, cols, rows):
        """Assert the cells from (`col`, `row`), `cols` by `rows`, are black."""
        left, top = (col - 1) * 8, (row - 1) * 16
        lit = [(x, y) for y in range(top, top + rows * 16) for x in range(left, left + cols * 8)
               if self.pixel(img, x, y) != self.BLACK]
        self.assertEqual(lit[:4], [], "cells at column %d, row %d not blank" % (col, row))

    def test_reads_every_golden(self):
        # Each of the variant's goldens is read by a test of this class, so
        # renaming or dropping one cannot leave a golden unchecked.
        read = set()

        class Recording(type(self)):
            def golden(inner, screen):
                read.add(screen)
                return PictureGoldenTest.golden(inner, screen)

        for name in dir(self):
            if name.startswith("test_") and name != "test_reads_every_golden":
                getattr(Recording(name), name)()
        want = {t3_evaluate.same_as(c, self.VARIANT)
                for c in t3_scenario.variant_cases(self.VARIANT)
                if c not in t3_evaluate.NO_GOLDEN}
        self.assertEqual(want, set(t3_evaluate.PICTURE_SCREENS) | {"img-scrollback"})
        self.assertEqual(read, want)

    def test_sixel(self):
        img = self.golden("img-sixel")
        self.placed(img, "quad", 5, 3)
        self.placed(img, "disc", 44, 3)
        self.shows_through(img, "disc", 44, 3)

    def scaled(self, img, col, row, width, height):
        """Assert the quarters, scaled to `width` by `height` pixels with
        their top left corner at cell (`col`, `row`), show each quarter's
        color over the middle of its quarter of that box, away from the
        edges a scaler blends."""
        left, top = (col - 1) * 8, (row - 1) * 16
        w, h = width // 2, height // 2
        for (qx, qy), color in (((0, 0), t3_pictures.RED), ((1, 0), t3_pictures.GREEN),
                                ((0, 1), t3_pictures.BLUE), ((1, 1), t3_pictures.YELLOW)):
            for x in range(left + qx * w + w // 8, left + qx * w + w - w // 8, 3):
                for y in range(top + qy * h + h // 6, top + qy * h + h - h // 6, 3):
                    self.assertEqual(self.pixel(img, x, y), t3_pictures.eight(color), (x, y))

    def test_kitty(self):
        img = self.golden("img-kitty")
        self.placed(img, "quad", 5, 3)
        self.placed(img, "quad", 30, 3)
        self.placed(img, "quad", 5, 8, part=(32, 24, 64, 48))
        self.scaled(img, 30, 8, 128, 96)

    def test_relative(self):
        img = self.golden("img-relative")
        for col, row in ((3, 4), (49, 12)):
            with self.subTest(parent=(col, row)):
                self.placed(img, "parent", col, row)
                self.placed(img, "child", col + 8, row + 1)
                self.placed(img, "child", col + 6, row + 3)
        # the moved chain left nothing where it was first placed
        self.blank(img, 41, 4, 16, 5)

    def test_overwrite(self):
        img = self.golden("img-overwrite")
        # the sixel: its top row whole, its middle row cut by the text, and
        # 4 cells of its bottom row cut by the erase
        self.placed(img, "quad", 3, 3, rows=range(0, 16))
        pic = [t3_pictures.eight(c) for c in (t3_pictures.RED, t3_pictures.GREEN,
                                              t3_pictures.BLUE, t3_pictures.YELLOW)]
        row4 = [self.pixel(img, x, y) for x in range(16, 80) for y in range(48, 64)]
        self.assertFalse(set(row4) & set(pic), "the text left the sixel under it")
        self.placed(img, "quad", 3, 5, rows=range(32, 48), part=(0, 32, 16, 48))
        self.blank(img, 5, 5, 4, 1)
        self.placed(img, "quad", 9, 5, rows=range(32, 48), part=(48, 32, 64, 48))
        # the kitty picture above text is whole; the one below text is whole
        # on the rows nothing was written on
        self.placed(img, "quad", 23, 3)
        self.placed(img, "quad", 43, 3, rows=list(range(0, 16)) + list(range(32, 48)))
        # on row 4 of the picture below text, columns 43 to 50, the # glyphs
        # are drawn over the picture, which shows between them
        seen = {self.pixel(img, x, y) for x in range(42 * 8, 50 * 8) for y in range(48, 64)}
        self.assertIn(self.GLYPH, seen)
        self.assertTrue({t3_pictures.eight(c) for c in (t3_pictures.RED, t3_pictures.GREEN,
                                                        t3_pictures.BLUE, t3_pictures.YELLOW)}
                        <= seen)

    def test_delete(self):
        img = self.golden("img-delete")
        self.placed(img, "quad", 3, 3)
        self.placed(img, "quad", 13, 9)
        for col in (13, 23, 33):
            with self.subTest(col):
                self.blank(img, col, 3, 8, 3)

    def test_scroll(self):
        img = self.golden("img-scroll")
        for col in (3, 20):
            with self.subTest(col):
                self.placed(img, "bands", col, 1, part=(0, 32, 64, 96))
        img = self.golden("img-scrollback")
        for col in (3, 20):
            with self.subTest(scrollback=col):
                self.placed(img, "bands", col, 2)

    def test_tmux_page(self):
        # tmux has exited and the page cleared: grey text on black, no
        # colored pixel of the video left anywhere
        img = self.golden("img-tmux-pane")
        colored = [(x, y) for y in range(img.height) for x in range(img.width)
                   if len(set(self.pixel(img, x, y))) > 1]
        self.assertEqual(colored[:4], [])
        self.assertNotEqual(img.data, bytes(len(img.data)))

    def test_animation(self):
        # the quarters on the frame the screen stopped them on, and the
        # child on the last of its frames, played once
        img = self.golden("img-animation")
        self.placed(img, "anim-stopped", 5, 3)
        self.placed(img, "anim-ended", 20, 3)

    def test_iterm2(self):
        img = self.golden("img-iterm2")
        self.placed(img, "quad", 3, 3)
        self.placed(img, t3_pictures.GIF_FRAMES[-1], 15, 3)
        self.scaled(img, 27, 3, 128, 96)
        self.scaled(img, 47, 3, 32, 24)

    def test_placeholder(self):
        # the picture followed its cells from column 5 to 35, and the
        # 32-bit image id drew the child
        img = self.golden("img-placeholder")
        self.placed(img, "quad", 35, 3)
        self.blank(img, 5, 3, 8, 3)
        self.placed(img, "child", 5, 9)

    def test_png(self):
        img = self.golden("img-png")
        self.placed(img, "quad", 3, 3)
        self.placed(img, "disc", 23, 3)
        self.shows_through(img, "disc", 23, 3)
        self.placed(img, "quad", 43, 3)
        self.placed(img, "quad", 63, 3)


class DefaultVariantsTest(unittest.TestCase):
    """The default display variants: their console built with images, the
    goldens that judge each case, and the benchmark's CPU figure."""

    VARIANT = "2d-1366x768-potato"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.root = os.path.join(self.dir.name, "goldens")
        os.makedirs(os.path.join(self.root, self.VARIANT))

    def golden(self, screen):
        t3img.write_png(os.path.join(self.root, self.VARIANT, screen + ".png"),
                        t3img.Image(8, 8, bytes(8 * 8 * 3)))

    def test_default_variants(self):
        # Every default variant draws the image screens, so its console is
        # built with images, and the GL one, which draws on the GPU path,
        # with gpu too; goldens judge each one.
        for name in t3_scenario.DEFAULT_VARIANTS:
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                features = ["gpu", "images"] if name == "gl-1280x800" else ["images"]
                self.assertEqual(v["tree_features"], features)
                self.assertIn(name, t3_scenario.golden_cases())
                self.assertEqual(t3_scenario.check_tree_features([name], {"tree_features": []}),
                                 ["%s needs a tree whose console is built with %s, and the "
                                  "tree's was built with no features; make tier3 and make "
                                  "tier3-images build one" % (name, " ".join(features))])

    def test_density(self):
        # The default variants' configurations state the density that draws
        # the text font set's first source, DejaVu Sans Mono, at 16 points in
        # a 10x19 cell:
        # 16 x 86 / 72 = 19.1 pixels, and 32 points in a 38-pixel cell.
        for name in t3_scenario.DEFAULT_VARIANTS:
            with self.subTest(name):
                with open(os.path.join(t3_common.GUEST, t3_scenario.VARIANTS[name]["conf"])) as f:
                    conf = f.read()
                self.assertIn("\ndisplay-dpi=86\n", conf)
                size = int(re.search(r"^font-size=(\d+)$", conf, re.M).group(1))
                self.assertEqual(round(size * 86 / 72), {16: 19, 32: 38}[size])

    def test_the_report_names_each_golden(self):
        # The report names the golden that judged each case: its own in
        # GOLDEN/VARIANT/, vt-restore cjk's, photo-scrollforward
        # photo-scroll's, and none for a check.
        with mock.patch.object(t3_evaluate, "GOLDEN", self.root):
            meta = t3_scenario.failure_meta("no boot")
            goldens = {e["name"].split("/", 1)[1]: e["golden"]
                       for e in t3_scenario.evaluate_variant(self.VARIANT, {}, meta, LOG)}
        named = lambda screen: t3_common.rel(os.path.join(self.root, self.VARIANT,
                                                          screen + ".png"))
        for case, screen in (("cjk", "cjk"), ("vt-restore", "cjk"), ("sgr", "sgr"),
                             ("photo-scrollforward", "photo-scroll")):
            with self.subTest(case):
                self.assertEqual(goldens[case], named(screen))
        self.assertIsNone(goldens["vt-switch"])

    def test_the_gpu_variant_is_judged_by_the_cpu_paths_captures(self):
        # gpu-1280x800 draws 2d-1280x800's screens with hwaccel on, on a
        # guest render node, must log the gpu renderer, and no golden
        # judges it: each case is judged by 2d-1280x800's capture of it in
        # the same run, byte for byte (ADR-1303 D1)
        v = t3_scenario.VARIANTS["gpu-1280x800"]
        self.assertEqual((v["renderer"], v["conf"], v["verdict"]),
                         ("gpu", "vtium-hwaccel.conf", "capture"))
        self.assertEqual(t3_scenario.SAME_AS_CAPTURE["gpu-1280x800"], "2d-1280x800")
        self.assertTrue(t3_scenario.uses_virgl("gpu-1280x800"))
        self.assertIn("gpu", v["tree_features"])
        self.assertNotIn("gpu-1280x800", t3_scenario.golden_cases())
        with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
            self.assertIn("\nhwaccel\n", f.read())
        # a console that drew the variant on the CPU path fails every case
        errors, drop = t3_scenario.variant_errors(
            None, ["Display Virtual-1 with backend [drm2d] text renderer [cpu], cell size 10x19"],
            "gpu", {}, {}, None)
        self.assertTrue(drop)
        self.assertIn("did not draw with gpu", errors[0])

    def test_the_gl_variant_is_judged_by_the_cpu_paths_goldens(self):
        # gl-1280x800 draws with hwaccel on, must log the gpu renderer, and
        # each case is judged by 2d-1280x800's golden of it, exactly, as
        # the GPU path draws the CPU path's bytes (ADR-1303 D1, D11). The
        # variant has no goldens of its own.
        v = t3_scenario.VARIANTS["gl-1280x800"]
        self.assertEqual((v["renderer"], v["conf"]), ("gpu", "vtium-gl.conf"))
        self.assertTrue(t3_scenario.uses_virgl("gl-1280x800"))
        self.assertIn("gpu", v["tree_features"])
        self.assertNotIn("verdict", v)
        self.assertIn("gl-1280x800", t3_scenario.golden_cases())
        self.assertEqual(t3_evaluate.GOLDENS_OF["gl-1280x800"], "2d-1280x800")
        self.assertFalse(os.path.exists(os.path.join(t3_evaluate.GOLDEN, "gl-1280x800")))
        with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
            self.assertIn("\nhwaccel\n", f.read())
        for case, screen in (("sgr", "sgr"), ("vt-restore", "cjk"),
                             ("photo-scrollforward", "photo-scroll")):
            with self.subTest(case):
                self.assertEqual(t3_evaluate.golden_for("gl-1280x800", case),
                                 t3_evaluate.golden_for("2d-1280x800", screen))
        # a golden of 2d-1280x800's in this tree's goldens: a capture with
        # the same bytes passes, one pixel off fails, and a provisional 2d
        # golden makes the gl case provisional too
        os.makedirs(os.path.join(self.root, "2d-1280x800"))
        black = t3img.Image(4, 4, bytes(4 * 4 * 3))
        t3img.write_png(os.path.join(self.root, "2d-1280x800", "sgr.png"), black)
        ppm = os.path.join(self.dir.name, "sgr.ppm")
        run = os.path.join(self.dir.name, "run")
        with mock.patch.object(t3_evaluate, "RUNDIR", run), \
                mock.patch.object(t3_evaluate, "PROVISIONAL", {}):
            t3_qemu.write_ppm(ppm, black)
            entry = t3_evaluate.evaluate("gl-1280x800", "sgr", ppm, {}, LOG, root=self.root)
            self.assertEqual(entry["status"], "pass")
            self.assertEqual(entry["golden"], t3_common.rel(
                os.path.join(self.root, "2d-1280x800", "sgr.png")))
            t3_qemu.write_ppm(ppm, t3img.Image(4, 4, bytes([1]) + bytes(4 * 4 * 3 - 1)))
            entry = t3_evaluate.evaluate("gl-1280x800", "sgr", ppm, {}, LOG, root=self.root)
            self.assertEqual(entry["status"], "fail")
            with mock.patch.object(t3_evaluate, "PROVISIONAL", {"2d-1280x800/sgr": "why"}):
                entry = t3_evaluate.evaluate("gl-1280x800", "sgr", ppm, {}, LOG, root=self.root)
                self.assertEqual(entry["status"], "provisional")
            # a provisional row in the gl variant's own name judges nothing:
            # the case stays failed, and the preflight refuses the row
            with mock.patch.object(t3_evaluate, "PROVISIONAL", {"gl-1280x800/sgr": "why"}):
                entry = t3_evaluate.evaluate("gl-1280x800", "sgr", ppm, {}, LOG, root=self.root)
                self.assertEqual(entry["status"], "fail")
            self.assertEqual(
                t3_evaluate.check_goldens({"gl-1280x800": ["sgr"]}, self.root,
                                          {"gl-1280x800/sgr": "why"}),
                ["provisional.tsv lists gl-1280x800/sgr, which has no golden"])
            # a golden of 2d-1280x800's that only gl-1280x800 compares with
            # is used
            self.assertEqual(t3_evaluate.check_goldens({"gl-1280x800": ["sgr"]}, self.root), [])
            # a golden in a gl-1280x800 directory is one no case draws
            os.makedirs(os.path.join(self.root, "gl-1280x800"))
            t3img.write_png(os.path.join(self.root, "gl-1280x800", "sgr.png"), black)
            self.assertEqual(
                t3_evaluate.check_goldens({"gl-1280x800": ["sgr"]}, self.root),
                ["%s is a golden no case draws" % t3_common.rel(
                    os.path.join(self.root, "gl-1280x800", "sgr.png"))])
            os.unlink(os.path.join(self.root, "gl-1280x800", "sgr.png"))
            # with sgr none of the variant's cases, its 2d golden is a stray
            self.assertEqual(
                t3_evaluate.check_goldens({"gl-1280x800": ["cjk"]}, self.root),
                ["%s is a golden no case draws" % t3_common.rel(
                    os.path.join(self.root, "2d-1280x800", "sgr.png"))])
        # a console that drew the variant on the CPU path fails every case
        errors, drop = t3_scenario.variant_errors(
            None, ["Display Virtual-1 with backend [drm2d] text renderer [cpu], cell size 10x19"],
            "gpu", {}, {}, None)
        self.assertTrue(drop)

    def test_judge_by_capture_passes_identical_bytes_and_refuses_a_run_without_the_cpu_variant(self):
        run = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, run)
        black = t3img.Image(2, 2, bytes(12))
        white = t3img.Image(2, 2, bytes([255]) * 12)
        for variant, case, img in (("gpu-1280x800", "same", black),
                                   ("2d-1280x800", "same", black),
                                   ("gpu-1280x800", "other", black),
                                   ("2d-1280x800", "other", white),
                                   ("gpu-1280x800", "lonely", black)):
            os.makedirs(os.path.join(run, variant), exist_ok=True)
            t3img.write_png(os.path.join(run, variant, case + ".ppm"), img)

        def entry(variant, case, status):
            return {"name": "%s/%s" % (variant, case), "status": status}

        def results():
            gpu = [entry("gpu-1280x800", c, "capture") for c in ("same", "other", "lonely")]
            gpu += [entry("gpu-1280x800", "broken", "fail"),
                    entry("gpu-1280x800", "vt-switch", "pass")]
            cpu = [entry("2d-1280x800", c, "fail") for c in ("same", "other")]
            return {"gpu-1280x800": (gpu, {}), "2d-1280x800": (cpu, {})}

        with mock.patch.object(t3_scenario, "RUNDIR", run):
            both = results()
            t3_scenario.judge_by_capture(["2d-1280x800", "gpu-1280x800"], both)
            got = {e["name"].split("/")[1]: (e["status"], e.get("error"))
                   for e in both["gpu-1280x800"][0]}
            # the CPU variant's own verdict, here a golden's failure, does
            # not move to the GPU variant
            self.assertEqual(got["same"], ("pass", None))
            self.assertEqual(got["other"],
                             ("fail", "4 pixel(s) differ from 2d-1280x800's capture"))
            self.assertEqual(got["lonely"], ("fail", "2d-1280x800 drew no capture of lonely"))
            self.assertEqual(got["broken"], ("fail", None))
            self.assertEqual(got["vt-switch"], ("pass", None))
            self.assertEqual([e["status"] for e in both["2d-1280x800"][0]], ["fail", "fail"])
            # a run without 2d-1280x800 judges nothing as passed
            alone = results()
            t3_scenario.judge_by_capture(["gpu-1280x800"], alone)
            for e in alone["gpu-1280x800"][0][:3]:
                self.assertEqual(e["status"], "fail", e["name"])
                self.assertIn("which this run did not draw", e["error"])

    def test_goldens_are_cases(self):
        # A golden of the variant must be one of its cases.
        self.golden("cjk")
        self.golden("stray")
        cases = {v: c for v, c in t3_scenario.golden_cases().items() if v == self.VARIANT}
        self.assertEqual(t3_evaluate.check_goldens(cases, self.root),
                         ["%s is a golden no case draws" % t3_common.rel(
                             os.path.join(self.root, self.VARIANT, "stray.png"))])

    def test_tree_features(self):
        # A tree built for a run has the features declared and those its
        # variants need, each once.
        for declared, variants, want in (
                (None, ["2d-potato-min"], []),
                (None, t3_scenario.DEFAULT_VARIANTS, ["gpu", "images"]),
                (None, ["2d-1280x800", "2d-7680x2160"], ["images"]),
                (["logind"], [self.VARIANT], ["images", "logind"]),
                (["images"], [self.VARIANT, "2d-potato-min"], ["images"]),
                ([], ["images-640x480", "2d-640x480-psf16"], ["images"])):
            with self.subTest(declared=declared, variants=variants):
                self.assertEqual(t3.tree_features(declared, variants), want)

    def test_provisional_rate(self):
        # A provisional golden passes the case's pixels, but not its
        # figures: mpv-sixel with a provisional golden is still judged on
        # its rate, so a video that showed no frame fails, and one that
        # kept up stays provisional.
        variant = "2d-1280x800"
        golden = t3_evaluate.golden_for(variant, "mpv-sixel", t3_evaluate.GOLDEN)[0]
        # the maintainer accepted mpv-sixel's golden on 2026-11-05, so the
        # test lists it as provisional itself
        listed = mock.patch.dict(t3_evaluate.PROVISIONAL,
                                 {"%s/mpv-sixel" % variant: "a provisional golden for this test"})
        listed.start()
        self.addCleanup(listed.stop)
        ppm = os.path.join(self.dir.name, "mpv-sixel.ppm")
        t3_qemu.write_ppm(ppm, t3img.read(golden))
        fine = {"mpv_frames": 150, "mpv_per_s": 29.1, "presented_per_s": 59.0,
                "console_cpu_ms_per_mpv_frame": 6.733}
        for figures, status in ((dict(fine, mpv_frames=0), "fail"), (fine, "provisional"),
                                (dict(fine, console_cpu_ms_per_mpv_frame=375.8), "fail")):
            with self.subTest(figures=figures), \
                    mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")):
                entry = t3_evaluate.evaluate(variant, "mpv-sixel", ppm,
                                             {"bench": {"mpv-sixel": figures}}, LOG,
                                             t3_evaluate.GOLDEN)
                self.assertEqual(entry["status"], status)
                self.assertIn("judged, it would be pass", entry["note"])

    def test_console_pid(self):
        # acceptance.inc's console_pid finds the console a benchmark screen
        # runs under: the nearest ancestor named vtium, or kmscon, and none
        # under any other name, so the CPU figure is never another
        # program's.
        with open(os.path.join(t3_common.GUEST, "screens", "acceptance.inc")) as f:
            func = re.search(r"^console_pid\(\) \{\n.*?^\}\n", f.read(), re.M | re.S).group(0)
        script = os.path.join(self.dir.name, "pid.sh")
        with open(script, "w") as f:
            f.write(func + "console_pid || echo none\n")
        for name, found in (("vtium", True), ("kmscon", True), ("vtium-config", False)):
            with self.subTest(name):
                console = os.path.join(self.dir.name, name)
                shutil.copy("/bin/bash", console)
                out = subprocess.run([console, "-c", "echo $$; bash %s; true" % shlex.quote(script)],
                                     capture_output=True, text=True, check=True).stdout.split()
                self.assertEqual(out[1], out[0] if found else "none")


class ShowcaseTest(unittest.TestCase):
    """The showcase (ADR-1502 D6, t3_showcase.py): its variant, whose cases
    are captures that judge nothing, its scenes, its pinned fonts and frits,
    its pictures, and the page it composes from a run."""

    VARIANT = "showcase-1920x1080"
    SCREENS = ["showcase-shell", "showcase-double-size",
               "showcase-fonts-iosevka", "showcase-fonts-terminus", "showcase-fonts-unifont",
               "showcase-fonts-jetbrains", "showcase-glyph-fit", "showcase-ligatures",
               "showcase-syllables", "showcase-emoji", "showcase-scripts",
               "showcase-text", "showcase-color", "showcase-boxes", "showcase-pictures",
               "showcase-pointer"]
    # Each scene's category and whether it is the category's hero, as
    # ADR-1502 D6 and its hero rule place them.
    PLACES = {"showcase-shell.sh": ("front page", False),
              "showcase-double-size.sh": ("text and fonts", True),
              "showcase-fonts.inc": ("text and fonts", False),
              "showcase-glyph-fit.sh": ("text and fonts", False),
              "showcase-ligatures.sh": ("text and fonts", False),
              "showcase-syllables.sh": ("scripts and emoji", True),
              "showcase-emoji.sh": ("scripts and emoji", False),
              "showcase-scripts.sh": ("scripts and emoji", False),
              "showcase-text.sh": ("styles and modes", True),
              "showcase-color.sh": ("color", True),
              "showcase-boxes.sh": ("lines and blocks", True),
              "showcase-pictures.sh": ("pictures", True),
              "showcase-pointer.sh": ("input and pointer", True)}
    FIGURES = {"showcase-osc52-copy.svg": "input and pointer",
               "showcase-rtl-paragraphs.svg": "scripts and emoji",
               "showcase-synchronized-output.svg": "styles and modes",
               "concepts-heads-parts.svg": "displays and VTs",
               "concepts-heads-mirror.svg": "displays and VTs",
               "concepts-vt-handover.svg": "displays and VTs"}

    # A grid of three panes of 4x3 pixels for the stand-in run's captures,
    # which are 32 pixels wide and 12 tall.
    PANES = [("double-size.png", 0, 0), ("emoji.png", 28, 9), ("hero.png", 5, 4)]

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def test_variant(self):
        # The default machine at 1920x1080 with a build with images and bidi
        # on the CPU renderer, run only when named, its cases the scenes alone: no VT
        # switch, no golden.
        v = t3_scenario.VARIANTS[self.VARIANT]
        self.assertIn("xres=1920,yres=1080", v["device"])
        self.assertEqual((v["cpus"], v["memory_mb"], v["renderer"], v["tree_features"]),
                         (4, 4096, "cpu", ["bidi", "images"]))
        self.assertEqual((v["verdict"], v["conf"]), ("capture", "vtium-showcase.conf"))
        self.assertEqual(t3_scenario.variant_cases(self.VARIANT), self.SCREENS)
        self.assertEqual(t3_showcase.SCREENS, self.SCREENS)
        self.assertNotIn(self.VARIANT, t3_scenario.DEFAULT_VARIANTS)
        self.assertFalse(os.path.exists(os.path.join(t3_common.GOLDEN, self.VARIANT)))
        self.assertNotIn(self.VARIANT, t3_scenario.golden_cases())

    def evaluated(self, captured=True, meta=None):
        cap = None
        if captured:
            cap = os.path.join(self.dir.name, "cap.png")
            t3img.write_png(cap, t3img.Image(8, 8, bytes(8 * 8 * 3)))
        with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")):
            return t3_evaluate.evaluate(self.VARIANT, "showcase-text", cap, meta or {}, LOG,
                                        verdict="capture")

    def test_capture_judges_nothing(self):
        # A capture carries the status capture, with no golden and no
        # difference counted; a case with no capture, or one the scenario
        # found wrong, fails.
        e = self.evaluated()
        self.assertEqual((e["status"], e["golden"], e["stats"]["pixels"]),
                         ("capture", None, 0))
        self.assertTrue(e["actual"].endswith("showcase-text.png"))
        self.assertEqual(self.evaluated(captured=False)["status"], "fail")
        e = self.evaluated(meta={"errors": {"showcase-text": "the font switch failed"}})
        self.assertEqual((e["status"], e["error"]), ("fail", "the font switch failed"))
        # a variant with no verdict of its own is judged by its golden
        with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")):
            cap = os.path.join(self.dir.name, "cap.png")
            e = t3_evaluate.evaluate(self.VARIANT, "showcase-text", cap, {}, LOG,
                                     self.dir.name)
        self.assertEqual(e["status"], "missing")

    def test_scenes(self):
        # Every screen is a script in guest/screens. The showcase finds its
        # scenes by their header lines, each in its category, the shell
        # session first, and each scene's image named for its script. Every
        # scene takes a caption and alt text from its header, with the lines
        # that sort it left out, and switches the console to its own font
        # set, so the order the variant draws them in changes nothing.
        for screen in self.SCREENS:
            with self.subTest(screen):
                path = os.path.join(t3_common.GUEST, "screens", screen + ".sh")
                self.assertTrue(os.access(path, os.X_OK), path)
        found = t3_showcase.SCENES
        self.assertEqual({s.script: (s.category, s.hero) for s in found}, self.PLACES)
        self.assertEqual((found[0].script, found[0].image), ("showcase-shell.sh", "hero.png"))
        self.assertEqual([s.image for s in found],
                         ["hero.png", "double-size.png", "fonts.png", "glyph-fit.png",
                          "ligatures.png", "syllables.png", "emoji.png", "scripts.png", "text.png", "color.png",
                          "boxes.png", "pictures.png", "pointer.png"])
        self.assertEqual(t3_showcase.header_faults(), [])
        for scene in found:
            with self.subTest(scene.script):
                text, alt = t3_showcase.caption(scene.script)
                self.assertTrue(text and alt, scene.script)
                for word in ("alt:", "shellcheck", "category:", "hero:", "front page"):
                    self.assertNotIn(word, text + alt)
                with open(os.path.join(t3_common.GUEST, "screens", scene.script)) as f:
                    self.assertRegex(f.read(), r"\n\s*switch_font \S+\n")
        text, _ = t3_showcase.caption("showcase-fonts.inc")
        self.assertIn("(../fonts.md)", text)

    def test_header_keeps_the_sorting_lines_out_of_the_caption(self):
        # t3_evaluate.header puts a scene's category and hero lines in
        # fields of their own, so neither reaches the caption or the alt
        # text of the page, or the caption of a tier 3 report.
        script = os.path.join(self.dir.name, "showcase-x.sh")
        with open(script, "w") as f:
            f.write("#!/bin/bash\n# category: color\n# hero: yes\n# Colors.\n"
                    "# alt: Swatches.\n\nprintf x\n")
        head = t3_evaluate.header(None, script)
        self.assertEqual((head[""], head["alt"], head["category"], head["hero"]),
                         (["Colors."], ["Swatches."], ["color"], ["yes"]))

    def screens(self, scripts, figures=None, borrowed=None, pages=None):
        """A screens directory and a figures directory in the temporary
        directory, holding `scripts`, {name: header lines}, and `figures`,
        {name: (comment lines, whether its claim file exists)}. `borrowed`
        gives the lines of the figures' list of borrowed figures, each a
        list of fields, and `pages` {path: text} the pages beside the
        figures' directory, as docs/ holds docs/figures and the pages."""
        screens = os.path.join(self.dir.name, "screens")
        figs = os.path.join(self.dir.name, "figures")
        os.makedirs(screens, exist_ok=True)
        os.makedirs(figs, exist_ok=True)
        if borrowed is not None:
            with open(os.path.join(figs, t3_showcase.BORROWED), "w") as f:
                f.write("# borrowed figures\n" + "".join("\t".join(r) + "\n" for r in borrowed))
        for path, text in (pages or {}).items():
            os.makedirs(os.path.dirname(os.path.join(self.dir.name, path)), exist_ok=True)
            with open(os.path.join(self.dir.name, path), "w") as f:
                f.write(text)
        for name, lines in scripts.items():
            with open(os.path.join(screens, name), "w") as f:
                f.write("#!/bin/bash\n" + "".join("# %s\n" % line for line in lines) +
                        "\nprintf x\n")
        for name, (lines, claims) in (figures or {}).items():
            with open(os.path.join(figs, name), "w") as f:
                f.write("".join("<!-- %s -->\n" % line for line in lines) + "<svg/>\n")
            if claims:
                open(os.path.join(figs, name[:-4] + ".claims.tsv"), "w").close()
        return screens, figs

    def test_scenes_are_found_by_their_header(self):
        # A script showcase-NAME.sh is a scene when its header names a
        # category, and its image is NAME.png; the shell session's is
        # hero.png. The fonts scene's bands are no scenes of their own, nor
        # is a script with no category, or one that is not showcase-*.sh. The order is the
        # front page, then the categories in D6's order, the hero first and
        # the others by name.
        screens, _ = self.screens({
            "showcase-shell.sh": ["category: front page", "The shell."],
            "showcase-zebra.sh": ["category: color", "Stripes.", "alt: Stripes."],
            "showcase-blue.sh": ["category: color", "hero: yes", "Blue.", "alt: Blue."],
            "showcase-apple.sh": ["category: color", "Red.", "alt: Red."],
            "showcase-words.sh": ["category: text and fonts", "hero: yes", "Words."],
            "showcase-fonts-iosevka.sh": ["A band."],
            "showcase-draft.sh": ["A scene with no category yet."],
            "showcase-fonts.inc": ["category: text and fonts", "Fonts."],
            "sgr.sh": ["category: color", "Not a scene."]})
        found = t3_showcase.scenes(screens)
        self.assertEqual([(s.script, s.image, s.cases) for s in found], [
            ("showcase-shell.sh", "hero.png", ["showcase-shell"]),
            ("showcase-words.sh", "words.png", ["showcase-words"]),
            ("showcase-fonts.inc", "fonts.png", t3_showcase.FONT_BANDS),
            ("showcase-blue.sh", "blue.png", ["showcase-blue"]),
            ("showcase-apple.sh", "apple.png", ["showcase-apple"]),
            ("showcase-zebra.sh", "zebra.png", ["showcase-zebra"])])
        self.assertEqual(t3_showcase.caption("showcase-blue.sh", screens), ("Blue.", "Blue."))

    def test_header_faults(self):
        # Each wrong header line is named: a scene with no category, one no
        # list names, a second category line, the front page named by
        # another scene, a band with a category, a hero line that is not
        # yes, a category with no hero and one with two; and a figure with
        # no category or an unknown one, no caption, no alt text or no
        # claim file. Right headers give none, and compose refuses to write
        # while there is one.
        good = {"showcase-shell.sh": ["category: front page", "The shell."],
                "showcase-a.sh": ["category: color", "hero: yes", "A."]}
        screens, figs = self.screens(good, {
            "showcase-right.svg": (["category: pictures", "caption: It moves.",
                                    "alt: Two panels."], True)})
        self.assertEqual(t3_showcase.header_faults(screens, figs), [])
        cases = [
            ({"showcase-b.sh": ["The color b."]}, {}, "showcase-b.sh names no category"),
            ({"showcase-b.sh": ["category: colour"]}, {},
             "showcase-b.sh names the category 'colour', which is not one of"),
            ({"showcase-b.sh": ["category: pictures", "category: color", "hero: yes"]}, {},
             "showcase-b.sh names 2 categories"),
            ({"showcase-b.sh": ["category: front page"]}, {},
             "showcase-b.sh names the front page, which only showcase-shell.sh may name"),
            ({"showcase-shell.sh": ["category: color", "The shell."]}, {},
             "showcase-shell.sh names the category 'color'; the shell session is the front "
             "page's hero"),
            ({"showcase-fonts-unifont.sh": ["category: text and fonts"]}, {},
             "showcase-fonts-unifont.sh is a band of the fonts scene"),
            ({"showcase-b.sh": ["category: color", "hero: no"]}, {},
             "showcase-b.sh has the hero line `# hero: no`"),
            ({"showcase-b.sh": ["category: pictures"]}, {},
             "the category pictures has scenes and no hero"),
            ({"showcase-b.sh": ["category: color", "hero: yes"]}, {},
             "the category color has 2 heroes, showcase-a.sh, showcase-b.sh"),
            ({}, {"showcase-x.svg": (["caption: C.", "alt: A."], True)},
             "showcase-x.svg names no category"),
            ({}, {"showcase-x.svg": (["category: motion", "caption: C.", "alt: A."], True)},
             "showcase-x.svg names the category 'motion'"),
            ({}, {"showcase-x.svg": (["category: color", "alt: A."], True)},
             "showcase-x.svg has no caption"),
            ({}, {"showcase-x.svg": (["category: color", "caption: C."], True)},
             "showcase-x.svg has no alt text"),
            ({}, {"showcase-x.svg": (["category: color", "caption: C.", "alt: A."], False)},
             "showcase-x.svg has no claim file beside it, showcase-x.claims.tsv"),
        ]
        for scripts, figures, fault in cases:
            with self.subTest(fault):
                shutil.rmtree(self.dir.name)
                os.makedirs(self.dir.name)
                screens, figs = self.screens({**good, **scripts}, figures)
                faults = t3_showcase.header_faults(screens, figs)
                self.assertEqual(len(faults), 1, faults)
                self.assertIn(fault, faults[0])
                with self.assertRaisesRegex(RuntimeError, "header lines are wrong"):
                    t3_showcase.compose(os.path.join(self.dir.name, "run"),
                                        os.path.join(self.dir.name, "report.json"),
                                        os.path.join(self.dir.name, "out"), screens, figs)

    def test_borrowed_figure_faults(self):
        # A figure another page shows is used as it is: the list of borrowed
        # figures names it, its category, that page and a caption, and the
        # showcase takes its alt text from that page. It needs no claim file
        # of its own. A line with a field missing, a figure that is not
        # there, a page that does not show it and an unknown category are
        # each named, and the figure with the fault is left out.
        good = {"showcase-shell.sh": ["category: front page", "The shell."]}
        page = {"concepts/x.md": "Text.\n\n![A [bracketed] alt text.](../figures/concepts-x.svg)\n"}
        figures = {"concepts-x.svg": (["no comment lines of its own"], False)}
        row = ["concepts-x.svg", "displays and VTs", "concepts/x.md", "It switches."]
        screens, figs = self.screens(good, figures, [row], page)
        self.assertEqual(t3_showcase.header_faults(screens, figs), [])
        [fig] = t3_showcase.figures(figs)
        self.assertEqual((fig.file, fig.category, fig.caption, fig.alt, fig.page),
                         ("concepts-x.svg", "displays and VTs", "It switches.",
                          "A [bracketed] alt text.", "concepts/x.md"))
        cases = [
            (row[:3], "showcase-borrowed.tsv line 2 has 3 fields"),
            (["concepts-y.svg"] + row[1:], "names concepts-y.svg, which is not in the figures'"),
            (row[:2] + ["concepts/y.md"] + row[3:], "says concepts/y.md shows concepts-x.svg, "
                                                    "but that page has no image of it"),
            (row[:1] + ["motion"] + row[2:], "concepts-x.svg names the category 'motion'"),
        ]
        for line, fault in cases:
            with self.subTest(fault):
                shutil.rmtree(self.dir.name)
                os.makedirs(self.dir.name)
                screens, figs = self.screens(good, figures, [line], page)
                faults = t3_showcase.header_faults(screens, figs)
                self.assertEqual(len(faults), 1, faults)
                self.assertIn(fault, faults[0])

    def test_a_category_with_figures_alone(self):
        # A category with figures and no scenes shows its first figure on
        # the showcase page in place of a hero, and links its page as a page
        # of figures; that page says it holds figures, and a category of
        # screens says it holds screens.
        screens, figs = self.screens(
            {"showcase-shell.sh": ["category: front page", "The shell.", "alt: A shell."],
             "showcase-a.sh": ["category: color", "hero: yes", "Red.", "alt: Red."]},
            {"showcase-x.svg": (["category: pictures", "caption: It scrolls.",
                                 "alt: Two panels."], True),
             "showcase-y.svg": (["category: pictures", "caption: It moves.",
                                 "alt: Three panels."], True)})
        with open(self.run_dir()[1]) as f:
            report = json.load(f)
        page = t3_showcase.page(report, "2026-11-15", screens, figs)
        self.assertIn("## Pictures\n\n![Two panels.](../figures/showcase-x.svg)\n\nIt scrolls.\n\n"
                      "[Every figure of pictures](pictures.md)\n", page)
        self.assertNotIn("showcase-y.svg", page)
        self.assertIn("[Every screen of color](color.md)", page)
        shown = t3_showcase.shown(screens, figs)
        pictures = t3_showcase.category_page("pictures", *shown["pictures"])
        self.assertIn("\n\nThese figures are part of [vtium's showcase](README.md)", pictures)
        self.assertLess(pictures.index("showcase-x.svg"), pictures.index("showcase-y.svg"))
        color = t3_showcase.category_page("color", *shown["color"])
        self.assertIn("\n\nThese screens are part of [vtium's showcase](README.md)", color)
        small = t3_showcase.category_page(t3_showcase.SMALL_MACHINES,
                                          *shown[t3_showcase.SMALL_MACHINES])
        self.assertEqual(small.count("These screens"), 1, small)

    def test_small_captions_stand_alone(self):
        # The page shows the hero of the small machines without the other
        # golden, so each caption names the build and its display itself
        # and points back at nothing.
        for src, _, alt, text in t3_showcase.SMALL:
            with self.subTest(src):
                size = re.search(r"2d-(\d+)x(\d+)-", src).groups()
                self.assertIn("vtium's minspec build draws this on a %s×%s display" % size, text)
                self.assertIn("%s×%s" % size, alt)
                self.assertNotRegex(text, r"\b(same|also)\b")

    def test_the_menu_lists_each_category_page(self):
        # zensical.toml keeps each category's page out of the menu by name,
        # so a page the menu neither lists nor names fails the site's check.
        with open(os.path.join(t3_common.ROOT, "zensical.toml")) as f:
            text = f.read()
        listed = re.findall(r"^/showcase/(\S+)$", text, re.M)
        self.assertEqual(listed, ["%s.md" % t3_showcase.slug(c) for c in t3_showcase.CATEGORIES])

    def test_captions_show_only_what_their_scenes_send(self):
        # A caption names a style only when its scene sends it, and no
        # caption describes vtium by comparison with another terminal.
        words = {"7": r"\b(reverse|reversed|inverse)\b", "9": r"\b(strike|struck)\b",
                 "53": r"\boverlined?\b"}
        for scene in t3_showcase.SCENES:
            text, _ = t3_showcase.caption(scene.script)
            with self.subTest(scene.script):
                self.assertNotRegex(text, r"(?i)\bas (the )?[\w ]*terminals? (did|do|does)\b")
                sent = set()
                for case in scene.cases:
                    sent |= set(re.findall(r"\x1b\[([0-9;:]*)m",
                                           self.scene_bytes(case).decode(errors="replace")))
                params = {p for seq in sent for p in seq.split(";")}
                for sgr, pattern in words.items():
                    if re.search(pattern, text):
                        self.assertIn(sgr, params, "the caption names %s" % pattern)
        for fig in t3_showcase.figures():
            with self.subTest(fig.file):
                self.assertNotRegex(fig.caption,
                                    r"(?i)\bas (the )?[\w ]*terminals? (did|do|does)\b")

    def test_figures(self):
        # The showcase's figures in docs/figures, each in its category with
        # a caption, alt text and a claim file whose rows include a test that
        # drives the behaviour.
        figs = t3_showcase.figures()
        self.assertEqual({f.file: f.category for f in figs}, self.FIGURES)
        for fig in figs:
            with self.subTest(fig.file):
                self.assertTrue(fig.caption and fig.alt)
                for word in ("caption:", "alt:", "category:"):
                    self.assertNotIn(word, fig.caption + fig.alt)
                if fig.page is not None:
                    continue
                self.assertTrue(fig.claims)
                with open(os.path.join(t3_showcase.FIGURES,
                                       fig.file[:-4] + ".claims.tsv")) as f:
                    rows = [line.rstrip("\n").split("\t") for line in f
                            if not line.startswith("#")]
                self.assertTrue(any(self.drives(row) for row in rows[1:]), fig.file)

    @staticmethod
    def drives(row):
        """Whether the claim row `row` is a test that drives a behaviour: its
        element says "test:" and its symbol is a function marked #[test] in
        its file."""
        element, file, symbol = row[:3]
        if "test:" not in element:
            return False
        with open(os.path.join(t3_common.ROOT, file)) as f:
            text = f.read()
        return re.search(r"#\[test\]\s*(?:(?://[^\n]*|#\[[^\]]*\])\s*)*fn %s\("
                         % re.escape(symbol.split("::")[-1]), text) is not None

    def test_drives(self):
        # A row is a driving test only when it says "test:" and names a
        # function marked #[test]; a row whose file merely sits under tests/
        # is not one.
        rtl = "crates/vtium-term/tests/bidi_frame.rs"
        self.assertTrue(self.drives(["3: test: x", rtl, "the_cursor_is_drawn_on_its_logical_cell"]))
        self.assertTrue(self.drives(["test: x", "crates/vtium/src/term.rs",
                                     "a_synchronized_update_that_waits_is_never_drawn_in_part"]))
        self.assertFalse(self.drives(["3: the cursor", rtl,
                                      "the_cursor_is_drawn_on_its_logical_cell"]))
        self.assertFalse(self.drives(["2: test: x", "crates/vtium-term/src/order.rs",
                                      "RowOrder::visual_at"]))

    def scene_bytes(self, screen, screens=None, env=None):
        """What scene `screen` writes on the host, in a session of its own
        with no terminal, so its queries go nowhere and get no answer. It
        runs from `screens`, the guest's screens by default, with `env` added
        to its environment."""
        screens = screens or os.path.join(t3_common.GUEST, "screens")
        return subprocess.run(["bash", os.path.join(screens, screen + ".sh")],
                              stdin=subprocess.DEVNULL, capture_output=True, timeout=60,
                              start_new_session=True,
                              env={"PATH": os.environ["PATH"], "LC_ALL": "C.UTF-8",
                                   **(env or {})}).stdout

    @staticmethod
    def styles(data):
        """The SGR parameters `data` sets, the colors' own parameters left
        out: 38, 48 and 58 with 5 take one more, with 2 three more."""
        out = set()
        for seq in re.findall(rb"\x1b\[([0-9;:]*)m", data):
            params = seq.decode().split(";")
            i = 0
            while i < len(params):
                p = params[i]
                if p in ("38", "48", "58"):
                    i += {"5": 3, "2": 5}.get(params[i + 1] if i + 1 < len(params) else "", 1)
                    continue
                out.add(p)
                i += 1
        return out

    def test_scenes_show_only_what_vtium_draws_right(self):
        # No scene draws bold or blinking text, which vtium draws as the
        # regular face widened and blinking. Italic is allowed: vtium slants
        # the regular face where a font has no italic one.
        self.assertEqual(self.styles(b"\x1b[1;38;2;1;3;5m\x1b[3m\x1b[4:3;58:2::1:3:5m"),
                         {"1", "3", "4:3", "58:2::1:3:5"})
        for screen in self.SCREENS:
            with self.subTest(screen):
                data = self.scene_bytes(screen)
                self.assertGreater(len(data), 100)
                self.assertFalse(self.styles(data) & {"1", "5", "6"})

    def test_text_and_fonts_scenes_show_the_styles(self):
        # The outline text scene and each band of the fonts scene show
        # italic, strike, dim, a curly underline, and raised and lowered text
        # (SGR 73 and 74); the text scene also overline, and an underline and
        # a strike on small text.
        for screen in ["showcase-text"] + [s for s in self.SCREENS if "fonts-" in s]:
            with self.subTest(screen):
                styles = self.styles(self.scene_bytes(screen))
                self.assertLessEqual({"3", "9", "2", "4:3", "73", "74"}, styles)
        self.assertLessEqual({"53", "73;4", "74;9"}, set(re.findall(
            r"\x1b\[([0-9;:]*)m", self.scene_bytes("showcase-text").decode())))

    def test_double_size_scene_pairs_its_tall_lines(self):
        # The double-size scene draws each line twice as tall as a top half
        # (ESC # 3) followed at once by its bottom half (ESC # 4) with the
        # same text, draws lines twice as wide (ESC # 6), and keeps each
        # doubled line within half of the 120 columns, so none wraps.
        lines = self.scene_bytes("showcase-double-size").split(b"\n")
        sizes = [line[:3] for line in lines]
        self.assertIn(b"\x1b#6", sizes)
        tops = [n for n, size in enumerate(sizes) if size == b"\x1b#3"]
        self.assertEqual(len(tops), 2)
        for n in tops:
            self.assertEqual(lines[n + 1], b"\x1b#4" + lines[n][3:])
        for line in lines:
            if line[:3] in (b"\x1b#3", b"\x1b#4", b"\x1b#6"):
                text = re.sub(rb"\x1b\[[0-9;:]*m", b"", line[3:]).decode()
                self.assertLessEqual(len(text), 60, text)

    def test_syllables_scene_keeps_one_color_in_each_syllable(self):
        # A change of foreground color or style inside a syllable draws its
        # clusters apart, so the syllables scene changes only the
        # background between the clusters of a tile, and every tile's
        # syllable is Tamil, Devanagari or Kannada.
        data = self.scene_bytes("showcase-syllables").decode()
        tiles = re.findall(r"((?:\x1b\[48;2;[0-9;]+m[^\x1b ]+)+)\x1b\[0m", data)
        self.assertGreater(len(tiles), 15)
        for tile in tiles:
            with self.subTest(tile):
                self.assertNotRegex(tile, r"\x1b\[(?!48;)")
                text = re.sub(r"\x1b\[[0-9;]+m", "", tile)
                self.assertTrue(all("\u0900" <= c <= "\u097f" or "\u0b80" <= c <= "\u0bff" or
                                    "\u0c80" <= c <= "\u0cff" for c in text), text)

    def test_scripts_scene_runs_right_to_left_lines_from_the_right(self):
        # The scripts scene writes Devanagari, Bengali, Georgian and
        # Ethiopic each on one line; the danda, in the Devanagari block, ends
        # Bengali sentences too. The Arabic and Hebrew lines and the Hebrew
        # mixed line, and no other, start with SCP 2, which makes the line's
        # paragraph run right to left, and end with SCP 0 before the line
        # feed, so the lines after them and the prompt run left to right. The
        # mixed lines put Latin letters and digits inside Hebrew, and Hebrew
        # with a digit inside English.
        scripts = {"Hindi": "[\u0900-\u0963\u0966-\u097f]", "Bengali": "[\u0980-\u09ff]",
                   "Georgian": "[\u10a0-\u10ff]", "Amharic": "[\u1200-\u137f]"}
        lines = self.scene_bytes("showcase-scripts").decode().split("\n")
        for name, script in scripts.items():
            with self.subTest(name):
                held = [line for line in lines if re.search(script, line)]
                self.assertEqual(len(held), 1)
                self.assertIn(name, held[0])
        plain = [re.sub("\x1b\\[[0-9;]*[a-zA-Z ]*?[mk]", "", line).split(None, 1)
                 for line in lines]
        rtl = [p[0] for line, p in zip(lines, plain) if line.startswith("\x1b[2 k")]
        self.assertEqual(rtl, ["Arabic", "Hebrew", "Mixed"])
        mixed = [p[1] for p in plain if p and p[0] == "Mixed"]
        self.assertEqual(len(mixed), 2)
        self.assertRegex(mixed[0], "^[\u0590-\u05ff].*vtium 0\\.1")
        self.assertRegex(mixed[1], "^In Hebrew, .*[\u0590-\u05ff]+ 1 .*[\u0590-\u05ff]")
        for line in lines:
            with self.subTest(line):
                if line.startswith("\x1b[2 k"):
                    self.assertTrue(line.endswith("\x1b[0 k"))
                    self.assertEqual(line.count(" k"), 2)
                else:
                    self.assertNotRegex(line, "\x1b\\[[0-9]* k")
        self.assertIn("bidi", t3_scenario.VARIANTS[self.VARIANT]["tree_features"])

    def test_scripts_scene_shares_the_walks_sentences(self):
        # Each language the acceptance walk's language pages show and the
        # scripts scene shows has the same sentence in both, from
        # tools/vt/sections/text/udhr-article1.tsv.
        tsv = os.path.join(t3_common.ROOT, "tools", "vt", "sections", "text",
                           "udhr-article1.tsv")
        with open(tsv, encoding="utf-8") as f:
            rows = [line.rstrip("\n").split("\t") for line in f if not line.startswith("#")]
        # vietnamese-marks, the same sentence in NFD, shares its name's first
        # word with vietnamese and is left out
        walk = {row[1].split(",")[0]: row[2] for row in rows if row[0] != "vietnamese-marks"}
        with open(os.path.join(t3_common.GUEST, "screens", "showcase-scripts.sh"),
                  encoding="utf-8") as f:
            scene = dict(re.findall(r"^(?:line|rtl) (\w+) '(.*)'$", f.read(), re.M))
        shared = sorted(set(walk) & set(scene))
        self.assertEqual(shared, ["Arabic", "Chinese", "Greek", "Hebrew", "Hindi", "Japanese",
                                  "Korean", "Russian", "Thai", "Vietnamese"])
        for name in shared:
            with self.subTest(name):
                self.assertEqual(scene[name], walk[name])

    def test_faults(self):
        # A font switch vtium did not take, pictures missing from the disc,
        # or a gallery the tool did not draw, reported while a screen draws,
        # fail that screen alone.
        events = ["T3-NO-GALLERY", "T3-DONE showcase-shell",
                  "T3-FONT-REFUSED showcase-terminus unifont",
                  "T3-BAND showcase-terminus rows=8 cell=16x32",
                  "T3-DONE showcase-fonts-terminus", "T3-FONT-REFUSED showcase-unifont",
                  "T3-NO-PICTURES", "T3-DONE showcase-fonts-unifont",
                  "T3-DONE showcase-fonts-jetbrains", "T3-NO-POINT",
                  "T3-DONE showcase-pointer"]
        self.assertEqual(t3_showcase.faults(events, "showcase-shell"),
                         ["the gallery tool drew no gallery"])
        self.assertEqual(t3_showcase.faults(events, "showcase-fonts-terminus"),
                         ["vtium did not switch to the font showcase-terminus: OSC 50 named "
                          "unifont"])
        self.assertEqual(t3_showcase.faults(events, "showcase-fonts-unifont"),
                         ["vtium did not switch to the font showcase-unifont: OSC 50 named "
                          "none", "the showcase's pictures are not on the disc"])
        self.assertEqual(t3_showcase.faults(events, "showcase-fonts-jetbrains"), [])
        self.assertEqual(t3_showcase.faults(events, "showcase-text"), [])
        self.assertEqual(t3_showcase.faults(events, "showcase-pointer"),
                         ["the scene asked for the pointer, and the terminal did not report "
                          "its cell size, so the pointer was not moved"])
        with open(os.path.join(t3_common.HERE, "t3_scenario.py")) as f:
            self.assertIn("why = t3_showcase.faults(events, name)", f.read())

    def test_fonts_pinned(self):
        # Every font is pinned by a URL and the SHA-256 of what it fetches
        # and of the font file, and a font the specimens or tier 2 pin is
        # pinned there alone.
        for font in t3_showcase.FONTS:
            with self.subTest(font.name):
                self.assertTrue(font.url.startswith("https://"))
                self.assertRegex(font.sha256, "^[0-9a-f]{64}$")
                self.assertRegex(font.file_sha256, "^[0-9a-f]{64}$")
                self.assertEqual(font.member is None, font.sha256 == font.file_sha256)
        shared = {f.file: (f.url, f.sha256) for f in t3_showcase.FONTS
                  if "specimens" in f.source}
        self.assertEqual(sorted(shared), ["JetBrainsMono-Regular.ttf", "NotoColorEmoji.ttf",
                                          "SymbolsNerdFontMono-Regular.ttf"])
        with open(t3_showcase.SPECIMEN_FONTS) as f:
            rs = f.read()
        for url, sha in shared.values():
            # the specimens name a downloaded font's URL as Origin::Url
            self.assertRegex(rs, r'origin: Origin::Url\(\s*"%s",?\s*\)' % re.escape(url))
            self.assertIn('sha256: "%s"' % sha, rs)
        tier2 = {f.file: (f.url, f.sha256) for f in t3_showcase.FONTS
                 if "tier2" in f.source}
        self.assertEqual(sorted(tier2), ["NotoSansArabic-Regular.ttf",
                                         "NotoSansHebrew-Regular.ttf",
                                         "NotoSansKannada-Regular.ttf",
                                         "NotoSansTamil-Regular.ttf"])
        with open(t3_showcase.TIER2_FONTS) as f:
            rs = f.read()
        for url, sha in tier2.values():
            self.assertRegex(rs, r'Origin::Url\(\s*"%s"' % re.escape(url))
            self.assertIn('sha256: "%s"' % sha, rs)
        for rs in (t3_showcase.SPECIMEN_FONTS, t3_showcase.TIER2_FONTS):
            with self.assertRaises(ValueError):
                t3_showcase.rust_pin(rs, "Missing.ttf")
        # a tier 2 font fetched from an archive names no URL of its own
        with self.assertRaises(ValueError):
            t3_showcase.rust_pin(t3_showcase.TIER2_FONTS, "DejaVuSansMono.ttf")

    def test_frits_name_the_pinned_fonts(self):
        # Each frit names only fonts the showcase pins, or the built-in
        # unifont, every pinned font is named, and each frit states its cell,
        # so the console never waits for the helper's answer; the
        # configuration sets no font-size, which would set the cells aside.
        files = {f.guest_path() for f in t3_showcase.FONTS}
        named = set()
        v = t3_scenario.VARIANTS[self.VARIANT]
        for frit in v["deferred_frits"]:
            with self.subTest(frit):
                with open(os.path.join(t3_common.GUEST, "frits", frit)) as f:
                    text = f.read()
                sources = re.findall(r"^source=(\w+):(.*)$", text, re.M)
                for kind, value in sources:
                    if kind == "builtin":
                        self.assertEqual(value, "unifont")
                    else:
                        self.assertIn(value, files)
                        named.add(value)
                self.assertRegex(text, r"(?m)^cell=\d+x\d+$")
        self.assertEqual(named, files)
        with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
            conf = f.read()
        self.assertIn("font-name=showcase-iosevka\n", conf)
        self.assertNotRegex(conf, r"(?m)^font-size=")

    def pin(self, payload, member=None, file="F.ttf", zip_name="f.zip"):
        """A Font pinned to a file:// URL of `payload`, a zip archive holding
        it as `member` when given."""
        path = os.path.join(self.dir.name, zip_name if member else file + ".src")
        if member:
            with zipfile.ZipFile(path, "w") as z:
                z.writestr(member, payload)
        else:
            with open(path, "wb") as f:
                f.write(payload)
        return t3_showcase.Font("F", "1", "OFL", "u", "s", "file://" + path,
                                t3_showcase.sha256_file(path), file,
                                hashlib.sha256(payload).hexdigest(), member=member)

    def test_fetch(self):
        # A font is fetched, checked and taken from its archive; one that
        # is not the pinned bytes is refused and not kept.
        cache = os.path.join(self.dir.name, "cache")
        font = self.pin(b"glyphs", member="dir/F.ttf")
        t3_showcase.fetch(cache, [font])
        with open(os.path.join(cache, "F.ttf"), "rb") as f:
            self.assertEqual(f.read(), b"glyphs")
        self.assertIsNone(t3_showcase.fault(cache, font))
        bad = self.pin(b"other", file="G.ttf")
        bad.sha256 = bad.file_sha256 = "0" * 64
        with self.assertRaisesRegex(RuntimeError, "not " + "0" * 64):
            t3_showcase.fetch(cache, [bad])
        self.assertFalse(os.path.exists(os.path.join(cache, "G.ttf")))
        self.assertEqual(os.listdir(os.path.join(cache, "downloads")), ["f.zip"])
        # a download that matches holding a font file that does not
        wrong = self.pin(b"other", member="H.ttf", file="H.ttf", zip_name="h.zip")
        wrong.file_sha256 = "1" * 64
        with self.assertRaisesRegex(RuntimeError, "H.ttf has SHA-256 .*, not " + "1" * 64):
            t3_showcase.fetch(cache, [wrong])
        self.assertFalse(os.path.exists(os.path.join(cache, "H.ttf")))
        with self.assertRaisesRegex(RuntimeError, "showcase.sh fetches"):
            t3_showcase.disc_fonts(os.path.join(self.dir.name, "disc"),
                                   os.path.join(self.dir.name, "empty"))

    def test_pictures(self):
        # The sixel picture holds the Mandelbrot set's pixels, each color
        # to sixel's percentages, in at most 256 colors; the kitty picture is
        # raw pixels compressed with zlib, its size beside it.
        rows = t3_showcase.mandelbrot(40, 30)
        self.assertEqual(rows[15][20], (0, 0, 0))  # inside the set
        self.assertNotEqual(rows[0][0], (0, 0, 0))
        got = sixel_pixels(t3_showcase.sixel(rows))
        want = {(x, y): tuple((c * 100 + 127) // 255 for c in p)
                for y, row in enumerate(rows) for x, p in enumerate(row)}
        self.assertEqual(got, want)
        with self.assertRaisesRegex(ValueError, "at most 256"):
            t3_showcase.sixel([[(r * 255 // 100, g * 255 // 100, 0)
                                for r in range(17) for g in range(17)]])
        names = t3_showcase.pictures(self.dir.name, (12, 12), (10, 8))
        self.assertEqual(names, ["mandelbrot.six", "sphere.rgba.z", "sphere.size"])
        with open(os.path.join(self.dir.name, "sphere.rgba.z"), "rb") as f:
            self.assertEqual(len(zlib.decompress(f.read())), 10 * 8 * 4)
        with open(os.path.join(self.dir.name, "sphere.size")) as f:
            self.assertEqual(f.read(), "10 8\n")

    def test_gallery(self):
        # The gallery holds the photographs as committed, each the pinned
        # file of 256x160 pixels and a few tens of KB, and the computed
        # pictures at the size asked for, every picture different; a
        # photograph that is not the pinned file is refused. The gallery
        # tool names nothing of tier 3, and the disc carries it beside the
        # pictures.
        g = t3_showcase_gallery
        self.assertEqual(sorted(os.listdir(g.PHOTO_DIR)), sorted(p.file for p in g.PHOTOS))
        for photo in g.PHOTOS:
            with self.subTest(photo.file):
                path = os.path.join(g.PHOTO_DIR, photo.file)
                self.assertEqual(t3_showcase.sha256_file(path), photo.file_sha256)
                img = t3img.read(path)
                self.assertEqual((img.width, img.height), g.GALLERY_SIZE)
                self.assertLess(os.path.getsize(path), 64 * 1024)
                self.assertRegex(photo.url, r"^https://upload\.wikimedia\.org/")
                self.assertRegex(photo.sha256, r"^[0-9a-f]{64}$")
                self.assertIn("public domain", photo.license)
        with open(g.TOOL) as f:
            self.assertNotRegex(f.read(), r"(?i)tier ?3|t3_|showcase")
        out = os.path.join(self.dir.name, "gallery")
        self.assertEqual(g.gallery(out, (16, 10)), g.GALLERY)
        self.assertEqual(sorted(os.listdir(out)), g.GALLERY)
        self.assertEqual(g.GALLERY, sorted(g.GALLERY))
        data = set()
        for name in g.GALLERY:
            img = t3img.read(os.path.join(out, name))
            if name not in {p.file for p in g.PHOTOS}:
                self.assertEqual((img.width, img.height), (16, 10), name)
            data.add(img.data)
        self.assertEqual(len(data), len(g.GALLERY))
        bad = os.path.join(self.dir.name, "photos")
        shutil.copytree(g.PHOTO_DIR, bad)
        with open(os.path.join(bad, g.PHOTOS[0].file), "ab") as f:
            f.write(b"\0")
        with mock.patch.object(g, "PHOTO_DIR", bad), \
                self.assertRaisesRegex(RuntimeError, "%s has SHA-256" % g.PHOTOS[0].file):
            g.gallery(os.path.join(self.dir.name, "refused"), (16, 10))
        with mock.patch.object(g, "gallery") as pictures:
            g.disc(os.path.join(self.dir.name, "disc"))
        pictures.assert_called_once_with(os.path.join(self.dir.name, "disc", "pictures"))
        tool = os.path.join(self.dir.name, "disc", "bin", "gallery")
        self.assertTrue(os.access(tool, os.X_OK))
        self.assertEqual(t3_showcase.sha256_file(tool), t3_showcase.sha256_file(g.TOOL))

    # The first prompt of the shell scene, the user's segment's background.
    PROMPT = b"\x1b[48;2;97;175;239m"
    GALLERY_PROMPT = b"$ gallery -s 4 -n 6 ~/pictures\n"

    def gallery_rows(self, data):
        """The shell scene's gallery in `data`, read from the row under its
        prompt to the next prompt: ({(row, column): (kitty keys, picture as
        PNG bytes)}, {(row, column): text}, the rows the gallery takes under
        its prompt). Rows count from 0 under the prompt and columns from 1;
        queries, colors and OSC sequences move nothing."""
        start = data.index(self.GALLERY_PROMPT) + len(self.GALLERY_PROMPT)
        part = data[start:data.index(self.PROMPT, start)]
        row, col, pictures, text = 0, 1, {}, {}
        sending = None
        for m in re.finditer(rb"\x1b\[(\d+)([ABG])|\x1b_G([^;\x1b]*)(?:;([^\x1b]*))?\x1b\\"
                             rb"|\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*\x07|(\n)|([^\x1b\n]+)", part):
            if m.group(1):
                n = int(m.group(1))
                row, col = {b"A": (row - n, col), b"B": (row + n, col), b"G": (row, n)}[m.group(2)]
            elif m.group(3) is not None:
                keys = dict(k.split("=") for k in m.group(3).decode().split(","))
                if keys.get("a") == "q":
                    continue
                if "a" in keys:
                    sending = (row, col)
                    pictures[sending] = (keys, b"")
                pictures[sending] = (pictures[sending][0], pictures[sending][1] + m.group(4))
                if keys["m"] == "0":
                    pictures[sending] = (pictures[sending][0], base64.b64decode(pictures[sending][1]))
            elif m.group(5):
                row, col = row + 1, 1
            elif m.group(6):
                text[(row, col)] = m.group(6).decode()
                col += len(m.group(6))
        return pictures, text, row

    @staticmethod
    def screen_rows(data):
        """(the row the cursor ends on, the lowest row it reaches) as `data`
        moves it from the top row, 0: down a row with each newline, and up
        or down with CSI A and B."""
        row = low = 0
        for m in re.finditer(rb"\n|\x1b\[(\d+)([AB])", data):
            row += 1 if m.group(0) == b"\n" else int(m.group(1)) * (-1 if m.group(2) == b"A" else 1)
            low = max(low, row)
        return row, low

    def on_terminal(self, screens, **traits):
        """What the shell scene in `screens` writes on a stand-in terminal
        of 28 rows by 128 columns in the scene's cell, which answers the
        scene's and the gallery tool's queries as vtium does, unless
        `traits` say otherwise; and the events it reports."""
        events = os.path.join(self.dir.name, "events")
        with contextlib.suppress(FileNotFoundError):
            os.unlink(events)
        status, data, _ = gallery_terminal.run(
            ["bash", os.path.join(screens, "showcase-shell.sh")],
            gallery_terminal.answers(**{"cell": t3_showcase_gallery.CELL,
                                        "font": "showcase-shell", **traits}),
            rows=28, cols=128,
            env={"PATH": os.environ["PATH"], "LC_ALL": "C.UTF-8", "T3_EVENTS": events})
        self.assertEqual(status, 0)
        with contextlib.suppress(FileNotFoundError), open(events) as f:
            return data, f.read()
        return data, ""

    def test_gallery_scene(self):
        # On a terminal that answers as vtium does, the shell scene runs the
        # gallery tool, which sends every picture of the gallery in name
        # order, each one's bytes as written, in a box of cells of the
        # picture's own shape on the four rows under its prompt, inside the
        # 128 columns and clear of each other, and centers each file's name
        # under its picture on the fifth row; the next prompt comes on the
        # sixth, and the screen stays at its 28 rows. When the tool draws
        # nothing, the scene keeps the same rows and reports it on the event
        # port.
        g = t3_showcase_gallery
        with open(os.path.join(t3_common.GUEST, "frits", "showcase-shell.vtfrit")) as f:
            self.assertIn("\ncell=%dx%d\n" % g.CELL, f.read())
        screens = os.path.join(self.dir.name, "screens")
        os.makedirs(screens)
        for name in ("showcase-shell.sh", "showcase.inc"):
            shutil.copy(os.path.join(t3_common.GUEST, "screens", name), screens)
        g.disc(os.path.join(screens, "showcase"))
        data, events = self.on_terminal(screens)
        self.assertEqual((self.screen_rows(data), events), ((27, 27), ""))
        pictures, text, rows = self.gallery_rows(data)
        self.assertEqual(rows, 5)
        files = {}
        for name in g.GALLERY:
            with open(os.path.join(screens, "showcase", "pictures", name), "rb") as f:
                files[f.read()] = name
        placed = sorted(pictures)
        self.assertEqual([files.get(pictures[at][1]) for at in placed], g.GALLERY)
        self.assertEqual({at[0] for at in text}, {4})
        end = 0
        for (row, col) in placed:
            keys, png = pictures[(row, col)]
            img = t3img.read_png(png)
            width, height = int(keys["c"]), int(keys["r"])
            with self.subTest(col=col):
                self.assertEqual((keys["f"], keys["C"], row), ("100", "1", 0))
                self.assertLessEqual(height, 4)
                # the box is the picture's shape, to within a cell
                self.assertLess(abs(width * g.CELL[0] * img.height
                                    - height * g.CELL[1] * img.width), g.CELL[0] * img.height)
                self.assertGreater(col, end)
                end = col + width - 1
                self.assertLessEqual(end, 128)
                # the label whose middle is within a cell of the picture's
                under = [label for (_, c), label in text.items()
                         if abs(2 * c + len(label) - 2 * col - width) <= 2]
                self.assertEqual(under, [files[png]])
        self.assertEqual(len(text), len(placed))
        # every chunk of a picture is at most 4096 bytes of base64, and every
        # one but a picture's last, which ends it with m=0, a multiple of 4
        chunks = [(dict(k.split("=") for k in keys.decode().split(",")), payload) for keys, payload
                  in re.findall(rb"\x1b_G([^;\x1b]*);([^\x1b]*)\x1b\\", data)]
        chunks = [(keys["m"], payload) for keys, payload in chunks if keys.get("a") != "q"]
        self.assertEqual(sum(m == "0" for m, _ in chunks), len(g.GALLERY))
        for m, payload in chunks:
            self.assertLessEqual(len(payload), 4096)
            if m == "1":
                self.assertEqual(len(payload) % 4, 0)
        data, events = self.on_terminal(screens, kitty=False)
        pictures, text, rows = self.gallery_rows(data)
        self.assertEqual((self.screen_rows(data), pictures, rows), ((27, 27), {}, 5))
        self.assertIn("gallery: the terminal did not answer", text[(0, 1)])
        self.assertEqual(events, "T3-NO-GALLERY\n")
        shutil.rmtree(os.path.join(screens, "showcase"))
        data, events = self.on_terminal(screens)
        pictures, text, rows = self.gallery_rows(data)
        self.assertEqual((self.screen_rows(data), pictures, rows), ((27, 27), {}, 5))
        self.assertEqual(events, "T3-NO-GALLERY\n")

    # The styles line's words, each with the SGR parameters that draw it,
    # and the ADR-1000 row of each parameter's feature.
    STYLES = [("underline", "4"), ("double", "4:2"), ("curly", "4:3"), ("dotted", "4:4"),
              ("dashed", "4:5"), ("red curly", "4:3;58:2::224:108:117"), ("strike", "9"),
              ("inverse", "7"), ("dim", "2")]
    FEATURES = {"4": "sgr.underline", "4:2": "sgr.underline-style", "4:3": "sgr.underline-style",
                "4:4": "sgr.underline-style", "4:5": "sgr.underline-style",
                "58:2::224:108:117": "sgr.underline-color", "9": "sgr.strike",
                "7": "sgr.inverse", "2": "sgr.dim"}

    def test_styles_line(self):
        # The shell scene's last line before the final prompt draws each
        # word in the style it names, every style one whose ADR-1000 row
        # vtium supports, and ends where the final prompt begins.
        with open(os.path.join(t3_common.ROOT, "docs", "architecture", "term",
                               "ADR-1000-features.tsv")) as f:
            status = {row["id"]: row["status"] for row in csv.DictReader(f, delimiter="\t")}
        data = self.scene_bytes("showcase-shell")
        start = data.rindex(b"$ cat greetings.txt styles.txt\n")
        lines = data[start:data.rindex(self.PROMPT)].split(b"\n")
        self.assertEqual((len(lines), lines[-1]), (6, b""))
        words = re.findall(rb"\x1b\[([0-9;:]+)m([^\x1b]+)\x1b\[0m", lines[4])
        self.assertEqual([(w.decode(), p.decode()) for p, w in words], self.STYLES)
        for _, params in self.STYLES:
            for p in params.split(";"):
                with self.subTest(p):
                    self.assertEqual(status[self.FEATURES[p]], "supported")

    def test_palettes(self):
        # vtium-term's named palettes, each with its 16 colors; a palette
        # short of a color is refused.
        names = [p[0] for p in t3_showcase.palettes()]
        self.assertEqual(names, ["legacy", "vga", "nord", "solarized", "solarized-black",
                                 "solarized-white", "soft-black", "base16-dark", "base16-light"])
        self.assertEqual(t3_showcase.palettes()[6][1][0], (0x3f, 0x3f, 0x3f))
        rs = os.path.join(self.dir.name, "palette.rs")
        with open(rs, "w") as f:
            f.write('    Named {\n        name: "short",\n        colors: [\n%s        ],\n'
                    '        fg: Rgb::new(1, 1, 1),\n        bg: Rgb::new(0, 0, 0),\n    }\n'
                    % ("            Rgb::new(1, 2, 3),\n" * 15))
        with self.assertRaisesRegex(ValueError, "short has 17 colors"):
            t3_showcase.palettes(rs)

    def run_dir(self, statuses=None):
        """A run of the variant in the temporary directory: a capture of
        each screen, 32 pixels wide and its index tall, the fonts' bands
        reported on the event port, and a report whose cases carry
        `statuses` (capture by default)."""
        run = os.path.join(self.dir.name, "run")
        os.makedirs(run)
        for n, screen in enumerate(self.SCREENS, 1):
            img = t3img.Image(32, 12, bytes([n]) * (32 * 12 * 3))
            t3img.write_png(os.path.join(run, screen + ".png"), img)
        with open(os.path.join(run, "events.log"), "w") as f:
            f.write("T3-LOGIN /dev/pts/0\nT3-BAND showcase-iosevka rows=2 cell=8x4\n"
                    "T3-BAND showcase-terminus rows=1 cell=8x3\n"
                    "T3-BAND showcase-unifont rows=3 cell=8x2\n"
                    "T3-BAND showcase-jetbrains rows=1 cell=8x5\n")
        statuses = statuses or {}
        report = os.path.join(self.dir.name, "report.json")
        with open(report, "w") as f:
            json.dump({"tools": {"vtium": "abc1234"},
                       "cases": [{"name": "%s/%s" % (self.VARIANT, s),
                                  "status": statuses.get(s, "capture")} for s in self.SCREENS],
                       "variants": {self.VARIANT: {
                           "cpus": 4, "memory_mb": 4096, "kernel": "7.2", "cell": [16, 40],
                           "console": {"name": "vtium", "revision": "v9"}}}}, f)
        return run, report

    def test_compose(self):
        # Each scene's capture as it is, the fonts' bands cropped to the
        # rows they reported and stacked in order, the small-machine goldens
        # copied, and the pages: the showcase page with the front page's
        # hero, a section for each category that shows something in D6's
        # order, with its hero or, with no scenes, its first figure, and a
        # page for each of those categories with every scene, the hero
        # first, then its figures under "How it behaves"; and the commit and
        # each font. A page an earlier run wrote that this run does not
        # write is removed, and a page no run wrote stays.
        run, report = self.run_dir()
        out = os.path.join(self.dir.name, "out")
        os.makedirs(out)
        for name, line in (("motion.md", t3_showcase.GENERATED), ("notes.md", "# Notes")):
            with open(os.path.join(out, name), "w") as f:
                f.write(line + "\n")
        wrote, removed = t3_showcase.compose(run, report, out, panes=self.PANES, size=(4, 3))
        self.assertEqual(removed, [os.path.join(out, "motion.md")])
        self.assertTrue(os.path.exists(os.path.join(out, "notes.md")))
        pages = ["README.md", "text-and-fonts.md", "scripts-and-emoji.md", "styles-and-modes.md",
                 "color.md", "lines-and-blocks.md", "pictures.md", "input-and-pointer.md",
                 "displays-and-vts.md", "small-machines.md"]
        self.assertEqual(sorted(os.path.relpath(p, out) for p in wrote),
                         sorted([s.image for s in t3_showcase.SCENES] +
                                [name for _, name, _, _ in t3_showcase.SMALL] +
                                [t3_showcase.GRID] + pages))
        hero = t3img.read(os.path.join(out, "hero.png"))
        self.assertEqual(hero.data[:3], bytes([1] * 3))
        # the grid holds the three panes in one row, each the color of its
        # screen, with a line of GRID_LINE between two panes
        grid = t3img.read(os.path.join(out, t3_showcase.GRID))
        self.assertEqual((grid.width, grid.height), (3 * 4 + 2, 3))
        line = bytes(t3_showcase.GRID_LINE)
        index = {"double-size.png": self.SCREENS.index("showcase-double-size") + 1,
                 "emoji.png": self.SCREENS.index("showcase-emoji") + 1, "hero.png": 1}
        for y in range(3):
            self.assertEqual(grid.row(y), b"".join(
                (bytes([index[name]]) * 12 + (line if i < 2 else b""))
                for i, (name, _, _) in enumerate(self.PANES)))
        fonts = t3img.read(os.path.join(out, "fonts.png"))
        self.assertEqual((fonts.width, fonts.height), (32, 8 + 3 + 6 + 5))
        self.assertEqual([fonts.data[y * 32 * 3] for y in (0, 7, 8, 10, 11, 16, 17, 21)],
                         [3, 3, 4, 4, 5, 5, 6, 6])
        for src, name, _, _ in t3_showcase.SMALL:
            with open(os.path.join(t3_common.ROOT, src), "rb") as a, \
                    open(os.path.join(out, name), "rb") as b:
                self.assertEqual(a.read(), b.read())

        def read(name):
            with open(os.path.join(out, name)) as f:
                return f.read()

        def shows(script):
            text, alt = t3_showcase.caption(script)
            image = {s.script: s.image for s in t3_showcase.SCENES}[script]
            return "![%s](%s)\n\n%s\n" % (alt, image, text)

        def figure(name):
            fig = {f.file: f for f in t3_showcase.figures()}[name]
            return "![%s](../figures/%s)\n\n%s\n" % (fig.alt, name, fig.caption)

        page = read("README.md")
        headings = re.findall(r"^## (.*)$", page, re.M)
        self.assertEqual(headings, ["Text and fonts", "Scripts and emoji", "Styles and modes",
                                    "Color", "Lines and blocks", "Pictures", "Input and pointer",
                                    "Displays and VTs", "Small machines",
                                    "How these screens are made"])
        self.assertLess(page.index(shows("showcase-shell.sh")), page.index("## Text and fonts"))
        for script in ("showcase-double-size.sh", "showcase-syllables.sh", "showcase-text.sh"):
            self.assertIn(shows(script), page)
        self.assertNotIn(shows("showcase-glyph-fit.sh"), page)
        self.assertIn("## Input and pointer\n\n" + shows("showcase-pointer.sh") +
                      "\n[Every screen and figure of input and pointer](input-and-pointer.md)\n",
                      page)
        # displays and VTs has figures alone, all borrowed from other pages
        self.assertIn("## Displays and VTs\n\n" + figure("concepts-heads-parts.svg") +
                      "\n[Every figure of displays and VTs](displays-and-vts.md)\n", page)
        displays = read("displays-and-vts.md")
        self.assertIn("These figures are part of [vtium's showcase](README.md)", displays)
        order = [displays.index(figure(f)) for f in
                 ("concepts-heads-parts.svg", "concepts-heads-mirror.svg",
                  "concepts-vt-handover.svg")]
        self.assertEqual(order, sorted(order))
        self.assertNotIn(figure("showcase-osc52-copy.svg"), page)
        self.assertIn("[Every screen of text and fonts](text-and-fonts.md)", page)
        small = read("small-machines.md")
        self.assertLess(small.index("(small/small-console.png)"), small.index("(small/sgr.png)"))
        self.assertIn("(small/small-console.png)", page)
        self.assertNotIn("(small/sgr.png)", page)
        text = read("text-and-fonts.md")
        order = [text.index(shows(s)) for s in
                 ("showcase-double-size.sh", "showcase-fonts.inc", "showcase-glyph-fit.sh")]
        self.assertEqual(order, sorted(order))
        self.assertNotIn("How it behaves", text)
        scripts = read("scripts-and-emoji.md")
        self.assertLess(scripts.index(shows("showcase-scripts.sh")),
                        scripts.index("## How it behaves\n\n" +
                                      figure("showcase-rtl-paragraphs.svg")))
        pointer = read("input-and-pointer.md")
        self.assertLess(pointer.index(shows("showcase-pointer.sh")),
                        pointer.index("## How it behaves\n\n" + figure("showcase-osc52-copy.svg")))
        self.assertIn("at commit `abc1234`", page)
        self.assertIn("starts in a character cell of 16×40 pixels. The shell scene, the hero, "
                      "switches to a font set of its own, `showcase-shell`, with a cell of 15×38 "
                      "pixels", page)
        # the features the page names are the ones the variant builds with
        features = t3_scenario.VARIANTS[self.VARIANT]["tree_features"]
        self.assertIn("built with the %s features" % " and ".join(
            "`%s`" % f for f in sorted(features, key=["images", "bidi"].index)), page)
        for font in t3_showcase.FONTS:
            self.assertIn("`%s`" % font.file_sha256, page)
        self.assertIn("| GNU Unifont |", page)
        for photo in t3_showcase_gallery.PHOTOS:
            self.assertIn("| `%s` | %s | %s | %s |" % (photo.file, photo.title, photo.author,
                                                    photo.license), page)
            self.assertIn("SHA-256 `%s`" % photo.sha256, page)
            self.assertIn("(%s)" % photo.url, page)
            self.assertIn("`%s` |" % photo.file_sha256, page)
        # a second run with the same pixels rewrites no image, only the pages
        self.assertEqual(sorted(os.path.relpath(p, out)
                                for p in t3_showcase.compose(run, report, out,
                                                             panes=self.PANES, size=(4, 3))[0]),
                         sorted(pages))

    def test_the_pages_keep_to_the_prose_limits(self):
        # The pages compose writes are pages people read, so their captions
        # and text keep to the limits docs/scripts/prose checks: no
        # sentence over 30 words and no page averaging over 20. The check
        # runs on the pages composed from a stand-in run, so a caption is
        # checked in the scene script where it is written, whether or not
        # the committed pages are current.
        run, report = self.run_dir()
        out = os.path.join(self.dir.name, "out")
        t3_showcase.compose(run, report, out, panes=self.PANES, size=(4, 3))
        with open(os.path.join(out, "README.md")) as f:
            self.assertIn("![%s](%s)" % (t3_showcase.GRID_ALT, t3_showcase.GRID), f.read())
        pages = sorted(os.path.join(out, f) for f in os.listdir(out) if f.endswith(".md"))
        self.assertIn(os.path.join(out, "README.md"), pages)
        check = subprocess.run([sys.executable, "-I", "-B",
                                os.path.join(t3_common.ROOT, "docs", "scripts", "prose"),
                                "--check"] + pages, capture_output=True, text=True, timeout=60)
        self.assertEqual(check.returncode, 0, check.stdout + check.stderr)
        self.assertIn("prose: %d pages read: 0 sentences over" % len(pages), check.stdout)

    def test_grid(self):
        # The committed grid is nine panes of 640x360 pixels, three to a
        # row, each cut out of its screen at its own size, with a line of
        # GRID_LINE between two panes. Its pixels are the ones grid() cuts
        # from the committed screens, so a change to GRID_PANES or a screen
        # that leaves the grid as it was fails here.
        shown = os.path.join(t3_common.ROOT, "docs", "showcase")
        grid = t3_showcase.grid(shown)
        self.assertEqual((grid.width, grid.height), (1922, 1082))
        committed = t3img.read(os.path.join(shown, t3_showcase.GRID))
        self.assertEqual((committed.width, committed.height, committed.data),
                         (grid.width, grid.height, grid.data))
        self.assertEqual([name for name, _, _ in t3_showcase.GRID_PANES],
                         ["double-size.png", "emoji.png", "pictures.png", "syllables.png",
                          "hero.png", "color.png", "ligatures.png", "boxes.png", "scripts.png"])
        # the middle pane, the hero's, is the hero's pixels at 1:1
        name, x, y = t3_showcase.GRID_PANES[4]
        hero = t3img.read(os.path.join(shown, name))
        for dy in (0, 1, 180, 359):
            row = grid.row(361 + dy)[641 * 3:(641 + 640) * 3]
            self.assertEqual(row, hero.row(y + dy)[x * 3:(x + 640) * 3], dy)
        line = bytes(t3_showcase.GRID_LINE)
        self.assertEqual(grid.row(360), line * 1922)
        self.assertEqual(grid.row(721), line * 1922)
        for row in (0, 500, 1081):
            for col in (640, 1281):
                self.assertEqual(grid.row(row)[col * 3:col * 3 + 3], line, (col, row))
        # writing it again leaves the file as it is
        out = os.path.join(self.dir.name, "grid")
        shutil.copytree(shown, out, ignore=shutil.ignore_patterns("*.md", "small"))
        self.assertIsNone(t3_showcase.write_grid(out))
        os.unlink(os.path.join(out, t3_showcase.GRID))
        self.assertEqual(t3_showcase.write_grid(out), os.path.join(out, t3_showcase.GRID))
        with open(os.path.join(out, t3_showcase.GRID), "rb") as a, \
                open(os.path.join(shown, t3_showcase.GRID), "rb") as b:
            self.assertEqual(a.read(), b.read())

    def test_crop(self):
        img = t3img.Image(4, 3, bytes(range(36)))
        self.assertEqual(t3_showcase.crop(img, 1, 1, 2, 2).data,
                         bytes(range(15, 21)) + bytes(range(27, 33)))
        self.assertEqual(t3_showcase.crop(img, 0, 0, 4, 3).data, img.data)
        for x, y, w, h in ((3, 0, 2, 1), (0, 2, 1, 2), (-1, 0, 1, 1)):
            with self.assertRaisesRegex(ValueError, "a crop of"):
                t3_showcase.crop(img, x, y, w, h)

    def test_compose_refuses_a_run_that_did_not_capture(self):
        run, report = self.run_dir({"showcase-emoji": "fail"})
        with self.assertRaisesRegex(RuntimeError, "showcase-emoji is fail"):
            t3_showcase.compose(run, report, os.path.join(self.dir.name, "out"))
        self.assertFalse(os.path.exists(os.path.join(self.dir.name, "out")))
        with open(os.path.join(run, "events.log"), "w") as f:
            f.write("T3-BAND showcase-iosevka rows=2 cell=8x4\n")
        with self.assertRaisesRegex(RuntimeError, "showcase-terminus reported no height"):
            t3_showcase.image_of("fonts.png", t3_showcase.FONT_BANDS, run,
                                 ["T3-BAND showcase-iosevka rows=2 cell=8x4"])

    def test_disc(self):
        # The disc carries the fonts and, beside the screens, the pictures,
        # the gallery and the palettes the scenes read.
        cache = os.path.join(self.dir.name, "cache")
        os.makedirs(cache)
        for font in t3_showcase.FONTS:
            with open(os.path.join(cache, font.file), "w") as f:
                f.write(font.name)
        fonts = {f.file: f.file_sha256 for f in t3_showcase.FONTS}
        with mock.patch.object(t3_showcase, "fault", lambda d, f: None):
            t3_showcase.disc_fonts(os.path.join(self.dir.name, "disc"), cache)
        self.assertEqual(sorted(os.listdir(os.path.join(self.dir.name, "disc",
                                                        "showcase-fonts"))), sorted(fonts))
        with mock.patch.object(t3_showcase, "pictures",
                               lambda d: os.makedirs(d, exist_ok=True)), \
                mock.patch.object(t3_showcase_gallery, "disc") as gallery:
            t3_showcase.disc_screens(self.dir.name)
        gallery.assert_called_once_with(os.path.join(self.dir.name, "showcase"))
        with open(os.path.join(self.dir.name, "showcase", "palettes.txt")) as f:
            first = f.readline().split()
        self.assertEqual(first[0], "legacy")
        self.assertEqual(len(first), 17)
        self.assertEqual(first[2], "205;0;0")



class SeatTest(unittest.TestCase):
    """The seat and kernel-panic checks' verdicts (t3_seat), on console
    logs as vtium and the kernel write them."""

    OPEN = (b"seat: the seat seat0 is open\n"
            b"seat: opened /dev/input/event0 through the seat manager\n"
            b"input: using /dev/input/event0 [AT Translated Set 2 keyboard] 0001:0001 "
            b"as keyboard\n")

    def test_a_seat_with_every_keyboard_through_it_passes(self):
        self.assertIsNone(t3_seat.seat_verdict(self.OPEN))

    def test_a_keyboard_opened_directly_fails(self):
        log = self.OPEN + (b"input: using /dev/input/event4 [QEMU Virtio Keyboard] "
                           b"0627:0001 as keyboard\n")
        self.assertEqual(t3_seat.seat_verdict(log),
                         "these devices did not open through the seat manager: "
                         "/dev/input/event4")

    def test_the_kernels_vt_fails(self):
        log = (b"seat: no seat manager answered, so vtium uses the kernel's VT: "
               b"Function not implemented\n" + self.OPEN)
        self.assertEqual(t3_seat.seat_verdict(log),
                         "vtium ran on the kernel's VT: no seat manager answered")

    DIRECT = (b"seat: vtium is in no logind session of its own, so it takes the seat from "
              b"seatd alone, and never from logind\n"
              b"seat: no seat manager answered, so vtium uses the kernel's VT: No such file\n"
              b"vt: using tty /dev/tty2\n")

    def test_direct_mode_on_the_kernels_vt_passes(self):
        self.assertIsNone(t3_seat.direct_verdict(self.DIRECT))

    def test_direct_mode_that_opens_the_seat_or_says_nothing_fails(self):
        self.assertEqual(t3_seat.direct_verdict(self.DIRECT + self.OPEN),
                         "vtium opened the seat in direct mode")
        self.assertEqual(t3_seat.direct_verdict(b"vt: using tty /dev/tty2\n"),
                         "the console never logged %r" % t3_seat.SEAT_DIRECT)

    def test_no_seat_and_no_keyboard_fail(self):
        self.assertEqual(t3_seat.seat_verdict(b"vt: using tty /dev/tty2\n"),
                         "the console never logged 'seat: the seat seat0 is open'")
        self.assertEqual(t3_seat.seat_verdict(b"seat: the seat seat0 is open\n"),
                         "the console used no keyboard")

    def test_the_panic_counts_only_after_the_request(self):
        line = t3_seat.PANIC_LINE.encode()
        log = b"boot\n" + line + b"\n"
        self.assertIsNone(t3_seat.panic_verdict(log, 0))
        self.assertEqual(t3_seat.panic_verdict(log, len(log)),
                         "the console never showed %r" % t3_seat.PANIC_LINE)

    # The agent's answer to `logins` as the guest gives it after a login
    # in a scope of its own (the spike of 2026-11-10, measured): vtium leads
    # c1 on seat0, the user's shell runs in session 2, with no seat.
    LOGINS = ["vtium 431",
              "session c1 User=0 Seat=seat0 VTNr=2 Class=greeter Active=yes TTY=tty2 "
              "Leader=431",
              "session 2 User=1000 Seat= VTNr=0 Class=user Active=no TTY=pts/0 Leader=502",
              "shell 517 1000 /user.slice/user-1000.slice/session-2.scope "
              "XDG_RUNTIME_DIR=/run/user/1000 XDG_SESSION_ID=2",
              "user 1000 active"]

    def test_a_login_in_a_session_of_its_own_with_no_seat_passes(self):
        self.assertIsNone(t3_seat.login_verdict(t3_seat.parse_logins(self.LOGINS)))

    def test_two_logins_wait_until_logind_lists_each_shells_session(self):
        # As a guest answered once: the second shell already ran in
        # session-4.scope, and logind did not list session 4 yet.
        second = ["shell 732 1000 /user.slice/user-1000.slice/session-4.scope "
                  "XDG_RUNTIME_DIR=/run/user/1000 XDG_SESSION_ID=4",
                  "user 1000 active"]
        early = t3_seat.parse_logins(self.LOGINS + second)
        self.assertFalse(t3_seat.logins_settled(early, 2))
        listed = t3_seat.parse_logins(
            self.LOGINS + ["session 4 User=1000 Seat= VTNr=0 Class=user Active=no TTY=pts/1 "
                           "Leader=700"] + second)
        self.assertTrue(t3_seat.logins_settled(listed, 2))
        self.assertFalse(t3_seat.logins_settled(t3_seat.parse_logins(self.LOGINS), 2))

    def test_vtiums_own_session_must_be_a_greeters(self):
        # Without class=greeter in vtium.pam, pam_systemd gives vtium's
        # root session on tty2 the class user-early (measured in the guest).
        logins = [line.replace("Class=greeter", "Class=user-early") for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(logins)),
                         "vtium's own session c1 has the class user-early, not greeter")

    def test_a_login_inside_vtiums_session_fails(self):
        logins = [line.replace("/user.slice/user-1000.slice/session-2.scope",
                               "/user.slice/user-0.slice/session-c1.scope")
                  for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(logins)),
                         "the shell 517 of uid 1000 runs in "
                         "/user.slice/user-0.slice/session-c1.scope, which is no logind "
                         "session of its user's own")

    def test_a_login_on_vtiums_seat_or_vt_fails(self):
        on_seat = [line.replace("Seat= VTNr=0", "Seat=seat0 VTNr=2") for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(on_seat)),
                         "the user's session 2 is on the seat seat0, not on none")
        with_vtnr = [line + " XDG_VTNR=2" if line.startswith("shell") else line
                     for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(with_vtnr)),
                         "the shell 517 has XDG_VTNR=2")

    def test_vtium_losing_the_seat_fails(self):
        lost = [line.replace("Active=yes", "Active=no") for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(lost)),
                         "vtium's own session c1 lost the seat: {'User': '0', 'Seat': 'seat0', "
                         "'VTNr': '2', 'Class': 'greeter', 'Active': 'no', 'TTY': 'tty2', "
                         "'Leader': '431'}")

    def test_no_runtime_dir_or_user_manager_fails(self):
        bare = [line.replace("XDG_RUNTIME_DIR=/run/user/1000 ", "") for line in self.LOGINS]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(bare)),
                         "the shell 517's XDG_RUNTIME_DIR is None")
        stopped = self.LOGINS[:-1] + ["user 1000 inactive"]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(stopped)),
                         "user@1000.service is inactive")

    def test_two_logins_need_two_sessions(self):
        second = ["session 3 User=1000 Seat= VTNr=0 Class=user Active=no TTY=pts/1 Leader=530",
                  "shell 545 1000 /user.slice/user-1000.slice/session-3.scope "
                  "XDG_RUNTIME_DIR=/run/user/1000 XDG_SESSION_ID=3",
                  "user 1000 active"]
        self.assertIsNone(t3_seat.login_verdict(t3_seat.parse_logins(self.LOGINS + second),
                                                shells=2))
        shared = [line.replace("session-3.scope", "session-2.scope") for line in second]
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(self.LOGINS + shared),
                                               shells=2),
                         "two shells share the session 2")
        self.assertEqual(t3_seat.login_verdict(t3_seat.parse_logins(self.LOGINS), shells=2),
                         "want 2 shell(s) logged in, the agent found 1")

    def test_a_vtium_line_is_found_by_its_message_and_pid(self):
        text = "\n".join([
            "[  4.1] vtium[311]: seat: the seat seat0 is open",
            "[  9.2] vtium[420]: seat: seatd listens: the seat opens once the VT is shown",
            "[  9.3] systemd[1]: seat: VT 4 is shown, so vtium opens the seat now",
            "[ 12.5] vtium[420]: seat: VT 4 is shown, so vtium opens the seat now",
            "[ 12.6] vtium[420]: seat: the seat seat0 is open",
        ])
        line, pid = t3_seat.vtium_line(text, t3_seat.SEAT_WAITS)
        self.assertEqual(pid, 420)
        self.assertTrue(line.endswith(t3_seat.SEAT_WAITS))
        # A line from another process, and one from another vtium, count
        # for nothing.
        self.assertEqual(t3_seat.vtium_line(text, t3_seat.SEAT_SHOWN % 4)[1], 420)
        self.assertEqual(t3_seat.vtium_line(text, t3_seat.SEAT_OPEN, 420),
                         ("[ 12.6] vtium[420]: seat: the seat seat0 is open", 420))
        self.assertEqual(t3_seat.vtium_line(text, t3_seat.SEAT_OPEN)[1], 311)
        self.assertIsNone(t3_seat.vtium_line(text, t3_seat.SEAT_BUSY))
        self.assertIsNone(t3_seat.vtium_line(text, t3_seat.SEAT_SHOWN % 4, 311))

    def test_the_seat_lines_are_the_ones_vtium_logs(self):
        # Each line the seat checks wait for, after its target, is a string
        # of seat.rs, so a reworded line fails here and not in a VM.
        with open(os.path.join(t3_common.ROOT, "crates", "vtium-platform", "src",
                               "seat.rs")) as f:
            source = f.read()
        for line in (t3_seat.SEAT_OPEN.replace("seat0", "{name}"), t3_seat.SEAT_WAITS,
                     t3_seat.SEAT_BUSY, t3_seat.SEAT_SHOWN.replace("%d", "{num}"),
                     t3_seat.SEAT_LOST):
            self.assertIn(line.split(": ", 1)[1], source)

    def test_a_hold_is_judged_both_ways(self):
        good = ["held 922", "refusals 3", "vtium 928", "opened no", "released 922",
                "restarts 0"]
        hold = t3_seat.parse_hold(good)
        self.assertEqual(hold, {"held": 922, "refusals": 3, "vtium": 928, "opened": False,
                                "released": 922, "restarts": 0})
        self.assertIsNone(t3_seat.hold_verdict(hold))
        bad = {
            "refused: chvt 8 failed, so tty8 is not shown and nothing holds the seat":
                "no client held the seat: chvt 8 failed",
            "refusals 2": "seatd refused vtium 2 times",
            "opened yes": "vtium opened the seat while another client held it",
            "restarts 1": "restarted 1 times",
            "vtium 0": "runs no process",
        }
        for line, want in bad.items():
            word = line.split()[0]
            answer = [l for l in good if l.split()[0] != word.rstrip(":")] + [line]
            with self.subTest(line):
                self.assertIn(want, t3_seat.hold_verdict(t3_seat.parse_hold(answer)))
        self.assertIn("did not end", t3_seat.hold_verdict(
            t3_seat.parse_hold([l for l in good if not l.startswith("released")])))

    def test_a_stop_of_the_seat_manager_is_judged_both_ways(self):
        good = ["vtium 512", "stopped seatd",
                "exited vtium@tty2.service: Main process exited, code=exited, status=1/FAILURE",
                "restarts 1"]
        stop = t3_seat.parse_stop(good)
        self.assertEqual(stop["vtium"], 512)
        self.assertTrue(stop["stopped"])
        self.assertEqual(stop["restarts"], 1)
        self.assertIsNone(t3_seat.stop_verdict(stop))
        bad = {
            "refused: vtium@tty2.service runs no process": "stopped nothing",
            "vtium 0": "ran no process",
            "stopped nothing": "did not stop",
            "exited vtium@tty2.service: Main process exited, code=killed, status=11/SEGV":
                "did not exit with status 1",
            "exited vtium@tty2.service: Main process exited, code=exited, status=0/SUCCESS":
                "did not exit with status 1",
            "restarts 0": "did not restart",
        }
        for line, want in bad.items():
            word = line.split()[0]
            answer = [l for l in good if l.split()[0] != word.rstrip(":")] + [line]
            with self.subTest(line):
                self.assertIn(want, t3_seat.stop_verdict(t3_seat.parse_stop(answer)))
        self.assertIn("did not exit once seatd stopped", t3_seat.stop_verdict(
            t3_seat.parse_stop([l for l in good if not l.startswith("exited")])))

    def test_the_seat_manager_stops_on_seatd(self):
        v = t3_scenario.VARIANTS["seat-seatd-stop"]
        self.assertEqual(v["seat"], "seatd")
        self.assertTrue(v["agent"])
        self.assertNotIn("mode", v)
        self.assertEqual(v["tree_features"], ["libseat", "logind"])
        self.assertEqual(t3_scenario.variant_cases("seat-seatd-stop")[-2:], ["seat", "seat-stop"])
        self.assertIn("seat-seatd-stop", t3_scenario.SEAT_VARIANTS)
        self.assertEqual(t3_evaluate.GOLDENS_OF["seat-seatd-stop"], "2d-potato-min")
        self.assertIs(t3_scenario.CHECK_RUNS["seat-stop"], t3_seat.check_seat_stop)
        self.assertIn("seat-stop", t3_seat.CHECKS)
        self.assertIn("seat-stop", t3_evaluate.NO_GOLDEN)

    def test_the_seat_waits_run_on_seatd(self):
        v = t3_scenario.VARIANTS["seat-seatd-wait"]
        self.assertEqual(v["seat"], "seatd")
        self.assertTrue(v["agent"])
        self.assertNotIn("mode", v)
        self.assertEqual(v["tree_features"], ["libseat", "logind"])
        self.assertEqual(t3_scenario.variant_cases("seat-seatd-wait")[-3:],
                         ["seat", "seat-shown", "seat-busy"])
        self.assertIn("seat-seatd-wait", t3_scenario.SEAT_VARIANTS)
        self.assertEqual(t3_evaluate.GOLDENS_OF["seat-seatd-wait"], "2d-potato-min")
        self.assertIs(t3_scenario.CHECK_RUNS["seat-shown"], t3_seat.check_seat_shown)
        self.assertIs(t3_scenario.CHECK_RUNS["seat-busy"], t3_seat.check_seat_busy)
        for case in ("seat-shown", "seat-busy"):
            self.assertIn(case, t3_seat.CHECKS)
            self.assertIn(case, t3_evaluate.NO_GOLDEN)

    def test_the_variants_run_their_checks(self):
        for name in ("seat-seatd", "seat-logind"):
            self.assertEqual(t3_scenario.VARIANTS[name]["tree_features"], ["libseat", "logind"])
            self.assertIn("seat", t3_scenario.variant_cases(name))
            self.assertEqual(t3_evaluate.GOLDENS_OF[name], "2d-potato-min")
        self.assertEqual(t3_scenario.variant_cases("seat-logind")[-2:],
                         ["seat", "login-session"])
        self.assertEqual(t3_scenario.variant_cases("seat-logind-survive")[-4:],
                         ["seat", "login-session", "seat-cgroup", "session-survives"])
        self.assertEqual(t3_scenario.variant_cases("seat-logind-two")[-3:],
                         ["seat", "login-session", "two-logins"])
        self.assertTrue(t3_scenario.VARIANTS["seat-logind"]["agent"])
        self.assertEqual(t3_scenario.VARIANTS["seat-seatd"]["seat"], "seatd")
        self.assertEqual(t3_scenario.VARIANTS["seat-logind"]["seat"], "logind")
        # The logind variants turn seat mode on through vtium console; the
        # others run direct mode, as the package's install does, and
        # direct-logind proves direct mode on the same tree under logind.
        for name in ("seat-logind", "seat-logind-survive", "seat-logind-two"):
            self.assertEqual(t3_scenario.VARIANTS[name]["mode"], "seat", name)
        self.assertNotIn("mode", t3_scenario.VARIANTS["seat-seatd"])
        direct = t3_scenario.VARIANTS["direct-logind"]
        self.assertEqual(direct["tree_features"], ["libseat", "logind"])
        self.assertNotIn("mode", direct)
        self.assertNotIn("seat", direct)
        self.assertEqual(t3_scenario.variant_cases("direct-logind")[-1:], ["direct"])
        self.assertEqual(t3_evaluate.GOLDENS_OF["direct-logind"], "2d-potato-min")
        self.assertIn("direct-logind", t3_scenario.SEAT_VARIANTS)
        self.assertEqual(t3_scenario.variant_cases("panic-sysrq"), ["kernel-panic"])
        self.assertTrue(t3_scenario.VARIANTS["panic-sysrq"]["agent"])
        self.assertEqual(t3_scenario.variant_cases("kmsg-notice"), ["kernel-message"])
        self.assertTrue(t3_scenario.VARIANTS["kmsg-notice"]["agent"])
        self.assertNotIn("kernel-message", t3_evaluate.NO_GOLDEN)
        self.assertEqual(t3_evaluate.golden_for("kmsg-notice", "kernel-message",
                                                t3_evaluate.GOLDEN)[0],
                         os.path.join(t3_evaluate.GOLDEN, "kmsg-notice", "kernel-message.png"))
        for case in t3_seat.CHECKS:
            self.assertIn(case, t3_evaluate.NO_GOLDEN)
            self.assertEqual(t3_evaluate.golden_for("seat-seatd", case), (None, None))


class BuiltinFontTest(unittest.TestCase):
    """The case of vtium's built-in font (t3_builtin, ADR-1204 D1): its
    variant, the agent's `bare` and the verdict on its answer, and the
    driver's path through both cases."""

    def test_the_variant(self):
        # The minspec machine of 2d-potato-min, in make tier3-minimal, with
        # the agent; its two cases are judged by goldens of its own.
        v = t3_scenario.VARIANTS["builtin-font"]
        potato = t3_scenario.VARIANTS["2d-potato-min"]
        for key in ("device", "cpus", "memory_mb", "renderer", "images", "conf"):
            self.assertEqual(v[key], potato[key], key)
        self.assertTrue(v["agent"])
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            minimal = re.search(r"^tier3-minimal:.*\n\t(.*)", f.read(), re.M).group(1)
        names = re.search(r"\$\(or \$\(VARIANTS\),([^)]*)\)", minimal).group(1).split()
        self.assertIn("builtin-font", names)
        self.assertEqual(t3_scenario.variant_cases("builtin-font"),
                         ["builtin-login", "builtin-notice"])
        self.assertIn("builtin-font", t3_scenario.golden_cases())
        for case in t3_builtin.CASES:
            self.assertNotIn(case, t3_evaluate.NO_GOLDEN)
            self.assertEqual(t3_evaluate.describe(case), (t3_builtin.CAPTIONS[case], None))
            self.assertEqual(t3_evaluate.golden_for("builtin-font", case, t3_evaluate.GOLDEN)[0],
                             os.path.join(t3_evaluate.GOLDEN, "builtin-font", case + ".png"))

    GOOD = ["before 765", "stopped vtium@tty2.service"] + [
        "gone %s" % p for p in t3_builtin.BARE_PATHS] + [
        "started vtium@tty2.service", "unit vtium@tty2.service active", "vtium 832 0.0"]

    def test_the_answer_to_bare_is_judged_both_ways(self):
        self.assertIsNone(t3_builtin.bare_verdict(t3_builtin.parse_bare(self.GOOD)))
        for change, why in (
                (("gone /etc/vtium", "kept /etc/vtium"), "the agent did not remove /etc/vtium: kept"),
                (("gone /usr/bin/vtium-config", "rm: cannot remove"),
                 "the agent did not remove /usr/bin/vtium-config: no answer"),
                (("started vtium@tty2.service", "Job failed"),
                 "vtium's unit did not start again: its state is active"),
                (("unit vtium@tty2.service active", "unit vtium@tty2.service failed"),
                 "vtium's unit did not start again: its state is failed"),
                (("vtium 832 0.0", "none"), "vtium's unit runs no process after its start"),
                (("vtium 832 0.0", "vtium 765 9.0"),
                 "vtium's PID is 765 before and after, so the agent started no new vtium")):
            with self.subTest(change):
                lines = [change[1] if line == change[0] else line for line in self.GOOD]
                self.assertEqual(t3_builtin.bare_verdict(t3_builtin.parse_bare(lines)), why)

    # vtium's unit, as a fake systemctl on PATH: each call is logged with
    # whether /etc/vtium still exists below T3_AGENT_ROOT, and a start
    # gives the unit a new PID, T3_FAKE_PID, a process that exists, whose
    # age the agent reads from /proc.
    FAKE_SYSTEMCTL = """#!/bin/bash
d=$T3_FAKE_UNIT
there=no; [ -e "$T3_AGENT_ROOT/etc/vtium" ] && there=yes
echo "$* etc-vtium=$there" >> "$d/calls"
case "$1 $2 $3" in
"show -P MainPID") cat "$d/pid" ;;
"show -P ActiveState") echo active ;;
"stop vtium@tty2.service ") ;;
"start vtium@tty2.service ") echo "$T3_FAKE_PID" > "$d/pid" ;;
*) echo "fake systemctl: $*" >&2; exit 1 ;;
esac
"""

    def test_the_agent_removes_what_bare_names_while_vtium_is_stopped(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        bin_dir, unit, root = (os.path.join(d.name, n) for n in ("bin", "unit", "root"))
        for path in (bin_dir, unit, os.path.join(root, "etc/vtium/fonts"),
                     os.path.join(root, "usr/local/share/fonts/vtium-tier3"),
                     os.path.join(root, "usr/bin")):
            os.makedirs(path)
        for path in ("etc/vtium/vtium.conf", "etc/vtium/fonts/minspec-16.vtfrit",
                     "usr/local/share/fonts/vtium-tier3/a.otf", "usr/bin/vtium-config",
                     "usr/bin/vtium"):
            with open(os.path.join(root, path), "w") as f:
                f.write("x\n")
        with open(os.path.join(bin_dir, "systemctl"), "w") as f:
            f.write(self.FAKE_SYSTEMCTL)
        os.chmod(os.path.join(bin_dir, "systemctl"), 0o755)
        with open(os.path.join(unit, "pid"), "w") as f:
            f.write("765\n")
        ask = HeadsTest.start_agent(self, d.name, {
            "PATH": bin_dir + os.pathsep + os.environ["PATH"], "T3_FAKE_UNIT": unit,
            "T3_AGENT_ROOT": root, "T3_AGENT_TEMPLATE": os.path.join(d.name, "none"),
            "T3_FAKE_PID": str(os.getpid())})
        answer = ask(1, "bare")
        self.assertEqual(answer[:-1], self.GOOD[:-1])
        self.assertRegex(answer[-1], r"^vtium %d [0-9.]+$" % os.getpid())
        self.assertIsNone(t3_builtin.bare_verdict(t3_builtin.parse_bare(answer)))
        # vtium is stopped before anything is removed and started after
        with open(os.path.join(unit, "calls")) as f:
            calls = [c for c in f.read().splitlines() if not c.startswith("show")]
        self.assertEqual(calls, ["stop vtium@tty2.service etc-vtium=yes",
                                 "start vtium@tty2.service etc-vtium=no"])
        # the console's own binary stays, and nothing the list leaves out goes
        self.assertEqual(sorted(os.listdir(root)), ["etc", "usr"])
        self.assertEqual(os.listdir(os.path.join(root, "etc")), [])
        self.assertEqual(os.listdir(os.path.join(root, "usr/bin")), ["vtium"])
        self.assertEqual(os.listdir(os.path.join(root, "usr/local/share/fonts")), [])

    class FakeVm:
        """A guest whose agent answers `bare` with `answer`."""

        def __init__(self, serial, answer):
            self.serial, self.answer, self.asked, self.waited = serial, answer, [], []

        def ask(self, request, timeout=30):
            self.asked.append(request)
            return self.answer

        def wait_serial(self, marker, timeout, since=0):
            self.waited.append((marker, since))
            return marker

    def drive(self, answer):
        """t3_builtin.run on a FakeVm answering `answer`, a screen that
        settles at once and a kernel message that is captured: (caps,
        errors, the vm, the phases)."""
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        serial = os.path.join(d.name, "serial.log")
        with open(serial, "wb") as f:
            f.write(b"0123456789")
        vm = self.FakeVm(serial, answer)
        caps, errors, phases = {}, {}, []

        def message(vm, work, caps, errors, log, case):
            caps[case] = os.path.join(work, case + ".ppm")
        with mock.patch.object(t3_builtin, "wait_stable") as stable, \
                mock.patch.object(t3_seat, "kernel_message", side_effect=message):
            t3_builtin.run(vm, d.name, caps, errors, LOG, phases.append)
        return caps, errors, vm, phases, stable, d.name

    def test_the_driver_runs_both_cases_on_the_new_vtium(self):
        caps, errors, vm, phases, stable, work = self.drive(self.GOOD)
        self.assertEqual(errors, {})
        self.assertEqual(vm.asked, ["bare"])
        # the new vtium's display line, written after the request
        self.assertEqual(vm.waited, [(t3_builtin.DISPLAY, 10)])
        self.assertEqual(stable.call_args.args[1], os.path.join(work, "builtin-login.ppm"))
        self.assertEqual(sorted(caps), ["builtin-login", "builtin-notice"])
        self.assertEqual(phases, ["builtin-login", "builtin-notice"])

    def test_a_bare_that_fails_fails_both_cases(self):
        lines = ["kept /etc/vtium" if l == "gone /etc/vtium" else l for l in self.GOOD]
        caps, errors, vm, phases, stable, _ = self.drive(lines)
        self.assertEqual(caps, {})
        self.assertEqual(errors, {case: "the agent did not remove /etc/vtium: kept"
                                  for case in t3_builtin.CASES})
        self.assertEqual((vm.waited, phases, stable.called), ([], [], False))


class ApplyRootTest(unittest.TestCase):
    """The check of vtium-config's privileged step as root (t3_apply,
    ADR-1403 D8): its variant, the agent's `apply-root` and the verdict on
    its answer, and the driver's path through the check."""

    def test_the_variant(self):
        # The minspec machine of 2d-potato-min with the agent; its one case
        # is a check without a golden.
        v = t3_scenario.VARIANTS["apply-root"]
        potato = t3_scenario.VARIANTS["2d-potato-min"]
        for key in ("device", "cpus", "memory_mb", "renderer", "images", "conf"):
            self.assertEqual(v[key], potato[key], key)
        self.assertTrue(v["agent"])
        self.assertEqual(t3_scenario.variant_cases("apply-root"), [t3_apply.CASE])
        self.assertIn(t3_apply.CASE, t3_evaluate.NO_GOLDEN)
        self.assertEqual(t3_evaluate.golden_for("apply-root", t3_apply.CASE), (None, None))
        self.assertIs(t3_scenario.CHECK_RUNS[t3_apply.CASE], t3_apply.check_apply_root)
        # tier3-minimal runs it, so a card that touches the install tree and
        # runs tier3-minimal at V1 reaches the root path.
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            make = f.read()
        minimal = re.search(r"^tier3-minimal:(.*)\n\t(.*)", make, re.M)
        names = re.search(r"\$\(or \$\(VARIANTS\),([^)]*)\)", minimal.group(2)).group(1).split()
        self.assertIn("apply-root", names)
        self.assertIn("vtium-config apply as root", minimal.group(1))

    GOOD = ["apply said wrote\t/etc/vtium/fonts/t3-apply.vtfrit",
            "apply said wrote\t/etc/vtium/vtium.conf",
            "apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
            "apply status 0", "apply frit root 644", "apply left 0", "apply conf 1",
            "apply console kept", "open was 755",
            "open said vtium-config: vtium reads no font set in /etc/vtium/fonts, so the "
            "install writes none there: other users can change \"/etc/vtium/fonts\" (mode "
            "777). Run `chmod go-w /etc/vtium/fonts` so that only its owner can change it.",
            "open said Nothing was installed.",
            "open status 1", "open frit none", "open left 0", "open conf 0", "open mode 755",
            "before 765", "stopped vtium@tty2.service", "started vtium@tty2.service",
            "unit vtium@tty2.service active", "vtium 832 0.0"]

    def test_the_answer_is_judged_both_ways(self):
        self.assertIsNone(t3_apply.apply_verdict(t3_apply.parse_apply(self.GOOD)))
        for change, why in (
                (("apply status 0", "apply status 1"), "apply as root exited 1: "),
                (("apply said wrote\t/etc/vtium/vtium.conf", "apply said failed"),
                 "apply as root did not write /etc/vtium/vtium.conf: "),
                (("apply frit root 644", "apply frit root 600"),
                 "the installed font set is root 600, not root's with mode 644"),
                (("apply conf 1", "apply conf 0"),
                 "the installed vtium.conf names t3-apply on 0 lines, not 1"),
                (("apply left 0", "apply left 1"), "apply as root left 1 private folders in /run"),
                (("apply console kept", "apply console changed"),
                 "the agent did not put the console back as it was after apply: changed"),
                (("apply console kept", "none"),
                 "the agent did not put the console back as it was after apply: None"),
                # a check that read no copy, so "left 0" would hold of a
                # folder never made under /run
                (("apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
                  "none"),
                 "apply's check did not read one copy of t3-apply in a folder of root's "
                 "under /run with mode 700: it read none"),
                (("apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
                  "apply checked /tmp/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700"),
                 "apply's check did not read one copy"),
                (("apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
                  "apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 755"),
                 "apply's check did not read one copy"),
                (("apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
                  "apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit aaron 700"),
                 "apply's check did not read one copy"),
                (("apply checked /run/vtium-config-apply-0123abcd/t3-apply.vtfrit root 700",
                  "apply checked /run/vtium-config-apply-0123abcd/other.vtfrit root 700"),
                 "apply's check did not read one copy"),
                (("open status 1", "open status 0"),
                 "apply wrote into a frits' directory at mode 0777 (exit 0): "),
                (("open said Nothing was installed.", "open said Something was installed."),
                 "the refused install does not say that nothing was installed: "),
                (("open frit none", "open frit root 644"),
                 "the refused install left the font set t3-open, root 644"),
                (("open conf 0", "open conf 1"),
                 "the refused install changed vtium.conf to name t3-open"),
                (("open left 0", "open left 2"), "the refused install left 2 private folders in /run"),
                (("open mode 755", "open mode 777"),
                 "the frits' directory has mode 777 afterwards, not its mode before, 755"),
                (("unit vtium@tty2.service active", "unit vtium@tty2.service failed"),
                 "vtium's unit did not start again: its state is failed"),
                (("vtium 832 0.0", "none"), "vtium's unit runs no process after its start"),
                (("vtium 832 0.0", "vtium 765 9.0"),
                 "vtium's PID is 765 before and after, so the agent started no new vtium")):
            with self.subTest(change):
                lines = [change[1] if line == change[0] else line for line in self.GOOD]
                got = t3_apply.apply_verdict(t3_apply.parse_apply(lines))
                self.assertIsNotNone(got)
                self.assertTrue(got.startswith(why), got)
        # two copies read are one too many
        lines = self.GOOD + [
            "apply checked /run/vtium-config-apply-4567cdef/t3-apply.vtfrit root 700"]
        self.assertIn("apply's check did not read one copy",
                      t3_apply.apply_verdict(t3_apply.parse_apply(lines)))
        # a refusal in other words than the walk's is not the refusal sought
        lines = [l for l in self.GOOD if "other users can change" not in l]
        self.assertIn("is not the walk's",
                      t3_apply.apply_verdict(t3_apply.parse_apply(lines)))

    # vtium-config's privileged step, as a fake below T3_AGENT_ROOT: it
    # refuses a bundle whose MANIFEST does not give the installed vtium.conf's
    # hash, or whose frits' directory others can write, in the walk's words;
    # otherwise it runs the console below T3_AGENT_ROOT as the check, on a
    # copy of the bundle's font set in a private folder in /run, and installs
    # the font set and vtium.conf. The folder is there while the fake works,
    # and it removes the folder.
    FAKE_APPLY = r"""#!/bin/bash
r=$T3_AGENT_ROOT
vtium=$r/usr/bin/vtium
[ "$1" = apply ] || exit 2
d=$2
run=$r/run/vtium-config-apply-fake
mkdir -m 700 "$run"
trap 'rm -r "$run"' EXIT
want=$(printf 'write\tvtium.conf\t%s' "$(sha256sum < "$r/etc/vtium/vtium.conf" | cut -d' ' -f1)")
frit=$(sed -n 's|^write\tfonts/\(.*\)\tnew$|\1|p' "$d/MANIFEST")
grep -qxF "$want" "$d/MANIFEST" || { echo "vtium-config: changed"; exit 1; }
if [ "$(stat -c %a "$r/etc/vtium/fonts")" = 777 ]; then
    echo "vtium-config: vtium reads no font set in /etc/vtium/fonts, so the install writes none there: other users can change it"
    echo "Nothing was installed."
    exit 1
fi
cp "$d/fonts/$frit" "$run/$frit"
"$vtium" check-config --configdir "$r/etc/vtium" --with-frit "$run/$frit" "$d/vtium.conf" \
    || exit 1
install -m644 "$d/fonts/$frit" "$r/etc/vtium/fonts/$frit"
install -m644 "$d/vtium.conf" "$r/etc/vtium/vtium.conf"
printf 'wrote\t/etc/vtium/fonts/%s\nwrote\t/etc/vtium/vtium.conf\n' "$frit"
"""

    def test_the_agent_installs_one_bundle_and_has_the_other_refused(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        bin_dir, unit, root = (os.path.join(d.name, n) for n in ("bin", "unit", "root"))
        for path in (bin_dir, unit, os.path.join(root, "etc/vtium/fonts"),
                     os.path.join(root, "usr/bin"), os.path.join(root, "run"),
                     os.path.join(root, "root")):
            os.makedirs(path)
        os.chmod(os.path.join(root, "etc/vtium/fonts"), 0o755)
        with open(os.path.join(root, "etc/vtium/vtium.conf"), "w") as f:
            f.write("no-blink\n")
        # the console's check passes when it reads the copy it is given
        fake_console = "#!/bin/bash\n[ \"$1\" = check-config ] && [ -f \"$5\" ]\n"
        for path, text in ((os.path.join(bin_dir, "systemctl"), BuiltinFontTest.FAKE_SYSTEMCTL),
                           (os.path.join(root, "usr/bin/vtium-config"), self.FAKE_APPLY),
                           (os.path.join(root, "usr/bin/vtium"), fake_console)):
            with open(path, "w") as f:
                f.write(text)
            os.chmod(path, 0o755)
        with open(os.path.join(unit, "pid"), "w") as f:
            f.write("765\n")
        ask = HeadsTest.start_agent(self, d.name, {
            "PATH": bin_dir + os.pathsep + os.environ["PATH"], "T3_FAKE_UNIT": unit,
            "T3_AGENT_ROOT": root, "T3_AGENT_TEMPLATE": os.path.join(d.name, "none"),
            "T3_FAKE_PID": str(os.getpid())})
        answer = ask(1, "apply-root")
        got = t3_apply.parse_apply(answer)
        user = pwd.getpwuid(os.getuid()).pw_name
        # the check read the copy in the private folder, which the agent's
        # program saw there with its owner and mode 700
        self.assertEqual(got["apply"], {
            "said": ["wrote\t/etc/vtium/fonts/t3-apply.vtfrit", "wrote\t/etc/vtium/vtium.conf"],
            "checked": ["%s/run/vtium-config-apply-fake/t3-apply.vtfrit %s 700" % (root, user)],
            "console": "kept", "status": 0, "frit": "%s 644" % user, "left": 0, "conf": 1},
            answer)
        self.assertEqual((got["open"]["checked"], got["open"]["console"]), ([], None), answer)
        # the console is back in its place, as it was, and the open run's
        # check, had it run, would have run the console itself
        with open(os.path.join(root, "usr/bin/vtium")) as f:
            self.assertEqual(f.read(), fake_console)
        self.assertEqual(got["open"]["status"], 1, answer)
        self.assertEqual((got["open"]["frit"], got["open"]["left"], got["open"]["conf"]),
                         ("none", 0, 0), answer)
        self.assertEqual((got["was"], got["mode"]), ("755", "755"))
        self.assertEqual((got["before"], got["started"], got["state"], got["after"]),
                         (765, True, "active", os.getpid()))
        # the set installed is the built-in Unscii-8 in its 8x8 cell, and
        # vtium.conf names it at its top level after what it held
        with open(os.path.join(root, "etc/vtium/fonts/t3-apply.vtfrit")) as f:
            self.assertEqual([l for l in f.read().splitlines() if not l.startswith("#")],
                             ["source=builtin:unscii-8", "cell=8x8"])
        with open(os.path.join(root, "etc/vtium/vtium.conf")) as f:
            self.assertEqual(f.read(), "no-blink\nfont-name=t3-apply\n")
        # the bundles are gone, and the agent stopped vtium before it
        # started it again
        self.assertEqual(os.listdir(os.path.join(root, "root")), [])
        with open(os.path.join(unit, "calls")) as f:
            calls = [c.split(" etc-vtium")[0] for c in f.read().splitlines()
                     if not c.startswith("show")]
        self.assertEqual(calls, ["stop vtium@tty2.service", "start vtium@tty2.service"])

    class FakeVm:
        """A guest whose agent answers `apply-root` with `answer`, and whose
        console logs `logged` while the agent works."""

        def __init__(self, serial, answer, logged):
            self.serial, self.answer, self.logged = serial, answer, logged
            self.asked, self.waited = [], []

        def ask(self, request, timeout=30):
            self.asked.append(request)
            with open(self.serial, "ab") as f:
                f.write(self.logged)
            return self.answer

        def wait_serial(self, marker, timeout, since=0, fail=None):
            self.waited.append((marker, since, fail))
            return marker

    # What the console logs while the agent works: the old vtium, PID 765,
    # and the new one, PID 832, which draws the set in its 8x8 cell.
    LOGGED = ("[    7.4] vtium[765]: the font is the frit terminus, drawn in the built-in "
              "font until its glyphs are generated\n"
              "[   11.9] vtium[832]: \x1b[0mvtium Revision v0.1.0\n"
              "[   11.9] vtium[832]: The font helper runs without its socket filter, because "
              "vtium has no seccomp filter for this machine's architecture.\n"
              "[   11.9] vtium[832]: the font is the frit t3-apply, drawn in the built-in "
              "font until its glyphs are generated\n"
              "[   12.0] vtium[832]: Display Virtual-1 with backend [drm2d] text renderer "
              "[cpu], cell size 8x8\n"
              "[   12.0] vtium[832]: the font's glyphs at 8x8 are generated: asked 39 ms "
              "after vtium started, the first in 30 ms, the last in 39 ms\n")

    def drive(self, answer, logged=LOGGED):
        """check_apply_root on a FakeVm answering `answer` and a screen that
        settles at once: (caps, errors, the vm, the wait_stable mock)."""
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        serial = os.path.join(d.name, "serial.log")
        with open(serial, "wb") as f:
            f.write(b"0123456789")
        vm = self.FakeVm(serial, answer, logged.encode())
        caps, errors = {}, {}
        with mock.patch.object(t3_apply, "wait_stable") as stable:
            t3_apply.check_apply_root({}, vm, d.name, caps, errors, LOG)
        return caps, errors, vm, stable, d.name

    def test_the_driver_waits_for_the_new_vtium_s_font(self):
        caps, errors, vm, stable, work = self.drive(self.GOOD)
        self.assertEqual(errors, {})
        self.assertEqual(vm.asked, [t3_apply.CASE])
        self.assertEqual(vm.waited, [(t3_apply.FONT_LINE, 10, None),
                                     (t3_apply.GLYPHS_LINE, 10, t3_apply.HELPER_FAILED)])
        self.assertEqual(caps, {t3_apply.CASE: os.path.join(work, "apply-root.ppm")})
        self.assertEqual(stable.call_args.args[1], os.path.join(work, "apply-root.ppm"))

    def test_a_new_vtium_that_does_not_draw_the_set_fails_the_check(self):
        # the old vtium's lines and the sandbox notice count for nothing;
        # each of these lines of the new vtium fails the check
        new = "[   12.1] vtium[832]: %s\n"
        for line, why in (
                ("the frit t3-apply: line 2: the key cel is not a frit key",
                 "the new vtium does not draw with the set t3-apply: the frit t3-apply: "),
                ("the font helper could not draw the font (it exited), so vtium uses its "
                 "built-in font, one character at a time with no combined characters",
                 "the new vtium does not draw with the set t3-apply: the font helper "),
                ("the font helper exited with status 1; restarting it (1 of 3 this boot)",
                 "the new vtium does not draw with the set t3-apply: the font helper "),
                ("the font t3-apply is not drawn (no cell): vtium draws its built-in font",
                 "the new vtium does not draw with the set t3-apply: the font t3-apply ")):
            with self.subTest(line):
                caps, errors, _, _, _ = self.drive(self.GOOD, self.LOGGED + new % line)
                self.assertEqual(caps, {})
                self.assertTrue(errors[t3_apply.CASE].startswith(why), errors)
        for gone, what in (("cell size 8x8", "a cell of 8x8"),
                           ("the font's glyphs at 8x8", "that the font helper generated")):
            with self.subTest(gone):
                logged = self.LOGGED.replace(gone, gone.replace("8x8", "8x16"))
                caps, errors, _, _, _ = self.drive(self.GOOD, logged)
                self.assertEqual(caps, {})
                self.assertIn(what, errors[t3_apply.CASE])
        # the same lines from the old vtium do not count
        caps, errors, _, _, _ = self.drive(
            self.GOOD, self.LOGGED + "[    9.0] vtium[765]: the frit terminus: a warning\n")
        self.assertEqual(errors, {})

    def test_an_answer_that_fails_fails_the_check_with_no_capture(self):
        lines = ["open status 0" if l == "open status 1" else l for l in self.GOOD]
        caps, errors, vm, stable, _ = self.drive(lines)
        self.assertEqual(caps, {})
        self.assertIn("apply wrote into a frits' directory at mode 0777", errors[t3_apply.CASE])
        self.assertEqual((vm.waited, stable.called), ([], False))


class HandOverTest(unittest.TestCase):
    """The display hand-over's check (t3_handover.py), without a guest."""

    PROBE = ["ROOTCHECK hand-over open /dev/dri/card0: ok",
             "ROOTCHECK hand-over master: yes, after 300 ms",
             "ROOTCHECK hand-over drew: connector 36 crtc 35 mode 1366x768",
             "ROOTCHECK hand-over flip: ok",
             "ROOTCHECK hand-over drop master: ok",
             "shown 2"]

    def test_a_probe_that_took_and_drew_on_the_card_passes(self):
        self.assertIsNone(t3_handover.probe_verdict(self.PROBE))

    def test_a_probe_that_never_took_the_card_fails(self):
        answer = ["ROOTCHECK hand-over open /dev/dri/card0: ok",
                  "ROOTCHECK hand-over master: no, vtium still held the card after 10 s",
                  "shown 2"]
        self.assertIn("never became", t3_handover.probe_verdict(answer))

    def test_a_probe_that_could_not_draw_fails(self):
        answer = [line for line in self.PROBE if "drew" not in line]
        self.assertIn("could not draw", t3_handover.probe_verdict(answer))

    def test_a_vt_switch_during_the_hand_over_fails(self):
        answer = self.PROBE[:-1] + ["shown 3"]
        self.assertIn("VT shown changed", t3_handover.probe_verdict(answer))

    def test_a_focus_out_then_a_focus_in_passes(self):
        self.assertIsNone(t3_handover.focus_verdict("T3-FOCUS 1b5b4f 1b5b49"))

    def test_other_reports_fail(self):
        for line in ("T3-FOCUS 1b5b49 1b5b4f", "T3-FOCUS none none", "T3-FOCUS 1b5b4f none",
                     "T3-FOCUS"):
            self.assertIsNotNone(t3_handover.focus_verdict(line), line)

    def test_the_variant_runs_the_agent_and_carries_the_probe(self):
        v = t3_scenario.VARIANTS["hand-over"]
        self.assertTrue(v["agent"] and v["probe"])
        self.assertEqual(v["checks"], [t3_handover.CASE])
        self.assertIn(t3_handover.CASE, t3_scenario.CHECK_RUNS)
        self.assertEqual(t3_evaluate.describe(t3_handover.CASE),
                         (t3_handover.CHECKS[t3_handover.CASE], None))

    def test_the_agent_answers_hand_over(self):
        with open(os.path.join(t3_common.GUEST, "t3-agent")) as f:
            agent = f.read()
        self.assertIn("        hand-over)\n", agent)
        self.assertIn("t3-probe take-master --check hand-over", agent)


def figures_line(t, frames, echoes=None, head="Virtual-1", pid=505, before=""):
    """A head's frame-times line on the serial log, as vtium logs it at debug
    level under the render target (Head::figures_line), with its
    key-to-present times when `echoes` is (count, mean, max)."""
    line = ("%s[%12.6f] vtium[%d]: render: head %s: %d frames, compose mean 1200 max 3400 us"
            % (before, t, pid, head, frames))
    if echoes:
        line += ", %d key echoes, key to present mean %d max %d us" % echoes
    return line


class KeyEchoTest(unittest.TestCase):
    """The key-echo check (t3_keyecho.py) and its parser of the frame-times
    line (t3_bench.figure_lines), without a guest."""

    EVENT = "T3-BENCH key-echo start=100.00 end=104.00 keys=54 read=54"
    CONF = ["no-blink", "render-timer=log", "debug"]

    def test_figure_lines(self):
        text = "\n".join([
            "[  99.000000] vtium[505]: render: head Virtual-1: 3 frames",  # cut short
            figures_line(99.5, 3),
            # a console line after a prompt on the same line, from a head
            # whose name has a space
            figures_line(100.5, 20, (18, 2100, 5300), head="left desk", before="$ "),
            timer_line(100.6, 3, 3),
        ])
        got = t3_bench.figure_lines(text)
        self.assertEqual(got, [
            {"t": 99.5, "pid": 505, "head": "Virtual-1", "frames": 3, "compose_us_mean": 1200,
             "compose_us_max": 3400, "echoes": 0, "echo_us_mean": 0, "echo_us_max": 0},
            {"t": 100.5, "pid": 505, "head": "left desk", "frames": 20, "compose_us_mean": 1200,
             "compose_us_max": 3400, "echoes": 18, "echo_us_mean": 2100, "echo_us_max": 5300},
        ])

    def test_lines_another_writer_broke_are_counted(self):
        # The serial console also carries the getty's and the kernel's
        # output, which can break into a frame-times line. figure_lines
        # leaves such a line out, and unread_figure_lines gives the stamp of
        # each one.
        text = "\n".join([
            "[  99.000000] vtium[505]: render: head Virtual-1: 3 frames",  # cut short
            figures_line(100.5, 20, (18, 2100, 5300)),
            # cut inside its echo part
            figures_line(101.5, 20) + ", 18 key ech",
            # a kernel message glued after it
            figures_line(102.5, 20, (18, 2100, 5300)) + "[  102.600000] random: crng init",
            # cut right after its compose times: its start reads as a line
            # with no echo, and its echo part follows another writer's line
            figures_line(103.5, 20),
            "[  103.600000] getty[400]: login: ",
            ", 18 key echoes, key to present mean 2100 max 5300 us",
            timer_line(103.7, 3, 3),
        ])
        self.assertEqual([x["t"] for x in t3_bench.figure_lines(text)], [100.5, 103.5])
        self.assertEqual(t3_bench.unread_figure_lines(text), [99.0, 101.5, 102.5, 103.5])
        self.assertEqual(t3_bench.unread_figure_lines(self.serial()), [])

    def test_key_echoes_in_the_window(self):
        # Only the lines stamped inside the window count. The mean is each
        # line's mean weighted by its echoes, and the max the largest.
        lines = t3_bench.figure_lines("\n".join([
            figures_line(99.9, 4, (12, 900, 1000)),      # before the window: out
            figures_line(100.8, 20, (20, 2000, 4000)),
            figures_line(101.8, 21, (20, 3000, 9000)),
            figures_line(102.8, 14, (14, 1000, 2000)),
            figures_line(103.2, 2),                       # no key echoed
            figures_line(104.6, 3, (3, 500, 600)),       # after the window: out
        ]))
        self.assertEqual(t3_bench.key_echoes(lines, 100.0, 104.0),
                         {"figure_lines": 4, "frames": 57, "key_echoes": 54,
                          "key_to_present_us_mean": 2111, "key_to_present_us_max": 9000})
        self.assertEqual(t3_bench.key_echoes(lines, 105.0, 106.0),
                         {"figure_lines": 0, "frames": 0, "key_echoes": 0})

    def serial(self):
        return "\n".join([figures_line(100.8, 20, (20, 2000, 4000)),
                          figures_line(101.8, 21, (20, 3000, 9000)),
                          figures_line(102.8, 14, (14, 1000, 2000))])

    def test_measure(self):
        got = t3_bench.measure(self.serial(), self.EVENT, self.CONF)
        self.assertEqual(got, {"seconds": 4.0, "keys": 54, "keys_read": 54, "figure_lines": 3,
                               "figure_lines_unread": 0, "frames": 55, "key_echoes": 54,
                               "key_to_present_us_mean": 2111, "key_to_present_us_max": 9000})

    def test_verdicts(self):
        serial = self.serial()
        # the echoing program read fewer keys than the driver typed
        got = t3_bench.measure(serial, self.EVENT.replace("read=54", "read=50"), self.CONF)
        self.assertEqual(got["error"], "the echoing program read 50 of the 54 keys typed")
        # more echoes than keys: the window holds keys typed outside the program
        got = t3_bench.measure(serial, self.EVENT.replace("keys=54 read=54", "keys=40 read=40"),
                               self.CONF)
        self.assertEqual(got["error"], "vtium measured 54 key echoes in the window, more than "
                                       "the 40 keys typed")
        # no echo measured, with debug logging on, fails
        got = t3_bench.measure(figures_line(101.0, 3), self.EVENT, self.CONF)
        self.assertEqual(got["error"], "no frame-times line stamped in the window 100.00 to "
                                       "104.00 s gives a key echo")
        # a frame-times line in the window that another writer broke into
        # fails the case, since its key echoes go uncounted
        broken = figures_line(103.5, 20, (18, 2100, 5300))
        for cut in (broken[:-20], broken + "[  103.600000] random: crng init"):
            with self.subTest(cut=cut):
                got = t3_bench.measure(serial + "\n" + cut, self.EVENT, self.CONF)
                self.assertEqual(got["figure_lines_unread"], 1)
                self.assertEqual(got["error"], "1 frame-times line stamped in the window 100.00 "
                                               "to 104.00 s cannot be read, because another "
                                               "writer broke into it on the serial console")
        # one broken outside the window is not needed, and fails nothing
        got = t3_bench.measure(serial + "\n" + broken.replace("103.5", "99.5")[:-20],
                               self.EVENT, self.CONF)
        self.assertEqual(got["figure_lines_unread"], 0)
        self.assertNotIn("error", got)
        # with debug logging off, vtium logs no frame-times line: a note
        got = t3_bench.measure("", self.EVENT, ["no-blink", "render-timer=log"])
        self.assertNotIn("error", got)
        self.assertEqual(got["note"], "debug is off for this variant, so vtium logs no "
                                      "frame-times line")

    def test_a_window_without_keys_fails(self):
        # The screen's name chooses the key-echo figures, so a window that
        # leaves out keys= fails instead of reading as another benchmark's.
        got = t3_bench.measure(self.serial(), self.EVENT.replace(" keys=54", ""), self.CONF)
        self.assertEqual(got["error"], "the key-echo screen reported no keys= count, so the "
                                       "check cannot tell how many keys were typed")
        self.assertEqual(t3_keyecho.CASE, t3_bench.KEY_ECHO)

    def test_the_minspec_variant_runs_the_check_with_debug_logging(self):
        v = t3_scenario.VARIANTS["2d-potato-min"]
        self.assertEqual(v["checks"], [t3_keyecho.CASE])
        self.assertEqual(v["conf_extra"], ["debug"])
        self.assertEqual(t3_heads.conf_lines(v), ["no-blink", "render-timer=log", "debug"])
        self.assertTrue(t3_heads.guest_conf(v).endswith("render-timer=log\ndebug\n"))
        self.assertIn(t3_keyecho.CASE, t3_scenario.CHECK_RUNS)
        self.assertIn(t3_keyecho.CASE, t3_evaluate.NO_GOLDEN)
        self.assertEqual(t3_scenario.variant_cases("2d-potato-min")[-1], t3_keyecho.CASE)
        self.assertEqual(t3_evaluate.describe(t3_keyecho.CASE),
                         (t3_keyecho.CHECKS[t3_keyecho.CASE], None))
        # the other variants of the minimal configuration keep it as it is
        self.assertNotIn("debug", t3_heads.conf_lines(t3_scenario.VARIANTS["builtin-font"]))
        # make tier3-minimal runs the variant
        with open(os.path.join(t3_common.ROOT, "Makefile")) as f:
            minimal = re.search(r"^tier3-minimal:.*\n\t(.*)", f.read(), re.M).group(1)
        self.assertIn("2d-potato-min", re.split(r"[\s,()]+", minimal))

    def test_the_keys_are_typable_and_the_script_reads_that_many(self):
        for ch in t3_keyecho.KEYS:
            t3_qemu.char_qcodes(ch)
        self.assertNotIn("\n", t3_keyecho.KEYS)
        with open(os.path.join(t3_common.GUEST, "screens", t3_keyecho.CASE + ".sh")) as f:
            script = f.read()
        self.assertIn("keys=%d\n" % len(t3_keyecho.KEYS), script)
        self.assertIn("stty raw -echo\n", script)
        self.assertIn('echo "T3-KEYS key-echo" > "$out"', script)

    def test_the_check_types_the_keys_and_judges_the_figures(self):
        # A stand-in VM: it answers the events the script reports and keeps
        # what the driver typed; the serial log holds the figures.
        with tempfile.TemporaryDirectory() as d:
            serial = os.path.join(d, "serial.log")
            with open(serial, "w") as f:
                f.write(self.serial() + "\n")
            events = os.path.join(d, "events.log")
            with open(events, "w") as f:
                f.write("T3-KEYS key-echo\n" + self.EVENT + "\nT3-DONE key-echo\n")
            typed = []
            vm = mock.Mock(serial=serial, events=events)
            vm.type.side_effect = typed.append
            img = t3img.Image(2, 1, b"\1" * 6)
            v = dict(t3_scenario.VARIANTS["2d-potato-min"])
            for figures, want in ((self.serial(), None),
                                  (figures_line(101.0, 3), "gives a key echo")):
                with self.subTest(want=want):
                    with open(serial, "w") as f:
                        f.write(figures + "\n")
                    typed.clear()
                    caps, errors = {}, {}
                    with mock.patch.object(t3_keyecho, "wait_stable", return_value=img):
                        t3_keyecho.check_key_echo(v, vm, d, caps, errors, LOG)
                    self.assertEqual(typed, ["t3 key-echo\n", t3_keyecho.KEYS])
                    if want:
                        self.assertIn(want, errors[t3_keyecho.CASE])
                        self.assertNotIn(t3_keyecho.CASE, caps)
                    else:
                        self.assertEqual(errors, {})
                        self.assertEqual(caps, {t3_keyecho.CASE: os.path.join(d, "key-echo.ppm")})
            # the figures go into the variant's records with the benchmarks'
            with open(serial, "w") as f:
                f.write(self.serial() + "\n")
            got = t3_scenario.bench_figures(vm, {t3_keyecho.CASE: "x"}, self.CONF, images=False)
            self.assertEqual(got[t3_keyecho.CASE]["key_to_present_us_max"], 9000)


class RootChecksTest(unittest.TestCase):
    """The driver of ADR-1400 D4's checks (t3_rootchecks.py), without a
    guest."""

    def test_answers_are_taken_after_libseat_log(self):
        # libseat writes its log line to the event port with no newline
        # before the probe's answer, so an answer can start mid-line.
        with tempfile.TemporaryDirectory() as d:
            events = os.path.join(d, "events.log")
            with open(events, "w") as f:
                f.write("ROOTCHECKS-BEGIN\n"
                        "ROOTCHECK 0 kernel: 7.2.11\n"
                        "00:00:00.085 \x1b[1;34m ROOTCHECK 3-built VT after the seat closed: kd=KD_TEXT\n"
                        "a line of libseat's alone\n")
            self.assertEqual(t3_rootchecks.answers(events),
                             ["ROOTCHECK 0 kernel: 7.2.11",
                              "ROOTCHECK 3-built VT after the seat closed: kd=KD_TEXT"])

    def test_disc_holds_the_script_the_probe_and_vtium_pam(self):
        with tempfile.TemporaryDirectory() as d:
            probe = os.path.join(d, "t3-probe")
            with open(probe, "w") as f:
                f.write("probe")
            disc = os.path.join(d, "disc")
            t3_rootchecks.stage(probe, disc)
            self.assertEqual(sorted(os.listdir(disc)), ["setup", "t3-probe", "vtium.pam"])
            with open(os.path.join(disc, "setup")) as f:
                self.assertIn("ROOTCHECKS-END", f.read())
            with open(os.path.join(disc, "vtium.pam")) as f:
                self.assertIn("pam_systemd.so class=greeter", f.read())

    def test_disc_carries_a_libseat_with_builtin_when_given(self):
        with tempfile.TemporaryDirectory() as d:
            probe = os.path.join(d, "t3-probe")
            lib = os.path.join(d, "libseat.so.1")
            for path in (probe, lib):
                with open(path, "w") as f:
                    f.write("x")
            disc = os.path.join(d, "disc")
            t3_rootchecks.stage(probe, disc, (lib, "seatd 0.9.3"))
            self.assertEqual(sorted(os.listdir(os.path.join(disc, "builtin"))),
                             ["VERSION", "libseat.so.1"])
            with open(os.path.join(disc, "builtin", "VERSION")) as f:
                self.assertEqual(f.read(), "seatd 0.9.3\n")


class FontSourcesTest(unittest.TestCase):
    """The case of the two font sets every build installs (t3_fonts,
    ADR-1204 D2): its variants, the sources' glyphs it reads, and its
    verdict on which font drew each probe."""

    def test_the_variants(self):
        # One variant for each set and height, on the minspec machine, each
        # logging in and running the check alone, which no golden judges.
        potato = t3_scenario.VARIANTS["2d-potato-min"]
        for size in t3_fonts.SIZES:
            for name, font in (("default-font-%d" % size, None),
                               ("cullet-mono-%d" % size, "cullet-mono")):
                v = t3_scenario.VARIANTS[name]
                for key in ("device", "cpus", "memory_mb", "renderer", "images"):
                    self.assertEqual(v[key], potato[key], (name, key))
                self.assertEqual(t3_scenario.variant_cases(name), [t3_fonts.CASE])
                with open(os.path.join(t3_common.GUEST, v["conf"])) as f:
                    conf = [line.strip() for line in f if not line.startswith("#")]
                self.assertIn("display-dpi=72", conf)
                self.assertIn("font-size=%d" % size, conf)
                names = [line for line in conf if line.startswith("font-name")]
                self.assertEqual(names, ["font-name=" + font] if font else [], name)
                self.assertEqual(v["size"], size)
                if font:
                    self.assertEqual(v["features"], "bidi,gpu,images")
                    self.assertEqual(v["deferred_frits"], ["cullet-alone.vtfrit"])
                    self.assertTrue(os.path.isfile(os.path.join(
                        t3_common.GUEST, "frits", "cullet-alone.vtfrit")))
                else:
                    self.assertNotIn("features", v)
        self.assertIn(t3_fonts.CASE, t3_evaluate.NO_GOLDEN)
        self.assertEqual(t3_evaluate.describe(t3_fonts.CASE),
                         (t3_fonts.CHECKS[t3_fonts.CASE], None))
        self.assertIs(t3_scenario.CHECK_RUNS[t3_fonts.CASE], t3_fonts.run)

    def test_the_probe_screen_prints_each_probe_at_its_column(self):
        out = subprocess.run(["bash", os.path.join(t3_common.GUEST, "screens", "font-probe.sh")],
                             capture_output=True, check=True).stdout.decode()
        row = out.split("\n")[t3_fonts.PROBE_ROW]
        found, column = {}, 0
        for c in row:
            if c != " ":
                found[c] = column
            column += 2 if c in t3_fonts.WIDE else 1
        self.assertEqual(found, t3_fonts.COLUMNS)

    BDF = ("STARTFONT 2.1\nCHARS 2\nSTARTCHAR a\nENCODING 103\nBBX 3 2 0 0\nBITMAP\n"
           "A0\n40\nENDCHAR\nSTARTCHAR b\nENCODING 104\nBBX 10 1 0 0\nBITMAP\n8040\n"
           "ENDCHAR\nENDFONT\n")

    def test_bdf_and_hex_glyphs_are_read(self):
        self.assertEqual(t3_fonts.parse_bdf(self.BDF, {103, 104}),
                         {103: (3, 2, {(0, 0), (2, 0), (1, 1)}), 104: (10, 1, {(0, 0), (9, 0)})})
        self.assertEqual(t3_fonts.parse_bdf(self.BDF, {104}), {104: (10, 1, {(0, 0), (9, 0)})})
        narrow = "0067:" + "80" + "00" * 14 + "01"
        wide = "020000:" + "8001" + "0000" * 15
        got = t3_fonts.parse_hex([narrow, wide, "0041:" + "00" * 16], {0x67, 0x20000})
        self.assertEqual(got, {0x67: (8, 16, {(0, 0), (7, 15)}),
                               0x20000: (16, 16, {(0, 0), (15, 0)})})

    def test_a_hollow_box_is_told_from_a_filled_one(self):
        frame = {(x, y) for x in range(2, 8) for y in range(3, 12)
                 if x in (2, 7) or y in (3, 11)}
        thick = {(x, y) for x in range(10) for y in range(14)
                 if x < 2 or x > 7 or y < 2 or y > 11}
        self.assertTrue(t3_fonts.hollow_box(frame))
        self.assertTrue(t3_fonts.hollow_box(thick))
        self.assertFalse(t3_fonts.hollow_box({(x, y) for x in range(4) for y in range(4)}))
        self.assertFalse(t3_fonts.hollow_box(frame - {(2, 5)}))
        # a box open at the top has no hollow inside
        self.assertFalse(t3_fonts.hollow_box({(x, y) for x, y in thick if y > 1}))
        self.assertFalse(t3_fonts.hollow_box(set()))

    # Glyphs of made-up sources, each a different shape: a probe's ink is
    # told apart by its shape alone.
    @staticmethod
    def glyph(width, height, seed):
        return (width, height, {(x, y) for x in range(width) for y in range(height)
                                if (x * 7 + y * 3 + seed) % 5 == 0})

    def sources(self):
        out = {}
        for char in t3_fonts.COLUMNS:
            code = ord(char)
            entry = {"Terminus %d px" % s: self.glyph(s // 2, s, code + s)
                     for s in t3_fonts.TERMINUS_STRIKES}
            entry["Unifont"] = self.glyph(16 if char in t3_fonts.WIDE else 8, 16, code)
            if char in t3_fonts.WIDE:
                entry = {"Unifont": entry["Unifont"]}
            out[code] = entry
        return out

    @staticmethod
    def screen(cell):
        width, height = 30 * cell[0], 3 * cell[1]
        return t3img.Image(width, height, bytearray(b"\x10\x10\x10" * width * height))

    @staticmethod
    def paint(img, char, cell, on, color=b"\xe8\xe8\xe8", k=1, at=(0, 0)):
        x0, y0, _, _ = t3_fonts.probe_box(char, cell)
        for x, y in on:
            for i in range(k):
                for j in range(k):
                    p = ((y0 + at[1] + y * k + j) * img.width + x0 + at[0] + x * k + i) * 3
                    img.data[p:p + 3] = color

    def terminus_screen(self, size, cell, glyphs, strike=None, grid=(0, 0)):
        """The probes as terminus draws them, in a grid that starts `grid`
        pixels right of and below the display's top left corner."""
        img = self.screen(cell)
        strike = strike or max(s for s in t3_fonts.TERMINUS_STRIKES if s <= size)
        for char in (t3_fonts.LETTER, t3_fonts.POWERLINE):
            self.paint(img, char, cell, glyphs[ord(char)]["Terminus %d px" % strike][2],
                       at=(grid[0], grid[1] + (size - strike) // 2))
        for char in (t3_fonts.HAN, t3_fonts.PLANE2, t3_fonts.EMOJI):
            self.paint(img, char, cell, glyphs[ord(char)]["Unifont"][2], k=size // 16, at=grid)
        self.paint(img, t3_fonts.ICON, cell, self.box(cell), at=grid)
        return img

    @staticmethod
    def box(cell):
        """A hollow box in a cell of `cell`, its sides two pixels thick and
        its top and bottom one."""
        return {(x, y) for x in range(1, cell[0] - 1) for y in range(2, cell[1] - 2)
                if x < 3 or x > cell[0] - 4 or y in (2, cell[1] - 3)}

    def test_terminus_is_judged_by_the_sources_glyphs(self):
        glyphs = self.sources()
        for size, cell in ((16, (8, 16)), (24, (12, 24)), (40, (20, 40))):
            img = self.terminus_screen(size, cell, glyphs)
            found, faults = t3_fonts.judge_terminus(img, cell, size, glyphs)
            self.assertEqual(faults, [], size)
            strike = "Terminus %d px" % min(size, 32)
            unifont = "Unifont x2" if size == 40 else "Unifont"
            self.assertEqual(found, {t3_fonts.LETTER: [strike], t3_fonts.POWERLINE: [strike],
                                     t3_fonts.HAN: [unifont], t3_fonts.PLANE2: [unifont],
                                     t3_fonts.EMOJI: [unifont], t3_fonts.ICON: ["a hollow box"]})
        # a grid that vtium centres on the display, as 1366 pixels hold 170
        # cells of 8 with 6 left over, and 768 pixels 19 rows of 40 with 8
        for size, cell, grid in ((16, (8, 16), (3, 0)), (40, (20, 40), (3, 4))):
            img = self.terminus_screen(size, cell, glyphs, grid=grid)
            self.assertEqual(t3_fonts.judge_terminus(img, cell, size, glyphs)[1], [], size)
        # the next smaller strike in a 24-pixel cell
        img = self.terminus_screen(24, (12, 24), glyphs, strike=22)
        _, faults = t3_fonts.judge_terminus(img, (12, 24), 24, glyphs)
        self.assertEqual(len(faults), 2)
        self.assertIn("the letter g matches Terminus 22 px, not Terminus 24 px alone", faults)
        # a letter whose shape two sources share names no one source
        same = self.sources()
        same[ord(t3_fonts.LETTER)]["Unifont"] = same[ord(t3_fonts.LETTER)]["Terminus 16 px"]
        img = self.terminus_screen(16, (8, 16), same)
        _, faults = t3_fonts.judge_terminus(img, (8, 16), 16, same)
        self.assertEqual(faults, ["the letter g matches Terminus 16 px and Unifont, not "
                                  "Terminus 16 px alone"])
        # Unifont at 1x where 2x fits
        img = self.terminus_screen(40, (20, 40), glyphs)
        x0, y0, w, h = t3_fonts.probe_box(t3_fonts.HAN, (20, 40))
        for y in range(y0, y0 + h):
            img.data[(y * img.width + x0) * 3:(y * img.width + x0 + w) * 3] = b"\x10" * 3 * w
        self.paint(img, t3_fonts.HAN, (20, 40), glyphs[ord(t3_fonts.HAN)]["Unifont"][2])
        _, faults = t3_fonts.judge_terminus(img, (20, 40), 40, glyphs)
        self.assertEqual(faults, ["the Han character U+4E2D matches Unifont, not Unifont x2 "
                                  "alone"])
        # an emoji in two colors is not drawn in 1 bit, and an icon drawn
        # from a font is no hollow box
        img = self.terminus_screen(16, (8, 16), glyphs)
        on = sorted(glyphs[ord(t3_fonts.EMOJI)]["Unifont"][2])
        self.paint(img, t3_fonts.EMOJI, (8, 16), on[:1], color=b"\xff\x00\x00")
        self.paint(img, t3_fonts.ICON, (8, 16), {(4, 8)})
        _, faults = t3_fonts.judge_terminus(img, (8, 16), 16, glyphs)
        self.assertEqual(faults, ["the emoji U+1F600 is drawn in 2 colors, not in 1 bit",
                                  "the Nerd Fonts icon U+F121 is not a hollow box"])

    def cullet_screens(self, cell, glyphs):
        """A capture of cullet-mono and the reference's, as Cullet Mono,
        Noto Color Emoji and Unifont would draw them."""
        img, reference = self.screen(cell), self.screen(cell)
        for char in (t3_fonts.LETTER, t3_fonts.ICON):
            shape = {(x, y) for x in range(1, cell[0] - 1) for y in range(2, cell[1] - 2)
                     if (x + y + ord(char)) % 3 == 0}
            for shot in (img, reference):
                self.paint(shot, char, cell, shape, color=b"\x80\x80\x80")
        self.paint(img, t3_fonts.EMOJI, cell, {(x, 3) for x in range(10)}, color=b"\xf0\xc0\x10")
        self.paint(img, t3_fonts.HAN, cell, glyphs[ord(t3_fonts.HAN)]["Unifont"][2],
                   k=cell[1] // 16)
        for shot in (img, reference):
            self.paint(shot, t3_fonts.LOGO, cell, self.box(cell))
        return img, reference

    def test_cullet_mono_is_judged_against_cullet_mono_alone(self):
        glyphs = self.sources()
        cell = (8, 16)
        img, reference = self.cullet_screens(cell, glyphs)
        found, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
        self.assertEqual(faults, [])
        self.assertEqual(found, {t3_fonts.LETTER: ["Cullet Mono"], t3_fonts.ICON: ["Cullet Mono"],
                                 t3_fonts.EMOJI: ["Noto Color Emoji"],
                                 t3_fonts.HAN: ["Unifont"], t3_fonts.LOGO: ["a hollow box"]})
        # a letter that the reference draws otherwise
        img, reference = self.cullet_screens(cell, glyphs)
        self.paint(reference, t3_fonts.LETTER, cell, {(0, 0)})
        _, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
        self.assertEqual(faults, ["the letter g differs from Cullet Mono's own glyph in "
                                  "cullet-alone"])
        # a letter that both captures draw from a bitmap source, as when
        # the console cannot read Cullet Mono and both draw from the
        # built-in font, whose 8x16 strike is Terminus's: the two captures
        # are the same, and the letter has the bitmap source's shape
        for source in ("Terminus 16 px", "Unifont"):
            img, reference = self.cullet_screens(cell, glyphs)
            x0, y0, w, h = t3_fonts.probe_box(t3_fonts.LETTER, cell)
            for shot in (img, reference):
                for y in range(y0, y0 + h):
                    shot.data[(y * shot.width + x0) * 3:(y * shot.width + x0 + w) * 3] = b"\x10" * 3 * w
                self.paint(shot, t3_fonts.LETTER, cell, glyphs[ord(t3_fonts.LETTER)][source][2])
            _, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
            self.assertEqual(faults, ["the letter g matches %s, not Cullet Mono's glyph" % source])
        # an icon drawn as a hollow box, an emoji in grey and a Font Logos
        # icon drawn as a glyph
        img, reference = self.cullet_screens(cell, glyphs)
        for shot in (img, reference):
            self.paint(shot, t3_fonts.ICON, cell, {(x, y) for x in range(8) for y in range(16)},
                       color=b"\x10\x10\x10")
            self.paint(shot, t3_fonts.ICON, cell, {(x, y) for x in (1, 6) for y in range(2, 14)}
                       | {(x, y) for x in range(1, 7) for y in (2, 13)})
        self.paint(img, t3_fonts.EMOJI, cell, {(x, 3) for x in range(10)}, color=b"\xa0\xa0\xa0")
        self.paint(img, t3_fonts.LOGO, cell, {(4, 8)})
        _, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
        self.assertEqual(faults, ["the Nerd Fonts icon U+F121 is a hollow box, not Cullet "
                                  "Mono's glyph",
                                  "the emoji U+1F600 holds no color, so Noto Color Emoji did "
                                  "not draw it",
                                  "the Font Logos icon U+F303 is not a hollow box"])

    def test_cullet_mono_draws_cjk_as_a_box_where_unifont_does_not_fit(self):
        # Cullet Mono's cell at 16 pixels is 7x16, narrower than Unifont's
        # 8-pixel glyphs, so the Han character draws as a hollow box
        # (ADR-1204 D2, amended 2026-11-18).
        glyphs = self.sources()
        cell = (7, 16)
        img, reference = self.cullet_screens(cell, glyphs)
        wide = (2 * cell[0], cell[1])
        # clear the probe and the columns Unifont's 16-pixel glyph spilled
        # past it, then draw the box
        x0, y0, w, h = t3_fonts.probe_box(t3_fonts.HAN, cell)
        w += cell[0]
        for y in range(y0, y0 + h):
            img.data[(y * img.width + x0) * 3:(y * img.width + x0 + w) * 3] = b"\x10" * 3 * w
        self.paint(img, t3_fonts.HAN, cell, self.box(wide))
        found, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
        self.assertEqual(faults, [])
        self.assertEqual(found[t3_fonts.HAN], ["a hollow box"])
        # a blank cell is not the hollow box
        for y in range(y0, y0 + h):
            img.data[(y * img.width + x0) * 3:(y * img.width + x0 + w) * 3] = b"\x10" * 3 * w
        _, faults = t3_fonts.judge_cullet(img, reference, cell, 16, glyphs)
        self.assertEqual(faults, ["the Han character U+4E2D is not a hollow box, though "
                                  "Unifont's 8-pixel glyphs do not fit a 7-pixel cell"])

    class FakeVm:
        """The parts of a VM the check reads: the console log, the event
        port, and the keys typed."""

        def __init__(self, d, cell, events=()):
            self.serial = os.path.join(d, "serial.log")
            self.events = os.path.join(d, "events.log")
            self.typed = []
            with open(self.serial, "w") as f:
                f.write("vtium[7]: Display Virtual-1 with backend [drm2d] text renderer [cpu], "
                        "cell size %dx%d\n" % cell)
            with open(self.events, "w") as f:
                f.write("".join(e + "\n" for e in events))

        def type(self, text):
            self.typed.append(text)

        def wait_event(self, marker, timeout):
            return marker

    def check(self, variant, shots, cell, events=()):
        """(caps, errors, the keys typed) of the check on a console whose
        screens are `shots`, the screen before the probes first."""
        glyphs = self.sources()
        with tempfile.TemporaryDirectory() as d:
            vm = self.FakeVm(d, cell, events)
            caps, errors = {}, {}
            with mock.patch.object(t3_fonts, "wait_stable", side_effect=shots), \
                    mock.patch.object(t3_fonts, "source_glyphs", return_value=glyphs):
                t3_fonts.run(t3_scenario.VARIANTS[variant], vm, d, caps, errors, LOG)
            return caps, errors, vm.typed

    def test_the_check_keeps_its_capture_only_when_it_passes(self):
        # A check without a golden passes whenever its capture is kept, so
        # a fault must leave the capture out.
        glyphs = self.sources()
        cell = (8, 16)
        good = self.terminus_screen(16, cell, glyphs)
        caps, errors, typed = self.check("default-font-16", [self.screen(cell), good], cell)
        self.assertEqual((list(caps), errors, typed), ([t3_fonts.CASE], {}, ["t3 font-probe\n"]))
        wrong = self.terminus_screen(16, cell, glyphs, strike=14)
        caps, errors, _ = self.check("default-font-16", [self.screen(cell), wrong], cell)
        self.assertEqual(caps, {})
        self.assertRegex(errors[t3_fonts.CASE], r"^in a 8x16 cell, the letter g matches "
                                                r"Terminus 14 px, not Terminus 16 px alone; ")
        # a cell of another height than the variant's
        caps, errors, _ = self.check("default-font-24", [self.screen(cell), good], cell)
        self.assertEqual((caps, errors), ({}, {t3_fonts.CASE: "RuntimeError: the console's cell "
                                                              "is 8x16, not 24 pixels high"}))
        # cullet-mono, with and without the switch to the reference set
        img, reference = self.cullet_screens(cell, glyphs)
        shots = [self.screen(cell), img, reference]
        caps, errors, typed = self.check("cullet-mono-16", shots, cell,
                                         ["T3-USE-FRIT set\tcullet-alone "])
        self.assertEqual((list(caps), errors), ([t3_fonts.CASE], {}))
        self.assertEqual(typed, ["t3 font-probe\n", "t3 font-probe-alone\n"])
        caps, errors, _ = self.check("cullet-mono-16", shots, cell,
                                     ["T3-USE-FRIT vtium-config: no font set cullet-alone "])
        self.assertEqual(caps, {})
        self.assertEqual(errors, {t3_fonts.CASE: "RuntimeError: vtium-config use-frit "
                                                 "cullet-alone did not switch the console: "
                                                 "T3-USE-FRIT vtium-config: no font set "
                                                 "cullet-alone "})

    def test_the_sources_are_read_from_the_install_tree(self):
        glyph = "0067:" + "80" + "00" * 15 + "\n"
        with tempfile.TemporaryDirectory() as d:
            tree = os.path.join(d, "tree.tar")
            with tarfile.open(tree, "w") as tar:
                def add(name, data):
                    info = tarfile.TarInfo("./" + t3_fonts.FONTS + name)
                    info.size = len(data)
                    tar.addfile(info, io.BytesIO(data))
                for strike in t3_fonts.TERMINUS_STRIKES:
                    add("ter-u%dn.bdf.gz" % strike, gzip.compress(self.BDF.encode()))
                add("unifont_all-17.0.03.hex", glyph.encode())
            got = t3_fonts.source_glyphs(tree, "gh")
            self.assertEqual(sorted(got), [0x67, 0x68])
            self.assertEqual(got[0x67]["Unifont"], (8, 16, {(0, 0)}))
            self.assertEqual(got[0x67]["Terminus 32 px"], (3, 2, {(0, 0), (2, 0), (1, 1)}))
            self.assertEqual(sorted(got[0x68]), ["Terminus %d px" % s
                                                 for s in t3_fonts.TERMINUS_STRIKES])
            os.unlink(tree)
            with tarfile.open(tree, "w"):
                pass
            with self.assertRaisesRegex(RuntimeError, "the install tree holds no "
                                        "usr/share/vtium/fonts/ter-u12n.bdf.gz"):
                t3_fonts.source_glyphs(tree, "g")


if __name__ == "__main__":
    # A skipped test says why, so a skip is never read as a pass.
    result = unittest.main(exit=False).result
    for test, reason in result.skipped:
        print("skipped %s: %s" % (test.id(), reason), file=sys.stderr)
    sys.exit(not result.wasSuccessful())
