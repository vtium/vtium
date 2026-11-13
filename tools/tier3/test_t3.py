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
import hashlib
import io
import json
import multiprocessing
import os
import pickle
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
import t3_bench  # noqa: E402
import t3_devices  # noqa: E402
import t3_evaluate  # noqa: E402
import t3_heads  # noqa: E402
import t3_images  # noqa: E402
import t3_impl  # noqa: E402
import t3_pictures  # noqa: E402
import t3_pointer  # noqa: E402
import t3_power  # noqa: E402
import t3_seat  # noqa: E402
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
        # The operator's budget is the minspec binary's bytes, 4 MiB
        # (ADR-1200 D19): at it the run passes, one byte past it fails. The
        # rest of the tree does not count.
        budget = t3_impl.BINARY_BUDGET
        self.assertEqual(budget, 4194304)
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
        # variant's its frits, which guest/setup builds into the store.
        impl = t3_impl.tree(self.tree, self.tar)
        disc = os.path.join(self.dir.name, "disc")
        for variant, extra in (("2d-potato-min", ["pointer"]), ("2d-potato-min-psf", ["frits"])):
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
                                         "deferred-frits"] + (["helper-held"] if held else [])))
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
        wanted = {t3_evaluate.golden_for(v, c)[0] for v, cases in cases_of.items()
                  for c in cases} - {None}
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
        # The maintainer reviewed every provisional golden on 2026-11-05, the
        # four notice goldens recaptured with the reworded notices on
        # 2026-11-06, and the ten screens of each default 2D variant
        # re-baselined on vtium's captures with the font set tier3-text
        # (vtium/vtium#275) on 2026-11-06, and the three sgr goldens of the
        # minimal variants recaptured with ADR-1302 D4's italic bands of
        # 2026-11-07 (vtium/vtium#273) on 2026-11-08, and card 1b's
        # kernel-message golden on 2026-11-11, so the list in the tree holds
        # none; a golden it names must be one a case draws.
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
        # frit of the test font the drawing variants draw, and the no-udev
        # file only for the variant without udev.
        tree = os.path.join(self.dir.name, "tree")
        write_tree(tree, ImplementationTest.TREE)
        impl = t3_impl.tree(tree, os.path.join(self.dir.name, "out", "tree.tar"))
        fonts = os.path.join(self.dir.name, "fonts")
        write_tree(fonts, {"vtium-test-8x16.psf": "font"})
        impl["fonts"] = fonts
        disc = os.path.join(self.dir.name, "disc")
        for variant, extra in (("sysvinit-640x480", ["fonts", "frits"]),
                               ("sysvinit-no-udev", ["fonts", "frits", "no-udev"]),
                               ("sysvinit-two-vts", ["fonts", "frits", "two-vts"]),
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

    def test_only_the_pointer_variant_has_the_case(self):
        for name in t3_scenario.VARIANTS:
            with self.subTest(name):
                cases = t3_scenario.variant_cases(name)
                if t3_scenario.VARIANTS[name].get("pointer"):
                    self.assertEqual(cases[-1], t3_pointer.CASE)
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
        self.assertIn("install -Dm644 -t /D/etc/vtium/fonts tools/specimens/frits/desktop.vtfrit "
                      "crates/vtium-config/frits/minspec-16.vtfrit "
                      "crates/vtium-config/frits/minspec-8.vtfrit", lines)
        self.assertFalse([l for l in lines if "heads.conf" in l], lines)
        self.assertIn("install -Dm644 crates/vtium/install/vtium.conf.example "
                      "/D/etc/vtium/vtium.conf.example", lines)
        self.assertFalse([l for l in lines if "vtium-gallery" in l], lines)
        self.assertIn("install -Dm755 tools/gallery/gallery /D/usr/bin/vtium-gallery", make_n("FEATURES=images"))

    def test_the_tree_installs_the_license_and_the_notices(self):
        # make install-tree installs LICENSE, COPYING and the two license
        # texts COPYING points to in vtium's license directory, as make
        # install does for a package. COPYING holds the notices for what
        # vtium builds in, such as Unicode's data tables, whose license asks
        # that its notice go with every copy, and Unifont, whose license, the
        # SIL Open Font License, asks that its text go with the font.
        r = subprocess.run(["make", "-n", "--no-print-directory", "install-tree", "DESTDIR=/D"],
                           cwd=t3_common.ROOT, capture_output=True, text=True, check=True)
        lines = [l for l in r.stdout.splitlines() if "/D/usr/share/licenses" in l]
        self.assertEqual(lines, ["install -Dm644 -t /D/usr/share/licenses/vtium LICENSE COPYING "
                                 "crates/vtium-font/data/OFL-1.1.txt "
                                 "crates/vtium-term/gen-color-names/src/rgb-license.txt"])

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
        "heads-closed-unmatched": ["no-display-notice", "no-login", "getty-fallback"],
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

    def test_the_agent_refuses_every_other_request(self):
        # The agent on a pty in place of ttyS2: each request it does not
        # know, spelled even one character differently, is refused and runs
        # nothing.
        import pty
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        # held open here too, so the pty stays up until the agent opens it
        self.addCleanup(os.close, slave)
        agent = subprocess.Popen(["bash", os.path.join(t3_common.GUEST, "t3-agent")],
                                 cwd=d.name, stdin=subprocess.DEVNULL,
                                 env=dict(os.environ, T3_AGENT_PORT=os.ttyname(slave)))
        self.addCleanup(agent.wait)
        self.addCleanup(agent.kill)
        requests = ["kill TERM", "kill KILL; touch pwned", "$(touch %s/pwned-sub)" % d.name,
                    "`touch %s/pwned-tick`" % d.name, "state ", "start now", "Start",
                    "hangup && touch pwned-and", "watch /dev/input/event0", "Watch",
                    "limit 99", "Limit", "reset-failed getty@tty2.service", "reset-failed;id",
                    "prelogin now", "Prelogin", "prelogin;id", "restart now", "Restart",
                    "restart;id", "ptys tty2", "Ptys", "ptys;id", "stop now", "Stop", "stop;id",
                    "vts 7", "Vts", "vts;id", "seat-wait 4", "Seat-wait", "seat-wait;id",
                    "seat-hold 8", "seat-hold;id", "Seat-hold", "seat-release",
                    "chvt", "chvt 3", "chvt 12", "Chvt 1", "chvt 1;id", "chvt 2 now",
                    "chvt 1 && touch pwned-chvt", "chvt $(touch %s/pwned-chvt)" % d.name]
        for n, request in enumerate(requests, 1):
            with self.subTest(request):
                os.write(master, b"%d %s\n" % (n, request.encode()))
                answer, buf = [], b""
                deadline = time.time() + 10
                while ("T3-AGENT-END %d" % n).encode() not in buf:
                    self.assertLess(time.time(), deadline, buf)
                    buf += os.read(master, 4096)
                answer = [l.strip() for l in buf.decode().splitlines()
                          if l.startswith("T3-AGENT %d " % n)]
                self.assertEqual(answer, ["T3-AGENT %d refused %s" % (n, request.strip())])
        self.assertEqual(os.listdir(d.name), [])

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
                         "i=33;ENOENT:image not found")
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
    SCREENS = ["showcase-text", "showcase-emoji", "showcase-scripts", "showcase-color",
               "showcase-boxes", "showcase-pictures", "showcase-shell",
               "showcase-fonts-iosevka", "showcase-fonts-terminus", "showcase-fonts-unifont",
               "showcase-fonts-jetbrains"]

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
        # Every screen is a script in guest/screens, and every image of the
        # page takes its caption and alt text from a script's header, each
        # screen in exactly one image, the hero first.
        for screen in self.SCREENS:
            with self.subTest(screen):
                path = os.path.join(t3_common.GUEST, "screens", screen + ".sh")
                self.assertTrue(os.access(path, os.X_OK), path)
        used = [case for _, _, _, cases in t3_showcase.IMAGES for case in cases]
        self.assertEqual(sorted(used), sorted(self.SCREENS))
        self.assertEqual(t3_showcase.IMAGES[0][1:3], ("hero.png", "showcase-shell.sh"))
        for title, name, script, _ in t3_showcase.IMAGES:
            with self.subTest(name):
                text, alt = t3_showcase.caption(script)
                self.assertTrue(text and alt, script)
                self.assertNotIn("alt:", text)
                self.assertNotIn("shellcheck", text)
        text, _ = t3_showcase.caption("showcase-fonts.inc")
        self.assertIn("(../fonts.md)", text)

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
                  "T3-DONE showcase-fonts-jetbrains"]
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
            self.assertIn('url: "%s"' % url, rs)
            self.assertIn('sha256: "%s"' % sha, rs)
        tier2 = {f.file: (f.url, f.sha256) for f in t3_showcase.FONTS
                 if "tier2" in f.source}
        self.assertEqual(sorted(tier2), ["NotoSansArabic-Regular.ttf",
                                         "NotoSansHebrew-Regular.ttf"])
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
        # copied, and the page with every image, its alt text and caption,
        # the commit and each font.
        run, report = self.run_dir()
        out = os.path.join(self.dir.name, "out")
        wrote = t3_showcase.compose(run, report, out)
        self.assertEqual(len(wrote), len(t3_showcase.IMAGES) + len(t3_showcase.SMALL) + 1)
        hero = t3img.read(os.path.join(out, "hero.png"))
        self.assertEqual(hero.data[:3], bytes([7] * 3))
        fonts = t3img.read(os.path.join(out, "fonts.png"))
        self.assertEqual((fonts.width, fonts.height), (32, 8 + 3 + 6 + 5))
        self.assertEqual([fonts.data[y * 32 * 3] for y in (0, 7, 8, 10, 11, 16, 17, 21)],
                         [8, 8, 9, 9, 10, 10, 11, 11])
        for src, name, _, _ in t3_showcase.SMALL:
            with open(os.path.join(t3_common.ROOT, src), "rb") as a, \
                    open(os.path.join(out, name), "rb") as b:
                self.assertEqual(a.read(), b.read())
        with open(os.path.join(out, "README.md")) as f:
            page = f.read()
        for title, name, script, _ in t3_showcase.IMAGES:
            text, alt = t3_showcase.caption(script)
            self.assertIn("## %s\n\n![%s](%s)\n\n%s\n" % (title, alt, name, text), page)
        self.assertIn("at commit `abc1234`", page)
        self.assertIn("starts in a cell of 16x40 pixels, and the shell scene, the hero, switches "
                      "to its own frit, `showcase-shell`, in a cell of 15x38 pixels", page)
        for font in t3_showcase.FONTS:
            self.assertIn("`%s`" % font.file_sha256, page)
        self.assertIn("| GNU Unifont |", page)
        for photo in t3_showcase_gallery.PHOTOS:
            self.assertIn("| `%s` | %s | %s | %s |" % (photo.file, photo.title, photo.author,
                                                    photo.license), page)
            self.assertIn("SHA-256 `%s`" % photo.sha256, page)
            self.assertIn("(%s)" % photo.url, page)
            self.assertIn("`%s` |" % photo.file_sha256, page)
        # a second run with the same pixels rewrites no image, only the page
        self.assertEqual(t3_showcase.compose(run, report, out),
                         [os.path.join(out, "README.md")])

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
                     t3_seat.SEAT_BUSY, t3_seat.SEAT_SHOWN.replace("%d", "{num}")):
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

if __name__ == "__main__":
    # A skipped test says why, so a skip is never read as a pass.
    result = unittest.main(exit=False).result
    for test, reason in result.skipped:
        print("skipped %s: %s" % (test.id(), reason), file=sys.stderr)
    sys.exit(not result.wasSuccessful())
