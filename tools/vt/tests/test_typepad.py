#!/usr/bin/env python3
"""Tests the typing pad of the guided checks, tools/vt/checks/typepad.py:
what it sends for each key, how Backspace asks the terminal where the
cursor is, the screens it draws, and a whole run on a pseudo-terminal.

    python3 -I tools/vt/tests/test_typepad.py [-v] [TestCase.test_name ...]
"""

import fcntl
import importlib.util
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time
import unittest

# The longest any test waits on the pad. It only makes a broken test fail instead of hanging.
WAIT_BOUND = 60

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
TYPEPAD = os.path.join(ROOT, "tools", "vt", "checks", "typepad.py")

spec = importlib.util.spec_from_file_location("typepad", TYPEPAD)
typepad = importlib.util.module_from_spec(spec)
spec.loader.exec_module(typepad)


CPR = b"\x1b[6n"


class Fake:
    """A pad and a stand-in terminal for it. The terminal answers each CSI
    6 n with the next position of answers, after the read that asked it,
    as a terminal's answer arrives in a later read. When answers runs out,
    the question waits for the test to answer it with answer()."""

    def __init__(self, answers=(), floor=7):
        self.answers = list(answers)
        self.due = []
        self.out = []
        self.pad = typepad.Pad(self.write, floor)

    def write(self, data):
        self.out.append(data)
        if data == CPR and self.answers:
            self.due.append(self.answers.pop(0))

    def answer(self, row, col):
        self.pad.feed(b"\x1b[%d;%dR" % (row, col))
        while self.due:
            self.answer(*self.due.pop(0))

    def feed(self, *keys):
        for k in keys:
            self.pad.feed(k)
            while self.due:
                self.answer(*self.due.pop(0))
        return self

    def sent(self):
        return b"".join(self.out)


def sent(*keys):
    """What the pad sends for keys that print nothing, each fed as one read."""
    return Fake().feed(*keys).sent()


def printed(*chars):
    """What the pad sends to print chars: each between two questions."""
    return b"".join(CPR + c + CPR for c in chars)


# Each character asks twice, and here the terminal answers that the cursor
# moved on, so no wrap is pending.
MOVING = [(9, 5), (9, 6)] * 20


class Keys(unittest.TestCase):
    def test_printable_characters_and_utf8_print_one_character_at_a_time(self):
        out = Fake(MOVING).feed(b"ab 1", "é中".encode()).sent()
        self.assertEqual(out, printed(b"a", b"b", b" ", b"1", "é".encode(), "中".encode()))

    def test_bytes_that_are_not_utf8_print_nothing(self):
        self.assertEqual(Fake(MOVING).feed(b"\xff\x80a\xc3").sent(), printed(b"a"))
        self.assertEqual(Fake(MOVING).feed(b"\xc3a").sent(), printed(b"a"))

    def test_each_editing_key_sends_its_sequence(self):
        cases = {
            b"\r": b"\r\n",
            b"\n": b"\r\n",
            b"\t": b"\t",
            b"\x1b[A": b"\x1b[A",
            b"\x1b[B": b"\x1b[B",
            b"\x1b[C": b"\x1b[C",
            b"\x1b[D": b"\x1b[D",
            # DECCKM's application form of the cursor keys
            b"\x1bOA": b"\x1b[A",
            b"\x1bOD": b"\x1b[D",
            b"\x1b[H": b"\r",
            b"\x1bOH": b"\r",
            b"\x1b[1~": b"\r",
            b"\x1b[F": b"\x1b[999C",
            b"\x1bOF": b"\x1b[999C",
            b"\x1b[4~": b"\x1b[999C",
            b"\x1b[2~": b"\x1b[@",
            b"\x1b[3~": b"\x1b[P",
            b"\x0b": b"\x1b[K",
            b"\x0c": b"\x1b[J",
            b"\x1bOR": b"\x1b#3",
            b"\x1bOS": b"\x1b#4",
            b"\x1b[15~": b"\x1b#5",
            b"\x1b[17~": b"\x1b#6",
        }
        for key, want in cases.items():
            with self.subTest(key=key):
                self.assertEqual(sent(key), want)

    def test_other_keys_send_nothing(self):
        # Esc, a modified arrow, Page Up, F1, Shift+F3, F7, the other
        # control keys, and a sequence too long to be a key's
        for key in (b"\x1b", b"\x1b[1;5C", b"\x1b[5~", b"\x1bOP", b"\x1b[1;2R", b"\x1b[18~",
                    b"\x01", b"\x1a", b"\x1b[" + b"1;" * 20 + b"A"):
            with self.subTest(key=key):
                self.assertEqual(sent(key), b"")
        # Alt with a key sends ESC and the key, and the key prints.
        self.assertEqual(Fake(MOVING).feed(b"\x1bx").sent(), printed(b"x"))

    def test_a_sequence_split_across_reads_is_one_key(self):
        self.assertEqual(sent(b"\x1b", b"[", b"2", b"~"), b"\x1b[@")
        self.assertEqual(sent(b"\x1bO", b"R"), b"\x1b#3")
        e = "é".encode()
        self.assertEqual(Fake(MOVING).feed(e[:1], e[1:]).sent(), printed(e))

    def test_ctrl_d_and_ctrl_c_end_the_pad_even_inside_a_sequence(self):
        for keys in (b"\x04", b"\x03", b"\x1b[2\x04", b"\x1bO\x04", b"\xc3\x04"):
            with self.subTest(keys=keys):
                with self.assertRaises(typepad.Done):
                    Fake().feed(keys)


