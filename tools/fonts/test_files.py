"""Tests of tools/fonts/files, which stages and checks the font files every
vtium build installs, and of the lines make install and make install-tree
stage them with. Runs with no network: each fetch reads an archive the test
puts in the cache.

References:
- ADR-1204 D2: every build, minspec included, installs the eighteen
  Terminus files, Unifont's all-planes file and Cullet Mono's two faces.
- ADR-1204 D5: Unifont's all-planes file and its SHA-256.
"""

import gzip
import hashlib
import io
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
# The Makefile stages the font files under the caller's CARGO_TARGET_DIR,
# or target. make gates gives each gate its own.
TARGET = os.environ.get("CARGO_TARGET_DIR") or "target"
SCRIPT = os.path.join(HERE, "files")
PINS = os.path.join(HERE, "pins.tsv")
HEIGHTS = (12, 14, 16, 18, 20, 22, 24, 28, 32)
TERMINUS = [f"ter-u{h}{w}.bdf.gz" for h in HEIGHTS for w in "nb"]
FACES = ["CulletMono-Regular.ttf", "CulletMono-Bold.ttf"]


def rows(path):
    with open(path) as f:
        return [l.rstrip("\n").split("\t") for l in f if l.strip() and not l.startswith("#")]


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Pins(unittest.TestCase):
    def test_the_pins_name_the_files_every_build_installs(self):
        pins = rows(PINS)
        self.assertEqual([r[0] for r in pins], ["source"] + ["gzip"] * 18 + ["copy"])
        source = pins[0]
        self.assertEqual(source[1:4], ["terminus-font-4.49.1.tar.gz", "648345",
                                       "d961c1b781627bf417f9b340693d64fc219e0113ad3a3af1a3424c7aa373ef79"])
        self.assertTrue(source[4].startswith("https://"), source[4])
        self.assertEqual(sorted(r[1] for r in pins[1:19]), sorted(TERMINUS))
        for r in pins[1:19]:
            self.assertEqual(r[4], "terminus-font-4.49.1/" + r[1][:-3], r)
        # D5's file and digest, and the tree's copy of it matches them
        self.assertEqual(pins[19][1:5], ["unifont_all-17.0.03.hex", "8329308",
                                         "90a5d4dd44a5ef8d3fd02a3f397926b2f9d98640f0b86a3f050faef7bb2683b2",
                                         "crates/vtium-font/data/unifont.hex"])
        with open(os.path.join(ROOT, pins[19][4]), "rb") as f:
            data = f.read()
        self.assertEqual((len(data), sha(data)), (8329308, pins[19][3]))
        # D2's figure for the eighteen Terminus files under gzip
        self.assertEqual(sum(int(r[2]) for r in pins[1:19]), 327314)


