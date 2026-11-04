#!/usr/bin/env python3
"""Tests the acceptance pass (ADR-1501): accept.sh's walk through the pages, the pass
record it writes, the resume, and feedback.sh's reading and comparing of
records, all on a pseudo-terminal with scripted keys and no VT or sudo. The
walk runs over stand-in pages (ACCEPT_PAGES), and the real pages in
tools/vt/sections are checked on their own: their guidance, their size, and
what they send when a terminal says it shows pictures and when it says it
does not.

    python3 -I tools/vt/tests/test_accept.py [-v] [TestCase.test_name ...]
"""

import fcntl
import hashlib
import math
import os
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unicodedata
import unittest

# The longest any test waits on another process. It only makes a broken test fail instead of hanging.
WAIT_BOUND = 120

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
VT = os.path.join(ROOT, "tools", "vt")
ACCEPT_SH = os.path.join(VT, "accept.sh")
FEEDBACK_SH = os.path.join(VT, "feedback.sh")
SECTIONS = os.path.join(VT, "sections")
KEYS = os.path.join(ROOT, "crates", "vtium-settings", "src", "keys.rs")

DA1_TEXT = b"\x1b[?62;22c"
DA1_PICTURES = b"\x1b[?62;4;22c"

# The stand-in pages: two plain ones, one this build cannot show, and one
# that turns on reverse video for the whole screen, which the walk must turn
# off before the next page.
PAGES = {
    "10-one.sh": "# look-for: the words page one\n# wrong-looks-like: anything else\nprintf 'page one\\n'\n",
    "20-two.sh": "# look-for: the words page two\n# wrong-looks-like: anything else\nprintf 'page two\\n'\n",
    "30-unshown.sh": "# look-for: a picture\n# wrong-looks-like: no picture\n"
                     "printf '  This build cannot show this page.\\n  no pictures here\\n'\nexit 77\n",
    "40-reverse.sh": "# look-for: the screen reversed\n# wrong-looks-like: a dark screen\n"
                     "printf '\\033[?5hpage four\\n'\n",
}
IDS = ["10-one", "20-two", "30-unshown", "40-reverse"]


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def write(path, text, mode=0o644):
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    os.chmod(path, mode)


