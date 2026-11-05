#!/usr/bin/env python3
"""Tests of tools/arm/sysroot's fetch, made without the Debian archives.

Each test copies the script into a directory of its own, beside a pin.tsv
that pins two small packages the test builds, and puts a stand-in curl first
on PATH. The pin.tsv names two stand-in archives: the main archive, which
the script asks first, and the snapshot. The stand-in curl records every URL
it is asked for. It serves a URL from the file at the same path under the
directory SERVE names, and answers 404 when no file is there. A file
HOST.status in SERVE, holding an HTTP status such as 503, makes every request
to that host fail with that status. Without SERVE, the stand-in fails every
request, as curl does when no archive can be reached.

Run: python3 -I -B tools/arm/test_sysroot.py
"""

import hashlib
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest

SYSROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sysroot")
ARCHIVE = "https://main.invalid/debian"
SNAPSHOT = "https://snapshot.invalid/archive/debian/20261020T000000Z"
ARCH = "test"
# Each package's name and the pool path its pin names.
PACKAGES = (("one", "pool/main/o/one/one_1_test.deb"), ("two", "pool/main/t/two/two_1_test.deb"))
# The longest any test waits on the script. It only makes a broken test fail instead of hanging.
WAIT_BOUND = 120

CURL = """#!/bin/sh
url=
out=
while [ $# -gt 0 ]; do
    case $1 in
    -o | --retry) [ "$1" = -o ] && out=$2; shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
    esac
done
printf '%s\\n' "$url" >>"$REQUESTS"
[ -n "${SERVE-}" ] || { echo "curl: (7) Failed to connect to ${url#https://}" >&2; exit 7; }
path=${url#https://}
host=${path%%/*}
if [ -f "$SERVE/$host.status" ]; then
    echo "curl: (22) The requested URL returned error: $(cat "$SERVE/$host.status")" >&2
    exit 22
fi
[ -f "$SERVE/$path" ] || { echo "curl: (22) The requested URL returned error: 404" >&2; exit 22; }
cp "$SERVE/$path" "$out"
"""


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def deb(work, name, path):
    """A package as the script reads one: an ar archive whose data.tar holds
    the file usr/share/sysroot-test/NAME, whose text is NAME."""
    tree = os.path.join(work, name + ".tree")
    os.makedirs(os.path.join(tree, "usr", "share", "sysroot-test"))
    with open(os.path.join(tree, "usr", "share", "sysroot-test", name), "w") as f:
        f.write(name + "\n")
    pkg = os.path.join(work, name + ".pkg")
    os.mkdir(pkg)
    with open(os.path.join(pkg, "debian-binary"), "w") as f:
        f.write("2.0\n")
    with tarfile.open(os.path.join(pkg, "data.tar"), "w") as t:
        t.add(os.path.join(tree, "usr"), arcname="./usr")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    subprocess.run(["ar", "rc", path, "debian-binary", "data.tar"], cwd=pkg, check=True)


