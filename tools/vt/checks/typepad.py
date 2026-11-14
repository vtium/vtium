#!/usr/bin/env python3
"""A typing pad for the guided checks (guide.sh): draws a screen of lines of
double and single size, then echoes the keys typed on it as the escape
sequences a line editor sends, so the terminal's own rules for each line
decide where the text and the cursor go. Ctrl-D (or Ctrl-C) ends it.

  typepad.py [--cursor=N] [--pair] SCREEN

SCREEN is one of:
  wide    two empty double-width lines
  tall    an empty double-height pair, its top line and its bottom line
  edit    a double-width line and a double-height pair, full of letters
  pair    a double-height pair full of letters
  mixed   a full single-width ruler, a double-width line, a double-height
          pair and a single-width line
--cursor=N sets the cursor's style with DECSCUSR N (1 blinking block, 4
steady underline, 6 steady bar, for example) while the pad runs, and puts
back the configured style (DECSCUSR 0) when the pad ends.
--pair makes the pad edit both lines of each double-height pair it drew,
as a program written for the VT510 does (below).

What each key sends:
  a printable character   itself
  Enter                   CR LF
  Tab                     HT
  the arrows              CUU, CUD, CUF and CUB, one column or row
  Home and End            CR, and CUF 999, which stops at the line's last
                          column
  Insert and Delete       ICH and DCH, one character
  Ctrl-K and Ctrl-L       EL 0 and ED 0, to the end of the line or screen
  F3, F4, F5 and F6       ESC # 3, ESC # 4, ESC # 5 and ESC # 6, which set
                          the size of the whole line the cursor is on
  Backspace               BS and ECH, which erase the character left of
                          the cursor. At the first column of a line the
                          pad moves up a line and to that line's last
                          column (CUU, CUF 999) and erases there, as a line
                          editor does when it goes back across a wrap.
Any other key does nothing.

The screen's top shows the step that guide.sh runs the pad for: its
heading, its do: lines and its expect: lines, which guide.sh hands a run:
command in GUIDE_STEP_HEADING, GUIDE_STEP_DO and GUIDE_STEP_EXPECT. The key
list comes below them, and the screen's lines below the key list. The pad
lays all of this out for the terminal's size, which it reads from the
terminal when it starts. When the step's text does not fit above the lines,
the do: lines come first, and the last line that fits says that the rest is
in the guide, GUIDE_FILE. Run by hand, with none of these set, the pad shows
the key list alone.

The VT510 edits each line of a double-height pair on its own: Insert on the
top line moves only the top halves of the letters. A program that shows tall
text therefore makes each edit twice, once on each line. With --pair the pad
does that for typing, Backspace, Insert, Delete and Ctrl-K on a line of a
pair it drew. It asks where the cursor is, makes the edit on the pair's
other line at the same column (CUP there, the edit, CUP back), and then
makes the edit on the cursor's own line. The cursor ends where the plain
edit leaves it. Two edits stay single in pair mode. A character typed while
a wrap is pending goes to the next line, as usual. Backspace in the first
column of a pair's line does nothing, because the line above is not the
line that the text wrapped from.

Why the pad and not cat under stty raw: cat writes back what a key sends,
and a terminal ignores most of that. Backspace sends DEL or BS, and BS stops
at the first column, so the cursor never goes back across a wrap. Insert,
Delete and the function keys send sequences that mean nothing as output. A
shell's line editor moves the cursor by the screen's full width, because it
does not know that a line of double size holds only half the columns, so a
shell prompt cannot show the terminal's own rules either.

The pad asks the terminal where the cursor is (CPR, CSI 6 n), because only
the terminal knows how wide each line is. It asks before each Backspace, and
before and after each character it prints. A character printed in a line's
last column leaves the cursor in that column with a wrap pending, and the
next character goes to the next line. CPR reports the same column with or
without a pending wrap, so the pad notes the wrap itself: the cursor did not
move when the character was printed. Backspace then erases the character in
the cursor's own column. Keys that arrive before an answer wait for it, so
they act in the order they were typed. The pad never moves above the first
line it drew, so Backspace leaves the step's text and the key list alone.
The pad uses python3, which guide.sh needs for its probes anyway, so it runs
on every machine where the guides run.
"""
import os
import signal
import sys
import termios
import textwrap
import tty