class Pty:
    """A program on a pseudo-terminal of 80x25, or SIZE as (rows, columns),
    read up to each prompt before
    it is answered. It answers DA1 with da1 and the cursor position query
    with row 10, as a terminal would."""

    def __init__(self, argv, env, da1=DA1_TEXT, frits=("unifont",), size=(25, 80)):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(argv[0], argv, env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", *size, 0, 0))
        self.da1 = da1
        # As vtium does: OSC 50 switches to a frit in its frits directory,
        # here FRITS, and to nothing else; its query names the font in use.
        self.frits = frits
        self.font = "unifont"
        # bytes the stand-in sends right after its next answer to the cursor
        # position query, in the same write: input that arrives while the
        # walk draws a page, before its prompt
        self.after_dsr = b""
        # out is what is left after the last wait_for; seen is everything.
        self.out = b""
        self.seen = b""
        self.answered = 0

    def pump(self, timeout):
        if select.select([self.fd], [], [], max(timeout, 0))[0]:
            try:
                data = os.read(self.fd, 4096)
            except OSError:
                data = b""
            if not data:
                return False
            self.out += data
            self.seen += data
            self.answer()
        return True

    def answer(self):
        """Answers every query seen since the last answer, in order, and
        takes each OSC 50 font switch."""
        while True:
            rest = self.seen[self.answered:]
            found = [(rest.find(q), q, a) for q, a in ((b"\x1b[c", self.da1), (b"\x1b[6n", b"\x1b[10;1R"),
                                                       (b"\x1b[14t", b"\x1b[4;400;640t"), (b"\x1b]50;?\x07", None))
                     if rest.find(q) >= 0]
            switch = re.search(rb"\x1b\]50;([^?\x07][^\x07]*)\x07", rest)
            if switch and (not found or switch.start() < min(found)[0]):
                self.answered += switch.end()
                name = switch.group(1).decode()
                if name in self.frits:
                    self.font = name
                continue
            if not found:
                return
            at, q, a = min(found)
            self.answered += at + len(q)
            if q == b"\x1b[6n" and self.after_dsr:
                a, self.after_dsr = a + self.after_dsr, b""
            os.write(self.fd, a if a is not None else b"\x1b]50;" + self.font.encode() + b"\x07")

    def wait_for(self, text, timeout=WAIT_BOUND):
        end = time.monotonic() + timeout
        while text not in self.out:
            if time.monotonic() > end or not self.pump(end - time.monotonic()):
                raise AssertionError(f"no {text!r} in:\n{self.out[-2000:].decode(errors='replace')}")
        self.out = self.out[self.out.index(text) + len(text):]

    def send(self, data):
        os.write(self.fd, data)

    def paste(self, *pieces):
        """Pastes the text of PIECES, sent one after another, as a terminal
        does: between ESC [ 200 ~ and ESC [ 201 ~ while the program has asked
        for bracketed paste (DECSET 2004), as it is otherwise."""
        on = self.seen.rfind(b"\x1b[?2004h") > self.seen.rfind(b"\x1b[?2004l")
        pieces = list(pieces)
        if on:
            pieces[0] = b"\x1b[200~" + pieces[0]
            pieces[-1] = pieces[-1] + b"\x1b[201~"
        for p in pieces:
            self.send(p)

    def finish(self, timeout=WAIT_BOUND):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                while self.pump(0.05):
                    pass
                return os.waitstatus_to_exitcode(status)
            self.pump(0.05)
        os.kill(self.pid, signal.SIGKILL)
        os.waitpid(self.pid, 0)
        raise AssertionError("did not exit")


ASK = b"y yes  n no  u unsure  s skip  c comment  r redraw  b back  q quit"


class Case(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="vtium-accept-")
        self.sysfs = os.path.join(self.tmp, "sys")
        self.proc = os.path.join(self.tmp, "proc")
        self.machine()
        self.passes = os.path.join(self.tmp, "my passes")
        self.pages = os.path.join(self.tmp, "pages")
        os.mkdir(self.pages)
        for name, text in PAGES.items():
            write(os.path.join(self.pages, name), text)

    def tearDown(self):
        subprocess.run(["rm", "-rf", "--", self.tmp], check=True)

    def machine(self, kernel="7.2.14-test"):
        """A stand-in /sys and /proc: a machine with one GPU, one display
        connected and one not, and the kernel KERNEL."""
        def put(path, data):
            path = os.path.join(self.sysfs, path)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as f:
                f.write(data if isinstance(data, bytes) else data.encode())
        if not os.path.isdir(self.sysfs):
            put("class/dmi/id/sys_vendor", "Example Boards\n")
            put("class/dmi/id/product_name", "EB-1\n")
            put("class/dmi/id/product_version", "2.0\n")
            dev = os.path.join(self.sysfs, "devices/pci0000:00/0000:03:00.0")
            os.makedirs(dev)
            os.makedirs(os.path.join(self.sysfs, "bus/pci/drivers/amdgpu"))
            os.symlink(os.path.join(self.sysfs, "bus/pci/drivers/amdgpu"), os.path.join(dev, "driver"))
            put("devices/pci0000:00/0000:03:00.0/vendor", "0x1002\n")
            put("devices/pci0000:00/0000:03:00.0/device", "0x744c\n")
            os.makedirs(os.path.join(self.sysfs, "class/drm/card0"))
            os.symlink(dev, os.path.join(self.sysfs, "class/drm/card0/device"))
            # an EDID: Dell (DEL), product 0xA0F1, named DELL U2723QE
            edid = bytearray(128)
            edid[0:8] = b"\x00\xff\xff\xff\xff\xff\xff\x00"
            v = (ord("D") - 64) << 10 | (ord("E") - 64) << 5 | (ord("L") - 64)
            edid[8:12] = bytes([v >> 8, v & 255, 0xF1, 0xA0])
            edid[72:90] = b"\x00\x00\x00\xfc\x00" + b"DELL U2723QE\n"
            put("class/drm/card0-DP-1/status", "connected\n")
            put("class/drm/card0-DP-1/modes", "3840x2160\n1920x1080\n")
            put("class/drm/card0-DP-1/edid", bytes(edid))
            put("class/drm/card0-HDMI-A-1/status", "disconnected\n")
        os.makedirs(os.path.join(self.proc, "sys/kernel"), exist_ok=True)
        write(os.path.join(self.proc, "sys/kernel/osrelease"), kernel + "\n")

    def env(self, label="vtium dev build v1-test", **extra):
        env = {"PATH": os.environ["PATH"], "TERM": "linux", "HOME": self.tmp,
               "ACCEPT_SYSFS": self.sysfs, "ACCEPT_PROC": self.proc,
               "ACCEPT_DIR": self.passes, "ACCEPT_PAGES": self.pages, "ACCEPT_LABEL": label}
        env.update(extra)
        return env

    def walk(self, env, *argv, da1=DA1_TEXT, frits=("unifont",), size=(25, 80)):
        return Pty(["/bin/bash", ACCEPT_SH, *argv], env, da1, frits, size)

    def record(self):
        """The one record in the passes directory, and its path."""
        names = [n for n in os.listdir(self.passes) if n.endswith(".md")]
        self.assertEqual(len(names), 1, names)
        path = os.path.join(self.passes, names[0])
        return path, read(path)

    def rows(self, text):
        """The table's rows of a record: (page, answer, comment) as written."""
        out = []
        for line in text.splitlines():
            if line.startswith("| ") and not line.startswith("| page |"):
                cells = re.split(r"(?<!\\) \| ", line[2:-2])
                out.append(tuple(cells))
        return out

    def feedback(self, *argv):
        return subprocess.run(["bash", FEEDBACK_SH, *argv], env=self.env(), capture_output=True, text=True,
                              timeout=WAIT_BOUND)


class Walk(Case):
    """accept.sh's pass: the keys, the record, back, quit and resume."""

    def test_a_pass_records_each_answer_its_comment_and_the_build(self):
        d = self.walk(self.env(ACCEPT_VT="5", ACCEPT_BINARY="/build/vtium", ACCEPT_FEATURES="images"))
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b"red | is orange \\ & <br> here\r")
        d.wait_for(b"> ")
        d.send(b"second line\r")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"y yes  n no  u unsure  s skip")
        d.send(b"n")
        # The page this build cannot show is recorded as skip at once.
        d.wait_for(b"no pictures here")
        d.wait_for(b"recorded as skip  any key: next page")
        d.send(b" ")
        d.wait_for(b"page four")
        d.wait_for(ASK)
        d.send(b"u")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        path, text = self.record()
        lines = text.splitlines()
        self.assertRegex(lines[0], r"^# vtium acceptance pass \d{8}T\d{6}-\d+$")
        meta = {m.group(1): m.group(2) for m in (re.match(r"^- ([a-z0-9]+): (.*)$", l) for l in lines) if m}
        self.assertEqual(meta["label"], "vtium dev build v1-test")
        self.assertEqual(meta["vt"], "5")
        self.assertEqual(meta["binary"], "/build/vtium")
        self.assertEqual(meta["features"], "images")
        self.assertEqual(meta["terminal"], "TERM=linux, 25 rows by 80 columns at the start")
        self.assertEqual(meta["da1"], "62;22")
        self.assertEqual(meta["answered"], "4 of 4")
        self.assertEqual(os.path.basename(path), meta["pass"] + ".md")
        self.assertEqual(self.rows(text), [
            ("10-one", "yes", ""),
            ("20-two", "no", "red \\| is orange \\\\ &amp; &lt;br&gt; here<br>second line"),
            ("30-unshown", "skip", "this build cannot show this page"),
            ("40-reverse", "unsure", ""),
        ])
        self.assertIn("- 20-two: the words page two\n", text)
        # The record's directory is the user's alone.
        self.assertEqual(os.stat(self.passes).st_mode & 0o777, 0o700)
        p = self.feedback()
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("20-two                    no\n    red | is orange \\ & <br> here\n    second line\n", p.stdout)
        self.assertIn("\nskipped\n  30-unshown\n", p.stdout)

    def test_the_walk_puts_back_what_a_page_changed_before_the_next(self):
        # the page that reverses the screen first, then another
        write(os.path.join(self.pages, "05-reverse.sh"), PAGES["40-reverse.sh"])
        d = self.walk(self.env(), "reverse", "one")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page four")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        seen = d.seen
        after = seen[seen.index(b"page four"):seen.index(b"page one")]
        for reset in (b"\x1b[?5l", b"\x1b[0 q", b"\x1b]104\x1b\\", b"\x1b[r", b"\x1b_Ga=d,d=A,q=2\x1b\\"):
            self.assertIn(reset, after)

    def test_back_answers_a_page_again_and_the_later_answer_stands(self):
        d = self.walk(self.env())
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"b")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"n")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertEqual(self.rows(text)[:2], [("10-one", "no", ""), ("20-two", "-", "")])

    def test_an_empty_comment_keeps_the_comment_the_page_had(self):
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b"first comment\r")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"y yes  n no  u unsure  s skip")
        d.send(b"n")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"b")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        # A comment ended at once is empty: the page is drawn again, and the
        # comment it had stands with the answer that follows.
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"n")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "no", "first comment"), ("20-two", "-", "")])

    def test_a_paste_at_the_answer_prompt_answers_nothing(self):
        # The walk asks for bracketed paste, and a paste's y, n, c, u, s, b,
        # r and q, arriving in more than one piece, answer nothing: the page
        # stays, nothing is recorded, and the next key typed answers it.
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.paste(b"https://example.org/ycnq?a=b", b"&u=s rb\nyes")
        # r, typed after the paste, draws the same page again: the walk read
        # the whole paste and is still on page one
        d.send(b"r")
        d.wait_for(b"[1/2] one")
        d.wait_for(ASK)
        self.assertNotIn(b"comment:", d.seen)
        path, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "-", ""), ("20-two", "-", "")])
        d.send(b"s")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        # the walk asked for bracketed paste, and turns it off as it ends
        self.assertIn(b"\x1b[?2004h", d.seen)
        self.assertIn(b"\x1b[?2004l", d.seen[d.seen.rindex(b"q exit"):])
        _, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "skip", ""), ("20-two", "-", "")])

    def test_a_paste_in_a_comment_is_comment_text(self):
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b"see ")
        d.paste(b"https://example.org/q?y=n")
        d.send(b"\r")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"y yes  n no  u unsure  s skip")
        d.send(b"n")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        # after the comment the walk asks for bracketed paste again, and a
        # paste answers nothing
        d.paste(b"yyy")
        d.send(b"r")
        d.wait_for(b"[2/2] two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "no", "see https://example.org/q?y=n"), ("20-two", "-", "")])

    def test_a_paste_at_a_prompt_any_key_answers_moves_nothing(self):
        # The title's "press a key" and a skipped page's "any key: next page"
        # take any key, but a paste is no key: it neither quits the walk at
        # the title nor moves it past the skipped page.
        d = self.walk(self.env(), "unshown", "reverse")
        d.wait_for(b"press a key to start the pass")
        d.paste(b"qqq")
        d.send(b" ")
        d.wait_for(b"no pictures here")
        d.wait_for(b"recorded as skip  any key: next page")
        d.paste(b"nnn")
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertNotIn(b"page four", d.seen)
        _, text = self.record()
        self.assertEqual(self.rows(text), [("30-unshown", "skip", "this build cannot show this page"),
                                           ("40-reverse", "-", "")])

    def test_a_paste_that_starts_before_the_prompt_answers_nothing(self):
        # The paste's start and first letters arrive while the page is drawn,
        # before its prompt, so drain reads them; the rest arrives once the
        # prompt is shown, and read_key drops it.
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.after_dsr = b"\x1b[200~nnn"
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        self.assertEqual(d.after_dsr, b"")
        d.send(b"yyy\x1b[201~")
        d.send(b"r")
        d.wait_for(b"[1/2] one")
        d.wait_for(ASK)
        _, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "-", ""), ("20-two", "-", "")])
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)

    def test_a_paste_in_a_comment_with_the_line_editors_paste_off(self):
        # With enable-bracketed-paste off in the user's inputrc, the walk
        # leaves the terminal's bracketed paste off while the line editor
        # reads, so the paste comes as plain lines, never as markers. The
        # inputrc also binds the start marker to self-insert, as a line editor
        # that does not know it would take it, so a marker reaching it would
        # land in the comment as junk. The paste's "." line ends the comment
        # and its y, after it, answers nothing.
        inputrc = os.path.join(self.tmp, "inputrc")
        write(inputrc, 'set enable-bracketed-paste off\n"\\e[200~": self-insert\n"\\e[201~": self-insert\n')
        d = self.walk(self.env(INPUTRC=inputrc), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"c")
        d.wait_for(b"> ")
        d.paste(b"a\r.\ry")
        d.wait_for(b"y yes  n no  u unsure  s skip")
        d.send(b"n")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertEqual(self.rows(text), [("10-one", "no", "a"), ("20-two", "-", "")])

    def test_a_terminal_smaller_than_80x25_is_named_on_the_title_and_each_page(self):
        d = self.walk(self.env(), "one", size=(24, 80))
        d.wait_for(b"The terminal is smaller than 80x25")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"(80x24, smaller than 80x25)")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertIn("- terminal: TERM=linux, 24 rows by 80 columns at the start\n", text)
        # at 80x25 neither shows
        os.unlink(self.record()[0])
        d = self.walk(self.env(), "one")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertNotIn(b"smaller than 80x25", d.seen)

    def quit_after_one(self, label="vtium dev build v1-test"):
        d = self.walk(self.env(label))
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"page one")
        d.wait_for(ASK)
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b"kept across the resume\r")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"y yes  n no  u unsure  s skip")
        d.send(b"y")
        d.wait_for(b"page two")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"3 pages still to answer: the next run offers to resume this pass.")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        path, text = self.record()
        self.assertIn("- answered: 1 of 4\n", text)
        return path

    def test_a_pass_quit_part_way_resumes_at_its_first_unanswered_page(self):
        path = self.quit_after_one()
        d = self.walk(self.env())
        d.wait_for(b"stopped part way")
        d.wait_for(b"r resume it at page 2 (1 of 4 answered)  n new pass  q quit")
        d.send(b"r")
        d.wait_for(b"[2/4] two")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"recorded as skip")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"n")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        # The same record, finished, with the first answer and its comment kept.
        again, text = self.record()
        self.assertEqual(again, path)
        self.assertIn("- answered: 4 of 4\n", text)
        self.assertEqual(self.rows(text), [
            ("10-one", "yes", "kept across the resume"),
            ("20-two", "yes", ""),
            ("30-unshown", "skip", "this build cannot show this page"),
            ("40-reverse", "no", ""),
        ])

    def test_a_new_pass_leaves_the_stopped_one_as_it_was(self):
        path = self.quit_after_one()
        before = read(path)
        d = self.walk(self.env())
        d.wait_for(b"n new pass")
        d.send(b"n")
        d.wait_for(b"[1/4] one")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertEqual(read(path), before)
        self.assertEqual(len([n for n in os.listdir(self.passes) if n.endswith(".md")]), 2)

    def test_another_build_never_resumes_a_pass(self):
        self.quit_after_one(label="vtium dev build v1-test")
        d = self.walk(self.env(label="vtium dev build v2-test"))
        d.wait_for(b"press a key to start the pass")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertNotIn(b"r resume it", d.seen)

    def test_nothing_is_recorded_in_a_directory_not_the_users_own(self):
        os.symlink(self.tmp, self.passes)
        d = self.walk(self.env())
        d.wait_for(b"is a symlink, or not a directory of yours alone")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"not recorded")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertEqual([n for n in os.listdir(self.tmp) if n.endswith(".md")], [])

    def test_nothing_is_recorded_in_a_directory_others_may_read(self):
        # The passes directory is the user's alone: one that group or others
        # may read or enter is refused, though it is the user's own.
        os.mkdir(self.passes, 0o755)
        os.chmod(self.passes, 0o755)
        d = self.walk(self.env())
        d.wait_for(b"is a symlink, or not a directory of yours alone")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(ASK)
        d.send(b"q")
        d.wait_for(b"not recorded")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertEqual(os.listdir(self.passes), [])

    def test_the_build_is_read_from_the_terminal_when_it_shows_pictures(self):
        d = self.walk(self.env(), "one", da1=DA1_PICTURES)
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"s")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        _, text = self.record()
        self.assertIn("- da1: 62;4;22\n", text)
        self.assertIn("- vt: not given\n", text)
        self.assertEqual(self.rows(text), [("10-one", "skip", "")])


