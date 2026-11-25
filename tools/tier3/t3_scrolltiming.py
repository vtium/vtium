#!/usr/bin/env python3
"""The scroll timing tool in the tier 3 guest: `make scroll-timing`.

Before smooth scrolling's CPU half is built, three ways to scroll are
measured in tier 3's 1-vCPU guest: the copy from a ring in system memory,
the move of rows within the display's buffer, and the pan over a buffer
taller than the screen. This driver builds tools/scroll-timing,
puts it on a disc with guest/scrolltiming as the disc's setup, and boots a
throwaway overlay of the tier 3 base image with that disc on the minspec
machine, 1 vCPU and 512 MB, once for each display size. The guest runs the
tool on its virtio-gpu card and writes the tool's rows to its event port.

The run measures and judges nothing: it fails only when a guest does not
finish or the tool fails. The rows of each size go to
target/tier3/scroll-timing/WxH.tsv, and the guest's serial console and event
port to the same directory.

Stdlib only, like the rest of tier 3.

References:
- ADR-1310 D6, "The measurement": the three ways, and the 1-vCPU guest
  they are measured in.
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

WORK = os.path.join(OUT, "scroll-timing")
MANIFEST = os.path.join(ROOT, "tools", "scroll-timing", "Cargo.toml")
# The sizes measured: 1920x1080, and the minspec machine's 1366x768.
SIZES = ((1920, 1080), (1366, 768))
CPUS, MEMORY_MB = 1, 512
# A boot takes about a minute; the tool's measurements at 1920x1080 on one
# vCPU take several more.
TIMEOUT = 1800


def build_tool(log):
    """Builds scroll-timing for release and returns its path."""
    target = os.path.join(os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target"),
                          "scroll-timing-tool")
    log("building scroll-timing")
    subprocess.run(["cargo", "build", "--release", "--locked", "--manifest-path", MANIFEST,
                    "--target-dir", target], check=True)
    return os.path.join(target, "release", "scroll-timing")


def stage(tool, disc, args):
    """The run's disc: the guest script as its setup, the tool, and the
    tool's arguments, one a line."""
    if os.path.exists(disc):
        shutil.rmtree(disc)
    os.makedirs(disc)
    shutil.copy(os.path.join(t3_common.GUEST, "scrolltiming"), os.path.join(disc, "setup"))
    shutil.copy(tool, os.path.join(disc, "scroll-timing"))
    with open(os.path.join(disc, "args"), "w") as f:
        f.write("".join(a + "\n" for a in args))


def device(size):
    """The virtio-gpu device whose EDID offers `size` as its preferred mode.
    Its 2D resources come from QEMU's host memory, which `max_hostmem`
    bounds; the tallest buffer the tool tries needs more than the 256 MiB
    QEMU gives by default."""
    return "virtio-vga,xres=%d,yres=%d,edid=on,max_hostmem=1G" % size


def qemu_version():
    """The first line of `qemu-system-x86_64 --version`."""
    out = subprocess.run(["qemu-system-x86_64", "--version"], capture_output=True, text=True)
    return (out.stdout.splitlines() or ["unknown"])[0]


def rows(events, prefix):
    """The lines of the event log after `prefix`."""
    with open(events, errors="replace") as f:
        return [line[line.index(prefix) + len(prefix):].rstrip("\r\n")
                for line in f if prefix in line]


def run_size(tool, size, args, log):
    """Boots the guest at `size`, runs the tool and returns its rows."""
    w, h = size
    work = os.path.join(WORK, "%dx%d" % size)
    os.makedirs(work, exist_ok=True)
    disc = os.path.join(work, "disc")
    stage(tool, disc, ["--sizes", "%dx%d" % size, *args])
    iso = os.path.join(work, "pkg.iso")
    make_iso(disc, "T3PKG", iso)
    disk = os.path.join(work, "disk.qcow2")
    if os.path.exists(disk):
        os.unlink(disk)
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2",
                    "-b", os.path.realpath(BASE), disk], check=True)
    vm = Qemu(work, [(disk, "qcow2", False), (iso, "raw", True)], device(size), log,
              cpus=CPUS, memory_mb=MEMORY_MB)
    try:
        vm.wait_event("SCROLLTIMING-END", timeout=TIMEOUT)
        log("the guest finished at %dx%d; waiting for it to power off" % size)
        vm.wait_exit(120)
    finally:
        vm.stop()
    lines = rows(vm.events, "SCROLLTIMING-ROW ")
    head = ["# run by\tmake scroll-timing (tools/tier3/t3_scrolltiming.py)",
            "# qemu\t" + qemu_version(),
            "# guest\tQEMU q35 with KVM, %d vCPU, %d MB, -cpu host, -device %s"
            % (CPUS, MEMORY_MB, device(size))]
    for line in rows(vm.events, "SCROLLTIMING-CARD "):
        head.append("# guest card\t" + line)
    out = os.path.join(WORK, "%dx%d.tsv" % size)
    with open(out, "w") as f:
        f.write("".join(line + "\n" for line in head + lines))
    status = rows(vm.events, "SCROLLTIMING-EXIT ")
    for line in rows(vm.events, "SCROLLTIMING-LOG "):
        if "error" in line.lower() or "warn" in line.lower() or "cannot" in line.lower():
            log("the tool: " + line)
    log("%d rows in %s; the serial console in %s" % (len(lines), rel(out), rel(vm.serial)))
    return status == ["0"], out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--sizes", default=",".join("%dx%d" % s for s in SIZES),
                    help="the display sizes, one guest each (default %(default)s)")
    ap.add_argument("args", nargs="*", help="more options for scroll-timing run")
    a = ap.parse_args()
    log = Logger()
    if not os.path.exists(BASE):
        raise SystemExit("no base image at %s; run `make tier3-image`" % rel(BASE))
    sizes = [tuple(int(n) for n in s.split("x")) for s in a.sizes.split(",")]
    tool = build_tool(log)
    ok = True
    for size in sizes:
        done, out = run_size(tool, size, a.args, log)
        ok = ok and done
        print(rel(out))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