ESC = b"\033"
CSI = b"\033["

# The key list, as paragraphs that the pad wraps to the screen's width.
KEYS = (
    "Ctrl-D ends the typing. Arrows, Home and End move. Backspace erases. Insert adds a blank. "
    "Delete removes a letter. Ctrl-K erases to the end of the line, and Ctrl-L to the end of the "
    "screen.",
    "F3 to F6 change the size of the whole line the cursor is on, letters already typed included: "
    "F3 tall top, F4 tall bottom, F5 normal, F6 wide.",
)
PAIR_KEYS = (
    "Pair mode: on a tall line, the pad sends typing, Backspace, Insert, Delete and Ctrl-K to "
    "both lines of the pair.",
)
# The width of the labels that start the step's text and the key list, as
# guide.sh prints them.
LABEL = 8

LETTERS = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"

# What the line-size sequences are called in the screens below.
SIZE = {"single": b"", "wide": ESC + b"#6", "top": ESC + b"#3", "bottom": ESC + b"#4"}

# The cursor keys, as CSI or SS3 finals, and what the pad sends for each.
END = CSI + b"999C"
MOVES = {b"A": CSI + b"A", b"B": CSI + b"B", b"C": CSI + b"C", b"D": CSI + b"D", b"H": b"\r", b"F": END}
INSERT = CSI + b"@"
DELETE = CSI + b"P"
ERASE_LINE = CSI + b"K"
# The keys that send CSI number ~.
TILDE = {
    b"1": b"\r", b"7": b"\r", b"4": END, b"8": END,
    b"2": INSERT, b"3": DELETE,
    b"13": ESC + b"#3", b"14": ESC + b"#4", b"15": ESC + b"#5", b"17": ESC + b"#6",
}
# F3 and F4 send SS3 R and SS3 S.
SS3_KEYS = {b"R": ESC + b"#3", b"S": ESC + b"#4"}
# The control keys the pad acts on. Ctrl-D and Ctrl-C end it.
CONTROLS = {0x0D: b"\r\n", 0x0A: b"\r\n", 0x09: b"\t", 0x0B: ERASE_LINE, 0x0C: CSI + b"J"}
QUIT = (0x04, 0x03)
BACKSPACE = (0x7F, 0x08)
# The edits that pair mode makes on both lines of a tall pair.
PAIRED = (INSERT, DELETE, ERASE_LINE)
# A parameter string longer than this is no key's, so the pad drops the
# sequence.
MAX_PARAMS = 16