RECORD = """# vtium acceptance pass {id}

- pass: {id}
- label: {label}
- vt: 5
- binary: not given
- features: not given
- terminal: TERM=xterm-vtium, 25 rows by 80 columns at the start
- da1: 62;22
- started: 2026-11-03T10:00:00-07:00
- updated: 2026-11-03T10:20:00-07:00
- answered: {n} of 4

| page | answer | comment |
|---|---|---|
{rows}
## What each page shows when it is right

- 10-sgr: colors
"""


class Feedback(Case):
    """feedback.sh: printing, listing and comparing records."""

    def put(self, pid, label, rows):
        os.makedirs(self.passes, mode=0o700, exist_ok=True)
        n = sum(1 for r in rows if r[1] != "-")
        text = RECORD.format(id=pid, label=label, n=n, rows="".join(f"| {p} | {a} | {c} |\n" for p, a, c in rows))
        path = os.path.join(self.passes, pid + ".md")
        write(path, text)
        return path

    def test_compare_lists_the_pages_whose_answer_changed_and_fails_on_yes_to_no(self):
        self.put("20261101T100000-1", "vtium dev build v1", [
            ("10-sgr", "yes", ""), ("20-underline", "yes", ""), ("30-strike", "no", "off the middle"),
            ("40-old", "yes", "")])
        self.put("20261103T100000-2", "vtium dev build v2", [
            ("10-sgr", "yes", ""), ("20-underline", "no", "curly is straight<br>since today"),
            ("30-strike", "yes", ""), ("50-new", "skip", "")])
        p = self.feedback("--compare")
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertEqual(p.stdout, (
            "A  20261101T100000-1  vtium dev build v1, 4 of 4 answered\n"
            "B  20261103T100000-2  vtium dev build v2, 4 of 4 answered\n"
            "\n"
            "page                      A        B\n"
            "20-underline              yes      no       <- went from yes to no\n"
            "    B: curly is straight\n"
            "    B: since today\n"
            "30-strike                 no       yes\n"
            "    A: off the middle\n"
            "40-old                    yes      (none)   (a page B does not have)\n"
            "50-new                    (none)   skip     (a page new in B)\n"
            "\n"
            "1 page went from yes to no: 20-underline\n"))

    def test_compare_passes_with_no_page_gone_from_yes_to_no(self):
        a = self.put("20261101T100000-1", "v1", [("10-sgr", "no", ""), ("20-underline", "-", "")])
        b = self.put("20261103T100000-2", "v2", [("10-sgr", "yes", ""), ("20-underline", "unsure", "")])
        # A committed record is named by its path, and a local one by its id.
        committed = os.path.join(self.tmp, "committed.md")
        os.rename(a, committed)
        p = self.feedback("--compare", committed, "20261103T100000-2")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("10-sgr                    no       yes\n", p.stdout)
        self.assertIn("20-underline              -        unsure\n", p.stdout)
        self.assertTrue(p.stdout.endswith("\nno page went from yes to no\n"))
        self.assertTrue(os.path.exists(b))

    def test_compare_of_one_pass_is_refused(self):
        self.put("20261101T100000-1", "v1", [("10-sgr", "yes", "")])
        p = self.feedback("--compare")
        self.assertEqual(p.returncode, 2)
        self.assertIn("--compare needs two passes", p.stderr)

    def test_list_and_newest(self):
        self.put("20261103T100000-2", "vtium dev build v2", [("10-sgr", "yes", ""), ("20-u", "-", "")])
        self.put("20261101T100000-1", "vtium dev build v1", [("10-sgr", "yes", "")])
        p = self.feedback("--list")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(p.stdout, "pass                     answered   label\n"
                                   "20261101T100000-1        1/4        vtium dev build v1\n"
                                   "20261103T100000-2        1/4        vtium dev build v2\n")
        p = self.feedback("--newest", "vtium dev build v1")
        self.assertEqual(p.stdout, os.path.join(self.passes, "20261101T100000-1.md") + "\n")
        self.assertEqual(self.feedback("--newest", "vtium dev build v3").returncode, 1)

    def test_a_comment_reads_back_as_it_was_typed(self):
        typed = "a | b \\ c & d <br> e &lt; f\\n g"
        # As accept.sh writes it: & < > as entities, \ and | escaped.
        cell = typed.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        cell = cell.replace("\\", "\\\\").replace("|", "\\|") + "<br>second"
        self.put("20261101T100000-1", "v1", [("10-sgr", "no", cell)])
        p = self.feedback("--tsv", "20261101T100000-1")
        self.assertEqual(p.returncode, 0, p.stderr)
        row = [l for l in p.stdout.splitlines() if not l.startswith("#")]
        self.assertEqual(row, ["10-sgr\tno\t" + (typed + "\nsecond").replace("\\", "\\\\").replace("\n", "\\n")])
        p = self.feedback("20261101T100000-1")
        self.assertIn("10-sgr                    no\n    " + typed + "\n    second\n", p.stdout)

    def test_an_unknown_pass_is_refused(self):
        p = self.feedback("20991101T100000-1")
        self.assertEqual(p.returncode, 2)
        self.assertIn("no pass 20991101T100000-1", p.stderr)


