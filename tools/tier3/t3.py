#!/usr/bin/env python3
"""Tier 3 driver: provision the guest, run the scenario under QEMU, compare.

Stdlib only. The work is split across the modules beside this one:

    t3_common.py     paths, the goldens' pin and the logger
    t3_qemu.py       one headless QEMU: QMP, VNC captures, key mapping
    t3_impl.py       the install tree under test and its size
    t3_scenario.py   the variants and the scenario each runs
    t3_evaluate.py   the cases, their screen text and the compare with a golden
    t3_runner.py     the worker pool, the run lock and the report
    t3img.py         PNG/PPM reading and writing and the image compare

Variants run one at a time, or with --jobs N up to N at once, each in its own
worker process.

    t3.py provision                       fetch and verify the pinned image into the image
                                          cache, build the base image from it unless the
                                          cache holds it, and link target/tier3/vm/base.qcow2
                                          to it
    t3.py provision --guest sysvinit      the same for the guest without systemd, linked
                                          from target/tier3/vm/base-sysvinit.qcow2
    t3.py run [--jobs N] [VARIANT ...]    install the tree, run the scenario, compare
                                          (default: every variant)
    t3.py run --tree-target TARGET        build the tree first, with make TARGET
                                          DESTDIR=target/tier3/install, under the run lock
    t3.py run --tree DIR                  install the tree at DIR, built already
    t3.py provenance --kmscon DIR         check each row of provenance.tsv against its
                                          source in the kmscon checkout DIR, read with
                                          git; a review step, never a gate

Paths are relative to the repository root, which is two levels above this file.
"""

import argparse
import base64
import fcntl
import hashlib
import os
import shutil
import signal
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3_common  # noqa: E402
import t3_evaluate  # noqa: E402
import t3_impl  # noqa: E402
import t3_runner  # noqa: E402
import t3_scenario  # noqa: E402
from t3_common import (BASE, BASE_SYSVINIT, GUEST, GUEST_SYSVINIT, IMAGES, OUT,  # noqa: E402
                       REPORT, ROOT, TREE, VM, Logger, base_image, rel)
from t3_qemu import Qemu, make_iso  # noqa: E402


# --- provisioning -----------------------------------------------------------

def guest_of(name):
    """What provisioning builds the guest `name` (--guest) from: its
    directory, the files of it the base image is built from, the file each
    placeholder of its user-data.in takes, base64-encoded, the prefix of
    the base image's name, the link to the base image, and the script that
    fetches and verifies its pristine image. "arch" is Arch Linux with
    systemd, "sysvinit" Debian with sysvinit as PID 1 (guest-sysvinit/)."""
    if name == "sysvinit":
        return {"dir": GUEST_SYSVINIT, "inputs": t3_common.SYSVINIT_INPUTS,
                "embed": {"@INSTALL_SCRIPT@": "tier3-install"}, "prefix": "sysvinit-",
                "base": BASE_SYSVINIT, "fetch": "fetch-debian.sh"}
    return {"dir": GUEST, "inputs": t3_common.BASE_INPUTS,
            "embed": {"@INSTALL_SCRIPT@": "tier3-install",
                      "@INSTALL_UNIT@": "tier3-install.service"},
            "prefix": "", "base": BASE, "fetch": "fetch-image.sh"}


