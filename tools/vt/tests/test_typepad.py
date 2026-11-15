#!/usr/bin/env python3
"""Tests the typing pad of the guided checks, tools/vt/checks/typepad.py:
what it sends for each key, how Backspace asks the terminal where the
cursor is, the edits of pair mode, insert mode and its key, the screens it
draws with the step's text above them, and a whole run on a
pseudo-terminal.

    python3 -I tools/vt/tests/test_typepad.py [-v] [TestCase.test_name ...]
"""

import fcntl
import importlib.util
import os
import pty
import re
import shlex
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
GUIDE = os.path.join(ROOT, "tools", "vt", "checks", "double-size.guide")
INSERT_GUIDE = os.path.join(ROOT, "tools", "vt", "checks", "insert.guide")

spec = importlib.util.spec_from_file_location("typepad", TYPEPAD)
typepad = importlib.util.module_from_spec(spec)
spec.loader.exec_module(typepad)


CPR = b"\x1b[6n"


class Fake:
    """A pad and a stand-in terminal for it. The terminal answers each CSI
    6 n with the next position of answers, after the read that asked it,
    as a terminal's answer arrives in a later read. When answers runs out,
    the question waits for the test to answer it with answer()."""

    def __init__(self, answers=(), floor=7, partner=None, state_at=None, irm=False):
        self.answers = list(answers)
        self.due = []
        self.out = []
        self.pad = typepad.Pad(self.write, floor, partner, state_at, irm)

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


# A tall pair on rows 9 and 10, as pair mode gets it.
PAIR = {9: 10, 10: 9}


def cup(row, col):
    return b"\x1b[%d;%dH" % (row, col)