ESCAPE = re.compile(r"\x1b(\[([0-9;:?$ ]*)([@-~])|\][^\x07\x1b]*(\x07|\x1b\\)|[P_][^\x1b]*\x1b\\|[()][0-9A-Z]|.)")


def widest(line):
    """The furthest column a line of a page's output reaches, as a terminal
    would place it: a carriage return goes back to column 0, CUP and CHA move
    to their column, other escape sequences take none, wide characters two
    columns, REP the last character's width again for each repeat, and
    combining marks, joiners and selectors none."""
    col = most = last = 0
    at = 0
    while at < len(line):
        m = ESCAPE.match(line, at)
        if m:
            if m.group(3) == "b":
                # REP repeats the last character printed
                col += int(m.group(2) or 1) * last
                most = max(most, col)
            elif m.group(3) in ("H", "f", "G"):
                params = (m.group(2) or "").split(";")
                n = params[-1] if m.group(3) != "G" else params[0]
                col = max(int(n or 1) - 1, 0) if n.isdigit() or n == "" else col
            at = m.end()
            continue
        ch = line[at]
        at += 1
        if ch == "\r":
            col = 0
        elif unicodedata.combining(ch) or ch in "\u200d\ufe0e\ufe0f\u20e3" or ord(ch) < 0x20 or \
                0x1f3fb <= ord(ch) <= 0x1f3ff:
            continue
        else:
            last = 2 if unicodedata.east_asian_width(ch) in "WF" else 1
            col += last
            most = max(most, col)
    return most


