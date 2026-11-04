"""Tests of tools/gallery/gallery: the bytes it writes for a directory of
pictures, its options, the files it skips, and its question to the
terminal, asked of a stand-in terminal (terminal.py). `make test-tools`
runs them."""

import base64
import os
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import terminal  # noqa: E402

GALLERY = os.path.join(os.path.dirname(HERE), "gallery")
# What the tool asks the terminal before it draws.
PROBE = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c"


def png(width, height, rgb=(200, 80, 40)):
    """A PNG file of `width` by `height` pixels of one color."""
    def chunk(kind, payload):
        return (struct.pack(">I", len(payload)) + kind + payload
                + struct.pack(">I", zlib.crc32(kind + payload)))
    raw = b"".join(b"\0" + bytes(rgb) * width for _ in range(height))
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

    def gallery(self, *args, columns=40, lines=24):
        """The tool run with `args` in a session with no terminal, so its
        size comes from LINES and COLUMNS: (exit status, stdout, stderr)."""
        p = subprocess.run([GALLERY, *args], stdin=subprocess.DEVNULL, capture_output=True,
                           start_new_session=True, timeout=60,
                           env={"PATH": os.environ["PATH"], "LC_ALL": "C.UTF-8",
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
        status, out = terminal.run([GALLERY, "-s", "2", self.dir],
                                   terminal.answers(cell=(16, 40)), cols=40)
        self.assertEqual(status, 0)
        self.assertEqual(out, PROBE + b"\n\n\x1b[2A\x1b[2G"
                         + kitty(b"a=T,f=100,t=d,q=2,C=1,c=5,r=2", square)
                         + b"\n\n\x1b[2Ga.png\n")
        status, out = terminal.run([GALLERY, self.dir], terminal.answers(kitty=False))
        self.assertEqual(status, 1)
        self.assertEqual(out, PROBE + b"gallery: the terminal did not answer that it draws kitty "
                         b"graphics; gallery needs kitty, WezTerm, Ghostty or vtium (--no-probe "
                         b"draws anyway)\n")


if __name__ == "__main__":
    unittest.main()