class Script(unittest.TestCase):
    """The script copied into a scratch tree with pins of its own."""

    def setUp(self):
        self.t = tempfile.mkdtemp(prefix="vtium-fonts-")
        self.addCleanup(shutil.rmtree, self.t)
        os.makedirs(os.path.join(self.t, "tools", "fonts"))
        os.makedirs(os.path.join(self.t, "tools", "cullet-mono"))
        os.makedirs(os.path.join(self.t, "data"))
        shutil.copy(SCRIPT, os.path.join(self.t, "tools", "fonts", "files"))
        self.cache = os.path.join(self.t, "cache")
        self.out = os.path.join(self.t, "out")
        self.cullet = os.path.join(self.t, "cullet")
        os.makedirs(os.path.join(self.cache, "vtium", "fonts"))
        # an archive of two BDF members, as Terminus's holds them
        self.members = {"t-1/a.bdf": b"STARTFONT 2.1\nA\n" * 50, "t-1/b.bdf": b"STARTFONT 2.1\nB\n" * 70}
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w:gz") as tar:
            for name, data in self.members.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        self.archive = buf.getvalue()
        self.hexfile = b"0041:0000000018242442427E424242420000\n"
        with open(os.path.join(self.t, "data", "u.hex"), "wb") as f:
            f.write(self.hexfile)
        self.faces = {"F-Regular.ttf": b"regular face", "F-Bold.ttf": b"bold face"}
        with open(os.path.join(self.t, "tools", "cullet-mono", "pins.tsv"), "w") as f:
            f.write("# kind\tfile\tbytes\tsha256\tfrom\n")
            f.write("source\tx.zip\t1\t00\thttps://example.invalid/x.zip\n")
            for name, data in self.faces.items():
                f.write(f"face\t{name}\t{len(data)}\t{sha(data)}\tfontforge\n")

    def pins(self, gz=None):
        """Writes the scratch pins, the gzip rows pinned at `gz`, a map of
        each file's bytes, or at a digest no file has."""
        gz = gz or {}
        with open(os.path.join(self.t, "tools", "fonts", "pins.tsv"), "w") as f:
            f.write("# kind\tfile\tbytes\tsha256\tfrom\n")
            f.write(f"source\tt-1.tar.gz\t{len(self.archive)}\t{sha(self.archive)}\thttps://example.invalid/t-1.tar.gz\n")
            for member in self.members:
                name = os.path.basename(member) + ".gz"
                data = gz.get(name, b"")
                f.write(f"gzip\t{name}\t{len(data)}\t{sha(data)}\t{member}\n")
            f.write(f"copy\tu_all.hex\t{len(self.hexfile)}\t{sha(self.hexfile)}\tdata/u.hex\n")

    def cache_archive(self, data):
        with open(os.path.join(self.cache, "vtium", "fonts", "t-1.tar.gz"), "wb") as f:
            f.write(data)

    def run_files(self, *args):
        env = dict(os.environ, XDG_CACHE_HOME=self.cache, PATH=os.environ["PATH"])
        return subprocess.run([os.path.join(self.t, "tools", "fonts", "files"), *args],
                              env=env, capture_output=True, text=True)

    def test_fetch_gzips_each_file_with_no_name_and_no_time(self):
        self.pins()
        self.cache_archive(self.archive)
        p = self.run_files("fetch", self.out)
        # the pins name digests no file has, so the closing check fails
        self.assertEqual(p.returncode, 1, p.stderr)
        made = {}
        for member, data in self.members.items():
            name = os.path.basename(member) + ".gz"
            with open(os.path.join(self.out, name), "rb") as f:
                made[name] = f.read()
            self.assertEqual(gzip.decompress(made[name]), data, name)
            # RFC 1952: FLG at byte 3 has no FNAME bit, and MTIME is zero
            self.assertEqual(made[name][3] & 0x08, 0, name)
            self.assertEqual(made[name][4:8], b"\0\0\0\0", name)
            self.assertIn(f"fonts: {self.out}/{name} has {len(made[name])} bytes, not 0; "
                          "make font-files stages it", p.stderr)
        with open(os.path.join(self.out, "u_all.hex"), "rb") as f:
            self.assertEqual(f.read(), self.hexfile)
        self.pins(made)
        p = self.run_files("fetch", self.out)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("fonts: the 3 font files match their pins", p.stdout)
        self.assertEqual(os.listdir(self.out).count("a.bdf.gz.part"), 0)

    def test_fetch_removes_an_archive_that_differs_from_its_pin(self):
        self.pins()
        self.cache_archive(self.archive + b"x")
        p = self.run_files("fetch", self.out)
        self.assertEqual(p.returncode, 1)
        cached = os.path.join(self.cache, "vtium", "fonts", "t-1.tar.gz")
        self.assertIn(f"fonts: {cached} has {len(self.archive) + 1} bytes, not {len(self.archive)}, "
                      "so fetch removed it; run make font-files again to fetch it", p.stderr)
        self.assertFalse(os.path.exists(cached))

    def test_check_names_the_target_that_stages_each_file(self):
        self.pins()
        self.cache_archive(self.archive)
        self.run_files("fetch", self.out)
        made = {}
        for name in ("a.bdf.gz", "b.bdf.gz"):
            with open(os.path.join(self.out, name), "rb") as f:
                made[name] = f.read()
        self.pins(made)
        os.makedirs(self.cullet)
        for name, data in self.faces.items():
            with open(os.path.join(self.cullet, name), "wb") as f:
                f.write(data)
        p = self.run_files("check", self.out, self.cullet)
        self.assertEqual((p.returncode, p.stdout), (0, "fonts: the 5 font files match their pins\n"), p.stderr)
        os.remove(os.path.join(self.out, "b.bdf.gz"))
        with open(os.path.join(self.cullet, "F-Bold.ttf"), "wb") as f:
            f.write(b"bold fac!")
        p = self.run_files("check", self.out, self.cullet)
        self.assertEqual(p.returncode, 1)
        self.assertEqual(p.stderr.splitlines(), [
            f"fonts: {self.out}/b.bdf.gz is missing; make font-files stages it",
            f"fonts: {self.cullet}/F-Bold.ttf has SHA-256 {sha(b'bold fac!')}, not {sha(b'bold face')}; "
            "make cullet-mono builds it",
        ])

    def test_check_reads_the_digest_of_a_file_whose_path_holds_a_backslash(self):
        # GNU sha256sum starts its line with a backslash when the file's
        # name holds one, so the digest is read from the file's contents.
        self.pins()
        self.cache_archive(self.archive)
        out = os.path.join(self.t, "o\\ut")
        self.run_files("fetch", out)
        made = {}
        for name in ("a.bdf.gz", "b.bdf.gz"):
            with open(os.path.join(out, name), "rb") as f:
                made[name] = f.read()
        self.pins(made)
        cullet = os.path.join(self.t, "cul\\let")
        os.makedirs(cullet)
        for name, data in self.faces.items():
            with open(os.path.join(cullet, name), "wb") as f:
                f.write(data)
        p = self.run_files("check", out, cullet)
        self.assertEqual((p.returncode, p.stdout), (0, "fonts: the 5 font files match their pins\n"), p.stderr)

    def test_a_usage_error_is_refused(self):
        for args in ([], ["fetch"], ["check", "x"], ["other", "x"]):
            p = self.run_files(*args)
            self.assertEqual(p.returncode, 1, args)
            self.assertTrue(p.stderr.startswith("fonts: usage: files"), p.stderr)