def extent(text, top=3, cell=(10, 20)):
    """The last row (1-based) and the furthest column a page's output reaches
    on a terminal whose cell is CELL pixels (width, height), starting at row
    TOP: text and cursor moves as a terminal makes them, and each picture the
    rows and columns it covers."""
    ESCAPE = re.compile(r"\x1b(\[([0-9;:?$ ]*)([@-~])|\][^\x07\x1b]*(\x07|\x1b\\)|([P_])([^\x1b]*)\x1b\\|[()][0-9A-Z]|.)")
    row, col, rows, cols, last = top, 0, top, 0, 1
    at = 0
    def cover(r, c):
        nonlocal rows, cols
        rows, cols = max(rows, r), max(cols, c)
    while at < len(text):
        m = ESCAPE.match(text, at)
        if m:
            at = m.end()
            if m.group(3):
                p = [int(x) if x.isdigit() else 0 for x in (m.group(2) or "").split(";")]
                f = m.group(3)
                if f in "Hf":
                    row = p[0] or 1
                    col = (p[1] if len(p) > 1 and p[1] else 1) - 1
                elif f == "G":
                    col = (p[0] or 1) - 1
                elif f == "d":
                    row = p[0] or 1
                elif f in "BE":
                    row += p[0] or 1
                elif f == "A":
                    row -= p[0] or 1
                elif f == "C":
                    col += p[0] or 1
                elif f == "b":
                    col += (p[0] or 1) * last
                    cover(row, col)
            elif m.group(4) and m.group(0).startswith("\x1b]1337;File="):
                keys = dict(kv.split("=", 1) for kv in m.group(0)[len("\x1b]1337;File="):].split(":")[0].split(";") if "=" in kv)
                cover(row + int(keys.get("height", 1)) - 1, col + int(keys.get("width", 1)))
            elif m.group(5) == "_" and m.group(6).startswith("G"):
                keys = dict(kv.split("=", 1) for kv in m.group(6)[1:].split(";")[0].split(",") if "=" in kv)
                if keys.get("a") in ("T", "p"):
                    cover(row + int(keys.get("r", 1)) - 1, col + int(keys.get("c", 1)))
            elif m.group(5) == "P":
                raster = re.search(r'"\d+;\d+;(\d+);(\d+)', m.group(6))
                if raster:
                    w, h = int(raster.group(1)), int(raster.group(2))
                    cover(row + math.ceil(h / cell[1]) - 1, col + math.ceil(w / cell[0]))
            continue
        ch = text[at]
        at += 1
        if ch == "\r":
            col = 0
        elif ch == "\n":
            row += 1
            cover(row, col)
        elif unicodedata.combining(ch) or ord(ch) < 0x20 or ch in "‍︎️":
            continue
        else:
            last = 2 if unicodedata.east_asian_width(ch) in "WF" else 1
            col += last
            cover(row, col)
    return rows, cols


TEXT = os.path.join(SECTIONS, "text", "udhr-article1.tsv")
LANGUAGES = [l.split("\t") for l in read(TEXT).splitlines() if not l.startswith("#")]
SENTENCES = {l[2] for l in LANGUAGES}


def responder_run(page, answers, timeout=WAIT_BOUND):
    """Runs PAGE on a pseudo-terminal that answers each query in ANSWERS, a
    list of (query, answer), and returns its exit status and everything it
    sent."""
    pid, fd = pty.fork()
    if pid == 0:
        os.execve("/bin/bash", ["/bin/bash", page], {"PATH": os.environ["PATH"], "TERM": "linux", "ACCEPT_TOP": "3"})
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 25, 80, 0, 0))
    seen, answered, end = b"", 0, time.monotonic() + timeout
    while time.monotonic() < end:
        if select.select([fd], [], [], 0.1)[0]:
            try:
                data = os.read(fd, 65536)
            except OSError:
                data = b""
            if not data:
                break
            seen += data
            while True:
                found = [(seen.find(q, answered), q, a) for q, a in answers if seen.find(q, answered) >= 0]
                if not found:
                    break
                at, q, a = min(found)
                answered = at + len(q)
                os.write(fd, a)
    _, status = os.waitpid(pid, 0)
    os.close(fd)
    return os.waitstatus_to_exitcode(status), seen


PICTURES = ["95-sixel.sh", "96-kitty.sh", "97-kitty-layers.sh", "98-kitty-png.sh", "99-iterm2.sh"]
KITTY_QUERY = b"\x1b_Gi=31,s=1,v=1,a=q,f=24;AAAA\x1b\\\x1b[c"
HELPER_QUERY = b"\x1b_Gi=32,a=q,f=100;"


