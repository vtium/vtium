#!/usr/bin/env python3
"""Self-test for the parts of tier 3 that run on the host without a guest:
the caption and screen text a case's report carries (t3_evaluate.py), the
colours and cells of the sixel goldens, the packages a run installs and the
guest's account of them, and the machine each variant boots
(t3_scenario.py), the console lines the driver waits for and the QEMU
command line (t3_qemu.py), a benchmark screen's figures (t3_bench.py), and
the logger a worker process receives (t3_common.py).

    python3 -I fork/tier3/test_t3.py
"""

import contextlib
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
import t3_qemu  # noqa: E402
import t3_runner  # noqa: E402
import t3_scenario  # noqa: E402
import t3img  # noqa: E402
import t3_common  # noqa: E402
from t3_common import Logger  # noqa: E402

LOG = Logger()

SCRIPTS = {
    "colours": r"printf '\033[1;31mred\033[0m plain\nnext\n'",
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

    def test_colours_removed(self):
        self.assertEqual(self.text("colours"), ["red plain", "next"])

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
        self.assertTrue(caption.startswith("S1 (ADR-103): text scrolling"))
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
    """ADR-103's sixel acceptance scenarios as tier 3 cases: A1 (photos,
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


LIBTSM = "ef2365d0f9f1a370d721a82470d69c3646fe97f0"


class SixelGoldenTest(unittest.TestCase):
    # The sixel page's colours (sixel.sh), as kmscon draws them: the bars,
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

    def test_colours_in_cells(self):
        # Every default variant's golden holds each picture in the cells the
        # page puts it in, in its colours.
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


class PackagesTest(unittest.TestCase):
    UP = ["kmscon-upstream-10.0.4.r68.g7d33779-1-x86_64.pkg.tar.zst",
          "kmscon-upstream-terminfo-10.0.4.r68.g7d33779-1-x86_64.pkg.tar.zst"]
    LINE = ("[  12.3] TIER3-INSTALL-OK packages: kmscon-upstream 10.0.4.r68.g7d33779-1 "
            "kmscon-upstream-terminfo 10.0.4.r68.g7d33779-1 ; libtsm: " + LIBTSM)

    def setUp(self):
        self.env = os.environ.pop("T3_LIBTSM_REV", None)

    def tearDown(self):
        os.environ.pop("T3_LIBTSM_REV", None)
        if self.env is not None:
            os.environ["T3_LIBTSM_REV"] = self.env

    def impl(self, name=t3_impl.C, kind="packages", paths=None, **more):
        """An implementation as t3_impl makes one, without reading files."""
        return dict({"name": name, "kind": kind, "paths": self.UP if paths is None else paths,
                     "bytes": [], "installed_bytes": 0}, **more)

    def test_newest_of_one_name(self):
        # Each build's packages are told apart by name, so the fork's never
        # stand in for upstream's; the newest build of each is taken.
        with tempfile.TemporaryDirectory() as d:
            names = ["kmscon-aaronsb-10.0.4.r1.gabc-1-x86_64.pkg.tar.zst",
                     "kmscon-aaronsb-terminfo-10.0.4.r1.gabc-1-x86_64.pkg.tar.zst",
                     "kmscon-upstream-terminfo-9-1-x86_64.pkg.tar.zst",
                     "kmscon-upstream-9-1-x86_64.pkg.tar.zst"] + self.UP
            for i, name in enumerate(names):
                open(os.path.join(d, name), "w").close()
                os.utime(os.path.join(d, name), (i, i))
            got = t3_scenario.newest_packages(d, "kmscon-upstream")
            self.assertEqual([os.path.basename(p) for p in got], self.UP)
            self.assertEqual([t3_scenario.package_name(p) for p in got],
                             ["kmscon-upstream", "kmscon-upstream-terminfo"])
            with self.assertRaises(SystemExit):
                t3_scenario.newest_packages(d, "kmscon-other")

    def test_install_checked(self):
        os.environ["T3_LIBTSM_REV"] = "ef2365d"
        got = t3_scenario.check_install(self.LINE, self.impl())
        self.assertEqual(got, {"packages": ["kmscon-upstream 10.0.4.r68.g7d33779-1",
                                            "kmscon-upstream-terminfo 10.0.4.r68.g7d33779-1"],
                               "libtsm": LIBTSM})
        # A dirty libtsm passes only when the run declares it.
        os.environ["T3_LIBTSM_REV"] = "ef2365d+dirty"
        t3_scenario.check_install(self.LINE.replace(LIBTSM, LIBTSM + "+dirty"), self.impl())

    def test_install_refused(self):
        # Another build's package in the guest, a package built from another
        # libtsm than the run names, from an unknown or undeclared dirty one,
        # a run naming no libtsm, or no account at all fails the variant.
        os.environ["T3_LIBTSM_REV"] = "ef2365d"
        line = self.LINE.replace("; libtsm", "kmscon-aaronsb 10.0.4-1 ; libtsm")
        with self.assertRaisesRegex(RuntimeError, "kmscon-aaronsb installed"):
            t3_scenario.check_install(line, self.impl())
        for libtsm in (LIBTSM + "+dirty", "unknown", "none"):
            with self.assertRaisesRegex(RuntimeError, "built with libtsm %s," % re.escape(libtsm)):
                t3_scenario.check_install(self.LINE.replace(LIBTSM, libtsm), self.impl())
        with self.assertRaisesRegex(RuntimeError, "cannot read"):
            t3_scenario.check_install("TIER3-INSTALL-OK", self.impl())
        for want in ("06331b1", "ef2365d+dirty"):
            os.environ["T3_LIBTSM_REV"] = want
            with self.assertRaisesRegex(RuntimeError, "built with libtsm ef2365d0"):
                t3_scenario.check_install(self.LINE, self.impl())
        os.environ.pop("T3_LIBTSM_REV")
        with self.assertRaisesRegex(RuntimeError, "names no libtsm commit"):
            t3_scenario.check_install(self.LINE, self.impl())


def write_tree(root, files):
    """Write {relative path: text} under `root`."""
    for name, text in files.items():
        os.makedirs(os.path.join(root, os.path.dirname(name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as f:
            f.write(text)


def write_package(path, files):
    """A package archive holding {relative path: text} and pacman's
    metadata files, as makepkg writes one."""
    with tarfile.open(path, "w:zst") as t:
        for name, text in dict({".PKGINFO": "size = 7\n", ".MTREE": "x"}, **files).items():
            data = text.encode()
            info = tarfile.TarInfo(name)
            info.size = len(data)
            t.addfile(info, io.BytesIO(data))


class ImplementationTest(unittest.TestCase):
    """The implementation under test (t3_impl.py, `t3.py run --vtium`): the C
    reference's packages, or vtium as an install tree or package files, with
    its size on disk measured the same way for each."""

    UNIT = t3_impl.VT_UNIT
    TREE = {"usr/bin/vtium": "12345678", UNIT: "[Service]\n", "usr/share/doc/x": "abc"}
    LINE = "[  12.3] TIER3-INSTALL-OK packages: ; libtsm: none; tree: %s"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.tree = os.path.join(self.dir.name, "tree")
        write_tree(self.tree, self.TREE)
        self.tar = os.path.join(self.dir.name, "out", "tree.tar")

    def test_tree(self):
        # The bytes of the regular files a tree installs; a link counts nothing.
        os.symlink("vtium", os.path.join(self.tree, "usr/bin/kmscon"))
        impl = t3_impl.vtium([self.tree], self.tar)
        self.assertEqual((impl["name"], impl["kind"]), ("vtium", "tree"))
        self.assertEqual(impl["installed_bytes"], 8 + 10 + 3)
        self.assertEqual(t3_impl.packages(impl), [])
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
        again = t3_impl.vtium([self.tree], self.tar + "2")
        self.assertEqual(again["tree_sha256"], impl["tree_sha256"])
        self.assertRegex(impl["tree_sha256"], r"^[0-9a-f]{64}$")
        d = t3_impl.describe(impl)
        self.assertEqual(d["installed_bytes"], 21)
        self.assertEqual(d["paths"], [{"path": t3_common.rel(self.tree), "bytes": 21}])
        self.assertEqual(d["tree_sha256"], impl["tree_sha256"])

    def test_packages(self):
        # A package's bytes are its installed files', which is the size its
        # .PKGINFO records; pacman's metadata is not installed.
        main = os.path.join(self.dir.name, "vtium-1-1-x86_64.pkg.tar.zst")
        terminfo = os.path.join(self.dir.name, "vtium-terminfo-1-1-x86_64.pkg.tar.zst")
        write_package(main, {"usr/bin/vtium": "12345678", self.UNIT: "u"})
        write_package(terminfo, {"usr/share/terminfo/k/kmscon": "ti"})
        self.assertEqual(t3_impl.contents("packages", main), {"usr/bin/vtium": 8, self.UNIT: 1})
        impl = t3_impl.vtium([terminfo, main])
        self.assertEqual((impl["kind"], impl["paths"]), ("packages", [main, terminfo]))
        self.assertEqual((impl["bytes"], impl["installed_bytes"]), ([9, 2], 11))
        # ADR-304's size target is the main package's .PKGINFO size alone
        self.assertEqual(impl["main_bytes"], 9)
        self.assertEqual(t3_impl.packages(impl), [main, terminfo])
        self.assertEqual(t3_impl.c_reference([main])["name"], "c")

    def test_main_size(self):
        # A tree has no package split: its main size leaves out the terminfo
        # entries, which the C reference ships in a package of its own. The
        # main size is compared with ADR-304's minspec target, 1.77 MiB.
        write_tree(self.tree, {"usr/share/terminfo/k/kmscon": "ti"})
        impl = t3_impl.vtium([self.tree], self.tar)
        self.assertEqual((impl["main_bytes"], impl["installed_bytes"]), (21, 23))
        d = t3_impl.describe(impl)
        self.assertEqual((d["main_bytes"], d["installed_bytes"]), (21, 23))
        self.assertEqual(d["minspec_target"], {"main_mib": 1.77, "met": True})
        impl["main_bytes"] = 1870998  # the C reference's minimal package at b1f7a7c5
        self.assertEqual(t3_impl.describe(impl)["minspec_target"], {"main_mib": 1.77, "met": False})
        impl["main_bytes"] = int(1.77 * (1 << 20))
        self.assertTrue(t3_impl.describe(impl)["minspec_target"]["met"])

    def test_refused(self):
        # A tree beside packages, a file that is no package, a missing package,
        # or a build without the VT unit the guest starts.
        pkg = os.path.join(self.dir.name, "vtium-1-1-x86_64.pkg.tar.zst")
        write_package(pkg, {"usr/bin/vtium": "x"})
        for paths, why in (([self.tree, pkg], "is neither"),
                           ([os.path.join(self.tree, "usr/bin/vtium")], "is neither"),
                           ([pkg + ".missing.pkg.tar.zst"], "no vtium package"),
                           ([pkg], "installs no /usr/lib/systemd/system/kmsconvt@")):
            with self.subTest(why):
                with self.assertRaisesRegex(SystemExit, why):
                    t3_impl.vtium(paths, self.tar)
        os.unlink(os.path.join(self.tree, self.UNIT))
        with self.assertRaisesRegex(SystemExit, "installs no"):
            t3_impl.vtium([self.tree], self.tar)

    def test_install_checked(self):
        # vtium's tree: no kmscon, libtsm or vtium package in the guest, and
        # the tree the run packed; vtium has no libtsm to check.
        os.environ.pop("T3_LIBTSM_REV", None)
        impl = t3_impl.vtium([self.tree], self.tar)
        sha = impl["tree_sha256"]
        self.assertEqual(t3_scenario.check_install(self.LINE % sha, impl),
                         {"packages": [], "libtsm": "none", "tree": sha})
        with self.assertRaisesRegex(RuntimeError, "the guest installed tree 0+, the run's is"):
            t3_scenario.check_install(self.LINE % ("0" * 64), impl)
        with self.assertRaisesRegex(RuntimeError, "kmscon-aaronsb installed, not only the "
                                                  r"run's packages \(none"):
            t3_scenario.check_install((self.LINE % sha).replace(
                "packages: ;", "packages: kmscon-aaronsb 10-1 ;"), impl)
        # vtium's packages: those on the disc and no other
        pkg = os.path.join(self.dir.name, "vtium-1-1-x86_64.pkg.tar.zst")
        write_package(pkg, {self.UNIT: "u"})
        impl = t3_impl.vtium([pkg])
        line = "TIER3-INSTALL-OK packages: vtium 1-1 ; libtsm: none; tree: none"
        self.assertEqual(t3_scenario.check_install(line, impl),
                         {"packages": ["vtium 1-1"], "libtsm": "none"})

    def test_disc(self):
        # A tree goes on the disc as tree.tar, packages in pkg/.
        impl = t3_impl.vtium([self.tree], self.tar)
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_pkg_disc("2d-potato-min", self.dir.name, impl)
        disc = os.path.join(self.dir.name, "disc")
        self.assertIn("tree.tar", os.listdir(disc))
        self.assertNotIn("pkg", os.listdir(disc))
        with open(os.path.join(disc, "kmscon.conf")) as f:
            self.assertIn("render-timer=log", f.read())
        pkg = os.path.join(self.dir.name, "kmscon-aaronsb-1-1-x86_64.pkg.tar.zst")
        write_package(pkg, {"usr/bin/kmscon": "x"})
        with mock.patch.object(t3_scenario, "make_iso"):
            t3_scenario.build_pkg_disc("2d-potato-min", self.dir.name, t3_impl.c_reference([pkg]))
        self.assertEqual(os.listdir(os.path.join(disc, "pkg")), [os.path.basename(pkg)])
        self.assertNotIn("tree.tar", os.listdir(disc))

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
        # ADR-304's minspec figures: after the first screen, and the peak
        # with the screen it followed.
        got = t3_scenario.resident({"sgr": 10547, "s1-scroll": 16077, "cjk": 15900})
        self.assertEqual(got, {"first_screen": "sgr", "first_kib": 10547,
                               "peak_kib": 16077, "peak_screen": "s1-scroll"})
        self.assertEqual(t3_scenario.resident({}), {"first_screen": "sgr", "first_kib": None,
                                                    "peak_kib": None, "peak_screen": None})
        self.assertEqual(t3_scenario.failure_meta("x", {"kind": "tree", "paths": []})["resident"],
                         t3_scenario.resident({}))

    def test_variant_errors(self):
        # The scenario's own error comes first and the renderer's after it, so
        # a console that never started reports why. A drawn screen without a
        # resident-memory reading fails the variant: ADR-304 §5 measures it
        # on every run. A failure drops the captures, so every case fails.
        bbulk = ["Display x with backend [drm2d] text renderer [bbulk]"]
        caps = {"sgr": "a", "cjk": "b", "vt-switch": "c"}
        waited = "no 'vt: using tty /dev/tty2' within 60 s"
        c = {"name": t3_impl.C}

        def errors(error, displays, caps, rss, console=None, impl=c):
            return t3_scenario.variant_errors(error, displays, "bbulk", caps, rss, console, impl)

        self.assertEqual(errors(waited, [], {}, {}),
                         ([waited, "the console did not draw with bbulk: []"], True))
        self.assertEqual(errors(None, bbulk, caps, {"sgr": 1}),
                         (["no resident memory reading (T3-RSS) after cjk"], True))
        self.assertEqual(errors(None, bbulk, caps, {"sgr": 1, "cjk": 2}), ([], False))
        # a scenario that stopped part way keeps the captures it took
        self.assertEqual(errors(waited, bbulk, {"sgr": "a"}, {"sgr": 1}), ([waited], False))
        # A vtium run must show vtium drew, by the console's Revision line;
        # only a run named a stand-in may draw with another console.
        rss = {"sgr": 1, "cjk": 2}
        kmscon = {"name": "kmscon", "revision": "v10"}
        vtium = {"name": "vtium", "revision": "0.1"}
        run = {"name": t3_impl.VTIUM, "standin": False}
        standin = {"name": t3_impl.VTIUM, "standin": True}
        self.assertEqual(errors(None, bbulk, caps, rss, vtium, run), ([], False))
        self.assertEqual(errors(None, bbulk, caps, rss, kmscon, run),
                         (["the console that drew logged itself as kmscon v10, not vtium; "
                           "a stand-in's run takes --standin"], True))
        self.assertEqual(errors(None, bbulk, caps, rss, None, run),
                         (["the console logged no Revision line, so nothing shows vtium drew"],
                          True))
        self.assertEqual(errors(None, bbulk, caps, rss, kmscon, standin), ([], False))
        self.assertEqual(errors(None, bbulk, caps, rss, None, c), ([], False))

    def test_console_revision(self):
        # The guest's own account of the console that ran: its log's
        # "NAME Revision" line, the first one on the console.
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] kmscon[459]: log: kmscon Revision v10.1.0-890-gb1f7a7c5 "
                    "Oct 27 2026 03:12:01\n"
                    "[    9.1] vtium[502]: log: vtium Revision 0.1.0\n")
        try:
            self.assertEqual(t3_scenario.console_revision(f.name),
                             {"name": "kmscon", "revision": "v10.1.0-890-gb1f7a7c5"})
            with open(f.name, "w") as g:
                g.write("[    9.1] vtium[502]: vtium stand-in: no console here\n")
            self.assertIsNone(t3_scenario.console_revision(f.name))
        finally:
            os.unlink(f.name)
        # the report's implementation names every console its variants logged
        metas = {"a": {"console": {"name": "kmscon", "revision": "v10"}},
                 "b": {"console": {"name": "kmscon", "revision": "v10"}}, "c": {"console": None}}
        self.assertEqual(t3_scenario.consoles(metas), ["kmscon v10"])
        impl = t3_impl.vtium([self.tree], self.tar, standin=True)
        self.assertTrue(t3_impl.describe(impl)["standin"])
        self.assertFalse(t3_impl.describe(t3_impl.vtium([self.tree], self.tar))["standin"])

    def test_summary(self):
        impl = t3_impl.vtium([self.tree], self.tar, standin=True)
        impl["main_bytes"], impl["installed_bytes"] = 1870998, 1879995
        metas = {"2d-potato-min": {"resident": t3_scenario.resident({"sgr": 10547,
                                                                     "s1-scroll": 16077}),
                                   "console": {"name": "kmscon", "revision": "v10"}},
                 "2d-potato-min-psf": {"resident": t3_scenario.resident({}), "console": None}}
        self.assertEqual(t3.summary(impl, metas).splitlines(), [
            "implementation: a stand-in for vtium, tree %s: 1.78 MiB without terminfo, over "
            "ADR-304's minspec target of 1.77 MiB; 1.79 MiB installed; the console logged "
            "kmscon v10" % t3_common.rel(self.tree),
            "2d-potato-min: resident 10.30 MiB after sgr, peak 15.70 MiB after s1-scroll",
            "2d-potato-min-psf: resident none after sgr, peak none after none"])

    def test_vtium_process(self):
        # vtium's process logs as vtium[pid]; its render-timer and startup
        # lines are read as the C reference's are.
        line = timer_line(100.5, 60, 60).replace("kmscon[505]", "vtium[505]")
        self.assertEqual(t3_bench.timer_lines(line)[0]["pid"], 505)
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] vtium[459]: log: vtium Revision 0.1\n"
                    "[    6.700000] kmscon[459]: terminal: Display a with backend [x]\n"
                    "[    6.739898] vtium[459]: terminal: Display b with backend [drm2d] "
                    "text renderer [bbulk]\n")
        try:
            self.assertEqual(t3_scenario.kmscon_startup_ms(f.name), 76.7)
        finally:
            os.unlink(f.name)
        with open(os.path.join(t3_common.GUEST, "t3")) as f:
            self.assertIn("pidof kmscon vtium", f.read())

    def test_tools(self):
        # A vtium run names no libtsm; the C reference's names the run's.
        env = {"T3_LIBTSM_REV": "3ec40dc", "T3_LIBTSM_REPO": "r", "T3_KMSCON_REV": "b1f7a7c5"}
        with mock.patch.dict(os.environ, env):
            c = t3_runner.run_tools({"name": t3_impl.C})
            v = t3_runner.run_tools({"name": t3_impl.VTIUM})
        self.assertEqual((c["libtsm"], c["libtsm_repo"], c["kmscon"]), ("3ec40dc", "r", "b1f7a7c5"))
        self.assertEqual((v["libtsm"], v["libtsm_repo"], v["kmscon"]), (None, None, "b1f7a7c5"))

    def test_option(self):
        # --vtium installs vtium, so it takes no package target; it may be
        # given once per path, before the variants.
        with mock.patch.object(t3, "run", side_effect=lambda args, log: args) as run:
            args = t3.main(["run", "--vtium", "a", "--vtium", "b", "2d-potato-min"])
        self.assertEqual((args.vtium, args.variants), (["a", "b"], ["2d-potato-min"]))
        self.assertTrue(run.called)
        # Refused before run is called, with the reason on stderr.
        for argv, why in ((["--vtium", "a", "--pkg-target", "pkg-minimal"],
                           "--vtium installs vtium, not a package target's build"),
                          (["--vtium", "a", "--compare-only"],
                           "--vtium installs vtium, not a package target's build"),
                          (["--standin"], "--standin names a build given with --vtium")):
            with self.subTest(argv), mock.patch.object(t3, "run") as run:
                err = io.StringIO()
                with contextlib.redirect_stderr(err), self.assertRaises(SystemExit):
                    t3.main(["run"] + argv)
                self.assertIn(why, err.getvalue())
                self.assertFalse(run.called)

    def test_standin(self):
        # The C reference's newest packages unpacked as an install tree,
        # without pacman's metadata: it then runs as a vtium tree.
        pkgdir = os.path.join(self.dir.name, "pkg")
        os.makedirs(pkgdir)
        write_package(os.path.join(pkgdir, "kmscon-aaronsb-1-1-x86_64.pkg.tar.zst"),
                      {"usr/bin/kmscon": "x", self.UNIT: "u"})
        write_package(os.path.join(pkgdir, "kmscon-aaronsb-terminfo-1-1-x86_64.pkg.tar.zst"),
                      {"usr/share/terminfo/k/kmscon": "ti"})
        out = os.path.join(self.dir.name, "standin")
        write_tree(out, {"stale": "from an earlier run"})
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(t3.main(["standin", "--pkg-dir", pkgdir, out]), 0)
        self.assertEqual(t3_impl.contents("tree", out),
                         {"usr/bin/kmscon": 1, self.UNIT: 1, "usr/share/terminfo/k/kmscon": 2})
        self.assertEqual(t3_impl.vtium([out], self.tar)["installed_bytes"], 4)


class BaseImageTest(unittest.TestCase):
    """The provisioned base image is named by what it was built from, in the
    images directory that worktrees share, so every checkout with the same
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
            self.assertEqual(os.path.dirname(first), os.path.join(t3_common.VM, "images"))
            self.assertRegex(os.path.basename(first), r"^base-[0-9a-f]{16}\.qcow2$")
            self.assertEqual(self.key(guest), first)
            self.assertNotEqual(self.key(guest, archive="2026/10/21"), first)
            self.assertNotEqual(self.key(guest, image="other.qcow2"), first)
            for name in t3_common.BASE_INPUTS:
                with open(os.path.join(guest, name), "a") as f:
                    f.write("changed\n")
                self.assertNotEqual(self.key(guest), first, name)
        self.assertIn("user-data.in", t3_common.BASE_INPUTS)


class VariantTest(unittest.TestCase):
    def test_every_variant_sizes_its_machine(self):
        for name, v in t3_scenario.VARIANTS.items():
            with self.subTest(name):
                self.assertIsInstance(v["cpus"], int)
                self.assertIsInstance(v["memory_mb"], int)

    def test_potato(self):
        # ADR-103's small machine, in the default run.
        v = t3_scenario.VARIANTS["2d-1366x768-potato"]
        self.assertEqual((v["cpus"], v["memory_mb"], v["renderer"]), (1, 512, "bbulk"))
        self.assertIn("xres=1366,yres=768", v["device"])
        self.assertFalse(t3_scenario.uses_virgl("2d-1366x768-potato"))
        self.assertIn("2d-1366x768-potato", t3_scenario.DEFAULT_VARIANTS)

    def test_potato_minimal(self):
        # The potato machine with the minimal build (ADR-103), unifont and a
        # PSF font: the minimal package, so out of the default run, whose
        # variants install the default package.
        for name, conf in (("2d-potato-min", "kmscon-minimal.conf"),
                           ("2d-potato-min-psf", "kmscon-minimal-psf.conf")):
            with self.subTest(name):
                v = t3_scenario.VARIANTS[name]
                potato = t3_scenario.VARIANTS["2d-1366x768-potato"]
                for key in ("device", "cpus", "memory_mb", "renderer"):
                    self.assertEqual(v[key], potato[key])
                self.assertEqual(v["conf"], conf)
                self.assertEqual(t3_scenario.package_target(name), "pkg-minimal")
                self.assertNotIn(name, t3_scenario.DEFAULT_VARIANTS)
                # no picture to key: the image screens' keyed cases are left out
                cases = t3_scenario.variant_cases(name)
                self.assertNotIn("photo-scrollback", cases)
                self.assertNotIn("sixel-evict-zoomed", cases)
                self.assertIn("photo-scroll", cases)
                self.assertIn("vt-restore", cases)
        for name in t3_scenario.DEFAULT_VARIANTS:
            self.assertEqual(t3_scenario.variant_cases(name), t3_evaluate.CASES)
            self.assertEqual(t3_scenario.package_target(name), "pkg")
        with open(os.path.join(t3_common.GUEST, "kmscon-minimal-psf.conf")) as f:
            self.assertIn("font-engine=psf\n", f.read())

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
        got = figures("T3-NO-SIXEL\nT3-DONE mpv-sixel\n", images=True)
        self.assertIn("no benchmark window", got["mpv-sixel"]["error"])

    def test_startup(self):
        # from kmscon's revision line to its display line, same process
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] kmscon[459]: log: kmscon Revision v10\n"
                    "[    6.700000] kmscon[460]: terminal: Display a with backend [x]\n"
                    "[    6.739898] kmscon[459]: terminal: Display pci-0000:00:01.0[Virtual-1] "
                    "with backend [drm2d] text renderer [bbulk]\n")
        try:
            self.assertEqual(t3_scenario.kmscon_startup_ms(f.name), 76.7)
        finally:
            os.unlink(f.name)
        with tempfile.NamedTemporaryFile("w", delete=False) as f:
            f.write("[    6.663227] kmscon[459]: log: kmscon Revision v10\n")
        try:
            self.assertIsNone(t3_scenario.kmscon_startup_ms(f.name))
        finally:
            os.unlink(f.name)

    def test_gl_draws_images(self):
        # gltex draws image placements (ADR-103 steps 3f and 4e), so the GL
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
        meta = t3_scenario.failure_meta("no boot", {"kind": "tree", "paths": []})
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