class Install(unittest.TestCase):
    """The lines make install and make install-tree stage the files with."""

    def make_n(self, *args):
        p = subprocess.run(["make", "-n", "--no-print-directory", *args], cwd=ROOT,
                           capture_output=True, text=True, check=True)
        return p.stdout.splitlines()

    def fonts_line(self, where):
        files = [f"{TARGET}/font-files/{n}" for n in TERMINUS] + [f"{TARGET}/font-files/unifont_all-17.0.03.hex"]
        files += [f"{TARGET}/cullet-mono/{n}" for n in FACES]
        return f"install -Dm644 -t {where} " + " ".join(files)

    def test_every_build_installs_the_font_files_and_cullet_monos_notices(self):
        notices = sorted(os.listdir(os.path.join(ROOT, "tools", "cullet-mono", "notices")))
        self.assertIn("NOTICES.txt", notices)
        for features in ([], ["FEATURES=images"], ["FEATURES=images,gpu,bidi"]):
            for target, args, fonts, licenses in [
                ("install-tree", ["DESTDIR=/D"], "/D/usr/share/vtium/fonts", "/D/usr/share/licenses/vtium"),
                ("install", ["DESTDIR=/D", "PREFIX=/usr", "PKGNAME=vtium-git"],
                 "/D/usr/share/vtium/fonts", "/D/usr/share/licenses/vtium-git"),
            ]:
                with self.subTest(target=target, features=features):
                    lines = self.make_n(target, *args, *features)
                    # the check comes first, before anything is built
                    self.assertEqual(lines[0], f"tools/fonts/files check {TARGET}/font-files {TARGET}/cullet-mono")
                    self.assertIn(self.fonts_line(fonts), lines)
                    self.assertIn(f"install -Dm644 -t {licenses}/cullet-mono "
                                  + " ".join(f"tools/cullet-mono/notices/{n}" for n in notices), lines)
                    self.assertFalse([l for l in lines if "sed -i" in l], lines)

    def test_install_writes_its_prefix_into_the_font_sets_paths(self):
        lines = self.make_n("install", "DESTDIR=/D", "PREFIX=/usr/local")
        self.assertIn(self.fonts_line("/D/usr/local/share/vtium/fonts"), lines)
        self.assertIn("find /D/etc/vtium/fonts -type f \\( -name '*.vtfrit' -o -name '*.vtlayer' \\) "
                      "-exec sed -i 's|/usr/share/vtium/fonts/|/usr/local/share/vtium/fonts/|g' {} +", lines)

    def test_font_files_fetches_and_checks_with_the_script(self):
        self.assertEqual(self.make_n("font-files"), [f"tools/fonts/files fetch {TARGET}/font-files"])
        self.assertEqual(self.make_n("font-files-check"),
                         [f"tools/fonts/files check {TARGET}/font-files {TARGET}/cullet-mono"])


if __name__ == "__main__":
    unittest.main()