class Pages(Case):
    """The real pages in tools/vt/sections."""

    def page_files(self):
        return sorted(f for f in os.listdir(SECTIONS) if f.endswith(".sh"))

    def test_every_page_has_its_guidance_and_fits_80x25(self):
        for f in self.page_files():
            with self.subTest(page=f):
                text = read(os.path.join(SECTIONS, f))
                for key in ("look-for", "wrong-looks-like"):
                    joined = " ".join(re.findall(rf"^# {key}: (.*)$", text, re.M))
                    self.assertTrue(joined, f"no # {key}: lines")
                    # four rows of the footer at 80 columns, after its label
                    self.assertLessEqual(len(joined), 4 * 69, key)
                p = subprocess.run(["bash", os.path.join(SECTIONS, f)], capture_output=True, timeout=WAIT_BOUND,
                                   stdin=subprocess.DEVNULL, env={"PATH": os.environ["PATH"]})
                self.assertIn(p.returncode, (0, 77), p.stderr)
                self.assertEqual(p.stderr, b"")
                lines = p.stdout.decode().split("\n")[:-1]
                # a language page's sentence may wrap onto a second row;
                # every other line fits its row
                rows = 0
                for line in lines:
                    w = widest(line)
                    rows += max(1, -(-w // 80))
                    # wide-margin puts characters at the right edge on
                    # purpose, and reads the width it does it at
                    if f != "55-wide-margin.sh" and line not in SENTENCES:
                        self.assertLessEqual(w, 80, line)
                    self.assertLessEqual(w, 160, line)
                self.assertLessEqual(rows, 17)

    def test_every_page_is_a_file_of_its_own_numbered_in_order(self):
        names = self.page_files()
        self.assertEqual(len(set(n[:2] for n in names)), len(names), "two pages share a number")
        for n in names:
            self.assertRegex(n, r"^[0-9]{2}-[a-z0-9-]+\.sh$")

    def picture_pages(self):
        return [f for f in self.page_files() if f in PICTURES]

    def test_a_build_without_pictures_gets_skip_from_every_picture_page(self):
        for f in self.picture_pages():
            with self.subTest(page=f):
                status, seen = responder_run(os.path.join(SECTIONS, f), [(KITTY_QUERY, DA1_TEXT),
                                                                         (b"\x1b[c", DA1_TEXT)])
                self.assertEqual(status, 77)
                self.assertIn(b"This build cannot show this page.", seen)
                self.assertIn(b"vtium was built without pictures", seen)
                self.assertNotIn(b"\x1bPq", seen.replace(b"\x1bP0;1;0q", b"\x1bPq"))

    def test_the_helper_pages_skip_when_the_helper_does_not_decode(self):
        for f in ("98-kitty-png.sh", "99-iterm2.sh"):
            with self.subTest(page=f):
                status, seen = responder_run(os.path.join(SECTIONS, f), [
                    (KITTY_QUERY, b"\x1b_Gi=31;OK\x1b\\" + DA1_PICTURES),
                    (HELPER_QUERY, b"\x1b_Gi=32;EINVAL:no helper\x1b\\")])
                self.assertEqual(status, 77)
                self.assertIn(b"the image helper did not decode a PNG", seen)

    def test_a_build_with_pictures_gets_each_picture_page(self):
        answers = [(KITTY_QUERY, b"\x1b_Gi=31;OK\x1b\\" + DA1_PICTURES), (b"\x1b[c", DA1_PICTURES),
                   (HELPER_QUERY, b"\x1b_Gi=32;OK\x1b\\"), (b"\x1b[16t", b"\x1b[6;20;10t")]
        want = {
            "95-sixel.sh": [b'\x1bP0;1;0q"1;1;60;40#1;2;100;0;0', b'\x1bP0;1;0q"1;1;540;60#1;2;100;60;0'],
            "96-kitty.sh": [b"\x1b_Ga=T,i=1,q=2,C=1,f=32,s=32,v=16,c=20,r=5,z=-1073741825,m=0;",
                            b"\x1b_Ga=T,i=2,q=2,C=1,f=32,s=32,v=32,c=20,r=5,z=-1,m=1;", b"\x1b\\\x1b_Gm=0;",
                            b"\x1b_Ga=T,i=3,q=2,C=1,f=24,s=32,v=16,c=20,r=5,z=0,m=0;",
                            b"\x1b_Ga=p,i=1,q=2,c=4,r=2,z=-1073741825;\x1b\\<cursor"],
            "97-kitty-layers.sh": [b"\x1b_Ga=t,i=5,f=32,s=32,v=24,q=2,m=0;", b"\x1b_Ga=d,d=i,i=5,p=3,q=2;\x1b\\"],
            "98-kitty-png.sh": [b"\x1b_Ga=T,i=7,q=2,C=1,f=100,c=20,r=5,m=0;iVBORw0KGgo",
                                b"\x1b_Ga=T,i=9,q=2,C=1,f=24,s=32,v=16,o=z,c=20,r=5,z=0,m=0;eNr"],
            "99-iterm2.sh": [b"\x1b]1337;File=inline=1;width=20;height=5;preserveAspectRatio=0:iVBORw0KGgo"],
        }
        self.assertEqual(sorted(want), sorted(PICTURES))
        self.assertEqual(self.picture_pages(), sorted(PICTURES))
        for f, sent in want.items():
            with self.subTest(page=f):
                status, seen = responder_run(os.path.join(SECTIONS, f), answers)
                self.assertEqual(status, 0, seen[-500:])
                self.assertNotIn(b"cannot show", seen)
                for s in sent:
                    self.assertIn(s, seen)


    def test_each_picture_page_fits_80x25_as_it_draws_its_pictures(self):
        # As a build with pictures draws them, with a cell 10 pixels wide and
        # 20 tall: every row and column the text and the pictures cover, from
        # the row the walk starts the page on, within the 17 rows and 80
        # columns a page has.
        answers = [(KITTY_QUERY, b"\x1b_Gi=31;OK\x1b\\" + DA1_PICTURES), (b"\x1b[c", DA1_PICTURES),
                   (HELPER_QUERY, b"\x1b_Gi=32;OK\x1b\\"), (b"\x1b[16t", b"\x1b[6;20;10t")]
        for f in self.picture_pages():
            with self.subTest(page=f):
                status, seen = responder_run(os.path.join(SECTIONS, f), answers)
                self.assertEqual(status, 0, seen[-500:])
                rows, cols = extent(seen.decode(errors="replace"), top=3, cell=(10, 20))
                self.assertLessEqual(rows, 3 + 17 - 1)
                self.assertLessEqual(cols, 80)


def flat(text):
    """TEXT with each run of spaces and line breaks one space, as a page's
    folded lines read."""
    return " ".join(text.split())


def sgr_blinks(text):
    """Whether TEXT sets SGR 5 or 6, blinking text, in any SGR sequence; the
    5 of an extended color (38;5;N, 48;5;N, 58;5;N) is no blink."""
    for m in re.finditer(r"\x1b\[([0-9;:]*)m", text):
        params = m.group(1).split(";")
        i = 0
        while i < len(params):
            p = params[i]
            if p in ("38", "48", "58"):
                i += 3 if params[i + 1:i + 2] == ["5"] else 5
                continue
            if p in ("5", "6"):
                return True
            i += 1
    return False


class Blink(Case):
    """The pages that blink text name the blink style vtium draws it in, read
    from the config file vtium reads (tools/vt/blink.inc), and the record
    keeps it."""

    def page(self, name, **env):
        return subprocess.run(["bash", os.path.join(SECTIONS, name)], capture_output=True, text=True, timeout=WAIT_BOUND,
                              stdin=subprocess.DEVNULL, env={"PATH": os.environ["PATH"], **env})

    def conf(self, text, mode=0o644):
        path = os.path.join(self.tmp, "vtium.conf")
        if os.path.exists(path):
            os.unlink(path)
        write(path, text, mode)
        return path

    def test_every_page_that_blinks_text_names_the_style(self):
        pages = [f for f in sorted(os.listdir(SECTIONS)) if f.endswith(".sh")]
        blinking = []
        for f in pages:
            p = self.page(f)
            if sgr_blinks(p.stdout):
                blinking.append(f)
                with self.subTest(page=f):
                    self.assertIn("\nBlink: ", p.stdout)
                    self.assertIn('"Blink:" line', read(os.path.join(SECTIONS, f)))
        self.assertEqual(blinking, ["10-sgr.sh", "35-blink.sh"])

    def test_each_style_and_where_it_comes_from(self):
        right = {
            "full": "Blink: full. Right: blinking text disappears in the off phase, about once a second, and comes back.",
            "partial": "Blink: partial. Right: blinking text dims halfway to its background about once a second and "
                       "never disappears.",
            "off": "Blink: off. Right: blinking text stays steady and never changes.",
        }
        for value in ("full", "partial", "off"):
            with self.subTest(value=value):
                # the last blink-text= line wins; comments and a value vtium
                # cannot use are ignored, and spaces around = are dropped
                conf = self.conf(f"# blink-text=nonsense\nblink-text=full\n  blink-text = {value}  # the operator's\n"
                                 "blink-text=sideways\n#blink-text=off\n")
                for f in ("10-sgr.sh", "35-blink.sh"):
                    out = self.page(f, ACCEPT_VTIUM_CONF=conf).stdout
                    self.assertIn(right[value], flat(out))
                    self.assertIn(f"The blink style is blink-text={value} in {conf}.", flat(out))
        missing = os.path.join(self.tmp, "none.conf")
        out = self.page("35-blink.sh", ACCEPT_VTIUM_CONF=missing).stdout
        self.assertIn(right["partial"], flat(out))
        self.assertIn(f"The blink style is vtium's default; {missing} does not exist.", flat(out))
        # a file the user cannot read is said to be one, and the default assumed
        if os.geteuid() != 0:
            conf = self.conf("blink-text=off\n", 0o000)
            out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=conf).stdout)
            self.assertIn("Blink: partial.", out)
            self.assertIn(f"{conf} cannot be read as you", out)
        # the blink switch off keeps every blinking text steady
        conf = self.conf("blink-text=full\nno-blink\n")
        out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=conf).stdout)
        self.assertIn("Blink: off.", out)
        self.assertIn(f"The blink style is no-blink in {conf}.", out)
        # vtium's command line wins over the file
        out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=conf, ACCEPT_BLINK="on",
                             ACCEPT_BLINK_TEXT="off").stdout)
        self.assertIn("Blink: off.", out)
        self.assertIn("The blink style is --blink-text=off on vtium's command line.", out)

    def keys_with(self, default):
        """A copy of the checkout's key table with blink-text's default set to
        DEFAULT, or with none when DEFAULT is None."""
        text = read(KEYS)
        at = text.index('name: "blink-text",')
        line = re.compile(r'default: Default::Text\("[a-z]+"\),')
        m = line.search(text, at)
        new = "" if default is None else f'default: Default::Text("{default}"),'
        path = os.path.join(self.tmp, "keys.rs")
        write(path, text[:m.start()] + new + text[m.end():])
        return path

    def test_the_default_is_the_key_tables(self):
        # The default the pages name is the one the checkout's key table
        # gives vtium, read independently of blink.inc here.
        text = read(KEYS)
        at = text.index('name: "blink-text",')
        table = re.compile(r'default: Default::Text\("([a-z]+)"\)').search(text, at).group(1)
        missing = os.path.join(self.tmp, "none.conf")
        out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=missing).stdout)
        self.assertIn(f"Blink: {table}.", out)
        self.assertIn("The blink style is vtium's default;", out)
        # a table with another default moves the page with it
        for default in ("full", "off", "partial"):
            with self.subTest(default=default):
                out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=missing,
                                     ACCEPT_VTIUM_KEYS=self.keys_with(default)).stdout)
                self.assertIn(f"Blink: {default}.", out)
                self.assertIn("The blink style is vtium's default;", out)
        # a table that gives none: the page says it cannot tell
        out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=missing, ACCEPT_VTIUM_KEYS=self.keys_with(None)).stdout)
        self.assertIn("Blink: unknown.", out)
        self.assertIn("the checkout's key table gives no default for blink-text", out)

    def test_partial_names_the_1_bit_display_and_each_page_fits(self):
        bit = "On a 1-bit display vtium draws partial as full: blinking text hides."
        cases = {"full": "blink-text=full\n", "partial": "blink-text=partial\n", "off": "blink-text=off\n"}
        for value, text in cases.items():
            with self.subTest(value=value):
                conf = self.conf(text)
                out = flat(self.page("35-blink.sh", ACCEPT_VTIUM_CONF=conf).stdout)
                (self.assertIn if value == "partial" else self.assertNotIn)(bit, out)
        # every page that blinks fits its 17 rows and 80 columns with the
        # longest source line, a file that cannot be read
        conf = self.conf("blink-text=off\n", 0o000)
        for f in ("10-sgr.sh", "35-blink.sh"):
            with self.subTest(page=f):
                lines = self.page(f, ACCEPT_VTIUM_CONF=conf).stdout.split("\n")[:-1]
                self.assertLessEqual(len(lines), 17)
                for line in lines:
                    self.assertLessEqual(widest(line), 80, line)

    def test_the_record_keeps_the_style_and_a_compare_names_a_change(self):
        for value in ("full", "partial"):
            conf = self.conf(f"blink-text={value}\n")
            d = self.walk(self.env(ACCEPT_VTIUM_CONF=conf), "one")
            d.wait_for(b"press a key to start the pass")
            d.send(b" ")
            d.wait_for(ASK)
            d.send(b"y")
            d.wait_for(b"q exit")
            d.send(b"q")
            self.assertEqual(d.finish(), 0)
        names = sorted(n for n in os.listdir(self.passes) if n.endswith(".md"))
        self.assertEqual(len(names), 2)
        texts = [read(os.path.join(self.passes, n)) for n in names]
        self.assertIn(f"- blink-text: full (blink-text=full in {conf}); the pages that blink text are judged in it\n",
                      texts[0])
        self.assertIn(f"- blink-text: partial (blink-text=partial in {conf}); the pages that blink text are judged in "
                      "it\n", texts[1])
        p = self.feedback("--compare", *(os.path.join(self.passes, n) for n in names))
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("the blink style differs, so a changed blink page may come from it:\n  A: full (", p.stdout)
        self.assertIn("\n  B: partial (", p.stdout)
        # the same style twice: no such lines
        p = self.feedback("--compare", os.path.join(self.passes, names[1]), os.path.join(self.passes, names[1]))
        self.assertNotIn("blink style differs", p.stdout)


