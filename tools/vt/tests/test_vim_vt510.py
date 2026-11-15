#!/usr/bin/env python3
"""Tests of vim-vt510.py, the proof editor of the guided checks. The unit
tests drive its buffer, its wrapping and its drawing as functions. The
Replay tests run the editor on a pseudo-terminal, type keys at it, and play
the bytes it wrote through vtium's terminal with vtium-replay, the replay
interface's command line over vtium-term, to read the screen that results.

  python3 -I tools/vt/tests/test_vim_vt510.py

The Replay tests build vtium-replay with cargo in the cargo target directory
the Makefile uses (CARGO_TARGET_DIR, or target)."""
import fcntl
import importlib.util
import os
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unicodedata
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
CHECKS = os.path.join(ROOT, "tools", "vt", "checks")
EDITOR = os.path.join(CHECKS, "vim-vt510.py")
SAMPLE = os.path.join(CHECKS, "vim-vt510-sample.txt")
GUIDE = os.path.join(CHECKS, "editor.guide")
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
REPLAY = os.path.join(TARGET, "release", "vtium-replay")
# The longest any test waits on another process. It only makes a broken
# test fail instead of hanging.
WAIT_BOUND = 60
BUILD_BOUND = 1800

spec = importlib.util.spec_from_file_location("vim_vt510", EDITOR)
vim = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vim)

ESC = "\033"
CSI = ESC + "["


def editor(cols, rows, *lines, name=None):
    """An editor on LINES, each a (size, text) pair, that has drawn once."""
    e = vim.Editor(cols, rows, [vim.Line(size, text) for size, text in lines], name)
    e.draw()
    return e


def type_keys(e, *keys):
    """Types KEYS, each a key name or a string of characters typed one at a
    time, and returns the bytes of the draws."""
    out = b""
    for k in keys:
        for key in ([k] if k in NAMED else list(k)):
            out += e.keys([key])
    return out


NAMED = {"esc", "enter", "backspace", "delete", "up", "down", "left", "right", "home", "end", "pageup",
         "pagedown", "redraw", "tab"}


def chunks(text, n):
    """TEXT cut into rows of N columns, as an editor wraps it: a character
    of East Asian width W or F takes two columns, and one that does not fit
    at the end of a row starts the next. vtium's screen view prints no
    spaces at the end of a row, so neither does each piece."""
    rows = [""]
    used = 0
    for ch in text:
        w = 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
        if used + w > n:
            rows.append("")
            used = 0
        rows[-1] += ch
        used += w
    return [r.rstrip(" ") for r in rows]


class Layout(unittest.TestCase):
    def test_a_line_wraps_into_rows_of_its_width(self):
        cells = vim.clusters("abcdefghij")
        self.assertEqual(vim.layout(cells, 4), [(0, 4), (4, 8), (8, 10)])

    def test_an_empty_line_takes_one_row(self):
        self.assertEqual(vim.layout([], 80), [(0, 0)])

    def test_a_line_that_fills_its_last_row_gets_an_empty_row_for_the_cursor(self):
        self.assertEqual(vim.layout(vim.clusters("abcd"), 4), [(0, 4), (4, 4)])
        self.assertEqual(vim.place(vim.clusters("abcd"), 4, 4), (1, 0))
        self.assertEqual(vim.place(vim.clusters("abcd"), 4, 3), (0, 3))

    def test_a_wide_character_never_splits_across_a_wrap(self):
        # abc takes three of four columns, so the wide character, two
        # columns, starts the next row and the last column stays empty.
        cells = vim.clusters("abc中d")
        self.assertEqual(vim.layout(cells, 4), [(0, 3), (3, 5)])
        self.assertEqual(vim.place(cells, 4, 4), (1, 2))
        self.assertEqual(vim.layout(vim.clusters("ab中中"), 4), [(0, 3), (3, 4)])

    def test_wide_characters_and_emoji_take_two_columns_and_marks_none(self):
        self.assertEqual([vim.width(c) for c in vim.clusters("a中\U0001F642❤️é")],
                         [1, 2, 2, 2, 1])
        self.assertEqual(vim.clusters("éx"), ["é", "x"])

    def test_a_wide_or_tall_line_is_half_the_screens_width(self):
        self.assertEqual(vim.line_width("normal", 80), 80)
        self.assertEqual(vim.line_width("wide", 80), 40)
        self.assertEqual(vim.line_width("tall", 81), 40)
        self.assertEqual(vim.line_width("wide", 320), 160)

    def test_each_row_of_a_line_has_the_lines_size(self):
        text = "w" * 50
        e = editor(80, 12, ("wide", text), ("tall", "t" * 45), ("normal", "n" * 90))
        self.assertEqual(e.screen_rows(), [
            ("w", "w" * 40), ("w", "w" * 10),
            ("T", "t" * 40), ("B", "t" * 40), ("T", "t" * 5), ("B", "t" * 5),
            ("s", "n" * 80), ("s", "n" * 10),
            ("s", "~"), ("s", "~"),
        ])

    def test_a_tall_row_that_does_not_fit_whole_at_the_bottom_is_left_empty(self):
        e = editor(20, 4, ("normal", "a"), ("tall", "b"))
        self.assertEqual(e.screen_rows(), [("s", "a"), ("s", "")])


