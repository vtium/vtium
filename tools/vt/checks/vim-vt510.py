#!/usr/bin/env python3
"""vim-vt510, a small editor for the guided checks (guide.sh). It holds the
text in memory and wraps each line itself, so you can see what an editor
does on top of a VT510 terminal. It is a test fixture: nothing installs it.

  vim-vt510.py [--prepend-step] [FILE]

Why it exists: the terminal follows the VT510. It never moves text to
another row when a letter is inserted, and letters pushed past the right
edge are lost. The typing pad (typepad.py) shows that raw behaviour. A shell
like zsh or bash, and an editor like vim, keep the text themselves and
redraw it, so their text flows onto the next row. This editor does the same,
in a few hundred lines, on lines of every size.

The buffer is a list of lines. Each line has a size and its text:
  normal  a line of the screen's width
  wide    a double-width line (ESC # 6), half the screen's width in columns
  tall    a double-height line, drawn as a pair of rows: the top halves
          (ESC # 3) and the bottom halves (ESC # 4), both rows written with
          the same text, half the screen's width in columns
A line longer than its width wraps onto continuation rows, and each
continuation row has the line's size. A wide character (CJK, emoji) takes
two of the line's columns and never splits across a wrap. A line that fills
its last row exactly gets an empty continuation row, where the cursor goes
after the last character.

The file is plain text. A line that starts with "[wide] " or "[tall] " has
that size, and the tag is not part of the text. Any other line is normal. A
normal line whose text starts with a tag is saved with "[normal] " in front,
so that it reads back as normal.

--prepend-step writes the step that guide.sh runs the editor for at the top
of FILE before the editor opens it. guide.sh hands a run: command the step's
heading, do: lines and expect: lines in GUIDE_STEP_HEADING, GUIDE_STEP_DO
and GUIDE_STEP_EXPECT. The heading becomes the file's first line. The do:
lines become one line that starts "Do:", and the expect: lines one that
starts "Expect:", except that an expect: line that starts with "Wrong:"
begins a line of its own. The editor wraps these long lines like any other.
The asterisks that mark bold text on guide.sh's screen are left out, since
the file is plain text. A blank line follows the step's text, then the
file's own text. With none of the three set, the file stays as it is.

Normal mode:
  arrows, h j k l    move through the text: Left and Right along the line,
                     Up and Down to the line above or below
  Home, End          the start or the end of the line
  PageUp, PageDown   a screen of text back or forward
  i, a               insert before or after the cursor
  o, O               open a new normal line below or above and insert there
  x                  delete the character under the cursor
  dd                 delete the line
  :                  a command, typed on the bottom row
  Ctrl-L             draw the whole screen again
Insert mode: typing inserts at the cursor. Enter splits the line, and both
parts keep its size. Backspace deletes the character before the cursor, and
at the start of a line joins the line to the one above. Delete deletes the
character under the cursor, and at the end of a line joins the next line.
The arrows, Home, End, PageUp and PageDown move as in normal mode. Esc goes
back to normal mode.
Commands:
  :e FILE   edit FILE (:e! FILE drops unsaved changes)
  :w [FILE] save, to FILE when given
  :q        quit; refused while there are unsaved changes
  :q!       quit and drop unsaved changes
  :wq       save and quit
  :wide, :tall, :normal   set the current line's size

The screen: the text fills every row but the last two. The second-to-last
row is the status line, in reverse video: the file name, the mode, a
reminder that the editor wraps lines, and the cursor's line and column. The
last row shows the ':' prompt and the editor's messages. The editor sets the
scroll region (DECSTBM) to the text's rows and turns autowrap (DECAWM) off,
so nothing it writes can scroll or wrap the two bottom rows.

How it draws: the editor keeps a copy of what each row of the screen shows,
its size and its text. After each key it works out what each row should
show from the buffer, and rewrites only the rows that differ. A row whose
size changes is written again whole. On a row of the same size, an insert
in the middle uses ICH and a delete uses DCH, so the terminal moves the rest
of the row and the editor writes only the new characters and what flows in
from the next row. When the view moves by a few rows, the editor scrolls
the text's rows with SU or SD and draws the rows that come into view.
Correctness comes from the buffer: whatever the terminal moved, each row
ends up as the buffer says.

The terminal's size comes from the pty (TIOCGWINSZ), and a SIGWINCH draws
the screen again at the new size. On exit the editor makes every row it
sized single again, resets the scroll region, insert mode (IRM), autowrap
and the attributes, clears the screen and leaves the cursor at the start of
the bottom row.
"""
import os
import select
import signal
import sys
import termios
import tty
import unicodedata