class Backspace(unittest.TestCase):
    def test_backspace_asks_where_the_cursor_is(self):
        for key in (b"\x7f", b"\x08"):
            with self.subTest(key=key):
                self.assertEqual(sent(key), CPR)

    def test_past_the_first_column_backspace_erases_the_character_before(self):
        # Column 40 is the last of a double-width line on 80 columns.
        for col in (2, 40, 80):
            with self.subTest(col=col):
                self.assertEqual(Fake([(9, col)]).feed(b"\x7f").sent(), CPR + b"\b\x1b[X")

    def test_at_the_first_column_backspace_goes_on_at_the_line_above(self):
        self.assertEqual(Fake([(8, 1)]).feed(b"\x7f").sent(), CPR + b"\x1b[A\x1b[999C\x1b[X")

    def test_backspace_never_moves_above_the_first_line_drawn(self):
        self.assertEqual(Fake([(7, 1)]).feed(b"\x7f").sent(), CPR)
        self.assertEqual(Fake([(3, 1)]).feed(b"\x7f").sent(), CPR)

    def test_after_a_character_that_left_a_wrap_pending_backspace_erases_in_place(self):
        # The cursor stood on column 40, the line's last, before and after
        # the character, so the character is in that column.
        f = Fake([(9, 40), (9, 40), (9, 40)]).feed(b"z\x7f")
        self.assertEqual(f.sent(), printed(b"z") + CPR + b"\x1b[D\x1b[C\x1b[X")
        # The next Backspace erases the character before, as usual.
        f.answers = [(9, 40)]
        f.feed(b"\x7f")
        self.assertTrue(f.sent().endswith(b"\x1b[X" + CPR + b"\b\x1b[X"))

    def test_a_move_after_the_character_ends_the_pending_wrap(self):
        for key in (b"\x1b[D", b"\x1b[H", b"\x1b[2~", b"\x1b[15~", b"\r"):
            with self.subTest(key=key):
                f = Fake([(9, 40), (9, 40), (9, 40)]).feed(b"z", key, b"\x7f")
                self.assertTrue(f.sent().endswith(CPR + b"\b\x1b[X"), f.sent())

    def test_keys_typed_before_the_answer_wait_and_keep_their_order(self):
        f = Fake().feed(b"\x7fx\x1b[Dy")
        self.assertEqual(f.sent(), CPR)
        f.answer(9, 5)
        # x prints at once, between its two questions, and the arrow and y
        # wait for both answers.
        self.assertEqual(f.sent(), CPR + b"\b\x1b[X" + printed(b"x"))
        f.answer(9, 4)
        self.assertEqual(f.sent(), CPR + b"\b\x1b[X" + printed(b"x"))
        f.answer(9, 5)
        self.assertEqual(f.sent(), CPR + b"\b\x1b[X" + printed(b"x") + b"\x1b[D" + printed(b"y"))

    def test_a_cursor_report_nobody_asked_for_does_nothing(self):
        self.assertEqual(sent(b"\x1b[9;5R"), b"")

    def test_ctrl_d_ends_the_pad_while_an_answer_is_awaited(self):
        f = Fake().feed(b"\x7fa")
        with self.assertRaises(typepad.Done):
            f.feed(b"\x04")


