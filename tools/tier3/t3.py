#!/usr/bin/env python3
"""Tier 3 driver: provision the guest, run the scenario under QEMU, compare.

Stdlib only. The work is split across the modules beside this one:

    t3_common.py     paths and the logger
    t3_qemu.py       one headless QEMU: QMP, VNC captures, key mapping
    t3_impl.py       the implementation under test, the C reference or vtium, and its size
    t3_scenario.py   the variants and the scenario each runs
    t3_evaluate.py   the cases, their screen text and the compare with a golden
    t3_runner.py     the worker pool, the run lock and the report
    t3img.py         PNG/PPM reading and writing and the image compare

Variants run one at a time, or with --jobs N up to N at once, each in its own
worker process.

    t3.py provision                       build the base image from the pinned image, or
                                          reuse it, and link fork/vm/base.qcow2 to it
    t3.py run [--jobs N] [VARIANT ...]    install the package, run the scenario, compare
                                          (default: every variant)
    t3.py run --pkg-dir DIR --pkg-name NAME --compare-only
                                          the same for another build, such as
                                          upstream's (fork/upstream.mk)
    t3.py run --vtium PATH [VARIANT ...]  the same for vtium, an install tree or its
                                          package files (--vtium for each), in place of
                                          the C reference
    t3.py standin --pkg-dir DIR OUT       unpack the C reference's newest packages in DIR
                                          into OUT, an install tree that stands in for vtium's
    t3.py bless [VARIANT ...]             copy the last run's captures over the goldens

Paths are relative to the repository root, which is two levels above this file.
"""

import argparse
import base64
import fcntl
import json
import os
import shutil
import signal
import subprocess
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3_impl  # noqa: E402
import t3_runner  # noqa: E402
import t3_scenario  # noqa: E402
from t3_common import BASE, GUEST, PKGOUT, REPORT, ROOT, VM, Logger, base_image, rel  # noqa: E402
from t3_evaluate import SAME_AS, golden_for  # noqa: E402
from t3_qemu import Qemu, make_iso  # noqa: E402


# --- provisioning -----------------------------------------------------------

