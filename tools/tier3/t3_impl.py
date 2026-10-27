"""The implementation under test: the C reference's packages, or a vtium
build given as an install tree or as package files.

Every variant of a run installs the same implementation from its disc
(guest/setup): packages through pacman, a tree as tree.tar unpacked over the
guest's root. Its size on disk is measured here, on the host, the same way for
both: the bytes of the regular files it installs, which for a package is
the size its .PKGINFO records. ADR-304 §5's size target is the C reference's
main package alone, so the main size, without the terminfo entry, is what
is compared with it.
"""

import hashlib
import os
import tarfile

from t3_common import RUNDIR, rel

C, VTIUM = "c", "vtium"
# ADR-304 §5: the minspec build installs at most the C reference's minimal
# package's .PKGINFO size at 92abd024, in MiB to two places.
MINSPEC_MAIN_MIB = 1.77
# Where a tree installs the terminfo entry, which the C reference ships in a
# package of its own.
TERMINFO = "usr/share/terminfo/"
# What the guest needs of a vtium install (README, "vtium through the same
# harness"): the VT unit the base image's autovt@.service links to.
VT_UNIT = "usr/lib/systemd/system/kmsconvt@.service"
# The package metadata at the root of a package archive, which pacman reads
# and does not install.
PKG_META = {".PKGINFO", ".BUILDINFO", ".MTREE", ".INSTALL", ".CHANGELOG"}


def c_reference(pkgs):
    """The C reference's packages, as `t3_scenario.newest_packages` found them."""
    return _impl(C, "packages", pkgs)


def vtium(paths, tar_path=None, standin=False):
    """A vtium build: one install tree (a directory laid out as the guest's
    root), or one or more package files. A tree is packed into `tar_path`
    (default fork/build/tier3/vtium-tree.tar) for the disc, owned by root.
    `standin` names a build that stands in for vtium's, whose console may
    be another (t3_scenario.variant_errors)."""
    paths = [os.path.abspath(p) for p in paths]
    if len(paths) == 1 and os.path.isdir(paths[0]):
        impl = _impl(VTIUM, "tree", paths, standin)
        impl.update(pack_tree(paths[0], tar_path or os.path.join(RUNDIR, "vtium-tree.tar")))
        return impl
    for p in paths:
        if os.path.isdir(p) or not p.endswith(".pkg.tar.zst"):
            raise SystemExit("--vtium takes one install tree or package files (.pkg.tar.zst); "
                             "%s is neither" % p)
        if not os.path.isfile(p):
            raise SystemExit("no vtium package at %s" % p)
    return _impl(VTIUM, "packages", sorted(paths), standin)


def _impl(name, kind, paths, standin=False):
    files = [contents(kind, p) for p in paths]
    if name == VTIUM and not any(VT_UNIT in f for f in files):
        raise SystemExit("the vtium build at %s installs no /%s, which the guest starts on "
                         "each VT" % (", ".join(map(rel, paths)), VT_UNIT))
    sizes = [sum(f.values()) for f in files]
    if kind == "tree":
        main = sum(b for n, b in files[0].items() if not n.startswith(TERMINFO))
    else:
        main = sum(b for p, b in zip(paths, sizes) if not _is_terminfo(p))
    return {"name": name, "kind": kind, "paths": paths, "standin": standin,
            "bytes": sizes, "main_bytes": main, "installed_bytes": sum(sizes)}


def _is_terminfo(pkg):
    """Whether a package file, NAME-VERSION-REL-ARCH.pkg.tar.zst, is a
    terminfo package (NAME-terminfo)."""
    return os.path.basename(pkg).rsplit("-", 3)[0].endswith("-terminfo")


def contents(kind, path):
    """{installed path: bytes} of every regular file a package or tree
    installs, paths relative to the root, without a leading "./"."""
    out = {}
    if kind == "tree":
        for d, dirs, names in os.walk(path):
            dirs.sort()
            for n in names:
                full = os.path.join(d, n)
                if os.path.isfile(full) and not os.path.islink(full):
                    out[os.path.relpath(full, path)] = os.lstat(full).st_size
        return out
    with tarfile.open(path, "r:zst") as t:
        for m in t:
            name = m.name.removeprefix("./")
            if m.isfile() and name not in PKG_META:
                out[name] = m.size
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


def packages(impl):
    """The package files the guest installs with pacman; none for a tree."""
    return impl["paths"] if impl["kind"] == "packages" else []


def describe(impl):
    """The report's account of the implementation: which was installed, from
    where, and its size on disk in bytes: by path, without the terminfo entry
    (compared with ADR-304's minspec target), and in all."""
    out = {"name": impl["name"], "kind": impl["kind"], "standin": impl["standin"],
           "paths": [{"path": rel(p), "bytes": b} for p, b in zip(impl["paths"], impl["bytes"])],
           "main_bytes": impl["main_bytes"], "installed_bytes": impl["installed_bytes"],
           "minspec_target": {"main_mib": MINSPEC_MAIN_MIB,
                              "met": round(impl["main_bytes"] / (1 << 20), 2) <= MINSPEC_MAIN_MIB}}
    if impl["kind"] == "tree":
        out["tree_sha256"] = impl["tree_sha256"]
    return out


def mib(nbytes):
    """`nbytes` in MiB (1 MiB is 1,024 KiB), to two places, or "none"."""
    return "none" if nbytes is None else "%.2f MiB" % (nbytes / (1 << 20))