class Frits(Case):
    """A page walked once in each font set its "# frits:" line names."""

    def setUp(self):
        super().setUp()
        for name in os.listdir(self.pages):
            os.unlink(os.path.join(self.pages, name))
        os.mkdir(os.path.join(self.pages, "frits"))
        self.font = os.path.join(self.tmp, "font.ttf")
        write(self.font, "the outline font's bytes\n")
        self.gone = os.path.join(self.tmp, "gone.ttf")
        write(os.path.join(self.pages, "frits", "outline.vtfrit"),
              f"# checks: the outline font\nname=outline\nsource=outline:{self.font}\nsource=builtin:unifont\n")
        write(os.path.join(self.pages, "frits", "absent.vtfrit"),
              f"name=absent\nsource=outline:{self.gone}\n")
        write(os.path.join(self.pages, "frits", "refused.vtfrit"), "name=refused\nsource=builtin:unifont\n")
        write(os.path.join(self.pages, "10-lang.sh"),
              "# look-for: the words\n# wrong-looks-like: anything else\n# frits: outline absent refused\n"
              "printf 'drawn in %s\\n' \"$ACCEPT_FRIT\"\n")
        write(os.path.join(self.pages, "20-plain.sh"),
              "# look-for: plain\n# wrong-looks-like: anything else\nprintf 'page plain\\n'\n")

    def sha(self, path):
        with open(path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()

    def walk_all(self):
        # vtium's frits directory holds outline and absent, not refused
        d = self.walk(self.env(), frits=("unifont", "outline", "absent"))
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(b"[1/4] lang @ outline")
        d.wait_for(b"drawn in outline")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"[2/4] lang @ absent")
        d.wait_for(b"the font set absent needs files this machine lacks: " + self.gone.encode())
        d.wait_for(b"recorded as skip  any key: next page")
        d.send(b" ")
        d.wait_for(b"[3/4] lang @ refused")
        d.wait_for(b"vtium did not switch to the font set refused: OSC 50 named 'outline'")
        d.wait_for(b"recorded as skip  any key: next page")
        d.send(b" ")
        d.wait_for(b"page plain")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        return d

    def test_each_frit_is_its_own_entry_and_a_missing_or_refused_one_is_a_skip(self):
        d = self.walk_all()
        _, text = self.record()
        self.assertEqual(self.rows(text), [
            ("10-lang@outline", "yes", ""),
            ("10-lang@absent", "skip", "the font set absent needs files this machine lacks: " + self.gone),
            ("10-lang@refused", "skip", "vtium did not switch to the font set refused: OSC 50 named &#39;outline&#39;"
             .replace("&#39;", "'")),
            ("20-plain", "yes", ""),
        ])
        self.assertIn("- font-start: unifont\n", text)
        self.assertIn(f"- frit: outline sha256 {self.sha(os.path.join(self.pages, 'frits', 'outline.vtfrit'))}\n",
                      text)
        self.assertRegex(text, rf"- font: outline {re.escape(self.font)} sha256 {self.sha(self.font)}( package .*)?\n")
        self.assertIn(f"- font: absent {self.gone} missing\n", text)
        # the console's own font comes back for the page that names none
        seen = d.seen
        self.assertIn(b"\x1b]50;unifont\x07", seen[seen.index(b"[3/4]"):seen.index(b"page plain")])

    def test_a_pass_never_resumes_across_a_changed_font_file(self):
        d = self.walk(self.env(), frits=("unifont", "outline"))
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(b"recorded as skip  any key: next page")
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        write(self.font, "the outline font, updated\n")
        d = self.walk(self.env(), frits=("unifont", "outline"))
        d.wait_for(b"press a key to start the pass")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertNotIn(b"r resume it", d.seen)

    def test_compare_names_the_font_files_that_changed(self):
        self.walk_all()
        first, _ = self.record()
        os.rename(first, os.path.join(self.tmp, "first.md"))
        write(self.font, "the outline font, updated\n")
        self.walk_all()
        p = self.feedback("--compare", os.path.join(self.tmp, "first.md"), self.record()[0])
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("font files that differ, so a changed page may come from a font update:\n", p.stdout)
        self.assertIn(f"  A: outline {self.font} sha256 ", p.stdout)
        self.assertIn(f"  B: outline {self.font} sha256 {self.sha(self.font)}", p.stdout)