def provision(args, log):
    """Links target/tier3/vm/images to the image cache, fetches and
    verifies the guest's pristine image into its pristine/ unless it is
    there (fetch-image.sh, with the signing key's fingerprint, or
    fetch-debian.sh), and builds the base image for these guest files,
    pristine image and archive date into its base/ unless it is there, then
    points the guest's link (target/tier3/vm/base.qcow2 or
    base-sysvinit.qcow2) at it. A lock beside the base image keeps two
    checkouts from building the same one at once; the second finds it
    built.
    """
    guest = guest_of(args.guest)
    link_images(args.images, log)
    pristine = pristine_image(args.image)
    if not os.path.isfile(pristine):
        subprocess.run(["sh", os.path.join(t3_common.HERE, guest["fetch"]),
                        os.path.dirname(pristine), args.url, args.image, args.sha256]
                       + ([args.key] if args.key else []), check=True)
    base = base_image(args.image, args.archive, guest["dir"], guest["inputs"], guest["prefix"])
    os.makedirs(os.path.dirname(base), exist_ok=True)
    with open(base + ".lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if os.path.isfile(base):
            log("base image already built: %s" % rel(base))
        else:
            with t3_runner.run_lock():
                _provision(args, log, base)
    link = guest["base"] + ".link"
    if os.path.lexists(link):
        os.unlink(link)
    os.symlink(os.path.relpath(base, os.path.dirname(guest["base"])), link)
    os.replace(link, guest["base"])
    log("%s -> %s" % (rel(guest["base"]), os.readlink(guest["base"])))
    return 0


def link_images(images, log):
    """Point target/tier3/vm/images at the cache `images`, made if missing.
    A directory there, rather than a link, is refused: it holds images the
    cache would not."""
    images = os.path.abspath(images)
    os.makedirs(images, exist_ok=True)
    os.makedirs(VM, exist_ok=True)
    here = os.path.join(VM, "images")
    if os.path.lexists(here) and not os.path.islink(here):
        raise SystemExit("%s is a directory; move its images to %s" % (rel(here), images))
    if os.path.islink(here) and os.readlink(here) == images:
        return
    link = here + ".link"
    if os.path.lexists(link):
        os.unlink(link)
    os.symlink(images, link)
    os.replace(link, here)
    log("%s -> %s" % (rel(here), images))


def pristine_image(image):
    """Where the pristine `image` lives: in the cache's pristine/, with the
    signature and keyring fetch-image.sh checks it with."""
    return os.path.join(VM, "images", "pristine", image)


def _provision(args, log, base):
    guest = guest_of(args.guest)
    pristine = pristine_image(args.image)
    work = os.path.join(VM, "provision")
    shutil.rmtree(work, ignore_errors=True)
    seed = os.path.join(work, "seed")
    os.makedirs(seed)
    with open(os.path.join(guest["dir"], "user-data.in")) as f:
        ud = f.read().replace("@ARCHIVE@", args.archive)
    for placeholder, name in guest["embed"].items():
        with open(os.path.join(guest["dir"], name), "rb") as f:
            ud = ud.replace(placeholder, base64.b64encode(f.read()).decode())
    with open(os.path.join(seed, "user-data"), "w") as f:
        f.write(ud)
    shutil.copy(os.path.join(guest["dir"], "meta-data"), seed)
    iso = os.path.join(work, "seed.iso")
    make_iso(seed, "cidata", iso)

    tmp = base + ".tmp"
    if os.path.exists(tmp):
        os.unlink(tmp)
    # The real path: a checkout reaches target/tier3/vm/images through a
    # link, and a shared base image must not name a checkout that may be
    # removed.
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2",
                    "-b", os.path.realpath(pristine), tmp, "16G"], check=True)
    vm = Qemu(work, [(tmp, "qcow2", False), (iso, "raw", True)],
              "virtio-vga", log, net=True)
    try:
        vm.wait_serial("TIER3-PROVISION-OK", timeout=1200)
        log("provisioned; waiting for poweroff")
        if vm.wait_exit(300) is None:
            raise RuntimeError("guest did not power off after provisioning")
    finally:
        vm.stop()
    os.replace(tmp, base)
    log("base image ready: %s" % rel(base))


# --- a run ------------------------------------------------------------------

def preflight(log, rows=None, root=None, golden=None):
    """Every reason the judges cannot judge a run, checked before anything is
    built or booted: a copy out of step with provenance.tsv (`rows`, read
    when None) or a file under the C reference's goldens (`golden`, default
    GOLDEN) without a row, paths from `root` (default ROOT); a departure
    naming a case no variant draws; and a golden of the C reference's or of
    vtium's that no case compares with."""
    rows = t3_common.read_provenance() if rows is None else rows
    root = root or t3_common.ROOT
    golden = golden or t3_common.GOLDEN
    log("goldens: %s, copied from kmscon %s"
        % (rel(golden), t3_common.golden_commit(rows, os.path.relpath(golden, root))))
    errors = t3_common.check_copies(rows, root, golden)
    errors += t3_evaluate.check_departures(t3_evaluate.DEPARTURES, t3_scenario.variant_cases)
    errors += t3_evaluate.check_goldens(t3_scenario.c_golden_cases(), golden)
    errors += t3_evaluate.check_goldens(t3_scenario.vtium_golden_cases(),
                                        t3_evaluate.VTIUM_GOLDEN, t3_evaluate.PROVISIONAL)
    return errors


