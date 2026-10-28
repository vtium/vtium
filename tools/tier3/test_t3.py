#!/usr/bin/env python3
"""Self-test for the parts of tier 3 that run on the host without a guest:
the caption and screen text a case's report carries (t3_evaluate.py), the
colors and cells of the sixel goldens, the tree a run installs and the
guest's account of it (t3_impl.py, t3_scenario.py), the machine each variant
boots, the console lines the driver waits for and the QEMU command line
(t3_qemu.py), the power cases' verdicts (t3_power.py), the pointer case's
reading of the cursor plane (t3_pointer.py), a benchmark screen's
figures (t3_bench.py), and the copies against provenance.tsv, the image
cache and the logger a worker process receives (t3_common.py).

    python3 -I -B tools/tier3/test_t3.py
"""

import argparse
import contextlib
import hashlib
import io
import os
import pickle
import re
import shlex
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3  # noqa: E402
import t3_bench  # noqa: E402
import t3_evaluate  # noqa: E402
import t3_impl  # noqa: E402
import t3_pointer  # noqa: E402
import t3_power  # noqa: E402
import t3_qemu  # noqa: E402
import t3_runner  # noqa: E402
import t3_scenario  # noqa: E402
import t3img  # noqa: E402
import t3_common  # noqa: E402
from t3_common import Logger  # noqa: E402

LOG = Logger()

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
                    "flood", "sync-hold")
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
            self.assertIn(name, t3_evaluate.SCREENS[:-1])

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
    LINE = "[  12.3] TIER3-INSTALL-OK packages: ; tree: %s"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.tree = os.path.join(self.dir.name, "tree")
        write_tree(self.tree, self.TREE)
        self.tar = os.path.join(self.dir.name, "out", "tree.tar")

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
            "binary_bytes": 8, "installed_bytes": 21,
            "budget": {"binary_bytes": 2621440, "met": True}})

    def test_budget(self):
        # The operator's budget is the binary's bytes, 2.50 MiB (#22): at it
        # the run passes, one byte past it fails. The rest of the tree does
        # not count.
        impl = t3_impl.tree(self.tree, self.tar)
        for size, met in ((2621440, True), (2621441, False)):
            impl["binary_bytes"] = size
            self.assertEqual(t3_impl.within_budget(impl), met, size)
            self.assertEqual(t3_impl.describe(impl)["budget"],
                             {"binary_bytes": 2621440, "met": met})

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
        self.assertEqual(t3_scenario.check_install(self.LINE % sha, impl), {"tree": sha})
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
        self.assertIn('"tree: $tree"', text)
        self.assertNotIn("pacman -U", text)

    def test_disc(self):
        # The tree goes on the disc as tree.tar, with the variant's config,
        # and the pointer variant's carries the `pointer` file, on which
        # guest/setup lets the screens read the DRM state.
        impl = t3_impl.tree(self.tree, self.tar)
        disc = os.path.join(self.dir.name, "disc")
        for variant, extra in (("2d-potato-min", ["pointer"]), ("2d-potato-min-psf", [])):
            with self.subTest(variant), mock.patch.object(t3_scenario, "make_iso"):
                t3_scenario.build_disc(variant, self.dir.name, impl)
                self.assertEqual(sorted(os.listdir(disc)),
                                 ["kmscon.conf"] + extra + ["screens", "setup", "t3", "tree.tar"])
                with open(os.path.join(disc, "kmscon.conf")) as f:
                    self.assertIn("render-timer=log", f.read())
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            self.assertIn('if [ -f "$disc"/pointer ]; then', f.read())

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
                with open(os.path.join(disc, "kmscon.conf")) as f:
                    self.assertIn("font-name=/usr/local/share/tier3-fonts/%s\n" % conf,
                                  f.read())
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_disc("2d-potato-min", self.dir.name, impl)
        self.assertNotIn("fonts", os.listdir(disc))
        with open(os.path.join(t3_common.GUEST, "setup")) as f:
            self.assertIn('install -Dm644 -t /usr/local/share/tier3-fonts "$disc"/fonts/*',
                          f.read())

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
                with open(os.path.join(disc, "kmscon.conf")) as f:
                    self.assertIn("grab-reboot=", f.read())
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
        impl = t3_impl.tree(self.tree, self.tar)
        impl["binary_bytes"], impl["installed_bytes"] = 2621441, 2630443
        metas = {"2d-potato-min": {"resident": t3_scenario.resident({"sgr": 10547,
                                                                     "s1-scroll": 16077}),
                                   "console": {"name": "vtium", "revision": "0.1"}},
                 "2d-potato-min-psf": {"resident": t3_scenario.resident({}), "console": None}}
        self.assertEqual(t3.summary(impl, metas).splitlines(), [
            "tree %s: binary 2621441 B, over the budget of 2621440 B by 1 B; 2.51 MiB "
            "installed; the console logged vtium 0.1" % t3_common.rel(self.tree),
            "2d-potato-min: resident 10.30 MiB after sgr, peak 15.70 MiB after s1-scroll",
            "2d-potato-min-psf: resident none after sgr, peak none after none"])
        impl["binary_bytes"] = 2368120
        self.assertTrue(t3.summary(impl, {}).startswith(
            "tree %s: binary 2368120 B, within the budget of 2621440 B; "
            % t3_common.rel(self.tree)))

    def test_vtium_process(self):
        # vtium's process logs as vtium[pid]; its render-timer and startup
        # lines are read as the C reference's were.
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
        # vtium logs its Display line without the C reference's "terminal:"
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
        # The report names vtium's commit, the guest's pins and the kmscon
        # commit the C reference's goldens were copied from.
        env = {"T3_VTIUM_REV": "84eec92+dirty", "T3_ARCHIVE": "2026/10/20"}
        with mock.patch.dict(os.environ, env, clear=True):
            tools = t3_runner.run_tools()
        self.assertEqual(tools, {"vtium": "84eec92+dirty", "guest_image": None,
                                 "guest_image_sha256": None, "guest_archive": "2026/10/20",
                                 "goldens": {"kmscon": "b1f7a7c5df563f3378584db7298753848a7a9a13",
                                             "provenance": "tools/tier3/provenance.tsv"}})

    def test_option(self):
        # --tree names a tree built already; --tree-target builds the default
        # one, so the two are refused together, before run is called.
        with mock.patch.object(t3, "run", side_effect=lambda args, log: args) as run:
            args = t3.main(["run", "--tree", "a", "2d-potato-min"])
        self.assertEqual((args.tree, args.tree_target, args.variants),
                         ("a", None, ["2d-potato-min"]))
        self.assertTrue(run.called)
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

        with mock.patch.object(t3.subprocess, "run", side_effect=make):
            t3.build_tree("install-tree", tree, lambda msg: None)
        self.assertEqual(seen, [(["make", "--no-print-directory", "install-tree",
                                  "DESTDIR=" + tree], t3_common.ROOT, False)])
        with mock.patch.object(t3.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 2)):
            with self.assertRaisesRegex(SystemExit, "make install-tree failed with exit code 2"):
                t3.build_tree("install-tree", tree, lambda msg: None)