class Languages(Case):
    """The language pages and their text."""

    def test_every_language_page_names_a_language_of_the_text_and_both_frits(self):
        codes = [l[0] for l in LANGUAGES]
        pages = [f for f in sorted(os.listdir(SECTIONS))
                 if f.endswith(".sh") and "language_page" in read(os.path.join(SECTIONS, f))]
        self.assertEqual(len(pages), 13)
        for f in pages:
            text = read(os.path.join(SECTIONS, f))
            code = re.search(r"^language_page (\S+)$", text, re.M).group(1)
            self.assertIn(code, codes)
            self.assertIn("# frits: acceptance-outline acceptance-bitmap\n", text)
        for frit in ("acceptance-outline", "acceptance-bitmap"):
            self.assertTrue(os.path.isfile(os.path.join(SECTIONS, "frits", frit + ".vtfrit")))

    def test_the_text_is_whole_and_each_excerpt_is_in_its_sentence(self):
        by = {l[0]: l for l in LANGUAGES}
        for code, fields in by.items():
            with self.subTest(language=code):
                self.assertEqual(len(fields), 6)
                if code not in ("vietnamese-marks", "emoji"):
                    self.assertIn(unicodedata.normalize("NFC", fields[3]), unicodedata.normalize("NFC", fields[2]))
        self.assertEqual(by["vietnamese-marks"][2], unicodedata.normalize("NFD", by["vietnamese"][2]))
        self.assertNotEqual(by["vietnamese-marks"][2], by["vietnamese"][2])
        self.assertEqual(by["english"][2], "All human beings are born free and equal in dignity and rights.")

    def test_a_language_page_shows_every_style_and_says_which_are_not_drawn(self):
        p = subprocess.run(["bash", os.path.join(SECTIONS, "84-greek.sh")], capture_output=True, timeout=WAIT_BOUND,
                           stdin=subprocess.DEVNULL,
                           env={"PATH": os.environ["PATH"], "ACCEPT_FRIT": "acceptance-bitmap", "ACCEPT_TOP": "3"})
        self.assertEqual(p.returncode, 0, p.stderr)
        out = p.stdout.decode()
        for sgr in ("0", "1", "3", "1;3", "2", "7", "4", "4:2", "4:3", "4:4", "4:5", "4;58:5:196", "9", "53", "73", "74"):
            self.assertIn(f"\x1b[{sgr}mελεύθεροι και ίσοι\x1b[0m", out)
        self.assertIn("* not drawn yet: these styles look plain.", out)
        self.assertNotIn("italic*", out)
        self.assertIn("Font acceptance-bitmap: whole-multiple bitmaps; marks placed by vtium's rules", out)


class Hardware(Case):
    """What a pass records of the machine it ran on."""

    HARDWARE = ("- machine: Example Boards EB-1 2.0\n"
                "- kernel: 7.2.14-test\n"
                "- gpu: card0 driver amdgpu, 0x1002:0x744c at 0000:03:00.0\n"
                "- display: DP-1, preferred mode 3840x2160, EDID make DEL product 0xA0F1 model DELL U2723QE\n"
                "- text-area: 640x400 pixels, as vtium reports it (XTWINOPS 14)\n")

    def one_pass(self, quit=False):
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b" ")
        d.wait_for(ASK)
        d.send(b"y")
        d.wait_for(ASK)
        d.send(b"q" if quit else b"y")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)

    def test_a_pass_names_its_hardware_and_says_it_is_evidence_about_it_only(self):
        self.one_pass()
        _, text = self.record()
        self.assertIn(self.HARDWARE, text)
        self.assertRegex(text, r"- hardware: [0-9a-f]{64}; this pass is evidence about the hardware above only\n")

    def test_a_pass_never_resumes_on_other_hardware(self):
        self.one_pass(quit=True)
        self.machine(kernel="7.3.0-test")
        d = self.walk(self.env(), "one", "two")
        d.wait_for(b"press a key to start the pass")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertNotIn(b"r resume it", d.seen)

    def test_compare_warns_when_two_passes_ran_on_different_hardware(self):
        self.one_pass()
        first, _ = self.record()
        os.rename(first, os.path.join(self.tmp, "first.md"))
        self.machine(kernel="7.3.0-test")
        self.one_pass()
        p = self.feedback("--compare", os.path.join(self.tmp, "first.md"), self.record()[0])
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("\nWARNING: the two passes ran on different hardware, and each is evidence about its own only:\n"
                      "  A: kernel: 7.2.14-test\n  B: kernel: 7.3.0-test\n", p.stdout)
        # the same hardware twice: no warning
        self.one_pass()
        newest = sorted(n for n in os.listdir(self.passes) if n.endswith(".md"))
        p = self.feedback("--compare", *[os.path.join(self.passes, n) for n in newest[-2:]])
        self.assertNotIn("WARNING", p.stdout)


if __name__ == "__main__":
    unittest.main()