class Edits(unittest.TestCase):
    def test_inserting_in_the_middle_pushes_the_rest_onto_the_next_rows(self):
        e = editor(10, 6, ("normal", "abcdefghijklmno"))
        type_keys(e, "right", "right", "right", "i", "XY")
        self.assertEqual([t for _, t in e.screen_rows()[:2]], ["abcXYdefgh", "ijklmno"])
        type_keys(e, "backspace", "backspace")
        self.assertEqual([t for _, t in e.screen_rows()[:2]], ["abcdefghij", "klmno"])

    def test_text_typed_at_the_edge_of_a_wide_line_goes_on_a_wide_row(self):
        e = editor(10, 6, ("wide", "abcde"))
        type_keys(e, "end", "a", "fg")
        self.assertEqual(e.screen_rows()[:2], [("w", "abcde"), ("w", "fg")])

    def test_typing_on_a_tall_line_writes_both_rows_of_each_pair(self):
        e = editor(10, 8, ("tall", "abcd"))
        type_keys(e, "a", "end", "efg")
        self.assertEqual(e.screen_rows()[:4], [("T", "abcde"), ("B", "abcde"), ("T", "fg"), ("B", "fg")])

    def test_delete_pulls_the_next_rows_text_back(self):
        e = editor(10, 6, ("normal", "abcdefghijklmno"))
        type_keys(e, "x", "x")
        self.assertEqual([t for _, t in e.screen_rows()[:2]], ["cdefghijkl", "mno"])
        type_keys(e, "i", "delete")
        self.assertEqual(e.lines[0].text, "defghijklmno")

    def test_enter_splits_a_line_and_both_parts_keep_its_size(self):
        e = editor(20, 6, ("tall", "abcdef"))
        type_keys(e, "right", "right", "i", "enter")
        self.assertEqual(e.lines, [vim.Line("tall", "ab"), vim.Line("tall", "cdef")])
        self.assertEqual((e.line, e.index), (1, 0))

    def test_backspace_at_the_start_of_a_line_joins_it_to_the_line_above(self):
        e = editor(20, 6, ("wide", "ab"), ("normal", "cd"))
        type_keys(e, "down", "i", "backspace")
        self.assertEqual(e.lines, [vim.Line("wide", "abcd")])
        self.assertEqual(e.index, 2)

    def test_delete_at_the_end_of_a_line_joins_the_next_line(self):
        e = editor(20, 6, ("normal", "ab"), ("tall", "cd"))
        type_keys(e, "a", "end", "delete")
        self.assertEqual(e.lines, [vim.Line("normal", "abcd")])

    def test_dd_deletes_the_line_and_o_opens_a_normal_line_below(self):
        e = editor(20, 6, ("wide", "one"), ("tall", "two"), ("normal", "three"))
        type_keys(e, "d", "d")
        self.assertEqual([(l.size, l.text) for l in e.lines], [("tall", "two"), ("normal", "three")])
        type_keys(e, "o", "new", "esc")
        self.assertEqual([(l.size, l.text) for l in e.lines],
                         [("tall", "two"), ("normal", "new"), ("normal", "three")])
        self.assertEqual(e.mode, "NORMAL")

    def test_a_combining_mark_joins_the_character_before_it(self):
        e = editor(20, 6, ("normal", ""))
        type_keys(e, "i", "éx")
        self.assertEqual(e.lines[0].cells, ["é", "x"])

    def test_up_and_down_move_through_the_text_not_the_screen(self):
        # The first line wraps onto two rows, and Down goes to the next
        # line of the text, under the same column of its own.
        e = editor(10, 8, ("normal", "abcdefghijklm"), ("wide", "nopqrst"))
        type_keys(e, "right", "right", "down")
        self.assertEqual((e.line, e.index), (1, 2))
        type_keys(e, "up", "end")
        self.assertEqual((e.line, e.index), (0, 12))

    def test_pagedown_moves_through_the_text_and_the_view_follows(self):
        e = editor(20, 6, *[("normal", "line %d" % i) for i in range(30)])
        type_keys(e, "pagedown")
        self.assertEqual(e.line, 2)
        type_keys(e, "pagedown", "pagedown")
        self.assertEqual(e.line, 6)
        self.assertEqual(e.screen_rows()[e.screen_of((6, 0))], ("s", "line 6"))
        type_keys(e, "pageup")
        self.assertEqual(e.line, 4)