class ProvenanceTest(unittest.TestCase):
    """The files tier 3 copied from the C reference (provenance.tsv): the
    goldens under c-goldens and the harness files they judge the output
    of, each the copy its row names."""

    COMMIT = "b1f7a7c5df563f3378584db7298753848a7a9a13"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.root = self.dir.name
        self.copied = os.path.join(self.root, "c")

    def table(self, files, extra=""):
        """A provenance.tsv for {path under root: text}, written beside them."""
        write_tree(self.root, files)
        path = os.path.join(self.root, "provenance.tsv")
        with open(path, "w") as f:
            f.write("# a comment\n")
            for name, text in files.items():
                f.write("\t".join((name, "kmscon", self.COMMIT, "fork/" + name,
                                   hashlib.sha256(text.encode()).hexdigest())) + "\n")
            f.write(extra)
        return path

    def test_read(self):
        rows = t3_common.read_provenance(self.table({"c/v/sgr.png": "golden"}))
        self.assertEqual(rows, [{"path": "c/v/sgr.png", "repo": "kmscon", "commit": self.COMMIT,
                                 "source": "fork/c/v/sgr.png",
                                 "sha256": hashlib.sha256(b"golden").hexdigest(),
                                 "line": 2}])
        self.assertEqual(t3_common.golden_commit(rows, "c"), self.COMMIT)

    def test_refused(self):
        sha = "0" * 64
        for line in ("a\tkmscon\t%s\tsrc\n" % self.COMMIT,
                     "a\tkmscon\tabc\tsrc\t%s\n" % sha,
                     "a\tkmscon\t%s\tsrc\t%s\n" % (self.COMMIT, "A" * 64),
                     "a\tlibtsm\t%s\tsrc\t%s\n" % (self.COMMIT, sha)):
            with self.subTest(line):
                with self.assertRaisesRegex(SystemExit, "want PATH kmscon COMMIT SOURCE SHA256"):
                    t3_common.read_provenance(self.table({}, line))
        twice = "a\tkmscon\t%s\tsrc\t%s\n" % (self.COMMIT, sha)
        with self.assertRaisesRegex(SystemExit, "a is already on line 2"):
            t3_common.read_provenance(self.table({}, twice * 2))
        other = "c/v/b.png\tkmscon\t%s\tsrc\t%s\n" % ("1" * 40, sha)
        rows = t3_common.read_provenance(self.table({"c/v/a.png": "x"}, other))
        with self.assertRaisesRegex(SystemExit, "goldens are copied from two commits"):
            t3_common.golden_commit(rows, "c")

    def test_copies_checked(self):
        # Each copy against its row, and each file under the goldens'
        # directory for a row.
        files = {"c/v/sgr.png": "golden", "c/v/cjk.png": "golden too", "guest/sgr.sh": "script"}
        rows = t3_common.read_provenance(self.table(files))
        self.assertEqual(t3_common.check_copies(rows, self.root, self.copied), [])
        write_tree(self.root, {"c/v/sgr.png": "moved", "c/v/stray.png": "no row",
                               "guest/own.sh": "vtium's own"})
        os.unlink(os.path.join(self.root, "guest/sgr.sh"))
        self.assertEqual(t3_common.check_copies(rows, self.root, self.copied),
                         ["c/v/sgr.png differs from kmscon %s:fork/c/v/sgr.png (provenance.tsv:2)"
                          % self.COMMIT[:12],
                          "c/v/stray.png has no row in provenance.tsv",
                          "guest/sgr.sh is missing (provenance.tsv:4)"])

    def test_the_tree(self):
        # The copies in the tree match their rows, every golden a variant
        # judged by the C reference's goldens compares with is copied, and
        # every copy is one a case compares with.
        rows = t3_common.read_provenance()
        self.assertEqual(t3_common.check_copies(rows), [])
        self.assertEqual(t3_common.golden_commit(rows), self.COMMIT)
        cases_of = t3_scenario.c_golden_cases()
        self.assertEqual(t3_evaluate.check_goldens(cases_of, t3_common.GOLDEN), [])
        wanted = {t3_evaluate.golden_for(v, c)[0] for v, cases in cases_of.items()
                  for c in cases if t3_evaluate.golden_for(v, c)[0]}
        self.assertEqual(sorted(p for p in wanted if not os.path.exists(p)), [])
        # the harness files the goldens judge the output of stay copies too
        copied = {r["path"] for r in rows}
        for name in ("tools/tier3/guest/screens/sgr.sh", "tools/tier3/guest/user-data.in",
                     "tools/tier3/t3img.py"):
            self.assertIn(name, copied)

    def planted(self):
        """Rows for a planted root whose goldens directory c holds two
        goldens 2d-potato-min's cases compare with."""
        return t3_common.read_provenance(self.table({"c/2d-potato-min/sgr.png": "sgr",
                                                     "c/2d-potato-min/cjk.png": "cjk"}))

    def test_preflight(self):
        # The checks a run makes before it builds or boots anything: clean
        # on the planted root, then a changed golden and a stray file.
        rows = self.planted()
        self.assertEqual(t3.preflight(LOG, rows, self.root, self.copied), [])
        write_tree(self.root, {"c/2d-potato-min/sgr.png": "moved",
                               "c/2d-potato-min/stray.png": "no row"})
        stray = t3_common.rel(os.path.join(self.copied, "2d-potato-min", "stray.png"))
        self.assertEqual(sorted(t3.preflight(LOG, rows, self.root, self.copied)),
                         sorted(["c/2d-potato-min/sgr.png differs from kmscon %s:"
                                 "fork/c/2d-potato-min/sgr.png (provenance.tsv:2)"
                                 % self.COMMIT[:12],
                                 "c/2d-potato-min/stray.png has no row in provenance.tsv",
                                 "%s is a golden no case draws" % stray]))

    def test_run_refuses_a_bad_tree(self):
        # run() makes those checks under the run lock and refuses before the
        # tree is built.
        rows = self.planted()
        write_tree(self.root, {"c/2d-potato-min/sgr.png": "moved"})
        args = argparse.Namespace(variants=["2d-potato-min"], tree=None,
                                  tree_target="install-tree", jobs=1, deadline=1)
        base = os.path.join(self.root, "base.qcow2")
        write_tree(self.root, {"base.qcow2": ""})
        with mock.patch.object(t3, "BASE", base), \
                mock.patch.object(t3_runner, "raise_on_sigterm", lambda log: None), \
                mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                mock.patch.object(t3_common, "read_provenance", lambda: rows), \
                mock.patch.object(t3_common, "ROOT", self.root), \
                mock.patch.object(t3_common, "GOLDEN", self.copied), \
                mock.patch.object(t3, "build_tree", side_effect=AssertionError("built")):
            with self.assertRaisesRegex(SystemExit, "c/2d-potato-min/sgr.png differs"):
                t3.run(args, LOG)

    def git(self, repo, *args):
        return subprocess.run(["git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@t",
                               "-c", "commit.gpgsign=false"] + list(args),
                              check=True, capture_output=True, text=True).stdout.strip()

    def test_verify_against_sources(self):
        # make tier3-provenance: each row's source read from a kmscon
        # checkout with git, at the row's commit, whatever its working tree
        # holds; a source that differs or is missing is reported, and the
        # exit status is 1.
        repo = os.path.join(self.root, "kmscon")
        os.makedirs(repo)
        self.git(repo, "init", "-q")
        write_tree(repo, {"fork/a.png": "a", "fork/b.png": "b"})
        self.git(repo, "add", ".")
        self.git(repo, "commit", "-q", "-m", "copies")
        commit = self.git(repo, "rev-parse", "HEAD")
        write_tree(repo, {"fork/a.png": "working tree"})
        sha = lambda text: hashlib.sha256(text.encode()).hexdigest()
        path = os.path.join(self.root, "provenance.tsv")
        with open(path, "w") as f:
            for name, digest in (("a.png", sha("a")), ("b.png", sha("not b")),
                                 ("gone.png", sha("x"))):
                f.write("\t".join(("c/" + name, "kmscon", commit, "fork/" + name, digest))
                        + "\n")
        args = argparse.Namespace(kmscon=repo)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(t3.verify_provenance(args, LOG, t3_common.read_provenance(path)), 1)
        lines = out.getvalue().splitlines()
        self.assertEqual(len(lines), 3)
        self.assertIn("provenance.tsv:2: %s %s:fork/b.png has another SHA-256" % (repo, commit[:12]),
                      lines[0])
        self.assertIn("provenance.tsv:3: %s %s:fork/gone.png: " % (repo, commit[:12]), lines[1])
        self.assertEqual(lines[2], "provenance: 3 rows, 1 match their sources")


