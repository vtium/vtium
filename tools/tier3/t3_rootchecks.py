#!/usr/bin/env python3
"""The checks of ADR-1400 D4 in the tier 3 guest: `make tier3-rootchecks`.

D4, "The checks the build makes first", lists nine facts that the design for
running vtium without root rests on. This driver builds the probe in
tools/tier3/probes, puts it on a disc with guest/rootchecks as the disc's
setup and vtium's PAM file, boots a throwaway overlay of the tier 3 base
image with that disc, and collects the answers the guest writes to its event
port, one line `ROOTCHECK <check> <key>: <value>` each.

The guest's libseat is Arch's, built without libseat's builtin backend. For
check 3, `--seatd-source DIR` names an unpacked seatd source tree: the driver
builds its libseat with the builtin backend (meson and ninja), and the guest
runs the builtin probe with that libseat as well.

The run measures and judges nothing: it fails only when the guest does not
finish. The answers go to target/tier3/rootchecks/answers.txt, and the
guest's serial console and event port to the same directory. The guest has
the run's network, because the last part of check 5 installs polkit from the
Arch Linux Archive snapshot the base image pins.

Stdlib only, like the rest of tier 3.
"""

import argparse
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import t3_common  # noqa: E402
from t3_common import BASE, OUT, ROOT, Logger, rel
from t3_qemu import Qemu, make_iso

WORK = os.path.join(OUT, "rootchecks")
ANSWERS = os.path.join(WORK, "answers.txt")
PROBE_CRATE = "vtium-tier3-probes"
# The guest boots, makes its account and runs every probe in about two
# minutes; installing polkit over the network takes the rest.
TIMEOUT = 900


def build_probe(log):
    """Builds t3-probe for release and returns its path."""
    log("building %s" % PROBE_CRATE)
    subprocess.run(["cargo", "build", "--release", "-p", PROBE_CRATE], cwd=ROOT, check=True)
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
    return os.path.join(target, "release", "t3-probe")


def build_libseat(source, log):
    """Builds libseat from the seatd source tree `source` with the builtin
    backend and nothing else, and returns the library's path and the
    tree's version line."""
    build = os.path.join(WORK, "libseat-builtin")
    if os.path.exists(build):
        shutil.rmtree(build)
    log("building libseat with the builtin backend from %s" % source)
    subprocess.run(["meson", "setup", build, source, "-Dlibseat-builtin=enabled",
                    "-Dlibseat-logind=disabled", "-Dlibseat-seatd=enabled", "-Dserver=disabled",
                    "-Dman-pages=disabled", "-Dexamples=disabled"],
                   check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["ninja", "-C", build], check=True, stdout=subprocess.DEVNULL)
    with open(os.path.join(source, "meson.build")) as f:
        version = next((line.strip() for line in f if "version:" in line), "unknown version")
    return os.path.join(build, "libseat.so.1"), "seatd %s %s" % (os.path.basename(os.path.abspath(source)), version)


def stage(probe, disc, libseat=None):
    """The run's disc: the guest script as its setup, the probe, vtium's
    PAM file, and the libseat built with the builtin backend when there is
    one, as (path, version)."""
    if os.path.exists(disc):
        shutil.rmtree(disc)
    os.makedirs(disc)
    shutil.copy(os.path.join(t3_common.GUEST, "rootchecks"), os.path.join(disc, "setup"))
    shutil.copy(probe, os.path.join(disc, "t3-probe"))
    shutil.copy(os.path.join(ROOT, "crates", "vtium", "install", "vtium.pam"), os.path.join(disc, "vtium.pam"))
    if libseat:
        os.makedirs(os.path.join(disc, "builtin"))
        shutil.copy(libseat[0], os.path.join(disc, "builtin", "libseat.so.1"))
        with open(os.path.join(disc, "builtin", "VERSION"), "w") as f:
            f.write(libseat[1] + "\n")


def answers(events):
    """The answer lines of the event log. libseat writes its own log to
    the same port, so an answer can follow a part of its line."""
    with open(events, errors="replace") as f:
        return [line[line.index("ROOTCHECK "):].rstrip("\r\n") for line in f if "ROOTCHECK " in line]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--seatd-source", help="an unpacked seatd source tree, for check 3's builtin backend")
    args = ap.parse_args()
    log = Logger()
    if not os.path.exists(BASE):
        raise SystemExit("no base image at %s; run `make tier3-image`" % rel(BASE))
    probe = build_probe(log)
    os.makedirs(WORK, exist_ok=True)
    libseat = build_libseat(args.seatd_source, log) if args.seatd_source else None
    disc = os.path.join(WORK, "disc")
    stage(probe, disc, libseat)
    iso = os.path.join(WORK, "pkg.iso")
    make_iso(disc, "T3PKG", iso)
    disk = os.path.join(WORK, "disk.qcow2")
    if os.path.exists(disk):
        os.unlink(disk)
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2",
                    "-b", os.path.realpath(BASE), disk], check=True)
    vm = Qemu(WORK, [(disk, "qcow2", False), (iso, "raw", True)],
              "virtio-vga,xres=1024,yres=768,edid=on", log, net=True, cpus=2, memory_mb=2048)
    try:
        vm.wait_event("ROOTCHECKS-END", timeout=TIMEOUT)
        log("the guest finished; waiting for it to power off")
        vm.wait_exit(120)
    finally:
        vm.stop()
        lines = answers(vm.events) if os.path.exists(vm.events) else []
        with open(ANSWERS, "w") as f:
            f.write("".join(line + "\n" for line in lines))
        log("%d answers in %s; the serial console in %s" % (len(lines), rel(ANSWERS), rel(vm.serial)))
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
