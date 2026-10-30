"""The build under test: vtium's install tree (make install-tree), a
directory laid out as the guest's root.

Every variant of a run installs the same tree from its disc (guest/setup), as
tree.tar unpacked over the guest's root. Its size on disk is measured here, on
the host: the bytes of the regular files it installs, and the binary's alone,
which tier 3 holds to a fixed budget.
"""

import hashlib
import os
import tarfile

from t3_common import RUNDIR, rel

# The operator's size budget for the minspec binary, 3 MiB, raised from
# 2.50 MiB on 2026-10-29 by the operator's decision recorded in ADR-1200
# D19, the 2.50 MiB figure being arbitrary, to be revisited after the
# first cut of heads. A run whose binary is larger fails, and raising the
# budget is a decision recorded in ADR-1200.
BINARY = "usr/bin/vtium"
BINARY_BUDGET = 3145728
# What the guest needs of a tree (README, "What the harness needs of the
# build"): the VT unit the base image's autovt@.service links to, and the
# binary it starts.
VT_UNIT = "usr/lib/systemd/system/kmsconvt@.service"
REQUIRED = (BINARY, VT_UNIT)
# The directories udev reads rules and hwdb files from: vtium installs its
# udev material as examples alone, under usr/share/doc/vtium/examples
# (ADR-1401 D4), so a tree holding anything here is refused.
UDEV_DIRS = ("usr/lib/udev", "lib/udev", "etc/udev")


def tree(path, tar_path=None):
    """vtium's install tree at `path`, measured and packed into `tar_path`
    (default RUNDIR/vtium-tree.tar) for the disc, owned by root."""
    path = os.path.abspath(path)
    if not os.path.isdir(path):
        raise SystemExit("no install tree at %s; make install-tree builds one" % rel(path))
    files = contents(path)
    missing = [f for f in REQUIRED if f not in files]
    if missing:
        raise SystemExit("the install tree at %s holds no %s, which the guest starts on each VT"
                         % (rel(path), ", ".join("/" + f for f in missing)))
    active = [d for d in UDEV_DIRS if os.path.lexists(os.path.join(path, d))]
    if active:
        raise SystemExit("the install tree at %s installs %s, where udev reads its rules; vtium "
                         "ships its udev material as examples alone (ADR-1401 D4)"
                         % (rel(path), ", ".join("/" + d for d in active)))
    impl = {"path": path, "binary_bytes": files[BINARY],
            "installed_bytes": sum(files.values())}
    impl.update(pack_tree(path, tar_path or os.path.join(RUNDIR, "vtium-tree.tar")))
    return impl


def contents(path):
    """{installed path: bytes} of every regular file a tree installs, paths
    relative to its root."""
    out = {}
    for d, dirs, names in os.walk(path):
        dirs.sort()
        for n in names:
            full = os.path.join(d, n)
            if os.path.isfile(full) and not os.path.islink(full):
                out[os.path.relpath(full, path)] = os.lstat(full).st_size
    return out


def pack_tree(tree, tar_path):
    """Pack `tree` into `tar_path`, every entry owned by root, in sorted order,
    so the same tree gives the same archive. Returns its "tar" path and
    "tree_sha256", which the guest reports back from the disc: it shows the
    guest unpacked this run's archive, not what built the tree, which the
    console's Revision line records."""
    def as_root(info):
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        return info

    os.makedirs(os.path.dirname(tar_path), exist_ok=True)
    with tarfile.open(tar_path, "w", format=tarfile.PAX_FORMAT) as t:
        for name in sorted(os.listdir(tree)):
            t.add(os.path.join(tree, name), arcname=name, filter=as_root)
    h = hashlib.sha256()
    with open(tar_path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return {"tar": tar_path, "tree_sha256": h.hexdigest()}


def within_budget(impl):
    return impl["binary_bytes"] <= BINARY_BUDGET


def describe(impl):
    """The report's account of the tree: where it was, its digest, the
    binary's bytes against the budget, and the bytes it installs in all."""
    return {"tree": rel(impl["path"]), "tree_sha256": impl["tree_sha256"],
            "binary_bytes": impl["binary_bytes"], "installed_bytes": impl["installed_bytes"],
            "budget": {"binary_bytes": BINARY_BUDGET, "met": within_budget(impl)}}


def mib(nbytes):
    """`nbytes` in MiB (1 MiB is 1,024 KiB), to two places, or "none"."""
    return "none" if nbytes is None else "%.2f MiB" % (nbytes / (1 << 20))