class DeparturesTest(unittest.TestCase):
    """departures.tsv: a listed difference inside its departure's rectangles
    is a "departure", which passes; anything else about a listed case fails,
    and a list that names an unknown case or departure refuses the run."""

    TITLE = "DA1 reports a VT220 with color, `CSI ? 62 ; 22 c`."

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def tsv(self, text):
        path = os.path.join(self.dir.name, "departures.tsv")
        with open(path, "w") as f:
            f.write("# a comment\n" + text)
        return path

    def test_shipped(self):
        # The list in the tree reads, and every case and title in it is known.
        deps = t3_evaluate.read_departures()
        self.assertEqual(t3_evaluate.check_departures(deps, t3_scenario.variant_cases), [])
        self.assertEqual(deps["2d-potato-min-psf/flood"],
                         {"readme": "crates/vtium-term/README.md", "title": self.TITLE,
                          "rects": [[203, 16, 491, 32]]})
        self.assertEqual(deps["2d-potato-min/underline-styles"]["rects"],
                         [[123, 80, 251, 96], [123, 224, 339, 240]])
        self.assertEqual(len(deps), 22)

    def test_read(self):
        path = self.tsv("a,b\tx,y\tR.md\tT\t0,0,8,16;8,0,16,16\n")
        deps = t3_evaluate.read_departures(path)
        self.assertEqual(sorted(deps), ["a/x", "a/y", "b/x", "b/y"])
        self.assertEqual(deps["b/y"], {"readme": "R.md", "title": "T",
                                       "rects": [[0, 0, 8, 16], [8, 0, 16, 16]]})

    def test_refused(self):
        for text, why in (("a\tx\tR.md\tT\n", "want VARIANTS, SCREENS"),
                          ("a\tx\tR.md\t\t0,0,1,1\n", "want VARIANTS, SCREENS"),
                          ("a\tx\tR.md\tT\t0,0,1\n", "a rectangle is"),
                          ("a\tx\tR.md\tT\t1,0,1,1\n", "a rectangle is"),
                          ("a\tx\tR.md\tT\tz,0,1,1\n", "a rectangle is"),
                          ("a\tx,x\tR.md\tT\t0,0,1,1\n", "a/x is listed twice")):
            with self.subTest(text):
                with self.assertRaisesRegex(SystemExit, re.escape(why)):
                    t3_evaluate.read_departures(self.tsv(text))

    def test_checked(self):
        readme = os.path.join(self.dir.name, "R.md")
        with open(readme, "w") as f:
            f.write("## Departures\n\n- **Listed.** Why.\n\n## Other\n\n- **Elsewhere.** No.\n")
        dep = {"readme": readme, "title": "Listed.", "rects": [[0, 0, 1, 1]]}
        deps = {"2d-potato-min/sixel": dep,
                "2d-potato-min/photo-scrollback": dep,
                "no-such/sgr": dep,
                "2d-potato-min/vt-switch": dep,
                "2d-potato-min/cjk": dict(dep, title="Elsewhere."),
                "2d-potato-min/sgr": dict(dep, readme=readme + ".missing")}
        errors = t3_evaluate.check_departures(deps, lambda v: t3_scenario.variant_cases(v)
                                              if v in t3_scenario.VARIANTS else [])
        self.assertEqual(len(errors), 5, errors)
        for why in ("lists 2d-potato-min/photo-scrollback, which no variant draws",
                    "lists no-such/sgr, which no variant draws",
                    "lists 2d-potato-min/vt-switch, a check without a golden",
                    "2d-potato-min/cjk: %s lists no departure 'Elsewhere.'" % readme,
                    "2d-potato-min/sgr: [Errno 2]"):
            self.assertTrue(any(why in e for e in errors), (why, errors))

    def judged(self, actual_pixels, listed=True, golden_pixels=()):
        """The entry for 2d-potato-min/sixel with a black 32x32 golden and a capture
        differing at `actual_pixels`, listed under a departure whose rectangle
        is 8,0,24,16."""
        golden = os.path.join(self.dir.name, "golden")
        img = t3img.Image(32, 32, bytes(32 * 32 * 3))
        os.makedirs(os.path.join(golden, "2d-potato-min"), exist_ok=True)
        t3img.write_png(os.path.join(golden, "2d-potato-min", "sixel.png"), img)
        data = bytearray(img.data)
        for x, y in actual_pixels:
            data[(y * 32 + x) * 3] = 200
        cap = os.path.join(self.dir.name, "cap.png")
        t3img.write_png(cap, t3img.Image(32, 32, bytes(data)))
        dep = {"readme": "R.md", "title": "T", "rects": [[8, 0, 24, 16]]}
        deps = {"2d-potato-min/sixel": dep} if listed else {}
        with mock.patch.object(t3_evaluate, "GOLDEN", golden), \
                mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")), \
                mock.patch.object(t3_evaluate, "DEPARTURES", deps):
            return t3_evaluate.evaluate("2d-potato-min", "sixel", cap, {}, LOG)

    def test_judged(self):
        inside = [(8, 0), (23, 15), (12, 7)]
        e = self.judged(inside)
        self.assertEqual((e["status"], e["departure"], e["stats"]["pixels"]),
                         ("departure", "T", 3))
        e = self.judged(inside + [(24, 0), (8, 16)])
        self.assertEqual((e["status"], e["error"]),
                         ("fail", "2 differing pixel(s) outside the rectangles of the "
                                  "departure 'T'"))
        e = self.judged([])
        self.assertEqual((e["status"], e["error"]),
                         ("fail", "listed under the departure 'T', which it no longer shows"))
        e = self.judged(inside, listed=False)
        self.assertEqual((e["status"], "departure" in e), ("fail", False))

    def test_departure_passes_the_run(self):
        # The run's verdict counts a departure and a provisional case as a pass.
        with open(os.path.join(t3_common.HERE, "t3.py")) as f:
            self.assertIn('c["status"] in ("pass", "near", "departure", "provisional")',
                          f.read())


