"""Tests of tools/gallery/gallery: the bytes it writes for a directory of
pictures, its options, the files it skips, and its question to the
terminal, asked of a stand-in terminal (terminal.py). `make test-tools`
runs them."""

import base64
import os
import random
import re
import signal
import struct
import subprocess
import sys
import termios
import tempfile
import unittest
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import terminal  # noqa: E402

GALLERY = os.path.join(os.path.dirname(HERE), "gallery")
# What the tool asks the terminal before it draws.
PROBE = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c"


def png(width, height, rgb=(200, 80, 40), seed=None):
    """A PNG file of `width` by `height` pixels of one color, or with a
    `seed`, of noise from a seeded random source, which does not
    compress."""
    def chunk(kind, payload):
        return (struct.pack(">I", len(payload)) + kind + payload
                + struct.pack(">I", zlib.crc32(kind + payload)))
    noise = random.Random(seed)
    raw = b"".join(b"\0" + (noise.randbytes(3 * width) if seed is not None else bytes(rgb) * width)
                   for _ in range(height))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def kitty(keys, data):
    """The kitty command `keys` carrying `data` in base64 chunks of 4096
    bytes, as the tool sends it."""
    text = base64.b64encode(data)
    chunks = [text[i:i + 4096] for i in range(0, len(text), 4096)]
    out = b""
    for n, part in enumerate(chunks):
        more = int(n + 1 < len(chunks))
        head = b"%s,m=%d" % (keys, more) if n == 0 else b"m=%d" % more
        out += b"\x1b_G%s;%s\x1b\\" % (head, part)
    return out