class Pair(unittest.TestCase):
    """Pair mode: each edit on a line of a tall pair is made on the pair's
    other line too, at the same column, and the cursor ends where the edit
    alone leaves it."""

    def test_insert_delete_and_ctrl_k_on_a_pair_line_edit_both_lines(self):
        for key, edit in ((b"\x1b[2~", b"\x1b[@"), (b"\x1b[3~", b"\x1b[P"), (b"\x0b", b"\x1b[K")):
            for row, other in ((9, 10), (10, 9)):
                with self.subTest(key=key, row=row):
                    f = Fake([(row, 11)], partner=PAIR).feed(key)
                    self.assertEqual(f.sent(), CPR + cup(other, 11) + edit + cup(row, 11) + edit)

    def test_ctrl_l_on_a_pair_line_erases_both_lines_from_the_cursors_column(self):
        # ED 0 runs from the pair's bottom line, so the top line is erased
        # only from the cursor's column, by EL 0. ED makes a line it erases
        # whole single size, so ESC # 4 makes the bottom line the bottom of
        # the pair again.
        for col in (11, 1):
            with self.subTest(row=9, col=col):
                f = Fake([(9, col)], partner=PAIR).feed(b"\x0c")
                self.assertEqual(f.sent(), CPR + cup(10, col) + b"\x1b[J\x1b#4" + cup(9, col) + b"\x1b[K")
            with self.subTest(row=10, col=col):
                f = Fake([(10, col)], partner=PAIR).feed(b"\x0c")
                self.assertEqual(f.sent(), CPR + cup(9, col) + b"\x1b[K" + cup(10, col) + b"\x1b[J\x1b#4")
        # Off a pair, Ctrl-L is ED 0 alone.
        self.assertEqual(Fake([(8, 11)], partner=PAIR).feed(b"\x0c").sent(), CPR + b"\x1b[J")

    def test_a_character_on_a_pair_line_prints_on_both_lines(self):
        f = Fake([(9, 5), (9, 6)], partner=PAIR).feed(b"T")
        self.assertEqual(f.sent(), CPR + cup(10, 5) + b"T" + cup(9, 5) + b"T" + CPR)
        f = Fake([(10, 5), (10, 6)], partner=PAIR).feed("é".encode())
        self.assertEqual(f.sent(), CPR + cup(9, 5) + "é".encode() + cup(10, 5) + "é".encode() + CPR)

    def test_backspace_on_a_pair_line_erases_on_both_lines(self):
        f = Fake([(9, 11)], partner=PAIR).feed(b"\x7f")
        self.assertEqual(f.sent(), CPR + cup(10, 10) + b"\x1b[X" + cup(9, 10) + b"\x1b[X")
        # After a character that left a wrap pending, Backspace erases the
        # last column of both lines.
        f = Fake([(9, 40), (9, 40), (9, 40)], partner=PAIR).feed(b"z\x7f")
        self.assertTrue(f.sent().endswith(CPR + cup(10, 40) + b"\x1b[X" + cup(9, 40) + b"\x1b[X"), f.sent())
        # In the first column, Backspace does nothing.
        self.assertEqual(Fake([(10, 1)], partner=PAIR).feed(b"\x7f").sent(), CPR)

    def test_a_character_typed_with_a_wrap_pending_prints_once(self):
        f = Fake([(9, 40), (9, 40), (9, 40), (10, 2)], partner=PAIR).feed(b"y", b"z")
        self.assertEqual(f.sent(), CPR + cup(10, 40) + b"y" + cup(9, 40) + b"y" + CPR + CPR + b"z" + CPR)

    def test_off_a_pair_and_for_the_other_keys_the_pad_sends_the_plain_edit(self):
        self.assertEqual(Fake([(8, 11)], partner=PAIR).feed(b"\x1b[2~").sent(), CPR + b"\x1b[@")
        self.assertEqual(Fake([(8, 5), (8, 6)], partner=PAIR).feed(b"a").sent(), CPR + b"a" + CPR)
        self.assertEqual(Fake([(8, 5)], partner=PAIR).feed(b"\x7f").sent(), CPR + b"\b\x1b[X")
        for key, want in ((b"\x1b[A", b"\x1b[A"), (b"\r", b"\r\n"), (b"\x1b[15~", b"\x1b#5")):
            with self.subTest(key=key):
                self.assertEqual(Fake(partner=PAIR).feed(key).sent(), want)

    def test_keys_typed_before_the_answer_wait_in_pair_mode(self):
        f = Fake(partner=PAIR).feed(b"\x1b[2~ab")
        self.assertEqual(f.sent(), CPR)
        f.answer(9, 3)
        self.assertEqual(f.sent(), CPR + cup(10, 3) + b"\x1b[@" + cup(9, 3) + b"\x1b[@" + CPR)

    def test_pair_mode_finds_the_pairs_the_screen_draws(self):
        lines, _ = typepad.screen("edit", 80)
        self.assertEqual(typepad.pairs(lines, 7), {9: 10, 10: 9})
        self.assertEqual(typepad.pairs(typepad.screen("pair", 80)[0], 12), {12: 13, 13: 12})
        self.assertEqual(typepad.pairs(typepad.screen("wide", 80)[0], 7), {})
        # Pair mode's key list takes two rows more than the plain one.
        self.assertEqual(typepad.draw("pair", 80, 24, None, pair=True)[1:], (9, {9: 10, 10: 9}, None))
        self.assertEqual(typepad.draw("pair", 80, 24, None)[1:], (7, {}, None))


# The key list's insert mode word, at row 6, column 22, as the pad draws it
# on 80 columns with --insert and no step.
STATE = (6, 22)
INSERT_ON_WORD = b"\x1b7\x1b[6;22Hon \x1b8\x1b[4h"
INSERT_OFF_WORD = b"\x1b[4l\x1b7\x1b[6;22Hoff\x1b8"