class VtiumGoldenTest(unittest.TestCase):
    """vtium's own goldens: the provisional list, and a golden no case draws."""

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
        # The operator accepted vtium's sixteen small-display goldens on
        # 2026-10-28, the four vtium/vtium#119 adds in advance, so the list
        # in the tree names none; every golden it would name must be one a
        # case draws.
        prov = t3_evaluate.read_provisional()
        self.assertEqual(sorted(prov), [])
        self.assertEqual(t3_evaluate.check_goldens(t3_scenario.vtium_golden_cases(),
                                                   t3_evaluate.VTIUM_GOLDEN, prov), [])
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
        # The C reference's goldens are never provisional.
        with mock.patch.object(t3_evaluate, "GOLDEN", self.root):
            self.golden("cjk")
            cap = os.path.join(self.dir.name, "cap.png")
            t3img.write_png(cap, t3img.Image(8, 8, bytes([200]) + bytes(8 * 8 * 3 - 1)))
            with mock.patch.object(t3_evaluate, "RUNDIR", os.path.join(self.dir.name, "run")), \
                    mock.patch.object(t3_evaluate, "PROVISIONAL", listed):
                e = t3_evaluate.evaluate(self.VARIANT, "cjk", cap, {}, LOG)
        self.assertEqual(e["status"], "fail")