ESC = "\033"
CSI = ESC + "["

SIZES = ("normal", "wide", "tall")
# The tags of the file format, and the tag that keeps a normal line normal
# when its text starts with one of the others.
TAGS = {"wide": "[wide] ", "tall": "[tall] "}
NORMAL_TAG = "[normal] "
# The line-size sequence of each kind of screen row: s single, w double
# width, T and B the top and bottom halves of a double-height pair.
ROW_SIZE = {"s": ESC + "#5", "w": ESC + "#6", "T": ESC + "#3", "B": ESC + "#4"}
# The longest the editor waits after a lone ESC for the rest of a key's
# sequence before it takes the ESC as the Esc key. It bounds the wait only;
# a sequence normally arrives in one read.
ESC_WAIT = 0.05
# The rows the view may move by with SU or SD instead of drawing every row.
SCROLL_MAX = 4

STATUS_NOTES = (
    "vim-vt510 — the editor wraps lines; the terminal does not",
    "the editor wraps lines; the terminal does not",
    "the editor wraps lines",
)
WELCOME = ":q quits, :w saves, i inserts, and Esc goes back to normal mode."


# The text: characters, clusters and widths.

def zero_width(ch):
    """True for a character that joins the character before it in one
    cell: a combining mark, a variation selector or a zero-width joiner."""
    return unicodedata.combining(ch) > 0 or unicodedata.category(ch) in ("Mn", "Me", "Cf")


def clusters(text):
    """TEXT as a list of clusters, each a base character with the
    zero-width characters that follow it."""
    out = []
    for ch in text:
        if out and zero_width(ch):
            out[-1] += ch
        else:
            out.append(ch)
    return out


def width(cluster):
    """The columns a cluster takes: two for a wide (W) or full-width (F)
    character and for an emoji presentation (U+FE0F), one otherwise."""
    if unicodedata.east_asian_width(cluster[0]) in ("W", "F") or "️" in cluster:
        return 2
    return 1


def shown(cluster):
    """The cluster as the editor writes it: a control character, which the
    terminal would act on, shows as a question mark."""
    return "?" if unicodedata.category(cluster[0]) == "Cc" else cluster


def text_width(cells):
    return sum(width(c) for c in cells)


class Line:
    """One line of the buffer: its size and its text as clusters."""

    def __init__(self, size="normal", text=""):
        self.size = size
        self.cells = clusters(text)

    @property
    def text(self):
        return "".join(self.cells)

    def __eq__(self, other):
        return isinstance(other, Line) and (self.size, self.cells) == (other.size, other.cells)

    def __repr__(self):
        return "Line(%r, %r)" % (self.size, self.text)