class Insert(unittest.TestCase):
    """--insert: F2 turns the terminal's insert mode on and off, and the
    key list says which."""

    def test_f2_turns_insert_mode_on_and_off_and_writes_the_state(self):
        # F2 sends SS3 Q, or CSI 12 ~ from a VT220-style keyboard.
        for f2 in (b"\x1bOQ", b"\x1b[12~"):
            with self.subTest(key=f2):
                f = Fake(state_at=STATE, irm=True)
                self.assertEqual(f.feed(f2).sent(), INSERT_ON_WORD)
                self.assertTrue(f.pad.insert)
                self.assertEqual(f.feed(f2).sent(), INSERT_ON_WORD + INSERT_OFF_WORD)
                self.assertFalse(f.pad.insert)

    def test_the_word_goes_in_while_insert_mode_is_off(self):
        # The word comes before CSI 4 h and after CSI 4 l, so it replaces
        # the old word instead of pushing the key list right.
        self.assertLess(INSERT_ON_WORD.index(b"on "), INSERT_ON_WORD.index(b"\x1b[4h"))
        self.assertGreater(INSERT_OFF_WORD.index(b"off"), INSERT_OFF_WORD.index(b"\x1b[4l"))

    def test_without_the_word_f2_still_turns_insert_mode_on_and_off(self):
        f = Fake(irm=True)
        self.assertEqual(f.feed(b"\x1bOQ", b"\x1bOQ").sent(), b"\x1b[4h\x1b[4l")

    def test_without_insert_f2_does_nothing(self):
        f = Fake(state_at=STATE)
        self.assertEqual(f.feed(b"\x1bOQ", b"\x1b[12~").sent(), b"")
        self.assertFalse(f.pad.insert)

    def test_f2_keeps_the_pads_note_of_a_pending_wrap(self):
        # A character that left a wrap pending, then F2: DECRC brings the
        # pending wrap back, so Backspace still erases in the last column.
        f = Fake([(9, 80), (9, 80), (9, 80)], state_at=STATE, irm=True).feed(b"z", b"\x1bOQ", b"\x7f")
        self.assertEqual(f.sent(), CPR + b"z" + CPR + INSERT_ON_WORD + CPR + b"\x1b[D\x1b[C\x1b[X")

    def test_in_pair_mode_a_letter_in_insert_mode_goes_to_both_lines(self):
        f = Fake([(9, 5), (9, 6)], partner=PAIR, state_at=STATE, irm=True).feed(b"\x1bOQ", b"T")
        self.assertEqual(f.sent(), INSERT_ON_WORD + CPR + cup(10, 5) + b"T" + cup(9, 5) + b"T" + CPR)
        f = Fake([(10, 7), (10, 8)], partner=PAIR, irm=True).feed(b"\x1bOQ", b"T")
        self.assertEqual(f.sent(), b"\x1b[4h" + CPR + cup(9, 7) + b"T" + cup(10, 7) + b"T" + CPR)

    def test_the_key_list_says_insert_mode_is_off_where_the_word_goes(self):
        text = typepad.layout(80, 24, [], None, insert=True)
        self.assertEqual(text[5], "        Insert mode: off. F2 turns insert mode on and off.")
        self.assertEqual(typepad.state_position(text), STATE)
        row, col = STATE
        self.assertEqual(text[row - 1][col - 1:col + 2], "off")
        out, floor, _, state_at = typepad.draw("insert", 80, 24, None, insert=True)
        self.assertEqual((floor, state_at), (8, STATE))
        self.assertIn(b"\x1b[6;1H        Insert mode: off.", out)
        # In pair mode the word comes after pair mode's two rows.
        self.assertEqual(typepad.draw("insert", 80, 24, None, pair=True, insert=True)[3], (8, 22))
        # A screen too narrow to keep "Insert mode: off" on one row has no
        # word for F2 to write.
        self.assertIsNone(typepad.state_position(typepad.layout(12, 60, [], None, insert=True)))

    def test_the_insert_screen_holds_every_size_of_line(self):
        lines, cursor = typepad.screen("insert", 80)
        self.assertEqual([size for size, _ in lines], ["single", "wide", "single", "top", "bottom"])
        self.assertEqual([len(text) for _, text in lines], [80, 40, 0, 40, 40])
        self.assertEqual(cursor, (1, 11))
        self.assertEqual(lines[0][1][10], "k")
        self.assertEqual(typepad.pairs(lines, 8), {11: 12, 12: 11})


def guide_intro(guide):
    """A guide's intro: lines, as guide.sh reads them."""
    with open(guide, encoding="utf-8") as f:
        return [m.group(1) for m in (re.match(r"^intro:\s?(.*)$", line) for line in f.read().splitlines()) if m]