class ProvisionTest(unittest.TestCase):
    """Where provisioning keeps its images: a vtium cache that
    target/tier3/vm/images links to, the pristine image fetched into its
    pristine/ and the base image built into its base/, each only when the
    cache lacks it."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.vm = os.path.join(self.dir.name, "vm")
        for name, value in (("VM", self.vm), ("BASE", os.path.join(self.vm, "base.qcow2"))):
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

    def args(self, cache):
        return argparse.Namespace(image="img.qcow2", archive="2026/10/20", url="u", key="k",
                                  sha256="0" * 64, images=cache)

    def provision(self, cache):
        """Provision into `cache`: [(what, detail)] for each fetch and build,
        which write the files they would."""
        done = []
        base = os.path.join(self.vm, "images", "base", "x.qcow2")

        def fetch(argv, check):
            done.append(("fetch", argv[2:]))
            write_tree(argv[2], {argv[4]: "pristine"})

        def build(args, log, path):
            done.append(("build", path))
            write_tree(os.path.dirname(path), {os.path.basename(path): "base"})

        with mock.patch.object(t3, "base_image", lambda i, a: base), \
                mock.patch.object(t3, "_provision", side_effect=build), \
                mock.patch.object(t3_runner, "run_lock", contextlib.nullcontext), \
                mock.patch.object(t3.subprocess, "run", side_effect=fetch):
            self.assertEqual(t3.provision(self.args(cache), LOG), 0)
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
                         "a801c98304d282ae.qcow2")


class VariantTest(unittest.TestCase):
    def test_every_variant_sizes_its_machine(self):
        for name, v in t3_scenario.VARIANTS.items():
            with self.subTest(name):
                self.assertIsInstance(v["cpus"], int)
                self.assertIsInstance(v["memory_mb"], int)

    def test_potato(self):
        # The minspec machine, in the default run.
        v = t3_scenario.VARIANTS["2d-1366x768-potato"]
        self.assertEqual((v["cpus"], v["memory_mb"], v["renderer"]), (1, 512, "bbulk"))
        self.assertIn("xres=1366,yres=768", v["device"])
        self.assertFalse(t3_scenario.uses_virgl("2d-1366x768-potato"))
        self.assertIn("2d-1366x768-potato", t3_scenario.DEFAULT_VARIANTS)

    def test_potato_minimal(self):
        # The potato machine with the minimal configuration, unifont and a
        # PSF font, which make tier3-minimal runs, out of the default run.
        for name, conf in (("2d-potato-min", "kmscon-minimal.conf"),
                           ("2d-potato-min-psf", "kmscon-minimal-psf.conf")):
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
        with open(os.path.join(t3_common.GUEST, "kmscon-minimal-psf.conf")) as f:
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
                self.assertEqual(v["goldens"], t3_evaluate.VTIUM_GOLDEN)
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
        # the 8x8 font is drawn at its own size, not the 16 pixels the
        # default size picks at 320x240
        with open(os.path.join(t3_common.GUEST, "kmscon-small-8x8.conf")) as f:
            self.assertIn("font-size=8\n", f.read())
        # vt-restore compares with the last screen, cjk, as in SCREENS
        self.assertEqual(t3_evaluate.SMALL_SCREENS[-1], t3_evaluate.SCREENS[-1])
        # every other variant is judged by the C reference's goldens
        entries = t3_scenario.evaluate_variant("2d-potato-min", {}, t3_scenario.failure_meta("x"),
                                               LOG)
        self.assertEqual(entries[0]["golden"], "tools/tier3/c-goldens/2d-potato-min/sgr.png")

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
        # gltex draws image placements, so the GL
        # variant runs every case, the image screens and their keyed cases
        # included, as the 2D variants do; the no-images check it ran while
        # gltex drew none is gone with its flag. Its goldens are blessed, so
        # it is in the default run.
        self.assertIn("gl-1280x800", t3_scenario.DEFAULT_VARIANTS)
        self.assertNotIn("draws_images", t3_scenario.VARIANTS["gl-1280x800"])
        self.assertNotIn("no-images", t3_evaluate.CASES)
        for name in t3_evaluate.IMAGE_SCREENS:
            self.assertIn(name, t3_evaluate.CASES)
        for screen, case, _ in t3_evaluate.KEYED:
            self.assertIn(case, t3_evaluate.CASES)

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


class PowerTest(unittest.TestCase):
    """The power variants (t3_power.py): their machine, their cases and the
    verdict each case takes from the console log and QEMU's SHUTDOWN
    event."""

    # A console log as each action leaves it, systemd's status lines
    # colored as on the guest's serial console.
    OK = "[\x1b[0;32m  OK  \x1b[0m] "
    POWER_OFF = ["[ 9.1] vtium[404]: grab-power-off: asking init to power off",
                 OK + "Reached target Unmount All Filesystems.",
                 OK + "Reached target System Power Off.",
                 "[ 12.0] reboot: Power down"]
    LOGS = {
        "reboot": ["[ 9.1] vtium[404]: grab-reboot: asking init to reboot",
                   OK + "Stopped target Graphical Interface.",
                   OK + "Reached target Unmount All Filesystems.",
                   OK + "Reached target System Reboot.",
                   "[ 12.0] reboot: Restarting system"],
        "reboot-hard": ["[ 9.1] vtium[404]: grab-reboot: restarting the kernel at once",
                        "[ 9.2] reboot: Restarting system"],
        "power-off": POWER_OFF,
        "power-off-after-s3": POWER_OFF,
        "suspend": ["[ 9.1] vtium[404]: grab-suspend: suspending",
                    OK + "Reached target Sleep.",
                    "[ 9.1] vtium[404]: grab-suspend: logind took the request",
                    "         Starting System Suspend...",
                    OK + "Finished System Suspend."],
        "suspend-sysfs": ["[ 9.1] vtium[404]: grab-suspend: suspending",
                          "[ 9.1] vtium[404]: grab-suspend: writing mem to /sys/power/state"],
    }
    # What a power-off after S3 ends with in QEMU: systemd-shutdown exits
    # when reboot(2) returns, ten seconds after the kernel's last line.
    PANIC = ["[ 22.0] Kernel panic - not syncing: Attempted to kill init! exitcode=0x00000000"]
    SHUTDOWN = {"reboot": "guest-reset", "reboot-hard": "guest-reset",
                "power-off": "guest-shutdown", "power-off-after-s3": "guest-shutdown",
                "suspend": None, "suspend-sysfs": None}

    def log(self, case, before=(), drop=None, extra=()):
        lines = list(before) + [l for l in self.LOGS[case] if drop is None or drop not in l]
        return ("\r\n".join(lines + list(extra)) + "\r\n").encode()

    def judge(self, case, serial, since=0):
        return t3_power.judge(case, serial, since, self.SHUTDOWN[case])

    def test_variants(self):
        # The minspec machine, out of the default run, each with its own
        # cases, all checks without a golden.
        potato = t3_scenario.VARIANTS["2d-potato-min"]
        seen = []
        for name in t3_scenario.POWER_VARIANTS:
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                for key in ("device", "cpus", "memory_mb", "renderer"):
                    self.assertEqual(v[key], potato[key])
                self.assertEqual(v["conf"], "kmscon-power.conf")
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
        # power-off runs on a guest that has not slept.
        self.assertEqual(t3_scenario.VARIANTS["power-off"]["power"], ["power-off"])
        # Suspend through logind runs vtium built with it; the minspec
        # build suspends through sysfs. Each is followed by a power-off.
        self.assertEqual(t3_scenario.VARIANTS["power-suspend-logind"]["features"], "logind")
        self.assertNotIn("features", t3_scenario.VARIANTS["power-suspend-off"])
        for name in ("power-suspend-off", "power-suspend-logind"):
            self.assertEqual(t3_scenario.VARIANTS[name]["power"][-1], "power-off-after-s3")

    def test_chords_match_the_configuration(self):
        # Each qcode chord the driver presses is the chord kmscon-power.conf
        # binds for its key.
        keys = {"reboot": "grab-reboot", "reboot-hard": "grab-reboot",
                "power-off": "grab-power-off", "power-off-after-s3": "grab-power-off",
                "suspend": "grab-suspend", "suspend-sysfs": "grab-suspend"}
        with open(os.path.join(t3_common.GUEST, "kmscon-power.conf")) as f:
            conf = dict(line.strip().split("=", 1) for line in f if line.startswith("grab-"))
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

    def test_forbidden_lines(self):
        for case, extra in (("reboot-hard", OK_TARGET_REBOOT),
                            ("reboot", "[ 12.1] reboot: Power down"),
                            ("power-off", "[ 12.1] reboot: Restarting system"),
                            ("suspend", "[ 12.1] reboot: Restarting system"),
                            ("suspend", "[ 9.1] grab-suspend: writing mem to /sys/power/state"),
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
        serial = self.log(case, drop="grab-power-off", extra=self.PANIC)
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
    MPV = "T3-BENCH mpv-sixel start=100.00 end=102.00 mpv_frames=60 mpv_dropped=2 kmscon_cpu_ms=500"
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
                          {"mpv_frames": 60, "mpv_dropped": 2, "kmscon_cpu_ms": 500}))
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
                "kmscon_cpu_ms=700 panes=4 tmux_sixels=%d")
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
                "kmscon_cpu_ms=%d panes=9 tmux_sixels=100")
        got = t3_bench.measure(serial, line % 1400, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["kmscon_cpu_ms_per_tmux_sixel"], 14.0)
        got = t3_bench.measure(serial, line % 3400, self.CONF)
        self.assertNotIn("error", got)
        self.assertEqual(got["kmscon_cpu_ms_per_tmux_sixel"], 34.0)

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
        self.assertEqual(got["kmscon_cpu_ms"], 500)
        self.assertEqual(got["kmscon_cpu_ms_per_mpv_frame"], 8.333)
        self.assertEqual(got["kmscon_cpu_pct"], 25.0)

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
            "kmscon_cpu_ms_per_mpv_frame": 6.733}
    SLOW = {"mpv_frames": 69, "mpv_per_s": 2.1, "presented_per_s": 4.2,
            "kmscon_cpu_ms_per_mpv_frame": 375.797}

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
        variant, issue = "v-known", "aaronsb/kmscon#0"
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

    def wait(self, chunks, marker="TIER3-INSTALL-OK", timeout=180):
        with tempfile.TemporaryDirectory() as d:
            vm = t3_qemu.Qemu.__new__(t3_qemu.Qemu)
            vm.serial, vm.proc = os.path.join(d, "serial.log"), self.Proc()
            with open(vm.serial, "wb") as f:
                f.write(b"[  OK  ] Started \x1b[0;1;39mUser Login Management\x1b[0m.\r\r\n")
            saved = t3_qemu.time
            t3_qemu.time = self.Clock(vm.serial, chunks)
            try:
                return vm.wait_serial(marker, timeout)
            finally:
                t3_qemu.time = saved

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


if __name__ == "__main__":
    # A skipped test says why, so a skip is never read as a pass.
    result = unittest.main(exit=False).result
    for test, reason in result.skipped:
        print("skipped %s: %s" % (test.id(), reason), file=sys.stderr)
    sys.exit(not result.wasSuccessful())