class Commands(unittest.TestCase):
    def test_wide_tall_and_normal_set_the_lines_size_and_it_rewraps(self):
        e = editor(20, 10, ("normal", "abcdefghijklmnopqrstuvwxy"))
        type_keys(e, ":wide", "enter")
        self.assertEqual(e.screen_rows()[:2], [("w", "abcdefghij"), ("w", "klmnopqrst")])
        type_keys(e, ":tall", "enter")
        self.assertEqual(e.screen_rows()[:6], [("T", "abcdefghij"), ("B", "abcdefghij"), ("T", "klmnopqrst"),
                                               ("B", "klmnopqrst"), ("T", "uvwxy"), ("B", "uvwxy")])
        type_keys(e, ":normal", "enter")
        self.assertEqual(e.screen_rows()[:2], [("s", "abcdefghijklmnopqrst"), ("s", "uvwxy")])

    def test_quit_is_refused_with_unsaved_changes_and_forced_with_a_bang(self):
        e = editor(80, 10, ("normal", "abc"))
        type_keys(e, "x", ":q", "enter")
        self.assertFalse(e.done)
        self.assertIn("unsaved changes", e.message)
        type_keys(e, ":q!", "enter")
        self.assertTrue(e.done)

    def test_write_saves_the_tags_and_edit_reads_them_back(self):
        d = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, d)
        path = os.path.join(d, "f.txt")
        e = editor(80, 10, ("normal", "plain"), ("normal", "big"), ("tall", "high"))
        type_keys(e, "down", ":wide", "enter", ":w " + path, "enter")
        with open(path, encoding="utf-8") as f:
            self.assertEqual(f.read(), "plain\n[wide] big\n[tall] high\n")
        self.assertFalse(e.modified)
        type_keys(e, ":q", "enter")
        self.assertTrue(e.done)
        other = editor(80, 10, ("normal", ""))
        type_keys(other, ":e " + path, "enter")
        self.assertEqual(other.lines, [vim.Line("normal", "plain"), vim.Line("wide", "big"), vim.Line("tall", "high")])
        self.assertEqual(other.name, path)

    def test_edit_is_refused_with_unsaved_changes(self):
        e = editor(80, 10, ("normal", "abc"))
        type_keys(e, "x", ":e other.txt", "enter")
        self.assertEqual(e.lines, [vim.Line("normal", "bc")])
        self.assertIn("unsaved changes", e.message)

    def test_the_file_format_round_trips_every_size(self):
        text = ("normal\n[wide] wide\n[tall] tall\n\n[normal] [wide] a normal line that starts with a tag\n"
                "[tall]no space, so normal\n")
        lines = vim.parse(text)
        self.assertEqual([(l.size, l.text) for l in lines], [
            ("normal", "normal"), ("wide", "wide"), ("tall", "tall"), ("normal", ""),
            ("normal", "[wide] a normal line that starts with a tag"), ("normal", "[tall]no space, so normal")])
        self.assertEqual(vim.unparse(lines), text)

    def test_the_sample_file_reads_with_lines_of_every_size(self):
        with open(SAMPLE, encoding="utf-8") as f:
            text = f.read()
        lines = vim.parse(text)
        self.assertEqual({l.size for l in lines}, {"normal", "wide", "tall"})
        self.assertEqual(vim.unparse(lines), text)
        self.assertTrue(any(vim.width(c) == 2 for l in lines for c in l.cells))

    def test_an_unknown_command_says_so(self):
        e = editor(80, 10, ("normal", "abc"))
        type_keys(e, ":frobnicate", "enter")
        self.assertEqual(e.message, "The command :frobnicate is not one this editor knows.")