class Fetch(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        tmp = self._tmp.name
        self.tools = os.path.join(tmp, "tools")
        os.mkdir(self.tools)
        shutil.copy(SYSROOT, self.tools)
        self.serve = os.path.join(tmp, "serve")
        work = os.path.join(tmp, "work")
        os.mkdir(work)
        rows = []
        # Both archives hold every package, unless a test takes one away.
        for name, path in PACKAGES:
            deb(work, name, self.at(SNAPSHOT, path))
            os.makedirs(os.path.dirname(self.at(ARCHIVE, path)))
            shutil.copy(self.at(SNAPSHOT, path), self.at(ARCHIVE, path))
            rows.append(f"deb\t{ARCH}\t{name}\t1\t{sha256(self.at(SNAPSHOT, path))}\t{path}\n")
        with open(os.path.join(self.tools, "pin.tsv"), "w") as f:
            f.write(f"archive\t{ARCHIVE}\nsnapshot\t{SNAPSHOT}\nsuite\tbookworm\n" + "".join(rows))
        # A third package, which no pin names, for an archive or a cache that
        # holds the wrong one.
        self.other = os.path.join(work, "other.deb")
        deb(work, "other", self.other)
        standins = os.path.join(tmp, "standins")
        os.mkdir(standins)
        with open(os.path.join(standins, "curl"), "w") as f:
            f.write(CURL)
        os.chmod(os.path.join(standins, "curl"), 0o755)
        self.cache = os.path.join(tmp, "cache")
        self.requests = os.path.join(tmp, "requests")
        self.env = {k: v for k, v in os.environ.items() if k not in ("VTIUM_ARM_SYSROOTS", "SERVE")}
        self.env.update(PATH=standins + os.pathsep + os.environ["PATH"], VTIUM_ARM_SYSROOTS=self.cache,
                        REQUESTS=self.requests)

    def tearDown(self):
        self._tmp.cleanup()

    def at(self, base, path):
        """Where the stand-in curl serves BASE/PATH from."""
        return os.path.join(self.serve, base.removeprefix("https://"), path)

    def answers(self, base, status):
        """Makes every request to BASE's host fail with HTTP STATUS."""
        with open(os.path.join(self.serve, base.removeprefix("https://").split("/")[0] + ".status"), "w") as f:
            f.write(f"{status}\n")

    def sysroot(self, *args, online=True):
        env = dict(self.env, SERVE=self.serve) if online else self.env
        return subprocess.run([os.path.join(self.tools, "sysroot"), *args], env=env, capture_output=True,
                              text=True, timeout=WAIT_BOUND)

    def fetched(self):
        """Every URL the script has asked the stand-in curl for."""
        try:
            with open(self.requests) as f:
                return f.read().splitlines()
        except FileNotFoundError:
            return []

    def dir(self):
        return self.sysroot("dir", ARCH).stdout.strip()

    def cached(self, path):
        return os.path.join(self.cache, "debs", os.path.basename(path))

    def populate(self):
        """A first fetch, which downloads every package from the main archive
        into the cache."""
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), [f"{ARCHIVE}/{path}" for _, path in PACKAGES])
        shutil.rmtree(self.dir())

    def assertExtracted(self):
        for name, _ in PACKAGES:
            with open(os.path.join(self.dir(), "usr", "share", "sysroot-test", name)) as f:
                self.assertEqual(f.read(), name + "\n")

    def assertNothingKept(self):
        self.assertEqual(os.listdir(os.path.join(self.cache, "debs")), [])
        self.assertFalse(os.path.exists(self.dir()))

    def test_a_fetch_of_an_extracted_sysroot_makes_no_request(self):
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        before = self.fetched()
        p = self.sysroot("fetch", ARCH, online=False)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), before)
        self.assertExtracted()

    def test_a_fetch_of_cached_packages_makes_no_request(self):
        self.populate()
        before = self.fetched()
        p = self.sysroot("fetch", ARCH, online=False)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), before)
        self.assertExtracted()

    def test_a_package_the_main_archive_no_longer_has_comes_from_the_snapshot(self):
        _, path = PACKAGES[1]
        os.unlink(self.at(ARCHIVE, path))
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), [f"{ARCHIVE}/{PACKAGES[0][1]}", f"{ARCHIVE}/{path}", f"{SNAPSHOT}/{path}"])
        self.assertIn(f"sysroot: {os.path.basename(path)}: not fetched from {ARCHIVE}, trying {SNAPSHOT}", p.stderr)
        self.assertExtracted()

    def test_a_main_archive_that_answers_503_leaves_the_snapshot_to_serve_every_package(self):
        self.answers(ARCHIVE, 503)
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), [f"{base}/{path}" for _, path in PACKAGES for base in (ARCHIVE, SNAPSHOT)])
        self.assertExtracted()

    def test_a_package_neither_archive_serves_fails_naming_both(self):
        _, path = PACKAGES[0]
        self.answers(ARCHIVE, 503)
        self.answers(SNAPSHOT, 504)
        p = self.sysroot("fetch", ARCH)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn(f"sysroot: {os.path.basename(path)}: download failed from {ARCHIVE} and {SNAPSHOT}", p.stderr)
        self.assertEqual(self.fetched(), [f"{ARCHIVE}/{path}", f"{SNAPSHOT}/{path}"])
        self.assertNothingKept()

    def test_a_main_archive_package_whose_sha256_differs_from_its_pin_stops_the_fetch(self):
        # The snapshot holds the right package, but a wrong one from any
        # archive is an error, and the script asks no other archive.
        _, path = PACKAGES[0]
        shutil.copy(self.other, self.at(ARCHIVE, path))
        p = self.sysroot("fetch", ARCH)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn(f"sysroot: {os.path.basename(path)}: the copy from {ARCHIVE} has a SHA-256 that differs from pin.tsv",
                      p.stderr)
        self.assertEqual(self.fetched(), [f"{ARCHIVE}/{path}"])
        self.assertNothingKept()

    def test_a_snapshot_package_whose_sha256_differs_from_its_pin_never_enters_the_cache(self):
        _, path = PACKAGES[0]
        os.unlink(self.at(ARCHIVE, path))
        shutil.copy(self.other, self.at(SNAPSHOT, path))
        p = self.sysroot("fetch", ARCH)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn(f"sysroot: {os.path.basename(path)}: the copy from {SNAPSHOT} has a SHA-256 that differs from pin.tsv",
                      p.stderr)
        self.assertNothingKept()

    def test_a_cached_package_that_differs_from_its_pin_is_fetched_again(self):
        self.populate()
        _, path = PACKAGES[1]
        with open(self.cached(path), "ab") as f:
            f.write(b"\0")
        before = self.fetched()
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), before + [f"{ARCHIVE}/{path}"])
        self.assertEqual(sha256(self.cached(path)), sha256(self.at(SNAPSHOT, path)))
        self.assertExtracted()

    def test_a_cached_package_that_differs_from_its_pin_is_never_extracted(self):
        # The cache holds another package under the pinned one's name, and
        # no archive can be reached to fetch the right one.
        self.populate()
        _, path = PACKAGES[1]
        shutil.copy(self.other, self.cached(path))
        p = self.sysroot("fetch", ARCH, online=False)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn(f"sysroot: {os.path.basename(path)}: download failed from {ARCHIVE} and {SNAPSHOT}", p.stderr)
        self.assertFalse(os.path.exists(self.cached(path)))
        self.assertFalse(os.path.exists(self.dir()))
        for top, _, files in os.walk(self.cache):
            self.assertNotIn("other", files, top)

    def test_a_pin_file_without_a_main_archive_fetches_from_the_snapshot(self):
        pins = os.path.join(self.tools, "pin.tsv")
        with open(pins) as f:
            rows = [r for r in f if not r.startswith("archive\t")]
        with open(pins, "w") as f:
            f.writelines(rows)
        p = self.sysroot("fetch", ARCH)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.fetched(), [f"{SNAPSHOT}/{path}" for _, path in PACKAGES])
        self.assertExtracted()


if __name__ == "__main__":
    unittest.main()