def timer_line(t, flips, frames, interval_ms=1000.0, cpu=(1.0, 1.0, 2.0, 3.0), damage=(500, 1.5),
               pid=505, before="", skipped=(0, 0.0)):
    return ("%s[%12.6f] kmscon[%d]: terminal: render-timer: display=pci-0000:00:01.0[Virtual-1] "
            "interval_ms=%.3f presented_per_s=%.2f flips=%d frames=%d cpu_ms_mean=%.3f "
            "cpu_ms_median=%.3f cpu_ms_p95=%.3f cpu_ms_max=%.3f damage_px_mean=%d "
            "damage_pct=%.2f skipped=%d skipped_cpu_ms=%.3f"
            % (before, t, pid, interval_ms, flips * 1000 / interval_ms, flips, frames, cpu[0],
               cpu[1], cpu[2], cpu[3], damage[0], damage[1], skipped[0], skipped[1]))


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
        # A4 is compatibility through tmux (ADR-103): kmscon's CPU time per
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
    """ADR-103's criterion for A2: kmscon is not the bottleneck. Its CPU
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
    LINE = (b"TIER3-INSTALL-OK packages: kmscon-aaronsb 10.0.4.r208.gb5e9e66-1 "
            b"kmscon-aaronsb-terminfo 10.0.4.r208.gb5e9e66-1 ; libtsm: " + LIBTSM.encode())

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
        cut = self.LINE.index(b" kmscon-aaronsb-terminfo")
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
    unittest.main()
