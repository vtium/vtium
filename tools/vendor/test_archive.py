#!/usr/bin/env python3
"""Self-test for `tools/vendor/archive verify`, the check of a release's files.

    python3 -I -B tools/vendor/test_archive.py

Each test makes a small git repository with a regular file, an executable, a file in a folder and a symlink, and commits it twice. It then writes the five release files and SHA256SUMS the way `archive pack` does: the source archives are real `git archive` output of the second commit, and the vendor archive, its .sha256 and the manifest are stand-ins, since verify reads them only through SHA256SUMS. The tests run verify on that folder, then on copies with one defect each, and read the exit status and the message.
"""

import hashlib
import os
import shutil
import subprocess
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "archive"
VERSION = "9.9.9-test"
TGZ = f"vtium-{VERSION}.tar.gz"
ZIP = f"vtium-{VERSION}.zip"
VENDOR = f"vtium-{VERSION}-vendor.tar.zst"
MANIFEST = f"vtium-{VERSION}-dependencies.tsv"
# The files SHA256SUMS lists, in the order archive lists them.
RELEASED = [TGZ, ZIP, VENDOR, VENDOR + ".sha256", MANIFEST]


class Verify(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        # The tests read no git configuration of this machine's, and sign
        # nothing.
        self.env = dict(
            os.environ,
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="Test",
            GIT_AUTHOR_EMAIL="test@example.invalid",
            GIT_COMMITTER_NAME="Test",
            GIT_COMMITTER_EMAIL="test@example.invalid",
            VTIUM_VENDOR_VERSION=VERSION,
        )
        self.repo = self.tmp / "repo"
        self.repo.mkdir()
        self.git("init", "-q", "-b", "main")
        (self.repo / "README.md").write_text("vtium, the first commit\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "first")
        (self.repo / "README.md").write_text("vtium, the second commit\n")
        (self.repo / "bin").mkdir()
        (self.repo / "bin" / "run").write_text("#!/bin/sh\nexit 0\n")
        os.chmod(self.repo / "bin" / "run", 0o755)
        (self.repo / "docs" / "guide").mkdir(parents=True)
        (self.repo / "docs" / "guide" / "page.md").write_text("A page.\n")
        os.symlink("README.md", self.repo / "LINK")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "second")
        self.release = self.tmp / "release"
        self.pack(self.release)

    def tearDown(self):
        self._tmp.cleanup()

    def git(self, *args):
        done = subprocess.run(["git", *args], cwd=self.repo, env=self.env, capture_output=True)
        self.assertEqual(done.returncode, 0, f"git {' '.join(args)}: {done.stderr.decode()}")
        return done.stdout

    def pack(self, out, commit="HEAD"):
        """Writes the release files of COMMIT into OUT, as archive pack does."""
        out.mkdir()
        prefix = f"--prefix=vtium-{VERSION}/"
        self.git("-c", "tar.tar.gz.command=git archive gzip", "archive", "--format=tar.gz", prefix,
                 f"--output={out / TGZ}", commit)
        self.git("archive", "--format=zip", prefix, f"--output={out / ZIP}", commit)
        (out / VENDOR).write_bytes(b"a stand-in for the vendor archive\n")
        (out / (VENDOR + ".sha256")).write_text(f"{self.sha(out / VENDOR)}  {VENDOR}\n")
        (out / MANIFEST).write_text("name\tversion\tsource\n")
        self.sums(out)

    def sha(self, path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def sums(self, out):
        """Writes OUT's SHA256SUMS again, as sha256sum writes it."""
        (out / "SHA256SUMS").write_text("".join(f"{self.sha(out / f)}  {f}\n" for f in RELEASED))

    def copy(self):
        """A copy of the good release, to plant one defect in."""
        bad = self.tmp / "bad"
        if bad.exists():
            shutil.rmtree(bad)
        shutil.copytree(self.release, bad)
        return bad

    def rezip(self, out, change):
        """Writes OUT's zip again, entry by entry, through CHANGE, which takes
        the new zip, an entry's ZipInfo and its bytes, and writes the entry,
        or writes it otherwise, or leaves it out. SHA256SUMS is written
        again, so only the zip's contents are wrong."""
        path = out / ZIP
        with zipfile.ZipFile(path) as old:
            entries = [(i, old.read(i)) for i in old.infolist()]
        path.unlink()
        with zipfile.ZipFile(path, "w") as new, warnings.catch_warnings():
            warnings.simplefilter("ignore")
            for info, data in entries:
                change(new, info, data)
        self.sums(out)

    def verify(self, out):
        return subprocess.run(["sh", str(SCRIPT), "verify", str(out)], cwd=self.repo, env=self.env,
                              capture_output=True, text=True, timeout=120)

    def refused(self, out, message):
        done = self.verify(out)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertIn(message, done.stderr)
        self.assertNotIn("every file matches", done.stdout)

    def test_the_release_of_the_commit_passes(self):
        done = self.verify(self.release)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn(f"every file matches SHA256SUMS, and {TGZ} and {ZIP} hold commit", done.stdout)
        # verify removes its scratch folder and changes no file.
        self.assertEqual(sorted(os.listdir(self.release)), sorted(RELEASED + ["SHA256SUMS"]))

    def test_a_byte_flipped_in_the_zip(self):
        bad = self.copy()
        data = bytearray((bad / ZIP).read_bytes())
        data[len(data) // 2] ^= 1
        (bad / ZIP).write_bytes(bytes(data))
        self.refused(bad, "a file does not match SHA256SUMS")

    def test_a_file_changed_inside_the_zip(self):
        bad = self.copy()
        name = f"vtium-{VERSION}/README.md"
        self.rezip(bad, lambda z, i, d: z.writestr(i, b"changed\n" if i.filename == name else d))
        self.refused(bad, f"the zip and the commit differ at {name}")

    def test_an_executable_whose_exec_bit_the_zip_dropped(self):
        bad = self.copy()
        name = f"vtium-{VERSION}/bin/run"

        def drop(z, i, d):
            if i.filename == name:
                self.assertTrue(i.external_attr >> 16 & 0o111)
                i.external_attr = (0o100644 << 16) | (i.external_attr & 0xFFFF)
            z.writestr(i, d)
        self.rezip(bad, drop)
        self.refused(bad, f"the zip and the commit differ at {name}")

    def test_a_symlink_the_zip_holds_as_a_file(self):
        bad = self.copy()
        name = f"vtium-{VERSION}/LINK"

        def flatten(z, i, d):
            if i.filename == name:
                self.assertEqual(i.external_attr >> 16 & 0o170000, 0o120000)
                i.external_attr = (0o100644 << 16) | (i.external_attr & 0xFFFF)
            z.writestr(i, d)
        self.rezip(bad, flatten)
        self.refused(bad, f"the zip and the commit differ at {name}")

    def test_a_file_the_zip_leaves_out_or_adds(self):
        name = f"vtium-{VERSION}/docs/guide/page.md"
        bad = self.copy()
        self.rezip(bad, lambda z, i, d: i.filename == name or z.writestr(i, d))
        self.refused(bad, f"the zip and the commit differ at {name}")
        bad = self.copy()

        def add(z, i, d):
            z.writestr(i, d)
            if i.filename == name:
                z.writestr(f"vtium-{VERSION}/extra", b"not in the commit\n")
        self.rezip(bad, add)
        self.refused(bad, f"the zip and the commit differ at vtium-{VERSION}/extra")

    def test_a_name_the_zip_holds_twice(self):
        bad = self.copy()
        name = f"vtium-{VERSION}/README.md"

        def twice(z, i, d):
            z.writestr(i, d)
            if i.filename == name:
                z.writestr(i, d)
        self.rezip(bad, twice)
        self.refused(bad, "the zip names a file twice")

    def test_a_line_deleted_from_sha256sums(self):
        bad = self.copy()
        lines = (bad / "SHA256SUMS").read_text().splitlines(keepends=True)
        (bad / "SHA256SUMS").write_text("".join(lines[:2] + lines[3:]))
        self.refused(bad, "SHA256SUMS does not list exactly")

    def test_a_release_file_missing(self):
        bad = self.copy()
        (bad / MANIFEST).unlink()
        self.refused(bad, f"no {bad}/{MANIFEST}; make vendor-archive writes it")

    def test_a_tar_gz_of_the_commit_before(self):
        older = self.tmp / "older"
        self.pack(older, "HEAD~1")
        bad = self.copy()
        shutil.copy(older / TGZ, bad / TGZ)
        self.sums(bad)
        self.refused(bad, f"{TGZ} does not hold commit")

    def test_a_tar_gz_gzipped_otherwise_passes(self):
        # verify compares the tar file inside the .tar.gz with git archive's,
        # byte for byte, and not the gzip around it.
        bad = self.copy()
        tar = subprocess.run(["gzip", "-d", "-c", str(bad / TGZ)], capture_output=True, check=True).stdout
        again = subprocess.run(["gzip", "-1", "-n", "-c"], input=tar, capture_output=True, check=True).stdout
        self.assertNotEqual(again, (bad / TGZ).read_bytes())
        (bad / TGZ).write_bytes(again)
        self.sums(bad)
        done = self.verify(bad)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)

if __name__ == "__main__":
    unittest.main()