def run(args, log):
    variants = args.variants or t3_scenario.DEFAULT_VARIANTS
    for base, target in sorted({t3_scenario.base_of(v) for v in variants}):
        if not os.path.exists(base):
            raise SystemExit("no base image at %s; run `make %s`" % (rel(base), target))
    t3_runner.raise_on_sigterm(log)
    with t3_runner.run_lock():
        errors = preflight(log)
        if errors:
            raise SystemExit("\n".join(errors))
        tree = args.tree or TREE
        if args.tree_target:
            build_tree(args.tree_target, tree, log)
        impl = t3_impl.tree(tree)
        impl["features"] = build_features(variants, log)
        impl["fonts"] = build_fonts(variants, log)
        log("tree: %s, binary %d B, %s installed"
            % (rel(impl["path"]), impl["binary_bytes"], t3_impl.mib(impl["installed_bytes"])))
        results, interrupted = t3_runner.run_variants(variants, args.jobs, impl,
                                                   args.deadline, log)
        cases = t3_runner.write_report(variants, results, impl)
    for c in cases:
        print("%-8s %s  (%d px)" % (c["status"], c["name"], c["stats"]["pixels"]))
    print(summary(impl, {v: results[v][1] for v in variants}))
    print("report: %s" % rel(REPORT))
    if interrupted is not None:
        return 130 if isinstance(interrupted, KeyboardInterrupt) else 128 + signal.SIGTERM
    # "near" is a difference within the case's reviewed tolerance,
    # "departure" one a listed departure explains, and "provisional" a case
    # whose golden is not yet reviewed: all pass.
    passed = all(c["status"] in ("pass", "near", "departure", "provisional") for c in cases)
    return 0 if passed and t3_impl.within_budget(impl) else 1


def build_tree(target, tree, log):
    """Stage the tree afresh with make `target` DESTDIR=`tree`, so nothing an
    earlier build left in it is installed."""
    log("building the tree: make %s DESTDIR=%s" % (target, rel(tree)))
    shutil.rmtree(tree, ignore_errors=True)
    r = subprocess.run(["make", "--no-print-directory", target, "DESTDIR=" + tree], cwd=ROOT)
    if r.returncode:
        raise SystemExit("make %s failed with exit code %d" % (target, r.returncode))


def build_features(variants, log):
    """vtium built with each feature set a variant in `variants` names, as
    {features: binary path}, each in a target directory of its own under
    OUT, with the minspec profile and the revision the tree's build logs."""
    out = {}
    for features in sorted({t3_scenario.VARIANTS[v]["features"] for v in variants
                            if "features" in t3_scenario.VARIANTS[v]}):
        target = os.path.join(OUT, "features-" + features)
        log("building vtium with %s in %s" % (features, rel(target)))
        rev = subprocess.run(["git", "describe", "--always", "--dirty"], cwd=ROOT,
                             capture_output=True, text=True).stdout.strip() or "unknown"
        r = subprocess.run(["cargo", "build", "--profile", "minspec", "-p", "vtium",
                            "--features", features, "--target-dir", target], cwd=ROOT,
                           env=dict(os.environ, VTIUM_REVISION=rev))
        if r.returncode:
            raise SystemExit("building vtium with %s failed with exit code %d"
                             % (features, r.returncode))
        out[features] = os.path.join(target, "minspec", "vtium")
    return out


# What tools/testfont writes: the fonts, and unifont's licence and the
# fonts' notice, which the OFL asks to travel with them onto the disc.
FONT_FILES = ("vtium-test-8x8.psf", "vtium-test-8x16.psf", "OFL-1.1.txt", "NOTICE")


def build_fonts(variants, log):
    """The test fonts tools/testfont writes, with their licence and notice,
    in OUT/fonts afresh, when a variant in `variants` draws with them; None
    otherwise. A directory missing any of FONT_FILES is refused."""
    if not any(t3_scenario.VARIANTS[v].get("fonts") for v in variants):
        return None
    fonts = os.path.join(OUT, "fonts")
    log("writing the test fonts into %s" % rel(fonts))
    shutil.rmtree(fonts, ignore_errors=True)
    r = subprocess.run(["cargo", "run", "--release", "--quiet", "-p", "vtium-testfont", "--",
                        fonts], cwd=ROOT)
    if r.returncode:
        raise SystemExit("writing the test fonts failed with exit code %d" % r.returncode)
    lacking = [f for f in FONT_FILES if not os.path.isfile(os.path.join(fonts, f))]
    if lacking:
        raise SystemExit("tools/testfont wrote no %s" % ", ".join(lacking))
    return fonts