class Keys(unittest.TestCase):
    def test_each_key_sequence_is_one_key(self):
        k = vim.Keys()
        self.assertEqual(k.feed(b"a\xc3\xa9\x1b[A\x1b[B\x1bOC\x1b[D\x1b[H\x1b[F\x1b[1~\x1b[4~\x1b[3~\x1b[5~\x1b[6~"
                                b"\r\x7f"),
                         ["a", "é", "up", "down", "right", "left", "home", "end", "home", "end", "delete",
                          "pageup", "pagedown", "enter", "backspace"])

    def test_a_sequence_split_across_reads_is_one_key(self):
        k = vim.Keys()
        self.assertEqual(k.feed(b"x\x1b["), ["x"])
        self.assertEqual(k.feed(b"C"), ["right"])
        self.assertEqual(k.feed(b"\xe4\xb8"), [])
        self.assertEqual(k.feed(b"\xad"), ["中"])

    def test_a_lone_esc_is_the_esc_key_once_no_more_bytes_come(self):
        k = vim.Keys()
        self.assertEqual(k.feed(b"\x1b"), [])
        self.assertEqual(k.flush(), ["esc"])
        self.assertEqual(k.feed(b"\x1bx"), ["esc", "x"])


class Drawing(unittest.TestCase):
    def test_the_first_draw_keeps_the_text_inside_a_scroll_region_with_autowrap_off(self):
        e = vim.Editor(80, 25, [vim.Line("wide", "big")])
        out = e.draw().decode()
        self.assertIn(CSI + "?7l", out)
        self.assertIn(CSI + "1;23r", out)
        self.assertIn(CSI + "1;1H" + ESC + "#6big" + CSI + "K", out)
        self.assertTrue(out.endswith(CSI + "1;1H" + CSI + "?25h"), out)

    def test_an_insert_in_the_middle_of_a_row_uses_ich(self):
        e = editor(10, 6, ("normal", "abcdefghijklmno"))
        type_keys(e, "right", "right", "i")
        out = type_keys(e, "X").decode()
        # Row 1: ICH makes room for X at column 3; row 2 takes the j the
        # first row pushed out, with its own ICH at column 1.
        self.assertIn(CSI + "1;3H" + CSI + "1@X", out)
        self.assertIn(CSI + "2;1H" + CSI + "1@j", out)

    def test_a_delete_in_the_middle_of_a_row_uses_dch(self):
        e = editor(10, 6, ("normal", "abcdefghijklmno"))
        out = type_keys(e, "right", "x").decode()
        self.assertIn(CSI + "1;2H" + CSI + "1P" + CSI + "1;10Hk", out)

    def test_a_row_that_changes_size_is_written_again_whole(self):
        e = editor(20, 6, ("normal", "abc"))
        out = type_keys(e, ":tall", "enter").decode()
        self.assertIn(CSI + "1;1H" + ESC + "#3abc" + CSI + "K", out)
        self.assertIn(CSI + "2;1H" + ESC + "#4abc" + CSI + "K", out)

    def test_moving_past_the_bottom_scrolls_the_text_rows_only(self):
        e = editor(40, 6, *[("normal", "line %d" % i) for i in range(10)])
        type_keys(e, "down", "down", "down")
        out = type_keys(e, "down").decode()
        self.assertTrue(out.startswith(CSI + "?25l" + CSI + "1S"), out)
        self.assertEqual(e.screen_rows(), [("s", "line %d" % i) for i in range(1, 5)])
        # Only the row that scrolled in is written, then the status line
        # (row 5) with the new line number, and the cursor goes to row 4.
        self.assertEqual(sorted(set(int(r) for r in re.findall(r"\x1b\[(\d+);\d+H", out))), [4, 5])

    def test_the_status_line_names_the_file_mode_position_and_who_wraps(self):
        e = editor(80, 25, ("normal", "abc"), name="dir/vim-vt510-sample.txt")
        type_keys(e, "down", "l", "l")
        s = e.status()
        self.assertEqual(len(s), 80)
        self.assertTrue(s.startswith(" vim-vt510-sample.txt  NORMAL "), s)
        self.assertIn(" the editor wraps lines ", s)
        self.assertTrue(s.endswith(" line 1, column 3 "), s)
        type_keys(e, "i", "x")
        self.assertIn("vim-vt510-sample.txt [+]  INSERT", e.status())
        wide = editor(320, 45, ("normal", ""), name="f.txt")
        self.assertEqual(len(wide.status()), 320)
        self.assertIn("vim-vt510 — the editor wraps lines; the terminal does not", wide.status())

    def test_the_end_puts_the_terminal_back(self):
        e = editor(20, 6, ("tall", "a"), ("wide", "b"))
        out = vim.finish(6, e.drawn).decode()
        self.assertEqual(out, CSI + "0m" + CSI + "4l" + CSI + "r" + CSI + "?7h" + CSI + "1;1H" + ESC + "#5"
                         + CSI + "2;1H" + ESC + "#5" + CSI + "3;1H" + ESC + "#5" + CSI + "H" + CSI + "2J"
                         + CSI + "6;1H" + ESC + "#5" + CSI + "?25h")