def screen(name, cols):
    """The lines of a screen, as (size, text) from its first line down, and
    the row and column, counted from 1 from the screen's first line, where
    the cursor starts."""
    half = max(1, cols // 2)
    letters = (LETTERS * (cols // len(LETTERS) + 1))[:half]
    if name == "wide":
        return [("wide", ""), ("wide", "")], (1, 1)
    if name == "tall":
        return [("top", ""), ("bottom", "")], (1, 1)
    if name == "edit":
        return [("wide", letters), ("single", ""), ("top", letters), ("bottom", letters)], (1, min(11, half))
    if name == "pair":
        return [("top", letters), ("bottom", letters)], (1, min(11, half))
    if name == "mixed":
        ruler = ("1234567890" * (cols // 10 + 1))[:cols]
        return [
            ("single", ruler),
            ("wide", "Double width: the cursor stops here"[:half]),
            ("top", "Double height"[:half]),
            ("bottom", "Double height"[:half]),
            ("single", "Single width again, with the whole width to type in"[:cols]),
        ], (1, max(1, cols - 9))
    raise ValueError(name)


def pairs(lines, floor):
    """The double-height pairs among a screen's lines: each pair's two
    screen rows, counted from 1, each mapped to the other."""
    partner = {}
    for i in range(len(lines) - 1):
        if lines[i][0] == "top" and lines[i + 1][0] == "bottom":
            partner[floor + i] = floor + i + 1
            partner[floor + i + 1] = floor + i
    return partner


def paragraphs(text):
    """A guide's do: or expect: lines as paragraphs. The guide wraps its
    lines by hand, so the pad joins them, and starts a new paragraph at a
    line that begins with "Wrong:"."""
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        if out and not line.startswith("Wrong:"):
            out[-1] += " " + line
        else:
            out.append(line)
    return out


def labelled(label, paras, cols):
    """Paragraphs wrapped to the screen's width, after a label as guide.sh
    prints it: the label on the first line, and the others indented as far.
    A screen too narrow for the label gets the text alone."""
    indent = LABEL if cols > 2 * LABEL else 0
    width = max(1, cols - indent)
    rows = []
    for para in paras:
        for line in textwrap.wrap(para, width) or [""]:
            rows.append(" " * indent + line)
    if rows and indent:
        rows[0] = (label + ":").ljust(indent) + rows[0][indent:]
    return rows


def layout(cols, rows, lines, step=None, pair=False):
    """The rows of text above a screen's lines, from the top of the
    screen: the step's heading, its do: and expect: lines and the key list,
    with a blank row after the step and after the key list. step is
    (heading, do, expect, where) or None, where being the file that holds
    the step. The screen's lines get the rows below, one more than they
    take, so the text typed past them has a row, and the bottom row stays
    free for guide.sh's question. When the step's text does not fit, the
    do: lines come first, and the last row that fits says where the rest
    is."""
    keys = labelled("Keys", KEYS + (PAIR_KEYS if pair else ()), cols) + [""]
    if not step:
        return keys
    heading, do, expect, where = step
    text = [heading[:cols]] + labelled("Do", paragraphs(do), cols) + labelled("Expect", paragraphs(expect), cols)
    room = rows - len(keys) - 1 - (len(lines) + 1) - 1
    if len(text) > room:
        more = "The rest of this step is in %s." % (where or "the guide")
        text = text[: max(0, room - 1)] + textwrap.wrap(more, cols)[:1] if room > 0 else []
    return text + ([""] if text else []) + keys


class Parser:
    """Splits the bytes a terminal sends for keys into events: ("char", c)
    for a printable character, its whole UTF-8 encoding, ("byte", b) for a
    control byte, ("csi", params, final) and ("ss3", final). A lone ESC, as
    from the Esc key or from Alt with a key, is dropped, and so is a byte
    that is not part of valid UTF-8. A control byte inside a sequence ends
    the sequence and counts on its own, so Ctrl-D always reaches the pad.
    The state carries across reads, since one read may end in the middle
    of a sequence."""

    def __init__(self):
        self.state = "ground"
        self.params = b""
        self.utf8 = b""
        self.utf8_left = 0

    def feed(self, data):
        events = []
        for b in data:
            self.step(b, events)
        return events

    def step(self, b, events):
        if self.state == "esc":
            if b == 0x5B:
                self.state, self.params = "csi", b""
                return
            if b == 0x4F:
                self.state = "ss3"
                return
            self.state = "ground"
        elif self.state == "csi":
            if 0x20 <= b <= 0x3F:
                # A sequence with more parameters than any key sends is
                # read to its end and dropped.
                self.params = (self.params + bytes([b]))[: MAX_PARAMS + 1]
                return
            self.state = "ground"
            if 0x40 <= b <= 0x7E:
                if len(self.params) <= MAX_PARAMS:
                    events.append(("csi", self.params, bytes([b])))
                return
            if b >= 0x20:
                return
        elif self.state == "ss3":
            self.state = "ground"
            if b >= 0x20:
                events.append(("ss3", bytes([b])))
                return
        if self.utf8_left:
            if 0x80 <= b <= 0xBF:
                self.utf8 += bytes([b])
                self.utf8_left -= 1
                if not self.utf8_left:
                    events.append(("char", self.utf8))
                return
            self.utf8_left = 0
        if b == 0x1B:
            self.state = "esc"
        elif b < 0x20 or b == 0x7F:
            events.append(("byte", b))
        elif b < 0x80:
            events.append(("char", bytes([b])))
        elif 0xC2 <= b <= 0xF4:
            self.utf8 = bytes([b])
            self.utf8_left = 1 if b < 0xE0 else 2 if b < 0xF0 else 3


class Done(Exception):
    """Ctrl-D or Ctrl-C: the pad ends."""


class Pad:
    """What the pad sends for the keys it reads. write is called with each
    piece of output. floor is the first row the pad drew, which Backspace
    never moves above. partner maps each row of a double-height pair to the
    pair's other row, in pair mode, and is empty otherwise."""

    def __init__(self, write, floor, partner=None):
        self.write = write
        self.floor = floor
        self.partner = partner or {}
        self.parser = Parser()
        # What to do with each answer to CSI 6 n still to come, in order,
        # and the keys read since the first of them was asked, which wait.
        self.asked = []
        self.held = []
        # Where the cursor stood before the last character printed, and
        # whether that character left a wrap pending.
        self.before = None
        self.pending = False

    def feed(self, data):
        for event in self.parser.feed(data):
            self.event(event)

    def event(self, event):
        if event[0] == "byte" and event[1] in QUIT:
            raise Done
        if self.asked:
            answer = self.report(event)
            if answer is None:
                self.held.append(event)
                return
            self.asked.pop(0)(answer)
            if not self.asked:
                held, self.held = self.held, []
                for e in held:
                    self.event(e)
            return
        self.act(event)

    def ask(self, then):
        self.asked.append(then)
        self.write(CSI + b"6n")

    @staticmethod
    def report(event):
        """The row and column of a CPR answer, CSI row ; column R, or None."""
        if event[0] != "csi" or event[2] != b"R":
            return None
        parts = event[1].split(b";")
        if len(parts) != 2 or not all(p.isdigit() for p in parts):
            return None
        return int(parts[0]), int(parts[1])

    def send(self, out):
        """Writes what a key sends, other than a character. Each of these
        moves the cursor or edits the line, so no wrap is pending after it
        as far as Backspace is concerned."""
        self.pending = False
        self.write(out)

    def act(self, event):
        kind = event[0]
        if kind == "char":
            if self.partner:
                self.ask(lambda position: self.paired_char(position, event[1]))
            else:
                self.ask(self.note_before)
                self.write(event[1])
                self.ask(self.note_after)
        elif kind == "byte":
            b = event[1]
            if b in BACKSPACE:
                self.ask(self.backspace)
            elif b in CONTROLS:
                self.edit(CONTROLS[b])
        elif kind == "csi":
            params, final = event[1], event[2]
            if final == b"~":
                out = TILDE.get(params)
            elif params == b"":
                out = MOVES.get(final)
            else:
                out = None
            if out:
                self.edit(out)
        elif kind == "ss3":
            out = MOVES.get(event[1]) or SS3_KEYS.get(event[1])
            if out:
                self.send(out)

    def edit(self, out):
        """Sends a key's sequence. In pair mode, Insert, Delete and Ctrl-K
        first ask where the cursor is, so that the pad can make the same
        edit on both lines of a pair."""
        if self.partner and out in PAIRED:
            self.ask(lambda position: self.send(self.both(position, out)))
        else:
            self.send(out)

    def both(self, position, out, col=None):
        """out made on the other line of the cursor's pair at column col
        (the cursor's own when None), then on the cursor's own line. CUP
        takes the cursor to the other line and back, so the cursor ends
        where out alone leaves it. Off a pair, out alone."""
        row, own = position
        other = self.partner.get(row)
        if other is None:
            return out
        col = own if col is None else col
        return CSI + b"%d;%dH" % (other, col) + out + CSI + b"%d;%dH" % (row, col) + out

    def paired_char(self, position, char):
        """Prints a character in pair mode, once the terminal has said where
        the cursor stood. On a pair's line the pad prints the character on
        the other line first. With a wrap pending, the character goes to
        the next line, so the pad prints it once, as usual."""
        self.note_before(position)
        self.write(char if self.pending else self.both(position, char))
        self.ask(self.note_after)

    def note_before(self, position):
        self.before = position

    def note_after(self, position):
        self.pending = position == self.before

    def backspace(self, position):
        """Erases the character left of the cursor, which stood at row and
        column (from 1) when the terminal answered. With a wrap pending, that
        character is in the cursor's own column, the line's last. BS from a
        pending wrap goes one column left of it, so the pad steps left and
        right again (CUB, CUF), which leaves the cursor on the last column
        with no wrap pending, and erases there."""
        row, col = position
        if row in self.partner:
            # In pair mode, the pad erases on both lines at the column that
            # ECH erases on the cursor's line, and CUP leaves the cursor
            # there. At the first column it does nothing.
            if self.pending:
                self.send(self.both(position, CSI + b"X"))
            elif col > 1:
                self.send(self.both(position, CSI + b"X", col - 1))
        elif self.pending:
            self.send(CSI + b"D" + CSI + b"C" + CSI + b"X")
        elif col > 1:
            self.send(b"\b" + CSI + b"X")
        elif row > self.floor:
            self.send(CSI + b"A" + END + CSI + b"X")


def draw(name, cols, rows, cursor_style, step=None, pair=False):
    """The bytes that clear the screen, set the cursor's style, write the
    step's text and the key list and draw the screen, the first row of the
    screen's lines, and the screen's double-height pairs (in pair mode)."""
    lines, (row, col) = screen(name, cols)
    text = layout(cols, rows, lines, step, pair)
    floor = len(text) + 1
    out = CSI + b"H" + CSI + b"2J"
    if cursor_style is not None:
        out += CSI + b"%d q" % cursor_style
    for i, line in enumerate(text):
        # Each row goes to its place, so a row as wide as the screen leaves
        # no wrap pending for the next.
        if line:
            out += CSI + b"%d;1H" % (i + 1) + line.encode()
    for i, (size, chars) in enumerate(lines):
        out += CSI + b"%d;1H" % (floor + i) + SIZE[size] + chars.encode()
    out += CSI + b"%d;%dH" % (floor + row - 1, col)
    return out, floor, pairs(lines, floor) if pair else {}


def step_text(env):
    """The step guide.sh runs the pad for, from its environment, as layout
    takes it, or None when no step is given."""
    heading = env.get("GUIDE_STEP_HEADING", "")
    do = env.get("GUIDE_STEP_DO", "")
    expect = env.get("GUIDE_STEP_EXPECT", "")
    if not (heading or do or expect):
        return None
    return heading, do, expect, env.get("GUIDE_FILE", "")


def finish(rows, cursor_style):
    """The bytes that leave the screen as the pad drew it and put the
    cursor on a clean line of single size at the bottom, for guide.sh's
    question, with the configured cursor style back."""
    out = CSI + b"%d;1H" % rows + ESC + b"#5" + CSI + b"2K"
    if cursor_style is not None:
        out += CSI + b"0 q"
    return out


def usage():
    print(__doc__.split("\n\n")[1], file=sys.stderr)
    return 2


def main(argv):
    cursor_style = None
    pair = False
    args = []
    for a in argv:
        if a == "--pair":
            pair = True
        elif a.startswith("--cursor="):
            value = a.split("=", 1)[1]
            if not value.isdigit() or int(value) > 6:
                print("typepad: --cursor takes a DECSCUSR style from 0 to 6", file=sys.stderr)
                return 2
            cursor_style = int(value)
        else:
            args.append(a)
    if len(args) != 1:
        return usage()
    fd = sys.stdin.fileno()
    if not os.isatty(fd):
        print("typepad: the pad reads keys from a terminal, and its input is not one", file=sys.stderr)
        return 2
    cols, rows = os.get_terminal_size(fd)
    try:
        out, floor, partner = draw(args[0], cols, rows, cursor_style, step_text(os.environ), pair)
    except ValueError:
        print("typepad: the screen '%s' is not one of wide, tall, edit, pair or mixed" % args[0], file=sys.stderr)
        return 2

    def write(data):
        os.write(1, data)

    # A hangup or a kill ends the pad through the finally below, so the
    # terminal gets its settings back.
    def stop(signum, frame):
        raise Done

    signal.signal(signal.SIGHUP, stop)
    signal.signal(signal.SIGTERM, stop)
    saved = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        write(out)
        pad = Pad(write, floor, partner)
        while True:
            data = os.read(fd, 1024)
            # An empty read is the terminal gone: nothing more will come.
            if not data:
                break
            pad.feed(data)
    except Done:
        pass
    finally:
        # A terminal that is gone takes neither the last bytes nor the
        # settings, and the pad ends all the same.
        try:
            write(finish(rows, cursor_style))
            # TCSAFLUSH drops keys typed after Ctrl-D, so none answers
            # guide.sh's question.
            termios.tcsetattr(fd, termios.TCSAFLUSH, saved)
        except (OSError, termios.error):
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
