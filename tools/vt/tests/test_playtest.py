#!/usr/bin/env python3
"""Tests the playtest (ADR-1501): demo.sh's walk through the pages, the pass
record it writes, the resume, and feedback.sh's reading and comparing of
records, all on a pseudo-terminal with scripted keys and no VT or sudo. The
walk runs over stand-in pages (PLAYTEST_PAGES), and the real pages in
tools/vt/sections are checked on their own: their guidance, their size, and
what they send when a terminal says it shows pictures and when it says it
does not.

    python3 -I tools/vt/tests/test_playtest.py [-v] [TestCase.test_name ...]
"""

import fcntl
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

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
VT = os.path.join(ROOT, "tools", "vt")
DEMO_SH = os.path.join(VT, "demo.sh")
FEEDBACK_SH = os.path.join(VT, "feedback.sh")
SECTIONS = os.path.join(VT, "sections")

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
    """A program on a pseudo-terminal of 80x25, read up to each prompt before
    it is answered. It answers DA1 with da1 and the cursor position query
    with row 10, as a terminal would."""

    def __init__(self, argv, env, da1=DA1_TEXT):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(argv[0], argv, env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", 25, 80, 0, 0))
        self.da1 = da1
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
        """Answers every query seen since the last answer, in order."""
        while True:
            rest = self.seen[self.answered:]
            found = [(rest.find(q), q, a) for q, a in ((b"\x1b[c", self.da1), (b"\x1b[6n", b"\x1b[10;1R"))
                     if rest.find(q) >= 0]
            if not found:
                return
            at, q, a = min(found)
            self.answered += at + len(q)
            os.write(self.fd, a)

    def wait_for(self, text, timeout=10):
        end = time.monotonic() + timeout
        while text not in self.out:
            if time.monotonic() > end or not self.pump(end - time.monotonic()):
                raise AssertionError(f"no {text!r} in:\n{self.out[-2000:].decode(errors='replace')}")
        self.out = self.out[self.out.index(text) + len(text):]

    def send(self, data):
        os.write(self.fd, data)

    def finish(self, timeout=10):
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
        self.tmp = tempfile.mkdtemp(prefix="vtium-playtest-")
        self.passes = os.path.join(self.tmp, "passes")
        self.pages = os.path.join(self.tmp, "pages")
        os.mkdir(self.pages)
        for name, text in PAGES.items():
            write(os.path.join(self.pages, name), text)

    def tearDown(self):
        subprocess.run(["rm", "-rf", "--", self.tmp], check=True)

    def env(self, label="vtium dev build v1-test", **extra):
        env = {"PATH": os.environ["PATH"], "TERM": "linux", "HOME": self.tmp,
               "PLAYTEST_DIR": self.passes, "PLAYTEST_PAGES": self.pages, "DEMO_LABEL": label}
        env.update(extra)
        return env

    def walk(self, env, *argv, da1=DA1_TEXT):
        return Pty(["/bin/bash", DEMO_SH, *argv], env, da1)

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
                              timeout=20)


class Walk(Case):
    """demo.sh's pass: the keys, the record, back, quit and resume."""

    def test_a_pass_records_each_answer_its_comment_and_the_build(self):
        d = self.walk(self.env(DEMO_VT="5", DEMO_BINARY="/build/vtium", DEMO_FEATURES="images"))
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
        self.assertRegex(lines[0], r"^# vtium playtest pass \d{8}T\d{6}-\d+$")
        meta = {m.group(1): m.group(2) for m in (re.match(r"^- ([a-z0-9]+): (.*)$", l) for l in lines) if m}
        self.assertEqual(meta["label"], "vtium dev build v1-test")
        self.assertEqual(meta["vt"], "5")
        self.assertEqual(meta["binary"], "/build/vtium")
        self.assertEqual(meta["features"], "images")
        self.assertEqual(meta["terminal"], "TERM=linux, 25 rows by 80 columns")
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