class Guide(unittest.TestCase):
    def test_the_guide_passes_its_check(self):
        p = subprocess.run(["bash", os.path.join(CHECKS, "guide.sh"), "--check", GUIDE], capture_output=True,
                           text=True, timeout=WAIT_BOUND)
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_each_step_runs_the_editor_on_a_copy_of_the_sample(self):
        # A step never saves over the sample in the checkout.
        with open(GUIDE, encoding="utf-8") as f:
            runs = [line.split(":", 1)[1].strip() for line in f if line.startswith("run:")]
        self.assertGreater(len(runs), 3)
        for run in runs:
            self.assertEqual(run, "cp tools/vt/checks/vim-vt510-sample.txt /tmp/vim-vt510.txt && "
                                  "python3 tools/vt/checks/vim-vt510.py /tmp/vim-vt510.txt")


class Session:
    """The editor on a pseudo-terminal. Each key waits for the draw it
    causes, which ends with the cursor shown (DECTCEM set), and the stream
    of everything the editor wrote is kept for the replay."""

    SHOW = b"\x1b[?25h"

    def __init__(self, test, cols, rows, *args, cwd=None):
        self.cols, self.rows = cols, rows
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            if cwd:
                os.chdir(cwd)
            os.execv(sys.executable, [sys.executable, "-I", EDITOR, *args])
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        test.addCleanup(self.reap)
        self.stream = b""
        self.draws = 0
        self.wait_draw()

    def reap(self):
        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except (ProcessLookupError, ChildProcessError):
            pass
        os.close(self.fd)

    def read(self):
        if not select.select([self.fd], [], [], WAIT_BOUND)[0]:
            raise AssertionError("the editor wrote nothing for %d s; it wrote %r" % (WAIT_BOUND, self.stream[-200:]))
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            data = b""
        self.stream += data
        return data

    def wait_draw(self):
        self.draws += 1
        while self.stream.count(self.SHOW) < self.draws:
            if not self.read():
                raise AssertionError("the editor ended; it wrote %r" % self.stream[-200:])

    def keys(self, *keys):
        """Types each key, a byte string, or each character of a str."""
        for k in keys:
            for b in ([k] if isinstance(k, bytes) else [c.encode() for c in k]):
                os.write(self.fd, b)
                self.wait_draw()

    def command(self, text):
        self.keys(":" + text, b"\r")

    def resize(self, cols, rows):
        """Gives the pty a new size, which sends the editor SIGWINCH, and
        tells the replay of the resize at the same point of the stream."""
        self.stream += b"\x1b_tsm-test;resize;c=%d;r=%d\x1b\\" % (cols, rows)
        self.draws = self.stream.count(self.SHOW)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        self.cols, self.rows = cols, rows
        self.wait_draw()

    def end(self):
        """Waits for the editor to exit, and returns its exit status."""
        while self.read():
            pass
        end = time.monotonic() + WAIT_BOUND
        while time.monotonic() < end:
            done, status = os.waitpid(self.pid, os.WNOHANG)
            if done:
                return os.waitstatus_to_exitcode(status)
            time.sleep(0.02)
        raise AssertionError("the editor did not exit")