def line_width(size, cols):
    """The columns of a line of SIZE on a screen COLS wide: the screen's
    width for a normal line, half of it for a wide or tall line, whose
    characters each take two of the screen's columns."""
    return cols if size == "normal" else max(1, cols // 2)


def layout(cells, cols):
    """Splits a line's clusters into the rows it takes at COLS columns, as
    (start, end) ranges of cluster indexes. A cluster that does not fit on
    a row starts the next one, so a wide character never splits. A line
    that fills its last row exactly gets an empty row after it, where the
    cursor stands after the last character. An empty line takes one row."""
    rows = []
    start = 0
    col = 0
    for i, c in enumerate(cells):
        w = width(c)
        if col + w > cols and i > start:
            rows.append((start, i))
            start = i
            col = 0
        col += w
    rows.append((start, len(cells)))
    if cells and col >= cols:
        rows.append((len(cells), len(cells)))
    return rows


def place(cells, cols, index):
    """The row (from 0) of the line's layout and the column (from 0) on it
    where the cursor stands before cluster INDEX."""
    rows = layout(cells, cols)
    for r, (start, end) in enumerate(rows):
        if start <= index < end or (index == end and r == len(rows) - 1):
            return r, text_width(cells[start:index])
    raise ValueError("index %d is past the line" % index)


def index_at(cells, start, end, col):
    """The index, from START up to END, of the cluster that column COL of
    the row falls on, or END when COL is past the row's text."""
    at = 0
    for i in range(start, end):
        w = width(cells[i])
        if col < at + w:
            return i
        at += w
    return end


# The file format.

def parse(text):
    """The lines of a file's text, each with the size its tag gives."""
    lines = []
    for raw in text.split("\n"):
        size = "normal"
        for name, tag in TAGS.items():
            if raw.startswith(tag):
                size, raw = name, raw[len(tag):]
                break
        else:
            if raw.startswith(NORMAL_TAG):
                raw = raw[len(NORMAL_TAG):]
        lines.append(Line(size, raw))
    # A file that ends with a newline has no line after it.
    if len(lines) > 1 and text.endswith("\n"):
        lines.pop()
    return lines


def unparse(lines):
    """The file's text for LINES: each line tagged with its size."""
    out = []
    for line in lines:
        text = line.text
        if line.size in TAGS:
            text = TAGS[line.size] + text
        elif text.startswith(tuple(TAGS.values()) + (NORMAL_TAG,)):
            text = NORMAL_TAG + text
        out.append(text + "\n")
    return "".join(out)


# The keys.

# The final byte of CSI and SS3 cursor keys, and the number of each CSI
# number ~ key.
FINALS = {"A": "up", "B": "down", "C": "right", "D": "left", "H": "home", "F": "end"}
TILDES = {"1": "home", "7": "home", "4": "end", "8": "end", "2": "insert", "3": "delete",
          "5": "pageup", "6": "pagedown"}


class Keys:
    """Turns the bytes a terminal sends into keys: a printable character
    as itself, the other keys by name. Bytes that end in the middle of a
    sequence wait for the next read."""

    def __init__(self):
        self.held = b""

    def feed(self, data):
        data = self.held + data
        self.held = b""
        keys = []
        i = 0
        while i < len(data):
            b = data[i]
            if b == 0x1B:
                end, key = self.sequence(data, i)
                if end is None:
                    self.held = data[i:]
                    break
                if key:
                    keys.append(key)
                i = end
                continue
            if b < 0x20 or b == 0x7F:
                keys.append({0x0D: "enter", 0x0A: "enter", 0x7F: "backspace", 0x08: "backspace",
                             0x09: "tab", 0x0C: "redraw", 0x03: "esc"}.get(b, ""))
                i += 1
                continue
            # A UTF-8 character: its lead byte says how many bytes follow.
            n = 1 if b < 0x80 else 2 if b < 0xE0 else 3 if b < 0xF0 else 4
            if i + n > len(data):
                self.held = data[i:]
                break
            keys.append(data[i:i + n].decode("utf-8", "replace"))
            i += n
        return [k for k in keys if k]

    @staticmethod
    def sequence(data, i):
        """The end of the sequence that starts at I and its key, or None
        for the end when the sequence is not complete yet."""
        if i + 1 >= len(data):
            return None, None
        kind = data[i + 1]
        if kind == ord("O"):
            if i + 2 >= len(data):
                return None, None
            return i + 3, FINALS.get(chr(data[i + 2]), "")
        if kind != ord("["):
            # ESC and another key: Esc, then that key.
            return i + 1, "esc"
        j = i + 2
        while j < len(data) and 0x30 <= data[j] <= 0x3F:
            j += 1
        if j >= len(data):
            return None, None
        params = data[i + 2:j].decode("ascii", "replace")
        final = chr(data[j])
        if final == "~":
            return j + 1, TILDES.get(params.split(";")[0], "")
        return j + 1, FINALS.get(final, "")

    def flush(self):
        """The keys the held bytes make once no more bytes come: a lone ESC,
        or the start of a sequence that never ended, is the Esc key. The
        start of a UTF-8 character stays held for the next read."""
        if not self.held.startswith(b"\x1b"):
            return []
        self.held = b""
        return ["esc"]


# The editor.

class Editor:
    """The buffer, the cursor and the screen. keys() takes the keys of one
    read and returns the bytes that bring the screen up to date."""

    def __init__(self, cols, rows, lines=None, name=None):
        self.lines = lines or [Line()]
        self.name = name
        self.modified = False
        self.line = 0
        self.index = 0
        # The column, in the line's own columns, that Up and Down aim for.
        self.goal = 0
        self.mode = "NORMAL"
        self.pending = ""
        self.prompt = None
        self.message = WELCOME
        self.done = False
        # The first row of the text that the screen shows: a line and a
        # row of its layout.
        self.top = (0, 0)
        self.resize(cols, rows)

    # The screen.

    def resize(self, cols, rows):
        """Takes a new size. The next draw clears the screen and writes
        every row."""
        self.cols = max(1, cols)
        self.rows = max(3, rows)
        self.text_rows = self.rows - 2
        self.drawn = None

    def width_of(self, line):
        return line_width(self.lines[line].size, self.cols)

    def layout_of(self, line):
        return layout(self.lines[line].cells, self.width_of(line))

    def row_height(self, line):
        return 2 if self.lines[line].size == "tall" else 1

    def cursor_row(self):
        """The cursor's (line, row of its layout) and its column."""
        line = self.lines[self.line]
        r, col = place(line.cells, self.width_of(self.line), self.index)
        return (self.line, r), col

    def next_row(self, at):
        """The text's row after AT, or None after the last."""
        line, r = at
        if r + 1 < len(self.layout_of(line)):
            return line, r + 1
        if line + 1 < len(self.lines):
            return line + 1, 0
        return None

    def prev_row(self, at):
        line, r = at
        if r > 0:
            return line, r - 1
        if line > 0:
            return line - 1, len(self.layout_of(line - 1)) - 1
        return None

    def follow(self):
        """Moves the view so that it shows the cursor's row whole. An edit
        or a resize can leave the view's first row past the text's end, so
        that row is brought back inside the text first."""
        line = min(self.top[0], len(self.lines) - 1)
        self.top = (line, min(self.top[1], len(self.layout_of(line)) - 1))
        at, _ = self.cursor_row()
        if at < self.top:
            self.top = at
            return
        while True:
            used = 0
            row = self.top
            while row is not None and row <= at:
                used += self.row_height(row[0])
                row = self.next_row(row)
            if used <= self.text_rows or self.top == at:
                return
            self.top = self.next_row(self.top)

    def screen_rows(self):
        """What each row of the text area shows, as (size, text) with the
        size one of s, w, T and B. A tall row that would not fit whole at
        the bottom is left empty, and rows past the end show a tilde."""
        out = []
        row = self.top
        while len(out) < self.text_rows:
            if row is None:
                out.append(("s", "~"))
                continue
            line = self.lines[row[0]]
            start, end = self.layout_of(row[0])[row[1]]
            text = "".join(shown(c) for c in line.cells[start:end])
            if line.size == "tall":
                if len(out) + 2 > self.text_rows:
                    out.append(("s", ""))
                    continue
                out += [("T", text), ("B", text)]
            else:
                out.append(("w" if line.size == "wide" else "s", text))
            row = self.next_row(row)
        return out

    def screen_of(self, at):
        """The row of the text area (from 0) where the text's row AT is
        drawn, or None when it is not on the screen."""
        r = 0
        row = self.top
        while row is not None and r < self.text_rows:
            if row == at:
                return r
            r += self.row_height(row[0])
            row = self.next_row(row)
        return None

    def status(self):
        """The status line, as wide as the screen."""
        name = os.path.basename(self.name) if self.name else "(no file)"
        left = " %s%s  %s" % (name, " [+]" if self.modified else "", self.mode)
        _, col = self.cursor_row()
        right = "line %d, column %d " % (self.line + 1, col + 1)
        room = self.cols - len(left) - len(right)
        for note in STATUS_NOTES:
            if len(note) + 4 <= room:
                pad = room - len(note)
                return left + " " * (pad // 2) + note + " " * (pad - pad // 2) + right
        if room >= 1:
            return left + " " * room + right
        return (left + " " + right)[:self.cols]

    def bottom(self):
        """The last row: the prompt while a command is typed, else the
        latest message."""
        text = ":" + self.prompt if self.prompt is not None else self.message
        return "".join(shown(c) for c in clusters(text))

    def draw(self):
        """The bytes that bring the screen from what the editor drew last
        to what the buffer says, ending with the cursor in place."""
        self.follow()
        out = CSI + "?25l"
        if self.drawn is None:
            # A first draw, or one after a resize or Ctrl-L: the screen is
            # cleared, and every row is single and empty.
            out += CSI + "0m" + CSI + "4l" + CSI + "?7l" + CSI + "r"
            for r in range(1, self.rows + 1):
                out += CSI + "%d;1H" % r + ROW_SIZE["s"]
            out += CSI + "2J" + CSI + "1;%dr" % self.text_rows
            self.drawn = {"text": [("s", "")] * self.text_rows, "status": None, "bottom": None,
                          "top": self.top}
        want = self.screen_rows()
        out += self.scroll(want)
        for r, (new, old) in enumerate(zip(want, self.drawn["text"])):
            if new != old:
                out += self.redraw_row(r + 1, old, new)
        self.drawn["text"] = want
        self.drawn["top"] = self.top
        status = self.status()
        if status != self.drawn["status"]:
            out += CSI + "%d;1H" % (self.rows - 1) + CSI + "7m" + status + CSI + "0m"
            self.drawn["status"] = status
        bottom = self.bottom()
        if bottom != self.drawn["bottom"]:
            out += CSI + "%d;1H" % self.rows + bottom[:self.cols] + CSI + "K"
            self.drawn["bottom"] = bottom
        if self.prompt is not None:
            out += CSI + "%d;%dH" % (self.rows, min(self.cols, text_width(clusters(self.bottom())) + 1))
        else:
            at, col = self.cursor_row()
            r = self.screen_of(at)
            out += CSI + "%d;%dH" % ((r or 0) + 1, col + 1)
        return (out + CSI + "?25h").encode()

    def scroll(self, want):
        """When the view moved a few rows, scrolls the text area's rows
        with SU or SD (inside the scroll region) and shifts the editor's
        copy of them to match. The rows that scroll in are empty and
        single, and the rows that differ are drawn after."""
        old = self.drawn["text"]
        if self.top == self.drawn["top"]:
            return ""
        for k in range(1, min(SCROLL_MAX, self.text_rows - 1) + 1):
            if want[:-k] == old[k:] and want[:-k]:
                self.drawn["text"] = old[k:] + [("s", "")] * k
                return CSI + "%dS" % k
            if want[k:] == old[:-k] and want[k:]:
                self.drawn["text"] = [("s", "")] * k + old[:-k]
                return CSI + "%dT" % k
        return ""

    def redraw_row(self, r, old, new):
        """The bytes that change screen row R (from 1) from OLD to NEW,
        each a (size, text) pair."""
        size, text = new
        cols = self.cols if size == "s" else max(1, self.cols // 2)
        new_cells = clusters(text)
        end = text_width(new_cells)
        erase = CSI + "K" if end < cols else ""
        if old[0] != size:
            return CSI + "%d;1H" % r + ROW_SIZE[size] + text + erase
        old_cells = clusters(old[1])
        p = 0
        while p < min(len(old_cells), len(new_cells)) and old_cells[p] == new_cells[p]:
            p += 1
        col = text_width(new_cells[:p])
        rest_old, rest_new = old_cells[p:], new_cells[p:]
        # An insert: the new row is the old one with characters put in at
        # P, cut where the row is full. ICH moves the old characters right,
        # and the characters past the row's edge are lost.
        for n in range(1, len(rest_new) + 1):
            tail = rest_new[n:]
            if tail and rest_old[:len(tail)] == tail:
                return (CSI + "%d;%dH" % (r, col + 1) + CSI + "%d@" % text_width(rest_new[:n])
                        + "".join(rest_new[:n]) + CSI + "%d;%dH" % (r, end + 1 if end < cols else cols)
                        + erase)
        # A delete: the new row is the old one with characters taken out at
        # P, and characters from the next row come in at its end. DCH moves
        # the old characters left, and the editor writes the ones that come
        # in.
        for n in range(1, len(rest_old) + 1):
            kept = rest_old[n:]
            if kept and rest_new[:len(kept)] == kept:
                added = rest_new[len(kept):]
                at = col + text_width(kept)
                return (CSI + "%d;%dH" % (r, col + 1) + CSI + "%dP" % text_width(rest_old[:n])
                        + CSI + "%d;%dH" % (r, at + 1) + "".join(added) + erase)
        return CSI + "%d;%dH" % (r, col + 1) + "".join(rest_new) + erase

    # The keys.

    def keys(self, keys):
        for key in keys:
            if self.done:
                break
            self.key(key)
        return b"" if self.done else self.draw()

    def key(self, key):
        if self.prompt is not None:
            self.command_key(key)
        elif self.mode == "INSERT":
            self.insert_key(key)
        else:
            self.normal_key(key)

    def last_index(self):
        """The furthest the cursor goes on its line: after the last
        character in insert mode, on it in normal mode."""
        n = len(self.lines[self.line].cells)
        return n if self.mode == "INSERT" else max(0, n - 1)

    def set_index(self, index, goal=True):
        self.index = max(0, min(index, self.last_index()))
        if goal:
            self.goal = text_width(self.lines[self.line].cells[:self.index])

    def go_line(self, line):
        """Moves to LINE, at the column Up and Down aim for."""
        self.line = max(0, min(line, len(self.lines) - 1))
        cells = self.lines[self.line].cells
        self.set_index(index_at(cells, 0, len(cells), self.goal), goal=False)

    def page(self, direction):
        """Moves the cursor and the view a screen of text back or forward,
        counting the screen rows the text's rows take."""
        step = max(1, self.text_rows - 2)
        at, _ = self.cursor_row()
        moved = 0
        while moved < step:
            nxt = self.next_row(at) if direction > 0 else self.prev_row(at)
            if nxt is None:
                break
            moved += self.row_height(nxt[0])
            at = nxt
            top = self.next_row(self.top) if direction > 0 else self.prev_row(self.top)
            if top is not None:
                self.top = top
        start, _ = self.layout_of(at[0])[at[1]]
        self.line = at[0]
        self.set_index(start)

    def move(self, key):
        """The keys that move the cursor in both modes. False for any other
        key."""
        if key == "left":
            self.set_index(self.index - 1)
        elif key == "right":
            self.set_index(self.index + 1)
        elif key == "up":
            self.go_line(self.line - 1)
        elif key == "down":
            self.go_line(self.line + 1)
        elif key == "home":
            self.set_index(0)
        elif key == "end":
            self.set_index(len(self.lines[self.line].cells))
        elif key in ("pageup", "pagedown"):
            self.page(1 if key == "pagedown" else -1)
        elif key == "redraw":
            self.drawn = None
        else:
            return False
        return True

    def normal_key(self, key):
        pending, self.pending = self.pending, ""
        if pending == "d":
            if key == "d":
                self.delete_line()
            return
        if self.move(key):
            return
        moves = {"h": "left", "l": "right", "k": "up", "j": "down"}
        if key in moves:
            self.move(moves[key])
        elif key in ("i", "a"):
            self.mode = "INSERT"
            self.set_index(self.index + (1 if key == "a" and self.lines[self.line].cells else 0))
        elif key in ("o", "O"):
            at = self.line + (1 if key == "o" else 0)
            self.lines.insert(at, Line())
            self.modified = True
            self.line = at
            self.mode = "INSERT"
            self.set_index(0)
        elif key == "x":
            cells = self.lines[self.line].cells
            if cells:
                del cells[self.index]
                self.modified = True
                self.set_index(self.index)
        elif key == "d":
            self.pending = "d"
        elif key == ":":
            self.prompt = ""

    def insert_key(self, key):
        if key == "esc":
            self.mode = "NORMAL"
            self.set_index(self.index - 1)
            return
        if self.move(key):
            return
        line = self.lines[self.line]
        if key == "enter":
            rest = Line(line.size)
            rest.cells = line.cells[self.index:]
            del line.cells[self.index:]
            self.lines.insert(self.line + 1, rest)
            self.line += 1
            self.set_index(0)
        elif key == "backspace":
            if self.index > 0:
                del line.cells[self.index - 1]
                self.set_index(self.index - 1)
            elif self.line > 0:
                above = self.lines[self.line - 1]
                at = len(above.cells)
                above.cells += line.cells
                del self.lines[self.line]
                self.line -= 1
                self.set_index(at)
            else:
                return
        elif key == "delete":
            if self.index < len(line.cells):
                del line.cells[self.index]
            elif self.line + 1 < len(self.lines):
                line.cells += self.lines.pop(self.line + 1).cells
            else:
                return
        elif key == "tab":
            self.insert(" " * (4 - self.goal % 4))
        elif len(key) == 1:
            self.insert(key)
            return
        else:
            return
        self.modified = True

    def insert(self, text):
        """Inserts TEXT at the cursor. A zero-width character joins the
        character before the cursor."""
        line = self.lines[self.line]
        for ch in text:
            if zero_width(ch) and self.index > 0:
                line.cells[self.index - 1] += ch
            else:
                line.cells.insert(self.index, ch)
                self.index += 1
        self.set_index(self.index)
        self.modified = True

    def delete_line(self):
        del self.lines[self.line]
        if not self.lines:
            self.lines.append(Line())
        self.line = min(self.line, len(self.lines) - 1)
        self.set_index(self.index)
        self.modified = True

    # The commands.

    def command_key(self, key):
        if key == "esc":
            self.prompt = None
        elif key == "enter":
            command, self.prompt = self.prompt, None
            self.command(command.strip())
        elif key == "backspace":
            if self.prompt:
                self.prompt = "".join(clusters(self.prompt)[:-1])
            else:
                self.prompt = None
        elif len(key) == 1 and key.isprintable():
            self.prompt += key

    def command(self, command):
        word, _, arg = command.partition(" ")
        arg = arg.strip()
        if word in SIZES and not arg:
            self.lines[self.line].size = word
            self.modified = True
            self.set_index(self.index)
            self.message = "This line is %s now." % word
        elif word in ("w", "wq"):
            if self.write(arg or self.name) and word == "wq":
                self.done = True
        elif word == "q" and not arg:
            if self.modified:
                self.message = "This file has unsaved changes. :w saves them, and :q! quits without saving."
            else:
                self.done = True
        elif word == "q!" and not arg:
            self.done = True
        elif word in ("e", "e!"):
            if not arg:
                self.message = ":e needs the name of a file, as :e notes.txt."
            elif self.modified and word == "e":
                self.message = "This file has unsaved changes. :w saves them, and :e! drops them."
            else:
                self.open(arg)
        elif command:
            self.message = "The command :%s is not one this editor knows." % command

    def write(self, name):
        if not name:
            self.message = "This text has no file name yet. :w FILE saves it to FILE."
            return False
        try:
            with open(name, "w", encoding="utf-8") as f:
                f.write(unparse(self.lines))
        except OSError as e:
            self.message = "Cannot save %s: %s." % (name, e.strerror)
            return False
        self.name = name
        self.modified = False
        self.message = "Saved %d lines to %s." % (len(self.lines), name)
        return True

    def open(self, name, first=False):
        """Reads the file NAME into the buffer. The first file the editor
        opens gets a short message, so that the keys to start with fit
        beside it on the bottom row."""
        lines, self.message = load(name, first)
        self.lines = lines
        self.name = name
        self.modified = False
        self.line = 0
        self.top = (0, 0)
        self.set_index(0)


def step_lines(env):
    """The lines of the step that guide.sh runs the editor for, from the
    step's text in ENV: the heading, a "Do:" line, an "Expect:" line, and a
    line for each expect: line that starts "Wrong:". The guide's asterisks
    are left out. A step with no text gives no lines."""
    lines = []
    heading = env.get("GUIDE_STEP_HEADING", "").strip()
    if heading:
        lines.append(heading)
    for label, key in (("Do:", "GUIDE_STEP_DO"), ("Expect:", "GUIDE_STEP_EXPECT")):
        part = []
        for raw in env.get(key, "").replace("*", "").splitlines():
            raw = raw.strip()
            if not raw:
                continue
            if raw.startswith("Wrong:"):
                part.append(raw)
            elif part:
                part[-1] += " " + raw
            else:
                part.append(label + " " + raw)
        lines += part
    return lines


def prepend_step(name, env):
    """Writes the step's lines and a blank line at the top of the file
    NAME. Each is a normal line, so a step line that starts with a size
    tag is saved with "[normal] " in front and reads back as it was. With
    no step text, the file is left as it is."""
    lines = step_lines(env)
    if not lines:
        return
    try:
        with open(name, encoding="utf-8") as f:
            text = f.read()
    except FileNotFoundError:
        text = ""
    with open(name, "w", encoding="utf-8") as f:
        f.write(unparse([Line("normal", t) for t in lines] + [Line()]) + text)


def load(name, first=False):
    """The lines of the file NAME and the message that says what was read,
    which for the FIRST file names the keys to start with. A file that does
    not exist yet starts empty."""
    try:
        with open(name, encoding="utf-8", errors="replace") as f:
            lines = parse(f.read())
    except FileNotFoundError:
        return [Line()], "%s is a new file." % name
    except OSError as e:
        return [Line()], "Cannot read %s: %s." % (name, e.strerror)
    if first:
        return lines, "Read %d lines. %s" % (len(lines), WELCOME)
    return lines, "Read %d lines from %s." % (len(lines), name)


def finish(rows, drawn):
    """The bytes that put the terminal back: every row the editor sized is
    single again, then the scroll region, insert mode, autowrap and the
    attributes are reset, the screen is cleared, and the cursor goes to the
    start of the bottom row."""
    out = CSI + "0m" + CSI + "4l" + CSI + "r" + CSI + "?7h"
    for r, (size, _) in enumerate(drawn["text"] if drawn else []):
        if size != "s":
            out += CSI + "%d;1H" % (r + 1) + ROW_SIZE["s"]
    out += CSI + "H" + CSI + "2J" + CSI + "%d;1H" % rows + ROW_SIZE["s"] + CSI + "?25h"
    return out.encode()


def usage():
    print(__doc__.split("\n\n")[1], file=sys.stderr)
    return 2


class Done(Exception):
    pass


def main(argv):
    step = bool(argv) and argv[0] == "--prepend-step"
    if step:
        argv = argv[1:]
    if len(argv) > 1 or (argv and argv[0].startswith("-")) or (step and not argv):
        return usage()
    fd = sys.stdin.fileno()
    if not os.isatty(fd):
        print("vim-vt510: the editor reads keys from a terminal, and its input is not one", file=sys.stderr)
        return 2
    if step:
        try:
            prepend_step(argv[0], os.environ)
        except OSError as e:
            print("vim-vt510: cannot write the step's text into %s: %s" % (argv[0], e.strerror), file=sys.stderr)
            return 1
    cols, rows = os.get_terminal_size(fd)
    editor = Editor(cols, rows)
    if argv:
        editor.open(argv[0], first=True)
    keys = Keys()

    def write(data):
        os.write(1, data)

    # SIGWINCH wakes the loop through this pipe, and the loop then reads
    # the new size. A hangup or a kill ends the editor through the finally
    # below, so the terminal gets its settings back.
    wake_r, wake_w = os.pipe()
    os.set_blocking(wake_w, False)
    signal.set_wakeup_fd(wake_w)
    signal.signal(signal.SIGWINCH, lambda signum, frame: None)

    def stop(signum, frame):
        raise Done

    signal.signal(signal.SIGHUP, stop)
    signal.signal(signal.SIGTERM, stop)
    saved = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        write(editor.draw())
        while not editor.done:
            ready = select.select([fd, wake_r], [], [])[0]
            if wake_r in ready:
                os.read(wake_r, 64)
                cols, rows = os.get_terminal_size(fd)
                if (cols, rows) != (editor.cols, editor.rows):
                    editor.resize(cols, rows)
                    write(editor.draw())
            if fd not in ready:
                continue
            data = os.read(fd, 1024)
            # An empty read is the terminal gone: nothing more will come.
            if not data:
                break
            got = keys.feed(data)
            while keys.held and select.select([fd], [], [], ESC_WAIT)[0]:
                more = os.read(fd, 1024)
                if not more:
                    break
                got += keys.feed(more)
            got += keys.flush()
            out = editor.keys(got)
            if out:
                write(out)
    except Done:
        pass
    finally:
        try:
            write(finish(editor.rows, editor.drawn))
            termios.tcsetattr(fd, termios.TCSAFLUSH, saved)
        except (OSError, termios.error):
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
