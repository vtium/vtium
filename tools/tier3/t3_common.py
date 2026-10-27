"""Paths, the goldens' pin and logging shared by the tier 3 modules.

Paths are relative to the repository root, which is two levels above this
file. What a run writes goes under target/tier3, which git ignores.
"""

import hashlib
import io
import os
import shutil
import subprocess
import sys
import tarfile
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.join(ROOT, "tools", "tier3")
GUEST = os.path.join(HERE, "guest")
PIN = os.path.join(HERE, "pin.tsv")
OUT = os.path.join(ROOT, "target", "tier3")
VM = os.path.join(OUT, "vm")
RUNDIR = os.path.join(OUT, "run")
REPORT = os.path.join(OUT, "report.json")
LOCK = os.path.join(OUT, "lock")
# The install tree a run builds (make install-tree) and installs.
TREE = os.path.join(OUT, "install")
BASE = os.path.join(VM, "base.qcow2")
# The kmscon checkout beside this one, whose pinned commit holds the goldens.
KMSCON = os.path.join(os.path.dirname(ROOT), "kmscon")
# Where the goldens sit in a kmscon commit.
GOLDEN_PATH = "fork/tier3/golden"
# The files in GUEST that provisioning builds the base image from.
BASE_INPUTS = ("user-data.in", "meta-data", "tier3-install", "tier3-install.service")


def read_pin(path=PIN):
    """pin.tsv's kmscon line: {"commit", "url"}. Lines starting with # are
    comments; anything else that is not `kmscon COMMIT URL`, with a 40-digit
    commit, is refused."""
    pin = None
    with open(path) as f:
        for n, line in enumerate(f, 1):
            if not line.strip() or line.startswith("#"):
                continue
            fields = line.rstrip("\n").split("\t")
            if (len(fields) != 3 or fields[0] != "kmscon" or len(fields[1]) != 40
                    or any(c not in "0123456789abcdef" for c in fields[1]) or pin):
                raise SystemExit("%s:%d: want one line: kmscon, a 40-digit COMMIT and URL, "
                                 "separated by tabs" % (rel(path), n))
            pin = {"commit": fields[1], "url": fields[2]}
    if pin is None:
        raise SystemExit("%s names no kmscon commit" % rel(path))
    return pin


# The goldens judging this run: the pinned commit's, extracted under OUT.
GOLDEN = os.path.join(OUT, "kmscon-%s" % read_pin()["commit"], GOLDEN_PATH)


def extract_goldens(kmscon, pin=None, golden=None):
    """Extract the pinned commit's goldens from the kmscon checkout at
    `kmscon` into `golden` (default GOLDEN), afresh, through `git archive`,
    so the checkout's branch and working tree never reach the judge. A
    checkout without the commit is refused with the command that fetches it."""
    pin = pin or read_pin()
    golden = golden or GOLDEN
    commit = pin["commit"]
    has = subprocess.run(["git", "-C", kmscon, "cat-file", "-e", commit + "^{commit}"],
                         capture_output=True)
    if has.returncode:
        raise SystemExit("%s has no commit %s: clone %s there, or fetch it (git -C %s fetch "
                         "origin)" % (kmscon, commit, pin["url"], kmscon))
    archive = subprocess.run(["git", "-C", kmscon, "archive", commit, GOLDEN_PATH],
                             capture_output=True)
    if archive.returncode:
        raise SystemExit("git archive %s %s in %s failed: %s"
                         % (commit, GOLDEN_PATH, kmscon, archive.stderr.decode().strip()))
    # The archive's paths start with GOLDEN_PATH: it is unpacked beside
    # `golden` and that directory moved into place.
    stage = golden + ".extract"
    for d in (stage, golden):
        shutil.rmtree(d, ignore_errors=True)
    os.makedirs(stage)
    with tarfile.open(fileobj=io.BytesIO(archive.stdout)) as t:
        t.extractall(stage, filter="data")
    os.rename(os.path.join(stage, GOLDEN_PATH), golden)
    shutil.rmtree(stage)
    return golden


def base_image(image, archive):
    """Where the base image built from the pristine `image`, the archive
    date `archive` and the guest's BASE_INPUTS lives: in VM/images, which
    checkouts share through a link, named by a digest of all of them.
    Checkouts with the same inputs share one build, and changed inputs get a
    new one beside it, so a base image a running VM backs on is never
    rewritten. BASE is a link to it."""
    h = hashlib.sha256()
    for part in (image, archive):
        h.update(part.encode() + b"\0")
    for name in BASE_INPUTS:
        with open(os.path.join(GUEST, name), "rb") as f:
            data = f.read()
        h.update(name.encode() + b"\0" + str(len(data)).encode() + b"\0" + data)
    return os.path.join(VM, "images", "base-%s.qcow2" % h.hexdigest()[:16])


def rel(path):
    return os.path.relpath(path, ROOT) if path else None


class Logger:
    """Writes log lines to stderr.

    Every line carries the seconds since the run started and, inside a
    variant's job, the variant's name, so lines from concurrent variants can be
    told apart and phases timed from the log alone. A logger is passed to a
    worker process, so its lines count from the run's start too.
    """

    def __init__(self, t0=None, label=""):
        self.t0 = time.time() if t0 is None else t0
        self.label = label

    def labelled(self, label):
        """A logger for the same run whose lines carry `label`."""
        return Logger(self.t0, label)

    def __call__(self, msg):
        tag = "t3 +%.1fs" % (time.time() - self.t0)
        if self.label:
            tag += " " + self.label
        print("[%s] %s" % (tag, msg), file=sys.stderr, flush=True)