DOWN = b"\x1b[B"
RIGHT = b"\x1b[C"
END = b"\x1b[F"
BACKSPACE = b"\x7f"


class Replay(unittest.TestCase):
    """The editor's output, played through vtium's terminal."""

    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "-q", "--release", "-p", "vtium-conformance", "--features", "images,bidi",
                        "--bin", "vtium-replay"], cwd=ROOT, check=True, timeout=BUILD_BOUND)

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir)
        self.file = os.path.join(self.dir, "sample.txt")
        shutil.copy(SAMPLE, self.file)

    def replay(self, s, extra=b"", view=()):
        path = os.path.join(self.dir, "stream.bin")
        with open(path, "wb") as f:
            f.write(s.stream + extra)
        p = subprocess.run([REPLAY, *view, str(s.cols), str(s.rows), path], capture_output=True, text=True,
                           timeout=WAIT_BOUND)
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout

    def screen(self, s):
        """Each row of the screen as (size, text), and the cursor (column,
        row). vtium's screen view gives the text. The size comes from the
        frame view: the test puts the terminal's cursor on each row in
        turn, and a cursor on a double-size row reaches across two cells
        (span=2), and on a double-height pair down both rows (rows=A-B)."""
        out = self.replay(s)
        texts = dict((int(r), t) for r, t in re.findall(r"^row (\d+) \|(.*)\|$", out, re.M))
        cursor = tuple(int(n) for n in re.search(r"^cursor (\d+),(\d+)$", out, re.M).groups())
        probe = b""
        ends = []
        for r in range(s.rows):
            probe += b"\x1b[%d;1H" % (r + 1)
            ends.append(len(s.stream) + len(probe))
        cuts = os.path.join(self.dir, "stream.cuts")
        with open(cuts, "w") as f:
            f.write("".join("%d\n" % e for e in ends))
        path = os.path.join(self.dir, "stream.bin")
        with open(path, "wb") as f:
            f.write(s.stream + probe)
        p = subprocess.run([REPLAY, "--frame", str(s.cols), str(s.rows), path, cuts], capture_output=True,
                           text=True, timeout=WAIT_BOUND)
        self.assertEqual(p.returncode, 0, p.stderr)
        sizes = []
        for r, block in enumerate(p.stdout.split("checkpoint ")[1:s.rows + 1]):
            line = re.search(r"^frame-cursor .*$", block, re.M).group(0)
            pair = re.search(r" rows=(\d+)-(\d+)", line)
            if pair:
                sizes.append("T" if int(pair.group(1)) == r else "B")
            else:
                sizes.append("w" if re.search(r" span=[24]", line) else "s")
        return [(sizes[r], texts[r]) for r in range(s.rows)], cursor

    def sample_rows(self, cols):
        """The sample's rows at COLS columns, wrapped by cutting each line
        into pieces of its width, as the editor's layout must for text of
        one-column characters."""
        with open(SAMPLE, encoding="utf-8") as f:
            lines = f.read().splitlines()
        rows = []
        for line in lines:
            size = "s"
            for tag, code in (("[wide] ", "w"), ("[tall] ", "T")):
                if line.startswith(tag):
                    size, line = code, line[len(tag):]
            for piece in chunks(line, cols if size == "s" else cols // 2):
                rows += [("T", piece), ("B", piece)] if size == "T" else [(size, piece)]
        return rows

    def test_the_sample_opens_with_each_line_at_its_size_at_80_by_25(self):
        s = Session(self, 80, 25, self.file)
        rows, cursor = self.screen(s)
        want = self.sample_rows(80)
        # The first 21 rows of the text, then an empty row where the next
        # tall pair would not fit whole, the status line and the message.
        self.assertEqual(rows[:21], want[:21])
        self.assertEqual(rows[0], ("w", "vim-vt510, a small editor"))
        self.assertEqual(rows[16:21], [("w", "A wide line wraps at half the screen's w"), ("w", "idth. Insert a word here."),
                                       ("s", ""), ("T", "A tall line"), ("B", "A tall line")])
        self.assertEqual(rows[22], ("s", ""))
        self.assertEqual(rows[23][0], "s")
        # The screen view prints no spaces at the end of a row, so the
        # status line's last space, in reverse video, does not show.
        self.assertRegex(rows[23][1], r"^ sample\.txt  NORMAL +the editor wraps lines +line 1, column 1$")
        self.assertEqual(len(rows[23][1]), 79)
        self.assertEqual(rows[24][1], "Read 21 lines. :q quits, :w saves, i inserts, and Esc goes back to normal mode.")
        self.assertEqual(cursor, (0, 0))

    def test_an_insert_flows_onto_the_next_rows_and_a_delete_flows_back(self):
        s = Session(self, 80, 25, self.file)
        before, _ = self.screen(s)
        s.keys(*[DOWN] * 5, *[RIGHT] * 20, "i", "NEW WORDS ")
        rows, cursor = self.screen(s)
        with open(SAMPLE, encoding="utf-8") as f:
            line = f.read().splitlines()[5]
        grown = line[:20] + "NEW WORDS " + line[20:]
        self.assertEqual(rows[11:11 + len(chunks(grown, 80))], [("s", p) for p in chunks(grown, 80)])
        self.assertEqual(cursor, (30, 11))
        # The rows above the line stay as they were, and the status line
        # and the message row stay on the last two rows.
        self.assertEqual(rows[:11], before[:11])
        self.assertIn("INSERT", rows[23][1])
        s.keys(*[BACKSPACE] * 10)
        rows, cursor = self.screen(s)
        self.assertEqual(rows[:23], before[:23])
        self.assertEqual(cursor, (20, 11))

    def test_wide_and_tall_rewrap_the_line_at_half_the_width(self):
        s = Session(self, 80, 25, self.file)
        with open(SAMPLE, encoding="utf-8") as f:
            line = f.read().splitlines()[5]
        s.keys(*[DOWN] * 5)
        s.command("wide")
        rows, _ = self.screen(s)
        pieces = chunks(line, 40)
        self.assertEqual(rows[11:11 + len(pieces)], [("w", p) for p in pieces])
        s.command("tall")
        rows, _ = self.screen(s)
        # The tall line's pairs fill the text area from row 11.
        pairs = [r for p in pieces for r in (("T", p), ("B", p))]
        self.assertEqual(rows[11:23], pairs[:12])
        s.command("normal")
        rows, _ = self.screen(s)
        self.assertEqual(rows[:23], self.sample_rows(80)[:21] + [("s", ""), ("s", "")])

    def test_typing_past_the_edge_of_a_tall_line_starts_a_second_pair(self):
        s = Session(self, 80, 25, self.file)
        s.keys(*[DOWN] * 9, "a", END, "x" * 35)
        rows, cursor = self.screen(s)
        text = "A tall line" + "x" * 35
        self.assertEqual(rows[19:23], [("T", text[:40]), ("B", text[:40]), ("T", text[40:]), ("B", text[40:])])
        self.assertEqual(cursor, (6, 21))

    def test_saving_keeps_each_lines_size_and_the_end_puts_the_terminal_back(self):
        s = Session(self, 80, 25, "sample.txt", cwd=self.dir)
        s.keys(*[DOWN] * 5)
        s.command("wide")
        s.command("q")
        rows, _ = self.screen(s)
        self.assertEqual(rows[24][1], "This file has unsaved changes. :w saves them, and :q! quits without saving.")
        s.command("w")
        with open(self.file, encoding="utf-8") as f:
            saved = f.read().splitlines()
        self.assertEqual(saved[5], "[wide] A long normal line. Put the cursor in the middle of this line and insert a "
                                   "few words. The text after the cursor moves right, and the letters at the end of "
                                   "each row move to the start of the next row. Delete the words again, and the text "
                                   "flows back up.")
        s.command("e sample.txt")
        rows, _ = self.screen(s)
        self.assertEqual(rows[11], ("w", "A long normal line. Put the cursor in th"))
        self.assertEqual(rows[0], ("w", "vim-vt510, a small editor"))
        os.write(s.fd, b":q\r")
        self.assertEqual(s.end(), 0)
        rows, cursor = self.screen(s)
        self.assertEqual(rows, [("s", "")] * 25)
        self.assertEqual(cursor, (0, 24))
        state = self.replay(s, b"\x1b_tsm-test;state\x1b\\")
        self.assertIn("test state margins 0-24 0-79", state)
        self.assertRegex(state, r"test state modes ansi (?!.*\b4\b)[0-9,]* dec 7,")
        self.assertIn("test state pen\n", state)

    def test_the_sample_at_320_by_45(self):
        s = Session(self, 320, 45, self.file)
        rows, _ = self.screen(s)
        want = self.sample_rows(320)
        # 21 lines, three of them tall, so 24 rows, and a tilde on each
        # row past the text's end.
        self.assertEqual(len(want), 24)
        self.assertEqual(rows[:24], want)
        self.assertEqual(rows[24:43], [("s", "~")] * 19)
        self.assertIn("vim-vt510 — the editor wraps lines; the terminal does not", rows[43][1])
        # The long tall line, line 12 of the file, wraps at 160 columns
        # once it is long enough.
        s.keys(*[DOWN] * 11, "a", END, "y" * 100)
        rows, _ = self.screen(s)
        text = "A long tall line wraps onto a second pair of rows. Type past the edge." + "y" * 100
        self.assertEqual(rows[12:16], [("T", text[:160]), ("B", text[:160]), ("T", text[160:]), ("B", text[160:])])

    def test_wide_characters_move_to_the_next_row_whole(self):
        s = Session(self, 80, 25, self.file)
        # Line 20 of the file starts "Wide characters".
        s.keys(*[DOWN] * 19)
        rows, (_, r) = self.screen(s)
        self.assertTrue(rows[r][1].startswith("Wide characters"), rows[r])
        # "Wide characters take two columns each: " is 39 columns, and the
        # 8 wide characters after it take 16. Pushed 26 columns right,
        # they would reach column 81: the last one moves whole to the next
        # row, and the row ends at column 79 with its last column empty.
        s.keys("i", "z" * 26, END)
        rows, _ = self.screen(s)
        r = [i for i, (_, t) in enumerate(rows) if t.startswith("z" * 26)][0]
        self.assertEqual(rows[r], ("s", "z" * 26 + "Wide characters take two columns each: 漢字かな交じり"))
        self.assertTrue(rows[r + 1][1].startswith("文 and "), rows[r + 1])

    def test_scrolling_down_to_the_end_keeps_each_rows_size(self):
        # The view moves a row or a few at a time, which the editor does
        # with SU inside the scroll region, so the terminal moves each
        # row's size with its text.
        s = Session(self, 80, 25, self.file)
        s.keys(*[DOWN] * 20)
        rows, (_, r) = self.screen(s)
        want = self.sample_rows(80)
        # The view moved just far enough to show the cursor's row, the
        # first of the last line's two rows.
        self.assertEqual(rows[:23], want[len(want) - 24:-1])
        self.assertEqual(rows[r], ("w", "On a wide line too: \u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8"
                                        "\u3001\u6f22"))
        self.assertIn(b"\x1b[1S", s.stream)

    def test_esc_alone_leaves_insert_mode(self):
        # A lone ESC is the Esc key once no more bytes follow it, so x then
        # deletes in normal mode instead of being typed.
        s = Session(self, 80, 25, os.path.join(self.dir, "new.txt"))
        s.keys("i", "abc", b"\x1b", "x")
        rows, cursor = self.screen(s)
        self.assertEqual(rows[0], ("s", "ab"))
        self.assertRegex(rows[23][1], r"^ new\.txt \[\+\]  NORMAL ")
        self.assertEqual(cursor, (1, 0))

    def test_a_kill_ends_the_editor_with_the_terminal_put_back(self):
        s = Session(self, 80, 25, self.file)
        os.kill(s.pid, signal.SIGTERM)
        self.assertEqual(s.end(), 0)
        self.assertTrue(termios.tcgetattr(s.fd)[3] & termios.ECHO)
        rows, cursor = self.screen(s)
        self.assertEqual(rows, [("s", "")] * 25)
        self.assertEqual(cursor, (0, 24))

    def test_a_resize_draws_the_screen_again_at_the_new_width(self):
        s = Session(self, 80, 25, self.file)
        s.resize(100, 30)
        rows, _ = self.screen(s)
        self.assertEqual(rows[:27], self.sample_rows(100)[:27])
        self.assertIn("the editor wraps lines", rows[28][1])
        self.assertEqual(len(rows[28][1]), 99)


if __name__ == "__main__":
    unittest.main()