def guide_steps(guide=GUIDE):
    """The steps of a guide that runs the pad, double-size.guide unless
    another is named, as guide.sh reads them and hands them to the pad:
    (heading, do, expect, the pad's arguments)."""
    steps = []
    with open(guide, encoding="utf-8") as f:
        lines = f.read().splitlines()
    for line in lines:
        m = re.match(r"^([a-z]+):\s?(.*)$", line.rstrip("\n"))
        if not m:
            continue
        key, text = m.groups()
        if key == "step":
            steps.append({"title": text, "do": "", "expect": "", "run": ""})
        elif steps and key in ("do", "expect"):
            steps[-1][key] += text + "\n"
        elif steps and key == "run":
            steps[-1]["run"] = text
    return [("Step %d of %d: %s" % (i, len(steps), s["title"]), s["do"], s["expect"], shlex.split(s["run"])[2:])
            for i, s in enumerate(steps, 1)]


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
        lines, cursor = self.lines("pair")
        self.assertEqual(lines, [("top", lines[0][1]), ("bottom", lines[0][1])])
        self.assertEqual(lines[0][1], "abcdefghijklmnopqrstuvwxyz0123456789ABCD")
        self.assertEqual(cursor, (1, 11))
        lines, cursor = self.lines("mixed")
        self.assertEqual([s for s, _ in lines], ["single", "wide", "top", "bottom", "single"])
        self.assertEqual(lines[0][1], "1234567890" * 8)
        self.assertEqual(cursor, (1, 71))

    def test_a_double_size_line_never_holds_more_than_half_the_columns(self):
        for cols in (80, 81, 100, 41):
            for name in ("edit", "pair", "mixed"):
                lines, _ = self.lines(name, cols)
                for size, text in lines:
                    with self.subTest(cols=cols, name=name, size=size):
                        self.assertLessEqual(len(text), cols if size == "single" else cols // 2)

    def test_the_drawing_starts_below_the_key_list(self):
        out, floor, _, state_at = typepad.draw("tall", 80, 24, 6)
        # Five rows of keys on 80 columns, and a blank row.
        self.assertEqual(floor, 7)
        self.assertTrue(out.startswith(b"\x1b[H\x1b[2J\x1b[6 q\x1b[1;1HKeys:   Ctrl-D ends the typing."))
        self.assertIn(b"\x1b[4;1H        F3 to F6 change the size of the whole line the cursor is on", out)
        self.assertTrue(out.endswith(b"\x1b[7;1H\x1b#3\x1b[8;1H\x1b#4\x1b[7;1H"))
        # Without --insert the key list does not say whether insert mode is on.
        self.assertIsNone(state_at)
        self.assertNotIn(b"Insert mode:", out)
        # On 40 columns the key list takes twelve rows.
        self.assertEqual(typepad.draw("tall", 40, 24, None)[1], 14)

    def test_the_steps_text_comes_first_with_the_key_list_below(self):
        step = ("Step 2 of 9: Two", "Press the\nkey.\n", "One thing\nhappens.\nWrong: another.\n", "g.guide")
        out, floor, _, _ = typepad.draw("wide", 80, 25, None, step)
        self.assertEqual(typepad.layout(80, 25, typepad.screen("wide", 80)[0], step)[:5], [
            "Step 2 of 9: Two",
            "Do:     Press the key.",
            "Expect: One thing happens.",
            "        Wrong: another.",
            "",
        ])
        self.assertIn(b"\x1b[2;1HDo:     Press the key.\x1b[3;1HExpect: One thing happens.", out)
        self.assertIn(b"\x1b[6;1HKeys:   Ctrl-D", out)
        self.assertEqual(floor, 12)
        self.assertTrue(out.endswith(b"\x1b[12;1H\x1b#6\x1b[13;1H\x1b#6\x1b[12;1H"))

    def test_a_step_that_does_not_fit_keeps_its_do_lines_and_says_where_the_rest_is(self):
        lines = typepad.screen("edit", 80)[0]
        # Each "Wrong:" line starts a paragraph, so the expect: text takes
        # 30 rows.
        step = ("Step 1 of 1: T", "do one\ndo two\n", "\n".join("Wrong: %d." % i for i in range(30)), "g.guide")
        # 25 rows: four for the lines and one to type past them, and one for
        # the question, so 19 for the text. The key list and its blank row
        # take six, and the step's blank row one, so 12 rows hold the step.
        text = typepad.layout(80, 25, lines, step)
        self.assertEqual(len(text), 19)
        self.assertEqual(text[:2], ["Step 1 of 1: T", "Do:     do one do two"])
        self.assertEqual(text[2:4], ["Expect: Wrong: 0.", "        Wrong: 1."])
        self.assertEqual(text[10], "        Wrong: 8.")
        self.assertEqual(text[-8:-6], ["The rest of this step is in g.guide.", ""])
        self.assertEqual(text[-6], "Keys:   Ctrl-D ends the typing. Arrows and Home move. End goes to the right")
        # On a screen with room for less, the do: lines go first.
        text = typepad.layout(80, 16, lines, step)
        self.assertEqual(text[:4], ["Step 1 of 1: T", "Do:     do one do two",
                                    "The rest of this step is in g.guide.", ""])

    def test_every_row_fits_the_screens_width(self):
        step = ("Step 1 of 1: " + "t" * 100, "a " * 80, "b " * 80 + "\nWrong: " + "c" * 200, "g.guide")
        for cols in (80, 60, 41, 20, 12, 5):
            for pair in (False, True):
                with self.subTest(cols=cols, pair=pair):
                    for row in typepad.layout(cols, 60, [], step, pair):
                        self.assertLessEqual(len(row), cols, row)

    def test_every_step_of_the_guides_fits_80_by_25(self):
        for guide, count in ((GUIDE, 14), (INSERT_GUIDE, 5)):
            steps = guide_steps(guide)
            self.assertEqual(len(steps), count, guide)
            for heading, do, expect, args in steps:
                with self.subTest(guide=os.path.basename(guide), step=heading):
                    name = [a for a in args if not a.startswith("--")]
                    self.assertEqual(len(name), 1, args)
                    lines, _ = typepad.screen(name[0], 80)
                    step = (heading, do, expect, guide)
                    text = typepad.layout(80, 25, lines, step, "--pair" in args, "--insert" in args)
                    self.assertFalse(any(r.startswith("The rest of this step") for r in text), text)
                    self.assertEqual(text[0], heading)
                    # The lines, a row for typing past them and the
                    # question's row fit below the text.
                    self.assertLessEqual(len(text) + len(lines) + 2, 25)

    def test_each_guides_intro_says_the_terminal_never_moves_text_to_another_row(self):
        # The maintainer read a letter lost at the right edge as a fault,
        # because a shell's line editor makes text flow onto the next row.
        # Each guide says at the start who makes the text flow.
        for guide in (GUIDE, INSERT_GUIDE):
            with self.subTest(guide=os.path.basename(guide)):
                intro = " ".join(guide_intro(guide))
                self.assertIn("A terminal never moves text to another row when you insert a letter.", intro)
                self.assertIn("Shells and editors such as zsh and vim redraw the line themselves, and that redraw "
                              "is what makes text flow onto the next row.", intro)

    def test_each_guide_shows_a_rule_that_looks_broken_in_one_step_that_quotes_the_manual(self):
        # A letter lost at the right edge and one line of a tall pair
        # edited alone look broken, and tier 1 checks both. Each guide
        # shows one of them once, in a step that quotes the manual's
        # sentence and says that tier 1 checks it.
        for guide in (GUIDE, INSERT_GUIDE):
            with self.subTest(guide=os.path.basename(guide)):
                expects = [(s[0], " ".join(s[2].split())) for s in guide_steps(guide)]
                shown = [e for e in expects if "Tier 1 checks" in e[1]]
                self.assertEqual(len(shown), 1, [e[0] for e in shown])
                self.assertRegex(shown[0][1], r'(VT510 manual|VT100 User Guide) says[^"]*: "[^"]+"')

    def test_every_step_of_the_insert_guide_runs_the_pad_with_insert(self):
        for heading, _, _, args in guide_steps(INSERT_GUIDE):
            with self.subTest(step=heading):
                self.assertIn("--insert", args)

    def test_the_step_comes_from_guide_shs_environment(self):
        self.assertIsNone(typepad.step_text({}))
        env = {"GUIDE_STEP_HEADING": "Step 1 of 2: T", "GUIDE_STEP_DO": "d\n", "GUIDE_STEP_EXPECT": "e\n",
               "GUIDE_FILE": "x.guide"}
        self.assertEqual(typepad.step_text(env), ("Step 1 of 2: T", "d\n", "e\n", "x.guide"))

    def test_the_end_puts_the_cursor_on_a_clean_single_line_at_the_bottom(self):
        # Insert mode goes off first, whether or not F2 turned it on.
        self.assertEqual(typepad.finish(24, None), b"\x1b[4l\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(typepad.finish(24, 6), b"\x1b[4l\x1b[24;1H\x1b#5\x1b[2K\x1b[0 q")


class Run(unittest.TestCase):
    """typepad.py itself, on a pseudo-terminal of 80 columns and 24 rows."""

    def start(self, *args, env=None):
        # The pad gets the environment of guide.sh's step, or none of it.
        env = {k: v for k, v in os.environ.items() if not k.startswith("GUIDE_")} | (env or {})
        pid, fd = pty.fork()
        if pid == 0:
            os.execve(sys.executable, [sys.executable, "-I", TYPEPAD, *args], env)
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

    def test_a_run_shows_the_step_and_edits_both_lines_of_a_pair(self):
        env = {"GUIDE_STEP_HEADING": "Step 6 of 14: Pairs", "GUIDE_STEP_DO": "Press Insert.\n",
               "GUIDE_STEP_EXPECT": "Both halves move.\n", "GUIDE_FILE": "tools/vt/checks/double-size.guide"}
        pid, fd = self.start("--pair", "pair", env=env)
        out = self.read_until(fd, b"\x1b[14;1H\x1b#4abcdefghijklmnopqrstuvwxyz0123456789ABCD\x1b[13;11H")
        self.assertIn(b"\x1b[1;1HStep 6 of 14: Pairs\x1b[2;1HDo:     Press Insert.\x1b[3;1HExpect: Both halves move.", out)
        self.assertIn(b"Pair mode: on a tall line", out)
        os.write(fd, b"\x1b[2~")
        self.read_until(fd, b"\x1b[6n")
        os.write(fd, b"\x1b[13;11R")
        self.read_until(fd, b"\x1b[14;11H\x1b[@\x1b[13;11H\x1b[@")
        os.write(fd, b"\x04")
        self.read_until(fd, b"\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(self.wait(pid), 0)

    def test_a_run_with_insert_turns_insert_mode_on_and_off_at_the_end(self):
        pid, fd = self.start("--insert", "insert")
        # The pad has drawn its screen once the cursor goes to the k of
        # the first line.
        out = self.read_until(fd, b"\x1b[12;1H\x1b#4abcdefghijklmnopqrstuvwxyz0123456789ABCD\x1b[8;11H")
        self.assertIn(b"\x1b[6;1H        Insert mode: off. F2 turns insert mode on and off.", out)
        os.write(fd, b"\x1bOQ")
        self.read_until(fd, b"\x1b7\x1b[6;22Hon \x1b8\x1b[4h")
        os.write(fd, b"\x04")
        self.read_until(fd, b"\x1b[4l\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(self.wait(pid), 0)

    def test_a_hangup_ends_the_pad_with_the_terminal_put_back(self):
        pid, fd = self.start("tall")
        self.read_until(fd, b"\x1b[7;1H\x1b#3\x1b[8;1H\x1b#4\x1b[7;1H")
        os.kill(pid, signal.SIGTERM)
        self.read_until(fd, b"\x1b[24;1H\x1b#5\x1b[2K")
        self.assertEqual(self.wait(pid), 0)
        self.assertTrue(termios.tcgetattr(fd)[3] & termios.ECHO)

    def test_a_wrong_call_is_refused(self):
        for args, why in ((["nonesuch"], "the screen 'nonesuch' is not one of wide, tall, edit, pair, mixed or insert"),
                          (["--cursor=7", "wide"], "--cursor takes a DECSCUSR style from 0 to 6"),
                          (["--cursor=bar", "wide"], "--cursor takes a DECSCUSR style from 0 to 6"),
                          ([], "typepad.py [--cursor=N] [--pair] [--insert] SCREEN")):
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
