#!/usr/bin/env python3
"""Self-test for tools/vendor/manifest, the list of the crates in the vendor archive.

    python3 -I -B tools/vendor/test_manifest.py

The tests of the rows give the script a Cargo.lock and cargo metadata's output that they write themselves. The tests of the command line run it on a crate of their own, which has no dependencies, so cargo needs no network and no crate from a registry.
"""

import importlib.machinery
import importlib.util
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "manifest"
_loader = importlib.machinery.SourceFileLoader("manifest", str(SCRIPT))
_spec = importlib.util.spec_from_loader("manifest", _loader)
manifest = importlib.util.module_from_spec(_spec)
_loader.exec_module(manifest)

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
BIDI = "git+https://github.com/vtium/unicode-bidi?rev=9a8c7b03#9a8c7b03be01def5cf7014b89d5af90f7382fdad"

LOCK = f"""
version = 4

[[package]]
name = "adler2"
version = "2.0.1"
source = "{CRATES_IO}"
checksum = "320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa"

[[package]]
name = "unicode-bidi"
version = "0.3.18"
source = "{BIDI}"

[[package]]
name = "vtium-term"
version = "0.1.0"
dependencies = ["adler2", "unicode-bidi"]
"""


def metadata(**licenses):
    """cargo metadata's output for LOCK's crates, with a license for each, which a keyword argument replaces."""
    have = {"adler2": "0BSD OR MIT OR Apache-2.0", "unicode-bidi": "MIT OR Apache-2.0", "vtium-term": "MIT"}
    have.update(licenses)
    sources = {"adler2": ("2.0.1", CRATES_IO), "unicode-bidi": ("0.3.18", BIDI), "vtium-term": ("0.1.0", None)}
    return {"packages": [{"name": n, "version": v, "source": s, "license": have[n]} for n, (v, s) in sources.items() if n in have]}


class Rows(unittest.TestCase):
    def test_rows(self):
        rows = manifest.rows_of(tomllib.loads(LOCK), metadata())
        self.assertEqual(rows, [
            ("adler2", "2.0.1", "registry", "https://github.com/rust-lang/crates.io-index", "", "0BSD OR MIT OR Apache-2.0", "320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa"),
            ("unicode-bidi", "0.3.18", "git", "https://github.com/vtium/unicode-bidi", "9a8c7b03be01def5cf7014b89d5af90f7382fdad", "MIT OR Apache-2.0", ""),
        ])

    def test_render(self):
        text = manifest.render(manifest.rows_of(tomllib.loads(LOCK), metadata()))
        lines = text.splitlines()
        self.assertEqual(lines[0], "name\tversion\tkind\tsource\tcommit\tlicense\tsha256")
        self.assertEqual(len(lines), 3)
        self.assertTrue(text.endswith("\n"))
        self.assertEqual({len(line.split("\t")) for line in lines}, {7})

    def test_no_license(self):
        with self.assertRaisesRegex(manifest.ManifestError, "adler2 2.0.1 declares no SPDX license"):
            manifest.rows_of(tomllib.loads(LOCK), metadata(adler2=None))

    def test_crate_that_metadata_does_not_describe(self):
        meta = metadata()
        meta["packages"] = [p for p in meta["packages"] if p["name"] != "unicode-bidi"]
        with self.assertRaisesRegex(manifest.ManifestError, "does not describe unicode-bidi 0.3.18"):
            manifest.rows_of(tomllib.loads(LOCK), meta)

    def test_tab_in_a_field(self):
        with self.assertRaisesRegex(manifest.ManifestError, "tab or a line break"):
            manifest.rows_of(tomllib.loads(LOCK), metadata(adler2="MIT\tApache-2.0"))

    def test_registry_crate_without_checksum(self):
        lock = tomllib.loads(LOCK)
        del lock["package"][0]["checksum"]
        with self.assertRaisesRegex(manifest.ManifestError, "pins no checksum for adler2"):
            manifest.rows_of(lock, metadata())

    def test_sources(self):
        self.assertEqual(manifest.parse_source("sparse+https://index.crates.io/"), ("registry", "https://index.crates.io/", ""))
        with self.assertRaisesRegex(manifest.ManifestError, "names no commit"):
            manifest.parse_source("git+https://github.com/vtium/unicode-bidi?branch=main")
        with self.assertRaisesRegex(manifest.ManifestError, "neither a registry nor a git repository"):
            manifest.parse_source("path+file:///src/vtium")
        with self.assertRaisesRegex(manifest.ManifestError, "cannot read the source"):
            manifest.parse_source("vendor")


class CommandLine(unittest.TestCase):
    """The script on a crate with no dependencies, whose manifest is the header line alone."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.src = self.tmp / "src"
        (self.src / "src").mkdir(parents=True)
        (self.src / "Cargo.toml").write_text('[package]\nname = "solo"\nversion = "0.1.0"\nedition = "2021"\nlicense = "MIT"\n\n[workspace]\n')
        (self.src / "src" / "lib.rs").write_text("")
        (self.src / "Cargo.lock").write_text('version = 4\n\n[[package]]\nname = "solo"\nversion = "0.1.0"\n')
        self.out = self.tmp / "deps.tsv"

    def tearDown(self):
        self._tmp.cleanup()

    def run_script(self, *args):
        return subprocess.run(["python3", "-I", "-B", str(SCRIPT), *args], capture_output=True, text=True, check=False)

    def test_write_then_check(self):
        done = self.run_script("write", str(self.src), str(self.out))
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(self.out.read_text(), "name\tversion\tkind\tsource\tcommit\tlicense\tsha256\n")
        done = self.run_script("check", str(self.src), str(self.out))
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_check_finds_a_row_that_differs(self):
        self.out.write_text("name\tversion\tkind\tsource\tcommit\tlicense\tsha256\nsolo\t0.1.0\tregistry\thttps://example.invalid\t\tGPL-3.0-only\t00\n")
        done = self.run_script("check", str(self.src), str(self.out))
        self.assertEqual(done.returncode, 1)
        self.assertIn("-solo\t0.1.0\tregistry", done.stderr)
        self.assertIn("does not list the crates of", done.stderr)

    def test_cargo_metadata_fails(self):
        (self.src / "Cargo.toml").write_text("[package\n")
        done = self.run_script("write", str(self.src), str(self.out))
        self.assertEqual(done.returncode, 1)
        self.assertIn("manifest: cargo metadata failed", done.stderr)
        self.assertFalse(self.out.exists())

    def test_usage(self):
        self.assertEqual(self.run_script("list", str(self.src)).returncode, 2)


if __name__ == "__main__":
    unittest.main()
