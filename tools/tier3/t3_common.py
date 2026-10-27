"""Paths and logging shared by the tier 3 modules.

Paths are relative to the repository root, which is two levels above this file.
"""

import hashlib
import os
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.join(ROOT, "fork", "tier3")
GUEST = os.path.join(HERE, "guest")
GOLDEN = os.path.join(HERE, "golden")
VM = os.path.join(ROOT, "fork", "vm")
PKGOUT = os.path.join(ROOT, "fork", "build", "pkg")
RUNDIR = os.path.join(ROOT, "fork", "build", "tier3")
REPORT = os.path.join(ROOT, "fork", "build", "tier3-report.json")
LOCK = os.path.join(ROOT, "fork", "build", "tier3.lock")
BASE = os.path.join(VM, "base.qcow2")
# The files in GUEST that provisioning builds the base image from.
BASE_INPUTS = ("user-data.in", "meta-data", "tier3-install", "tier3-install.service")


def base_image(image, archive):
    """Where the base image built from the pristine `image`, the archive
    date `archive` and the guest's BASE_INPUTS lives: in fork/vm/images,
    which worktrees link from the main checkout, named by a digest of all
    of them. Checkouts with the same inputs share one build, and changed
    inputs get a new one beside it, so a base image a running VM backs on
    is never rewritten. BASE is a link to it."""
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