class GalleryTest(unittest.TestCase):

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = os.path.join(tmp.name, "pictures")
        os.makedirs(self.dir)
        self.files = {}

    def add(self, name, data):
        with open(os.path.join(self.dir, name), "wb") as f:
            f.write(data)
        self.files[name] = data
        return data

    def gallery(self, *args, columns=40, lines=24, locale="C.UTF-8"):
        """The tool run with `args` in a session with no terminal, so its
        size comes from LINES and COLUMNS: (exit status, stdout, stderr)."""
        p = subprocess.run([GALLERY, *args], stdin=subprocess.DEVNULL, capture_output=True,
                           start_new_session=True, timeout=60, cwd=self.dir,
                           env={"PATH": os.environ["PATH"], "LC_ALL": locale,
                                "COLUMNS": str(columns), "LINES": str(lines)})
        return p.returncode, p.stdout, p.stderr

    def test_help(self):
        for flag in ("-h", "--help"):
            status, out, err = self.gallery(flag)
            self.assertEqual((status, err), (0, b""))
            self.assertTrue(out.startswith(b"usage: gallery [-s N] [-n N] [-L] [--no-probe]"))
            for word in (b"kitty, WezTerm, Ghostty or", b"skips any other file", b"--no-labels"):
                self.assertIn(word, out)

    def test_a_directory_in_name_order(self):
        # A square picture and one twice as wide, two rows tall in cells
        # twice as tall as wide: boxes of 4x2 and 8x2 cells in columns of
        # 10, each centered with its name under it; the text file is noted
        # and skipped. The same directory gives the same bytes every run.
        wide = self.add("b.png", png(32, 16))
        square = self.add("a.png", png(16, 16))
        self.add("c.txt", b"not a picture\n")
        status, out, err = self.gallery("--no-probe", "-s", "2", self.dir)
        self.assertEqual(status, 0)
        self.assertEqual(err, b"gallery: skipped 1 file(s) that are not PNG: c.txt\n")
        self.assertEqual(out, b"\n\n\x1b[2A"
                         + b"\x1b[4G" + kitty(b"a=T,f=100,t=d,q=2,C=1,c=4,r=2", square)
                         + b"\x1b[12G" + kitty(b"a=T,f=100,t=d,q=2,C=1,c=8,r=2", wide)
                         + b"\n\n\x1b[3Ga.png\x1b[13Gb.png\n")
        self.assertEqual(self.gallery("--no-probe", "-s", "2", self.dir)[1], out)

    def test_count_labels_and_rows(self):
        # -n takes the first pictures in name order and says how many more
        # there are; -L leaves the names out; pictures past one row of the
        # grid start a row of their own.
        for name in ("c.png", "a.png", "b.png"):
            self.add(name, png(16, 16))
        status, out, _ = self.gallery("--no-probe", "-s", "2", "-n", "2", self.dir)
        self.assertEqual(status, 0)
        self.assertEqual(out.count(b"a=T"), 2)
        self.assertIn(b"\x1b[1Ga.png\x1b[7Gb.png\n", out)
        self.assertNotIn(b"c.png", out)
        self.assertTrue(out.endswith(b"\n\xe2\x80\xa6 1 more\n"))
        status, out, _ = self.gallery("--no-probe", "-L", "-s", "2", self.dir)
        self.assertNotIn(b".png", out)
        self.assertEqual(out.count(b"\n"), 4)
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir, columns=6)
        self.assertEqual(out.count(b"a=T"), 3)
        self.assertEqual(out.count(b"\x1b[2A"), 3)

    def test_a_name_wider_than_its_column_is_cut(self):
        # In columns of 6, a name of 5 characters fits and one of 6 or more
        # is cut to 4 and an ellipsis.
        for name in ("abcde", "abcdef", "long-name.png"):
            self.add(name, png(16, 16))
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir, columns=18)
        self.assertEqual(status, 0)
        self.assertIn(b"\x1b[1Gabcde\x1b[7Gabcd\xe2\x80\xa6\x1b[13Glong\xe2\x80\xa6\n", out)

    def test_files_and_refusals(self):
        # Files given by name keep their order; a missing path, a bad
        # option or no PNG at all is refused on one line, before anything is
        # drawn.
        self.add("b.png", png(16, 16))
        self.add("a.png", png(16, 16))
        self.add("c.txt", b"text")
        b, a = (os.path.join(self.dir, n) for n in ("b.png", "a.png"))
        status, out, _ = self.gallery("--no-probe", "-s", "2", b, a)
        self.assertIn(b"\x1b[1Gb.png\x1b[7Ga.png\n", out)
        for args, status, line in [
                (["--no-probe", os.path.join(self.dir, "none")], 2,
                 b"gallery: no such file or directory: %s\n"
                 % os.path.join(self.dir, "none").encode()),
                (["--no-probe", os.path.join(self.dir, "c.txt")], 1,
                 b"gallery: skipped 1 file(s) that are not PNG: c.txt\n"
                 b"gallery: no PNG pictures in %s\n" % os.path.join(self.dir, "c.txt").encode()),
                (["-s", "0", self.dir], 2,
                 b"gallery: -s wants a whole number of 1 or more, not '0'\n"),
                (["-x"], 2, b"gallery: unknown option -x (gallery --help lists them)\n")]:
            with self.subTest(args=args):
                self.assertEqual(self.gallery(*args), (status, b"", line))

    def test_it_asks_the_terminal_first(self):
        # On a terminal that draws kitty graphics, the tool asks first and
        # sizes each box to the cell the terminal reports, 16x40 here, and
        # to the width stty gives; on one that does not, it says so on one
        # line and draws nothing.
        square = self.add("a.png", png(16, 16))
        status, out, _ = terminal.run([GALLERY, "-s", "2", self.dir],
                                   terminal.answers(cell=(16, 40)), cols=40)
        self.assertEqual(status, 0)
        self.assertEqual(out, PROBE + b"\n\n\x1b[2A\x1b[2G"
                         + kitty(b"a=T,f=100,t=d,q=2,C=1,c=5,r=2", square)
                         + b"\n\n\x1b[2Ga.png\n")
        status, out, _ = terminal.run([GALLERY, self.dir], terminal.answers(kitty=False))
        self.assertEqual(status, 1)
        self.assertEqual(out, PROBE + b"gallery: the terminal did not answer that it draws kitty "
                         b"graphics; gallery needs kitty, WezTerm, Ghostty or vtium (--no-probe "
                         b"draws anyway)\n")


    def test_a_silent_terminal_or_none(self):
        # A terminal that answers nothing at all ends the wait after a
        # second, and the tool says so on one line; with no terminal to
        # ask, it says that instead. The run's timeout only bounds the wait.
        self.add("a.png", png(16, 16))
        status, out, _ = terminal.run([GALLERY, self.dir], lambda query: b"", timeout=10)
        self.assertEqual(status, 1)
        self.assertEqual(out, PROBE + b"gallery: the terminal did not answer that it draws kitty "
                         b"graphics; gallery needs kitty, WezTerm, Ghostty or vtium (--no-probe "
                         b"draws anyway)\n")
        self.assertEqual(self.gallery(self.dir), (
            1, b"", b"gallery: no terminal to ask whether it draws kitty graphics "
                    b"(--no-probe draws anyway)\n"))

    def test_an_interrupted_question_restores_the_terminal(self):
        # Interrupted while it waits for the terminal's answer, the tool puts
        # back the terminal's modes, echo and line editing, before it ends.
        self.add("a.png", png(16, 16))
        status, out, modes = terminal.run([GALLERY, self.dir], terminal.answers(), timeout=10,
                                          cooked=True, interrupt=signal.SIGINT)
        self.assertEqual(status, 130)
        self.assertEqual(modes & (termios.ECHO | termios.ICANON), termios.ECHO | termios.ICANON)

    def test_long_pictures_go_in_chunks_of_4096(self):
        # A picture whose base64 passes 8192 bytes goes in three or more
        # chunks of 4096 and a last one, and one whose base64 is exactly
        # 8192 bytes in two of 4096, the last with m=0 and no empty chunk
        # after it.
        noise = self.add("a.png", png(64, 64, seed=1))
        self.assertGreater(len(base64.b64encode(noise)), 3 * 4096)
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir)
        self.assertEqual(status, 0)
        self.assertIn(kitty(b"a=T,f=100,t=d,q=2,C=1,c=4,r=2", noise), out)
        os.unlink(os.path.join(self.dir, "a.png"))
        exact = png(16, 16)
        exact = self.add("b.png", exact + b"\0" * (6144 - len(exact)))
        self.assertEqual(len(base64.b64encode(exact)), 8192)
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir)
        self.assertIn(kitty(b"a=T,f=100,t=d,q=2,C=1,c=4,r=2", exact), out)
        self.assertEqual(out.count(b"\x1b_G"), 2)

    def test_names_cannot_command_the_terminal(self):
        # Control characters in a name, ESC, BEL, a newline and C1's CSI,
        # reach the terminal as '?', in a picture's label, in the note of a
        # skipped file and in a refusal; the only escape sequences on stdout
        # are the tool's own, and stderr carries none.
        bad = b"\x1b]52;c;aGk=\x07\n\xc2\x9b"
        with open(os.path.join(self.dir.encode(), b"a" + bad + b".png"), "wb") as f:
            f.write(png(16, 16))
        with open(os.path.join(self.dir.encode(), b"b" + bad + b".txt"), "wb") as f:
            f.write(b"text")
        status, out, err = self.gallery("--no-probe", "-s", "10", self.dir, columns=80)
        self.assertEqual(status, 0)
        own = re.sub(rb"\x1b\[\d+[ABG]|\x1b_G[^\x1b]*\x1b\\", b"", out)
        for c in (b"\x1b", b"\x07", b"\xc2\x9b"):
            self.assertNotIn(c, own)
            self.assertNotIn(c, err)
        self.assertIn(b"a?]52;c;aGk=???.png", own)
        self.assertEqual(out.count(b"\n"), 21)
        self.assertEqual(err, b"gallery: skipped 1 file(s) that are not PNG: b?]52;c;aGk=???.txt\n")
        status, out, err = self.gallery(os.path.join(self.dir, "x\x1b]0;t\x07"))
        self.assertEqual((status, out), (2, b""))
        self.assertTrue(err.endswith(b"/x?]0;t?\n"), err)

    def test_sizes_from_the_environment_are_numbers(self):
        # LINES and COLUMNS that are not numbers are not read as arithmetic,
        # which would run a command, and 80 by 24 stands in.
        self.add("a.png", png(16, 16))
        canary = os.path.join(self.dir, "ran")
        p = subprocess.run([GALLERY, "--no-probe", "-s", "2", self.dir],
                           stdin=subprocess.DEVNULL, capture_output=True, start_new_session=True,
                           timeout=60, env={"PATH": os.environ["PATH"], "LC_ALL": "C.UTF-8",
                                            "COLUMNS": "PATH[$(touch %s)]" % canary,
                                            "LINES": "x[$(touch %s)]" % canary})
        self.assertEqual(p.returncode, 0)
        self.assertFalse(os.path.exists(canary))
        self.assertEqual(p.stdout, self.gallery("--no-probe", "-s", "2", self.dir, columns=80)[1])

    def test_narrow_terminals_and_byte_locales(self):
        # Columns too narrow for a name take none, and no position is less
        # than 1; in a locale that counts bytes, a cut name keeps whole
        # characters.
        self.add("été-long.png", png(16, 16))
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir, columns=1)
        self.assertEqual(status, 0)
        self.assertNotIn(b"\x1b[-", out)
        self.assertNotIn(b".png", out)
        status, out, _ = self.gallery("--no-probe", "-s", "2", self.dir, columns=6, locale="C")
        label = out.rsplit(b"G", 1)[1]
        self.assertEqual(label, "ét…\n".encode())
        label.decode()


if __name__ == "__main__":
    unittest.main()