def summary(impl, metas):
    """The tree, its binary against the budget, the consoles that ran, and
    each variant's resident memory after the first screen and at its peak."""
    def kib(n):
        return t3_impl.mib(None if n is None else n * 1024)

    over = impl["binary_bytes"] - t3_impl.BINARY_BUDGET
    lines = ["tree %s: binary %d B, %s the budget of %d B%s; %s installed; the console "
             "logged %s"
             % (rel(impl["path"]), impl["binary_bytes"],
                "within" if over <= 0 else "over", t3_impl.BINARY_BUDGET,
                " by %d B" % over if over > 0 else "", t3_impl.mib(impl["installed_bytes"]),
                ", ".join(t3_scenario.consoles(metas)) or "no Revision line")]
    for v, meta in metas.items():
        r = meta["resident"]
        lines.append("%s: resident %s after %s, peak %s after %s"
                     % (v, kib(r["first_kib"]), r["first_screen"], kib(r["peak_kib"]),
                        r["peak_screen"] or "none"))
    return "\n".join(lines)


def verify_provenance(args, log, rows=None):
    """Each row of provenance.tsv (or `rows`) against its source, read from
    the kmscon checkout args.kmscon with git cat-file and never written. 1
    when any source's SHA-256 differs from its row's or cannot be read."""
    rows = rows or t3_common.read_provenance()
    faults = []
    for r in rows:
        blob = subprocess.run(["git", "-C", args.kmscon, "cat-file", "blob",
                               "%s:%s" % (r["commit"], r["source"])], capture_output=True)
        if blob.returncode:
            faults.append("provenance.tsv:%d: %s %s:%s: %s"
                          % (r["line"], args.kmscon, r["commit"][:12], r["source"],
                             blob.stderr.decode().strip()))
        elif hashlib.sha256(blob.stdout).hexdigest() != r["sha256"]:
            faults.append("provenance.tsv:%d: %s %s:%s has another SHA-256 than the row's"
                          % (r["line"], args.kmscon, r["commit"][:12], r["source"]))
    for f in faults:
        print(f)
    print("provenance: %d rows, %d match their sources" % (len(rows), len(rows) - len(faults)))
    return 1 if faults else 0


def main(argv=None):
    log = Logger()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("provision")
    p.add_argument("--guest", choices=("arch", "sysvinit"), default="arch",
                   help="Arch Linux with systemd (default), or Debian with sysvinit")
    p.add_argument("--image", required=True, help="the pristine image's file name")
    p.add_argument("--archive", required=True,
                   help="Arch Linux Archive date, YYYY/MM/DD, or snapshot.debian.org's "
                        "timestamp, YYYYMMDDTHHMMSSZ")
    p.add_argument("--url", required=True, help="where the pristine image is fetched from")
    p.add_argument("--sha256", required=True, help="the pristine image's SHA-256")
    p.add_argument("--key", help="the fingerprint of the key that signs it (arch)")
    p.add_argument("--images", default=IMAGES, metavar="DIR",
                   help="the image cache target/tier3/vm/images links to (default %s)" % IMAGES)
    v = sub.add_parser("provenance")
    v.add_argument("--kmscon", required=True, metavar="DIR",
                   help="a kmscon checkout holding the commits provenance.tsv names")
    r = sub.add_parser("run")
    r.add_argument("--jobs", type=int, default=1, metavar="N",
                   help="run up to N variants at once (default 1)")
    r.add_argument("--deadline", type=int, default=t3_runner.VARIANT_DEADLINE, metavar="SECONDS",
                   help="with --jobs above 1, stop a variant still running after this "
                        "(default %d)" % t3_runner.VARIANT_DEADLINE)
    r.add_argument("--tree-target", metavar="TARGET",
                   help="make target that stages the tree into DESTDIR, run under the lock "
                        "first, into %s" % rel(TREE))
    r.add_argument("--tree", metavar="DIR",
                   help="the install tree to run, built already (default %s)" % rel(TREE))
    r.add_argument("variants", nargs="*", help="any of: " + ", ".join(t3_scenario.VARIANTS))
    args = ap.parse_args(argv)
    unknown = set(getattr(args, "variants", [])) - set(t3_scenario.VARIANTS)
    if unknown:
        ap.error("unknown variant(s): " + ", ".join(sorted(unknown)))
    named = getattr(args, "variants", [])
    if len(set(named)) != len(named):
        ap.error("a variant is named twice; each has one work directory")
    if getattr(args, "jobs", 1) < 1:
        ap.error("--jobs must be at least 1")
    if args.cmd == "provision" and args.guest == "arch" and not args.key:
        ap.error("--key: the arch guest's image is checked against its signing key")
    if getattr(args, "tree", None) and args.tree_target:
        ap.error("--tree names a tree built already; --tree-target builds %s" % rel(TREE))
    return {"provision": provision, "provenance": verify_provenance,
            "run": run}[args.cmd](args, log)


if __name__ == "__main__":
    sys.exit(main())