def provision(args, log):
    """Builds the base image for these guest files, pristine image and
    archive date unless a checkout sharing fork/vm/images already has, then
    points fork/vm/base.qcow2 at it. A lock beside the image keeps two
    checkouts from building the same one at once; the second finds it built.
    """
    base = base_image(args.image, args.archive)
    os.makedirs(os.path.dirname(base), exist_ok=True)
    with open(base + ".lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if os.path.exists(base):
            log("base image already built: %s" % rel(base))
        else:
            with t3_runner.run_lock():
                _provision(args, log, base)
    link = BASE + ".link"
    if os.path.lexists(link):
        os.unlink(link)
    os.symlink(os.path.relpath(base, os.path.dirname(BASE)), link)
    os.replace(link, BASE)
    log("%s -> %s" % (rel(BASE), os.readlink(BASE)))
    return 0


def _provision(args, log, base):
    pristine = os.path.join(VM, "images", args.image)
    work = os.path.join(VM, "provision")
    shutil.rmtree(work, ignore_errors=True)
    seed = os.path.join(work, "seed")
    os.makedirs(seed)

    def b64(name):
        with open(os.path.join(GUEST, name), "rb") as f:
            return base64.b64encode(f.read()).decode()

    with open(os.path.join(GUEST, "user-data.in")) as f:
        ud = f.read()
    ud = (ud.replace("@ARCHIVE@", args.archive)
            .replace("@INSTALL_SCRIPT@", b64("tier3-install"))
            .replace("@INSTALL_UNIT@", b64("tier3-install.service")))
    with open(os.path.join(seed, "user-data"), "w") as f:
        f.write(ud)
    shutil.copy(os.path.join(GUEST, "meta-data"), seed)
    iso = os.path.join(work, "seed.iso")
    make_iso(seed, "cidata", iso)

    tmp = base + ".tmp"
    if os.path.exists(tmp):
        os.unlink(tmp)
    # The real path: a worktree reaches fork/vm/images through a link, and a
    # shared base image must not name a worktree that may be removed.
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

def run(args, log):
    if not os.path.exists(BASE):
        raise SystemExit("no base image at %s; run `make tier3-image`" % BASE)
    variants = args.variants or t3_scenario.DEFAULT_VARIANTS
    t3_runner.raise_on_sigterm(log)
    with t3_runner.run_lock():
        impl = (t3_impl.vtium(args.vtium, standin=args.standin) if args.vtium
                else c_reference(args, variants, log))
        log("implementation: %s %s, %s installed"
            % (impl["name"], " ".join(map(rel, impl["paths"])),
               t3_impl.mib(impl["installed_bytes"])))
        results, interrupted = t3_runner.run_variants(variants, args.jobs, impl,
                                                   args.deadline, log)
        cases = t3_runner.write_report(variants, results, impl)
    for c in cases:
        print("%-8s %s  (%d px)" % (c["status"], c["name"], c["stats"]["pixels"]))
    print(summary(impl, {v: results[v][1] for v in variants}))
    print("report: %s" % rel(REPORT))
    if interrupted is not None:
        return 130 if isinstance(interrupted, KeyboardInterrupt) else 128 + signal.SIGTERM
    if args.compare_only:
        # Another build measured against the fork's goldens: a difference is
        # what the comparison shows, and only a missing capture fails.
        return 0 if all(c["actual"] for c in cases) else 1
    # "near" is a difference within the case's reviewed tolerance: it passes.
    return 0 if all(c["status"] in ("pass", "near") for c in cases) else 1


def c_reference(args, variants, log):
    """The C reference's packages: built by the package target first when
    the run names one, and refused for a variant that installs another
    target's."""
    if args.pkg_target:
        log("building the package: make %s" % args.pkg_target)
        r = subprocess.run(["make", "--no-print-directory", args.pkg_target], cwd=ROOT)
        if r.returncode:
            raise SystemExit("package build failed with exit code %d" % r.returncode)
    # every variant of a run installs the same packages, so a variant that
    # needs other ones (the minimal build's) runs in a run of its own
    for v in variants:
        want = os.path.join(ROOT, "fork", "build", t3_scenario.package_target(v))
        if not args.compare_only and os.path.realpath(args.pkg_dir) != os.path.realpath(want):
            raise SystemExit("%s installs the packages of make %s, in %s; this run's are "
                             "in %s" % (v, t3_scenario.package_target(v), rel(want),
                                        rel(args.pkg_dir)))
    pkgs = t3_scenario.newest_packages(args.pkg_dir, args.pkg_name)
    log("packages: %s" % ", ".join(map(os.path.basename, pkgs)))
    return t3_impl.c_reference(pkgs)


def summary(impl, metas):
    """Which implementation ran, its size on disk, and each variant's
    resident memory after the first screen and at its peak."""
    def kib(n):
        return t3_impl.mib(None if n is None else n * 1024)

    name = {t3_impl.C: "the C reference", t3_impl.VTIUM: "vtium"}[impl["name"]]
    if impl["standin"]:
        name = "a stand-in for vtium"
    target = t3_impl.describe(impl)["minspec_target"]
    lines = ["implementation: %s, %s %s: %s without terminfo, %s ADR-304's minspec target of "
             "%.2f MiB; %s installed; the console logged %s"
             % (name, impl["kind"], " ".join(map(rel, impl["paths"])),
                t3_impl.mib(impl["main_bytes"]), "within" if target["met"] else "over",
                target["main_mib"], t3_impl.mib(impl["installed_bytes"]),
                ", ".join(t3_scenario.consoles(metas)) or "no Revision line")]
    for v, meta in metas.items():
        r = meta["resident"]
        lines.append("%s: resident %s after %s, peak %s after %s"
                     % (v, kib(r["first_kib"]), r["first_screen"], kib(r["peak_kib"]),
                        r["peak_screen"] or "none"))
    return "\n".join(lines)


def standin(args, log):
    """Unpack the C reference's newest packages in --pkg-dir into OUT: an
    install tree laid out as vtium's is, for `run --vtium OUT`."""
    if os.path.lexists(args.out):
        shutil.rmtree(args.out)
    os.makedirs(args.out)
    for p in t3_scenario.newest_packages(args.pkg_dir, args.pkg_name):
        with tarfile.open(p, "r:zst") as t:
            t.extractall(args.out, filter=lambda m, path: None if m.name.removeprefix("./") in
                         t3_impl.PKG_META else tarfile.tar_filter(m, path))
        log("unpacked %s into %s" % (os.path.basename(p), rel(args.out)))
    return 0


def bless(args, log):
    with t3_runner.run_lock():
        return _bless(args, log)


def _bless(args, log):
    with open(REPORT) as f:
        report = json.load(f)
    for c in report["cases"]:
        variant, case = c["name"].split("/", 1)
        # Only a case with a golden of its own: not vt-switch, a check, nor
        # a case compared with another's golden.
        golden, _ = golden_for(variant, case)
        if golden is None or case in SAME_AS or not c["actual"]:
            continue
        if args.variants and variant not in args.variants:
            continue
        os.makedirs(os.path.dirname(golden), exist_ok=True)
        shutil.copy(os.path.join(ROOT, c["actual"]), golden)
        log("blessed %s" % rel(golden))
    return 0


def main(argv=None):
    log = Logger()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("provision")
    p.add_argument("--image", required=True, help="pristine image file in fork/vm/images")
    p.add_argument("--archive", required=True, help="Arch Linux Archive date, YYYY/MM/DD")
    r = sub.add_parser("run")
    r.add_argument("--jobs", type=int, default=1, metavar="N",
                   help="run up to N variants at once (default 1)")
    r.add_argument("--pkg-target", metavar="TARGET",
                   help="make target that builds the package, run under the lock first")
    r.add_argument("--deadline", type=int, default=t3_runner.VARIANT_DEADLINE, metavar="SECONDS",
                   help="with --jobs above 1, stop a variant still running after this "
                        "(default %d)" % t3_runner.VARIANT_DEADLINE)
    r.add_argument("--pkg-dir", default=PKGOUT, metavar="DIR",
                   help="where the package target puts the packages (default %s)" % rel(PKGOUT))
    r.add_argument("--pkg-name", default="kmscon-aaronsb", metavar="NAME",
                   help="the console's package; its terminfo is NAME-terminfo "
                        "(default kmscon-aaronsb)")
    r.add_argument("--compare-only", action="store_true",
                   help="fail only a case without a capture, for a build the goldens "
                        "do not describe, such as upstream's")
    r.add_argument("--vtium", action="append", metavar="PATH",
                   help="run vtium in place of the C reference: one install tree, laid out "
                        "as the guest's root, or its package files (.pkg.tar.zst), each "
                        "named by a --vtium of its own")
    r.add_argument("--standin", action="store_true",
                   help="with --vtium, a build that stands in for vtium's, such as make "
                        "tier3-standin's, which may run another console")
    r.add_argument("variants", nargs="*", help="any of: " + ", ".join(t3_scenario.VARIANTS))
    st = sub.add_parser("standin")
    st.add_argument("--pkg-dir", default=PKGOUT, metavar="DIR",
                    help="where the C reference's packages are (default %s)" % rel(PKGOUT))
    st.add_argument("--pkg-name", default="kmscon-aaronsb", metavar="NAME")
    st.add_argument("out", help="the install tree to write; replaced if it exists")
    b = sub.add_parser("bless")
    b.add_argument("variants", nargs="*")
    args = ap.parse_args(argv)
    unknown = set(getattr(args, "variants", [])) - set(t3_scenario.VARIANTS)
    if unknown:
        ap.error("unknown variant(s): " + ", ".join(sorted(unknown)))
    named = getattr(args, "variants", [])
    if len(set(named)) != len(named):
        ap.error("a variant is named twice; each has one work directory")
    if getattr(args, "jobs", 1) < 1:
        ap.error("--jobs must be at least 1")
    if getattr(args, "vtium", None) and (args.pkg_target or args.compare_only):
        ap.error("--vtium installs vtium, not a package target's build")
    if getattr(args, "standin", False) and not args.vtium:
        ap.error("--standin names a build given with --vtium")
    return {"provision": provision, "run": run, "standin": standin,
            "bless": bless}[args.cmd](args, log)


if __name__ == "__main__":
    sys.exit(main())