RECORD = """# vtium playtest pass {id}

- pass: {id}
- label: {label}
- vt: 5
- binary: not given
- features: not given
- terminal: TERM=xterm-vtium, 25 rows by 80 columns
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
        # As demo.sh writes it: & < > as entities, \ and | escaped.
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
    columns, and combining marks, joiners and selectors none."""
    col = most = 0
    at = 0
    while at < len(line):
        m = ESCAPE.match(line, at)
        if m:
            if m.group(3) in ("H", "f", "G"):
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
            col += 2 if unicodedata.east_asian_width(ch) in "WF" else 1
            most = max(most, col)
    return most


def responder_run(page, answers, timeout=20):
    """Runs PAGE on a pseudo-terminal that answers each query in ANSWERS, a
    list of (query, answer), and returns its exit status and everything it
    sent."""
    pid, fd = pty.fork()
    if pid == 0:
        os.execve("/bin/bash", ["/bin/bash", page], {"PATH": os.environ["PATH"], "TERM": "linux", "DEMO_TOP": "3"})
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
                    # two rows of the footer at 80 columns, after its label
                    self.assertLessEqual(len(joined), 4 * 69, key)
                p = subprocess.run(["bash", os.path.join(SECTIONS, f)], capture_output=True, timeout=20,
                                   stdin=subprocess.DEVNULL, env={"PATH": os.environ["PATH"]})
                self.assertIn(p.returncode, (0, 77), p.stderr)
                self.assertEqual(p.stderr, b"")
                lines = p.stdout.decode().split("\n")
                self.assertLessEqual(len(lines) - 1, 17)
                # wide-margin puts characters at the right edge on purpose,
                # and reads the width it does it at
                if f != "55-wide-margin.sh":
                    for line in lines:
                        self.assertLessEqual(widest(line), 80, line)

    def test_every_page_is_a_file_of_its_own_numbered_in_order(self):
        names = self.page_files()
        self.assertEqual(len(set(n[:2] for n in names)), len(names), "two pages share a number")
        for n in names:
            self.assertRegex(n, r"^[0-9]{2}-[a-z0-9-]+\.sh$")

    def picture_pages(self):
        return [f for f in self.page_files() if f.startswith("9")]

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
        for f in ("93-kitty-png.sh", "94-iterm2.sh"):
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
            "90-sixel.sh": [b'\x1bP0;1;0q"1;1;60;40#1;2;100;0;0', b'\x1bP0;1;0q"1;1;540;60#1;2;100;60;0'],
            "91-kitty.sh": [b"\x1b_Ga=T,i=1,q=2,C=1,f=32,s=32,v=16,c=20,r=5,z=-1073741825,m=0;",
                            b"\x1b_Ga=T,i=2,q=2,C=1,f=32,s=32,v=32,c=20,r=5,z=-1,m=1;", b"\x1b\\\x1b_Gm=0;",
                            b"\x1b_Ga=T,i=3,q=2,C=1,f=24,s=32,v=16,c=20,r=5,z=0,m=0;",
                            b"\x1b_Ga=p,i=1,q=2,c=4,r=2,z=-1073741825;\x1b\\<cursor"],
            "92-kitty-layers.sh": [b"\x1b_Ga=t,i=5,f=32,s=32,v=24,q=2,m=0;", b"\x1b_Ga=d,d=i,i=5,p=3,q=2;\x1b\\"],
            "93-kitty-png.sh": [b"\x1b_Ga=T,i=7,q=2,C=1,f=100,c=20,r=5,m=0;iVBORw0KGgo",
                                b"\x1b_Ga=T,i=9,q=2,C=1,f=24,s=32,v=16,o=z,c=20,r=5,z=0,m=0;eNr"],
            "94-iterm2.sh": [b"\x1b]1337;File=inline=1;width=20;height=5;preserveAspectRatio=0:iVBORw0KGgo"],
        }
        self.assertEqual(sorted(want), self.picture_pages())
        for f, sent in want.items():
            with self.subTest(page=f):
                status, seen = responder_run(os.path.join(SECTIONS, f), answers)
                self.assertEqual(status, 0, seen[-500:])
                self.assertNotIn(b"cannot show", seen)
                for s in sent:
                    self.assertIn(s, seen)


if __name__ == "__main__":
    unittest.main()