class Screens(unittest.TestCase):
    def lines(self, name, cols=80):
        return typepad.screen(name, cols)

    def test_each_screen_holds_its_lines_and_starts_the_cursor_on_the_first(self):
        self.assertEqual(self.lines("wide"), ([("wide", ""), ("wide", "")], (1, 1)))
        self.assertEqual(self.lines("tall"), ([("top", ""), ("bottom", "")], (1, 1)))
        lines, cursor = self.lines("edit")
        self.assertEqual([s for s, _ in lines], ["wide", "single", "top", "bottom"])
        self.assertEqual(lines[0][1], "abcdefghijklmnopqrstuvwxyz0123456789ABCD")
        self.assertEqual(lines[2][1], lines[0][1])
        self.assertEqual(lines[3][1], lines[0][1])
        self.assertEqual(cursor, (1, 11))
        lines, cursor = self.lines("mixed")
        self.assertEqual([s for s, _ in lines], ["single", "wide", "top", "bottom", "single"])
        self.assertEqual(lines[0][1], "1234567890" * 8)
        self.assertEqual(cursor, (1, 71))

    def test_a_double_size_line_never_holds_more_than_half_the_columns(self):
        for cols in (80, 81, 100, 41):
            for name in ("edit", "mixed"):
                lines, _ = self.lines(name, cols)
                for size, text in lines:
                    with self.subTest(cols=cols, name=name, size=size):
                        self.assertLessEqual(len(text), cols if size == "single" else cols // 2)

    def test_the_drawing_starts_below_the_legend(self):
        out, floor = typepad.draw("tall", 80, 6)
        self.assertEqual(floor, 7)
        self.assertTrue(out.startswith(b"\x1b[H\x1b[2J\x1b[6 q"))
        self.assertTrue(out.endswith(b"\x1b[7;1H\x1b#3\x1b[8;1H\x1b#4\x1b[7;1H"))
        # On 40 columns each of the legend's five lines takes two rows.
        self.assertEqual(typepad.draw("tall", 40, None)[1], 12)

    def test_the_legend_fits_80_columns(self):
        for line in typepad.LEGEND:
            self.assertLessEqual(len(line), 80, line)

    def test_the_end_puts_the_cursor_on_a_clean_single_line_at_the_bottom(self):
        self.assertEqual(typepad.finish(24, None), b"\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(typepad.finish(24, 6), b"\x1b[24;1H\x1b#5\x1b[2K\x1b[0 q")


class Run(unittest.TestCase):
    """typepad.py itself, on a pseudo-terminal of 80 columns and 24 rows."""

    def start(self, *args):
        pid, fd = pty.fork()
        if pid == 0:
            os.execv(sys.executable, [sys.executable, "-I", TYPEPAD, *args])
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        self.addCleanup(self.reap, pid, fd)
        return pid, fd

    def reap(self, pid, fd):
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except (ProcessLookupError, ChildProcessError):
            pass
        os.close(fd)

    def read_until(self, fd, text):
        out = b""
        end = time.monotonic() + WAIT_BOUND
        while text not in out:
            left = end - time.monotonic()
            if left <= 0 or not select.select([fd], [], [], left)[0]:
                raise AssertionError(f"no {text!r} in {out!r}")
            try:
                data = os.read(fd, 4096)
            except OSError:
                data = b""
            if not data:
                raise AssertionError(f"no {text!r} in {out!r}")
            out += data
        return out

    def wait(self, pid):
        end = time.monotonic() + WAIT_BOUND
        while time.monotonic() < end:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                return os.waitstatus_to_exitcode(status)
            time.sleep(0.02)
        raise AssertionError("the pad did not exit")

    def test_a_run_types_erases_and_ends_with_the_terminal_put_back(self):
        pid, fd = self.start("--cursor=6", "wide")
        # The pad has drawn its screen and set raw mode once the cursor
        # goes to the screen's first line.
        out = self.read_until(fd, b"\x1b[7;1H\x1b#6\x1b[8;1H\x1b#6\x1b[7;1H")
        self.assertIn(b"\x1b[6 q", out)
        self.assertIn(b"Ctrl-D ends the typing.", out)
        # Linux's pseudo-terminals keep one set of settings for both ends.
        raw = termios.tcgetattr(fd)
        self.assertFalse(raw[3] & (termios.ECHO | termios.ICANON))
        # The test answers the pad's questions as the terminal would.
        os.write(fd, b"a")
        self.read_until(fd, b"\x1b[6na\x1b[6n")
        os.write(fd, b"\x1b[7;1R\x1b[7;2R\x7f")
        self.read_until(fd, b"\x1b[6n")
        os.write(fd, b"\x1b[7;2R")
        self.read_until(fd, b"\b\x1b[X")
        os.write(fd, b"\x04")
        self.read_until(fd, b"\x1b[24;1H\x1b#5\x1b[2K\x1b[0 q")
        self.assertEqual(self.wait(pid), 0)
        cooked = termios.tcgetattr(fd)
        self.assertTrue(cooked[3] & termios.ECHO and cooked[3] & termios.ICANON)

    def test_a_hangup_ends_the_pad_with_the_terminal_put_back(self):
        pid, fd = self.start("tall")
        self.read_until(fd, b"\x1b[7;1H\x1b#3\x1b[8;1H\x1b#4\x1b[7;1H")
        os.kill(pid, signal.SIGTERM)
        self.read_until(fd, b"\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(self.wait(pid), 0)
        self.assertTrue(termios.tcgetattr(fd)[3] & termios.ECHO)

    def test_a_wrong_call_is_refused(self):
        for args, why in ((["nonesuch"], "the screen 'nonesuch' is not one of wide, tall, edit or mixed"),
                          (["--cursor=7", "wide"], "--cursor takes a DECSCUSR style from 0 to 6"),
                          (["--cursor=bar", "wide"], "--cursor takes a DECSCUSR style from 0 to 6"),
                          ([], "typepad.py [--cursor=N] SCREEN")):
            with self.subTest(args=args):
                pid, fd = self.start(*args)
                self.read_until(fd, why.encode())
                self.assertEqual(self.wait(pid), 2)
        p = subprocess.run([sys.executable, "-I", TYPEPAD, "wide"], stdin=subprocess.DEVNULL,
                           capture_output=True, text=True, timeout=WAIT_BOUND)
        self.assertEqual(p.returncode, 2)
        self.assertIn("its input is not one", p.stderr)
        self.assertEqual(p.stdout, "")


if __name__ == "__main__":
    unittest.main()
