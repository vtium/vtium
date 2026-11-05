"""Paths, the copies from the C reference and logging shared by the tier 3
modules.

Paths are relative to the repository root, which is two levels above this
file. What a run writes goes under target/tier3, which git ignores.
"""

import hashlib
import os
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.join(ROOT, "tools", "tier3")
GUEST = os.path.join(HERE, "guest")
# The guest without systemd's own files: Debian with sysvinit as PID 1
# (t3_scenario.SYSVINIT). The screens, t3 and the configurations are GUEST's.
GUEST_SYSVINIT = os.path.join(HERE, "guest-sysvinit")
OUT = os.path.join(ROOT, "target", "tier3")
VM = os.path.join(OUT, "vm")
RUNDIR = os.path.join(OUT, "run")
REPORT = os.path.join(OUT, "report.json")
LOCK = os.path.join(OUT, "lock")
# The install tree a run builds (make install-tree) and installs.
TREE = os.path.join(OUT, "install")
# What the run that built TREE recorded of it: the features its console was
# built with and the tree's digest, which a later run on TREE alone
# (tier3-run) reads in place of taking the features on trust.
TREE_RECORD = os.path.join(RUNDIR, "tree.json")
BASE = os.path.join(VM, "base.qcow2")
BASE_SYSVINIT = os.path.join(VM, "base-sysvinit.qcow2")
# The guest images: a cache every vtium checkout shares, which VM/images
# links to. The pristine image, its signature and the keyring that checks it
# are in its pristine/, and the base images in its base/.
IMAGES = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"),
                      "vtium", "tier3", "images")
# The goldens, one directory a variant, as VARIANT/SCREEN.png.
GOLDEN = os.path.join(HERE, "goldens")
# The files in GUEST that provisioning builds the base image from.
BASE_INPUTS = ("user-data.in", "meta-data", "tier3-install", "tier3-install.service")
# The files in GUEST_SYSVINIT that provisioning builds its base image from.
SYSVINIT_INPUTS = ("user-data.in", "meta-data", "tier3-install")


def base_image(image, archive, guest=None, inputs=BASE_INPUTS, prefix=""):
    """Where the base image built from the pristine `image`, the archive
    date `archive` and the `inputs` in the guest's directory `guest`
    (default GUEST) lives: in VM/images/base, in the cache checkouts share
    through a link, named by `prefix` and a digest of all of them. Checkouts
    with the same inputs share one build, and changed inputs get a new one
    beside it, so a base image a running VM backs on is never rewritten.
    BASE is a link to it, and BASE_SYSVINIT to the one of GUEST_SYSVINIT,
    whose prefix is "sysvinit-"."""
    h = hashlib.sha256()
    for part in (image, archive):
        h.update(part.encode() + b"\0")
    for name in inputs:
        with open(os.path.join(guest or GUEST, name), "rb") as f:
            data = f.read()
        h.update(name.encode() + b"\0" + str(len(data)).encode() + b"\0" + data)
    return os.path.join(VM, "images", "base", "%s%s.qcow2" % (prefix, h.hexdigest()[:16]))


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
