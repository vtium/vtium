"""Tier 3's variants and the scenario each one runs: install the tree, log
in, draw the screens, switch VTs, then evaluate every case. A power variant
presses the power keys on the console instead (t3_power)."""

import os
import re
import shutil
import subprocess
import time

import t3_bench
import t3_devices
import t3_heads
import t3_images
import t3_pointer
import t3_pictures
import t3_power
import t3_showcase
import t3img
from t3_common import BASE, BASE_SYSVINIT, GUEST, GUEST_SYSVINIT, ROOT, RUNDIR, rel
from t3_evaluate import (BENCHMARKS, CASES, DRAWS_PICTURES, IMAGE_SCREENS, KEYED, KEYS_AFTER,
                         NO_GOLDEN, PICTURE_SCREENS, REPLIES, SCREENS, SHOWS, SMALL_SCREENS,
                         VTIUM_GOLDEN, cases, evaluate, golden_for, not_shown)
from t3_qemu import Qemu, char_qcodes, make_iso, not_blank, wait_stable

# Display variants. Each boots its own guest from the base image, with its
# own vCPUs and memory, and names the text renderer its console must log:
# bbulk for the 2D variants, gltex on virgl for the GL one, which the C
# reference drew with and vtium does not yet. Each draws every screen, the
# image ones included (t3_evaluate.IMAGE_SCREENS), so its console is built
# with images ("tree_features"). The C reference drew the 2D ones' goldens
# in DejaVu Sans Mono through pango, and vtium draws the font through its
# own glyph engine, so a case of theirs is judged by vtium's re-baselined
# golden where goldens/ holds one ("rebaselined"), and by the C reference's
# otherwise (golden_root).
# The screens the frit variants draw: Latin text the frit's font holds, and
# CJK it lacks, which the built-in unifont draws in its place (#141); cjk
# last, as vt-restore compares with it.
FRIT_SCREENS = ["small-console", "cjk"]

VARIANTS = {
    "2d-1280x800": {
        "device": "virtio-vga,xres=1280,yres=800,edid=on",
        "display": "none",
        "conf": "kmscon.conf",
        "renderer": "bbulk",
        "cpus": 4,
        "memory_mb": 4096,
        "tree_features": ["images"],
        "rebaselined": True,
    },
    # The minspec machine: one vCPU and 512 MB at a common laptop size.
    # The vCPU is a core of the host, so it bounds parallelism and memory
    # rather than reproducing slow hardware.
    "2d-1366x768-potato": {
        "device": "virtio-vga,xres=1366,yres=768,edid=on",
        "display": "none",
        "conf": "kmscon.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "tree_features": ["images"],
        "rebaselined": True,
    },
    # The same machine with the minimal configuration: unifont, and a PSF
    # console font, which make tier3-minimal runs. Their image screens draw
    # no picture ("images": False), so the keys those screens take, which act
    # on pictures, are not sent and their cases are not in the variant's
    # report.
    "2d-potato-min": {
        "device": "virtio-vga,xres=1366,yres=768,edid=on",
        "display": "none",
        "conf": "kmscon-minimal.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "pointer": True,
        "rebaselined": True,
    },
    "2d-potato-min-psf": {
        "device": "virtio-vga,xres=1366,yres=768,edid=on",
        "display": "none",
        "conf": "kmscon-minimal-psf.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "frits": ["default8x16.vtfrit"],
        "rebaselined": True,
    },
    # 2d-potato-min also runs the pointer case ("pointer"): after the VT
    # switch it moves the pointer with QMP and reads the display's cursor
    # plane from the guest (t3_pointer).
    # A variant drawing a PSF font names the frits the guest builds into the
    # store before the console starts ("frits", guest/frits), since vtium
    # draws only sets (ADR-1200).
    # The small displays (vtium/vtium#75) on the minspec machine: minspec's
    # floor, 320x240 in an 8x8 PSF font as 40 by 30 cells, and 640x480 in an
    # 8x16 one as 80 by 30, both fonts tools/testfont's ("fonts"), each with
    # a built-in set filling what it lacks. They draw
    # SMALL_SCREENS ("screens"), and the C reference drew neither, so their
    # goldens are vtium's own ("goldens": VTIUM_GOLDEN).
    "2d-320x240-psf8": {
        "device": "virtio-vga,xres=320,yres=240,edid=on",
        "display": "none",
        "conf": "kmscon-small-8x8.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "fonts": True,
        "frits": ["vtium-test-8x8.vtfrit"],
        "screens": SMALL_SCREENS,
        "goldens": VTIUM_GOLDEN,
    },
    "2d-640x480-psf16": {
        "device": "virtio-vga,xres=640,yres=480,edid=on",
        "display": "none",
        "conf": "kmscon-small-8x16.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "fonts": True,
        "frits": ["vtium-test-8x16.vtfrit"],
        "screens": SMALL_SCREENS,
        "goldens": VTIUM_GOLDEN,
    },
    # A configured frit, generated at run time by the helper (ADR-1301 D2):
    # 640x480 on the minspec machine, its font the guest's DejaVu Sans Mono
    # named by a frit installed with no set built ("deferred_frits"). The
    # first draws the font once the helper's glyphs arrive; the second holds
    # the helper so it never answers ("helper": "held"), which shows what the
    # screen draws before any answer, the built-in unifont, until the console
    # gives the helper up. Their goldens are vtium's own.
    "2d-640x480-frit": {
        "device": "virtio-vga,xres=640,yres=480,edid=on",
        "display": "none",
        "conf": "kmscon-frit.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "deferred_frits": ["dejavu-mono.vtfrit"],
        "screens": FRIT_SCREENS,
        "goldens": VTIUM_GOLDEN,
    },
    "2d-640x480-frit-held": {
        "device": "virtio-vga,xres=640,yres=480,edid=on",
        "display": "none",
        "conf": "kmscon-frit.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
        "deferred_frits": ["dejavu-mono.vtfrit"],
        "helper": "held",
        "screens": FRIT_SCREENS,
        "goldens": VTIUM_GOLDEN,
    },
    "2d-7680x2160": {
        # Each 7680x2160 XRGB buffer is 63 MiB; the console, two terminal
        # instances and their double buffers exceed the 256 MiB default.
        "device": "virtio-vga,xres=7680,yres=2160,edid=on,max_hostmem=1G",
        "display": "none",
        "conf": "kmscon.conf",
        "renderer": "bbulk",
        "cpus": 4,
        "memory_mb": 4096,
        "tree_features": ["images"],
        "rebaselined": True,
    },
    # The 7680x2160 display with DejaVu Sans Mono at 32 points, a 20x38 cell
    # (the C reference drew 19x38), near the 20x40 of Monoid at 30 the
    # operator runs: pictures sized from the cell (sixel) are checked at a
    # cell other than the 10x19 of the rest.
    "2d-7680x2160-font32": {
        "device": "virtio-vga,xres=7680,yres=2160,edid=on,max_hostmem=1G",
        "display": "none",
        "conf": "kmscon-font32.conf",
        "renderer": "bbulk",
        "cpus": 4,
        "memory_mb": 4096,
        "tree_features": ["images"],
        "rebaselined": True,
    },
    "gl-1280x800": {
        "device": "virtio-vga-gl,xres=1280,yres=800,edid=on",
        "display": "egl-headless,rendernode=%s"
                   % os.environ.get("T3_RENDERNODE", "/dev/dri/renderD128"),
        "conf": "kmscon-gl.conf",
        "renderer": "gltex",
        "capture": "vnc",
        "cpus": 4,
        "memory_mb": 4096,
        "tree_features": ["images"],
    },
}

# The power keys (vtium/vtium#21) on the minspec machine, with the
# minimal configuration and the keys bound (guest/kmscon-power.conf). Each
# variant's "power" lists its cases (t3_power), pressed in order on the
# console's login prompt. A reboot or a power-off ends the VM, so each
# has a variant of its own, and only a suspend comes before another case.
# power-off-after-s3 shows a power-off after each suspend path, a known
# failure of the guest's (t3_power.KNOWN). The hard reboot's guest sets
# "sysctl" before vtium starts, and the suspending guests log the kernel at
# info level, so its ACPI lines reach the console. QEMU offers S3,
# so the guest suspends to RAM, which QMP reports. A variant naming
# "features" runs vtium built with them (t3.build_features) in place of
# the tree's minspec binary.
S3 = ["-global", "ICH9-LPC.disable_s3=0"]
VERBOSE = "kernel.printk = 7 4 1 7\n"
POWER = {
    "device": "virtio-vga,xres=1366,yres=768,edid=on",
    "display": "none",
    "conf": "kmscon-power.conf",
    "renderer": "bbulk",
    "cpus": 1,
    "memory_mb": 512,
    "images": False,
}
VARIANTS.update({
    "power-reboot": dict(POWER, power=["reboot"]),
    "power-reboot-hard": dict(POWER, power=["reboot-hard"], sysctl="kernel.ctrl-alt-del = 1\n"),
    "power-off": dict(POWER, power=["power-off"]),
    "power-suspend-off": dict(POWER, power=["suspend-sysfs", "power-off-after-s3"], qemu=S3,
                              sysctl=VERBOSE),
    "power-suspend-logind": dict(POWER, power=["suspend", "power-off-after-s3"], qemu=S3,
                                 sysctl=VERBOSE, features="logind"),
})
POWER_VARIANTS = ["power-reboot", "power-reboot-hard", "power-off", "power-suspend-off",
                  "power-suspend-logind"]

# The guest without systemd (guest-sysvinit/, "guest": "sysvinit"): Debian
# with sysvinit as PID 1, on the minspec machine, judging docs/systemd.md's
# column for a host without systemd. Its inittab runs vtium on tty2 from the
# boot, where it ran a getty, and nothing spawns a console on a switch; tty3
# keeps its getty, but for a variant with "two_vts", whose guest runs a
# second vtium there from the boot, as an inittab does for each VT it runs
# vtium on. Its udev daemon runs unless the variant says "udev": False,
# whose guest stops it and removes its database before vtium starts. The
# drawing variants draw at 640x480 in the 8x16 test font, cjk last for
# vt-restore, judged by vtium's own goldens. sysvinit-two-vts is
# vtium/vtium#139's check: the two consoles race for DRM master at the
# boot, so it stays out of make tier3-nosystemd's run until that lands. The power variants press one
# key each, as each ends the VM: grab-reboot, whose SIGINT runs the
# inittab's ctrlaltdel entry; grab-reboot with kernel.ctrl-alt-del at 1,
# the kernel's own restart; and grab-power-off, whose SIGRTMIN+4 sysvinit
# takes as a power-off.
SYSVINIT = {
    "guest": "sysvinit",
    "device": "virtio-vga,xres=640,yres=480,edid=on",
    "display": "none",
    "conf": "kmscon-small-8x16.conf",
    "renderer": "bbulk",
    "cpus": 1,
    "memory_mb": 512,
    "images": False,
    "fonts": True,
    "frits": ["vtium-test-8x16.vtfrit"],
    "screens": ["sgr", "cjk"],
    "goldens": VTIUM_GOLDEN,
}
VARIANTS.update({
    "sysvinit-640x480": SYSVINIT,
    "sysvinit-no-udev": dict(SYSVINIT, udev=False),
    "sysvinit-two-vts": dict(SYSVINIT, two_vts=True, screens=["cjk"]),
    "sysvinit-reboot": dict(POWER, guest="sysvinit", power=["reboot-sysvinit"]),
    "sysvinit-restart": dict(POWER, guest="sysvinit", power=["reboot-hard"],
                             sysctl="kernel.ctrl-alt-del = 1\n"),
    "sysvinit-power-off": dict(POWER, guest="sysvinit", power=["power-off-sysvinit"]),
})
SYSVINIT_VARIANTS = ["sysvinit-640x480", "sysvinit-no-udev", "sysvinit-reboot",
                     "sysvinit-restart", "sysvinit-power-off"]

# Displays and keyboards beyond one of each (t3_devices), on the minspec
# machine at 640x480 in the 8x16 test font, judged by vtium's own goldens,
# each run only when named. two-outputs-mirror is one card with two outputs
# and two-cards-mirror two cards with one each: each screen is captured
# from every output ("outputs"), and the mirror check holds every output's
# capture to the first's, the one grid vtium draws on every display today.
# usb-keyboards puts a keyboard in the USB port ADR-1401's example rule
# names, has the guest's udev read the example files the tree installs, and
# plugs a second keyboard in and pulls it out through QMP ("checks").
DEVICES = {
    "device": "virtio-vga,id=video0,xres=640,yres=480,edid=on",
    "display": "none",
    "conf": "kmscon-small-8x16.conf",
    "renderer": "bbulk",
    "cpus": 1,
    "memory_mb": 512,
    "images": False,
    "fonts": True,
    "frits": ["vtium-test-8x16.vtfrit"],
    "screens": ["sgr", "cjk"],
    "goldens": VTIUM_GOLDEN,
}
VARIANTS.update({
    "two-outputs-mirror": dict(DEVICES, device="virtio-vga,id=video0,max_outputs=2,xres=640,"
                                               "yres=480,edid=on",
                               outputs=[{"suffix": "@1", "device": "video0", "head": 1,
                                         "size": (640, 480)}],
                               mirror=True),
    "two-cards-mirror": dict(DEVICES, qemu=["-device", "virtio-gpu-pci,id=video1,xres=640,"
                                                       "yres=480,edid=on"],
                             outputs=[{"suffix": "@card1", "device": "video1"}],
                             mirror=True),
    "usb-keyboards": dict(DEVICES, qemu=t3_devices.USB, screens=["cjk"],
                          checks=["udev-examples", "keyboard-hotplug"]),
})
DEVICE_VARIANTS = ["two-outputs-mirror", "two-cards-mirror", "usb-keyboards"]

# vtium's pictures on the minspec machine at 640x480 in the 8x16 test font,
# judged by vtium's own goldens, run by make tier3-images. The tree's
# console must be built with images ("tree_features"), which puts the image
# helper and its sysusers file in the tree (ADR-1101 D1, D2), and the disc
# carries the test pictures t3_pictures.py writes ("pictures"). It draws
# PICTURE_SCREENS, whose last, img-png, vt-restore is compared with
# (t3_evaluate.RESTORED). Before login, the agent (guest/t3-agent) writes
# pictures to the login prompt's pty ("prelogin", t3_images). Its guest
# mounts rpc_pipefs for img-png, which reads a file there ("rpc_pipefs"). Its configuration keeps two display buffers, so
# img-tmux-pane's damage per frame counts two frames' changes, not three
# (t3_bench.pane_verdict).
VARIANTS["images-640x480"] = dict(DEVICES, device="virtio-vga,xres=640,yres=480,edid=on",
                                  conf="kmscon-images.conf",
                                  images=True, tree_features=["images"], pictures=True,
                                  screens=PICTURE_SCREENS, agent=True, prelogin=True,
                                  rpc_pipefs=True)
# The same machine and tree with the account vtium-image removed after
# systemd-sysusers made it ("image_account": "nobody"), so the image helper
# runs as nobody before login (ADR-1101 D2, D10). Its one case is the check
# before login, so the driver never logs in ("login": False).
VARIANTS["images-640x480-nobody"] = dict(VARIANTS["images-640x480"], image_account="nobody",
                                         login=False, screens=[], rpc_pipefs=False)
IMAGE_VARIANTS = ["images-640x480", "images-640x480-nobody"]

# The showcase (ADR-1502 D6, t3_showcase): the default machine at 1920x1080,
# its console built with images and drawing on the CPU renderer, with the
# scenes the documentation shows ("screens") in Iosevka Term and the fonts
# t3_showcase pins, which the disc carries ("showcase"), named by frits the
# helper generates at run time ("deferred_frits"). The fonts scene
# switches the console to each of the other frits with OSC 50. Its cases
# carry the status capture ("verdict"): each passes once it is captured and
# judges nothing, and no golden judges it. It switches no VT, and runs only
# when named, by tools/tier3/showcase.sh.
VARIANTS["showcase-1920x1080"] = {
    "device": "virtio-vga,xres=1920,yres=1080,edid=on",
    "display": "none",
    "conf": "kmscon-showcase.conf",
    "renderer": "bbulk",
    "cpus": 4,
    "memory_mb": 4096,
    "tree_features": ["images"],
    "deferred_frits": ["showcase-iosevka.vtfrit", "showcase-shell.vtfrit",
                       "showcase-jetbrains.vtfrit", "showcase-terminus.vtfrit",
                       "showcase-unifont.vtfrit"],
    "screens": t3_showcase.SCREENS,
    "showcase": True,
    "verdict": "capture",
    "vt_switch": False,
}

# The head file (ADR-1401) on the guest's real KMS device and systemd
# (t3_heads), on the device variants' machine, each run only when named.
# A variant names its head file ("heads", from the repository's root), the
# policy its checks expect ("policy"), and the case of the console's first
# screen, judged by vtium's provisional golden ("first"). One whose console
# gives no login ("login": False) draws no screen and switches no VT; its
# checks follow the first screen. Each runs the agent (t3_heads), and the
# getty-fallback check fails vtium in the ways "failures" names; the
# start-limit check kills it past its unit's start limit.
# heads-partly-open is the open control of the closed variants' fallback:
# there the getty does start. The two without a display device ("device":
# None) wait for vtium on tty2 with no screen to capture, one under each
# policy, at a display-wait of 5 s (guest/kmscon-nodisplay.conf). The kiosk
# switches no VT ("vt_switch": False): its claimed keyboard's VT keys reach
# its session, and the kiosk check presses them itself. heads-policy-typo's
# file holds a policy line vtium cannot read ("policy_line"), which makes
# it closed and broken (ADR-1401 D7), so its checks expect the closed
# policy.
HEAD_VARIANT = dict(DEVICES, agent=True, screens=[])
NO_DISPLAY = dict({k: v for k, v in HEAD_VARIANT.items() if k not in ("fonts", "frits")},
                  device=None, conf="kmscon-nodisplay.conf", renderer=None, login=False)
VARIANTS.update({
    "heads-partly-open": dict(HEAD_VARIANT, heads="tools/tier3/guest/heads/partly-open.conf",
                              policy="open", first="partly-notice", screens=["cjk"],
                              checks=["getty-fallback", "start-limit"],
                              failures=["kill", "crash", "hangup"]),
    "heads-closed-broken": dict(HEAD_VARIANT, heads="tools/tier3/guest/heads/broken-closed.conf",
                                policy="closed", first="panic", login=False,
                                checks=["no-login", "getty-fallback", "start-limit"],
                                failures=["kill", "crash", "hangup"]),
    "heads-policy-typo": dict(HEAD_VARIANT, heads="tools/tier3/guest/heads/policy-typo.conf",
                              policy="closed", policy_line="policy = opne",
                              first="policy-panic", login=False,
                              checks=["policy-line", "no-login", "getty-fallback"],
                              failures=["kill"]),
    "heads-closed-unmatched": dict(HEAD_VARIANT,
                                   heads="tools/tier3/guest/heads/closed-unmatched.conf",
                                   policy="closed", first="no-display-notice", login=False,
                                   checks=["no-login", "getty-fallback"],
                                   failures=["kill"]),
    "heads-two-heads": dict(HEAD_VARIANT, agent=False,
                            device="virtio-vga,id=video0,max_outputs=2,xres=640,yres=480,"
                                   "edid=on",
                            second={"device": "video0", "head": 1, "size": (640, 480)},
                            heads="tools/tier3/guest/heads/two-heads.conf", policy="open",
                            frits=["desk.vtfrit", "panel.vtfrit"],
                            screens=["cjk"], checks=["two-heads"]),
    "heads-off": dict(HEAD_VARIANT, heads="docs/heads/off.conf", policy="closed",
                      renderer=None, login=False, checks=["no-login", "getty-fallback"],
                      failures=["kill"]),
    "heads-kiosk": dict(HEAD_VARIANT, qemu=t3_devices.USB,
                        heads="tools/tier3/guest/heads/kiosk.conf", policy="closed",
                        screens=["cjk"], vt_switch=False, checks=["kiosk"]),
    "heads-closed-nodisplay": dict(NO_DISPLAY, heads="tools/tier3/guest/heads/closed-valid.conf",
                                   policy="closed", checks=["display-held", "getty-fallback"],
                                   failures=["kill"]),
    "heads-open-nodisplay": dict(NO_DISPLAY, heads="docs/heads/default.conf", policy="open",
                                 checks=["display-exit"]),
})
HEAD_VARIANTS = ["heads-two-heads", "heads-partly-open", "heads-closed-broken",
                 "heads-policy-typo", "heads-closed-unmatched", "heads-off", "heads-kiosk",
                 "heads-closed-nodisplay", "heads-open-nodisplay"]

# Each guest's base image, the make target that provisions it, the console
# lines its boot ends with before the driver presses Ctrl+Alt+F2, and the
# line that shows the switch took, which no boot logs: under systemd, logind
# up and the login prompts reached, then the console the switch spawns
# logging its VT; under sysvinit, the console the inittab started on tty2
# logging its VT, then its display set up once the VT is shown.
GUESTS = {
    "arch": {"base": BASE, "image": "tier3-image",
             "boot": ["Started User Login Management", "Reached target Login Prompts"],
             "shown": "vt: using tty /dev/tty2"},
    "sysvinit": {"base": BASE_SYSVINIT, "image": "tier3-image-nosystemd",
                 "boot": ["vt: using tty /dev/tty2"], "shown": "text renderer ["},
}

# What `t3.py run` runs when no variant is named, in the order workers take
# them: with JOBS=2 the longest, 2d-7680x2160, starts at once, the potato
# and then 2d-7680x2160-font32 follow 2d-1280x800 as workers free, and
# gl-1280x800 follows the potato, so the worker that runs three variants
# runs the three shortest.
DEFAULT_VARIANTS = ["2d-1280x800", "2d-7680x2160", "2d-1366x768-potato",
                    "2d-7680x2160-font32", "gl-1280x800"]

# Seconds a screen may take to draw; a benchmark scrolls for longer.
SCREEN_TIMEOUT = 60
BENCH_TIMEOUT = 300


def guest_of(variant):
    """The guest `variant` boots (GUESTS)."""
    return GUESTS[VARIANTS[variant].get("guest", "arch")]


def base_of(variant):
    """(the base image `variant` boots, the make target that provisions
    it)."""
    guest = guest_of(variant)
    return guest["base"], guest["image"]


def uses_virgl(variant):
    return (VARIANTS[variant]["device"] or "").startswith("virtio-vga-gl")


def screens_of(variant):
    """The screens `variant` draws: its own list, or SCREENS."""
    return VARIANTS[variant].get("screens", SCREENS)


def variant_cases(variant):
    """The cases of `variant`'s report: a power variant's own; otherwise
    every case of its screens, but for a variant without images the keyed
    cases of the image screens, whose keys act on pictures, each case with
    a golden followed by the same case on each of its other outputs, then
    the mirror check for a variant that runs it, the pointer case for one
    that runs it, and its other checks (t3_devices.CHECKS, t3_heads.CHECKS).
    A head-file variant's first screen comes first, as does an images
    variant's check before login (t3_images), and one whose console gives
    no login has no case but those and its checks."""
    v = VARIANTS[variant]
    if "power" in v:
        return list(v["power"])
    first = ([v["first"]] if "first" in v else []) + (
        [t3_images.CASE] if v.get("prelogin") else [])
    if not v.get("login", True):
        return first + list(v.get("checks", []))
    out = first + (CASES if "screens" not in v else cases(screens_of(variant)))
    if not v.get("vt_switch", True):
        out = [c for c in out if c not in ("vt-switch", "vt-restore")]
    if not v.get("images", True):
        skipped = {case for screen, case, _ in KEYED if screen in DRAWS_PICTURES}
        out = [c for c in out if c not in skipped]
    outputs = v.get("outputs", [])
    out = [c for case in out for c in [case] + (
        t3_devices.output_cases(case, outputs) if case not in NO_GOLDEN else [])]
    if v.get("mirror"):
        out = out + ["mirror"]
    if v.get("pointer"):
        out = out + [t3_pointer.CASE]
    return out + list(v.get("checks", []))


def check_tree_features(variants, impl):
    """Every reason the tree `impl` (t3_impl.tree) cannot run `variants`: a
    variant whose screens need the console built with features
    ("tree_features") the tree's console was built without."""
    errors = []
    for variant in variants:
        lacking = sorted(set(VARIANTS[variant].get("tree_features", ()))
                         - set(impl["tree_features"]))
        if lacking:
            errors.append("%s needs a tree whose console is built with %s, and the tree's was "
                          "built with %s; make tier3 and make tier3-images build one"
                          % (variant, " ".join(lacking),
                             " ".join(impl["tree_features"]) or "no features"))
    return errors


def vtium_golden_cases():
    """{variant: its cases} for every variant judged by vtium's goldens,
    wholly or where it has re-baselined one ("rebaselined")."""
    return {v: variant_cases(v) for v in VARIANTS
            if VARIANTS[v].get("goldens") or VARIANTS[v].get("rebaselined")}


def c_golden_cases():
    """{variant: its cases} for every variant judged by the C reference's
    goldens: all but those judged by vtium's ("goldens") and those whose
    cases no golden judges ("verdict")."""
    return {v: variant_cases(v) for v in VARIANTS
            if not VARIANTS[v].get("goldens") and not VARIANTS[v].get("verdict")}


def c_goldens_judging(root):
    """The paths under `root`, the C reference's goldens, of every golden a
    case compares with: each case of a variant c_golden_cases names, but
    for a check without a golden and a re-baselined case that vtium's
    golden judges (golden_root)."""
    return {golden_for(v, c, root)[0] for v, cases in c_golden_cases().items() for c in cases
            if golden_root(v, c) is None and golden_for(v, c, root)[0]}


def golden_root(variant, case):
    """The goldens that judge `case` of `variant`: vtium's for a variant
    naming them ("goldens"); for a re-baselined variant ("rebaselined"),
    vtium's where goldens/ holds the case's golden, and the C reference's,
    None, where it does not; the C reference's for any other variant. The
    C reference's golden of a re-baselined case stays in c-goldens/ as the
    record of what the C reference drew."""
    v = VARIANTS[variant]
    if v.get("goldens"):
        return v["goldens"]
    if v.get("rebaselined"):
        golden = golden_for(variant, case, VTIUM_GOLDEN)[0]
        if golden is not None and os.path.exists(golden):
            return VTIUM_GOLDEN
    return None


def evaluate_variant(variant, caps, meta, log):
    """The report entries of `variant`'s cases from their captures, each
    judged by the goldens golden_root names, or given the variant's own
    verdict ("verdict"), which no golden decides."""
    return [evaluate(variant, case, caps.get(case), meta, log, golden_root(variant, case),
                     verdict=VARIANTS[variant].get("verdict"))
            for case in variant_cases(variant)]


def wait_install(vm):
    """The guest's TIER3-INSTALL-OK line, which its setup writes once the
    install is done. A setup step that fails writes a TIER3-INSTALL-FAIL
    line instead and stops, so that line ends the wait at once with its
    text, where waiting on would only time out."""
    return vm.wait_serial("TIER3-INSTALL-OK", timeout=180, fail="TIER3-INSTALL-FAIL")


def check_install(line, impl):
    """The guest's account of what it installed (the TIER3-INSTALL-OK line
    of guest/setup or guest-sysvinit/setup), as metadata: the tree, the
    kernel the guest runs (`uname -r`), and any other field the guest
    reports, such as guest-sysvinit's init, systemd and udev. Raises when
    the guest holds a kmscon, libtsm or vtium package, which the base image
    never installs, or unpacked a tree other than the run's: either would
    let another build's results stand for this one's. Raises too when the
    guest names no kernel, as the result would not say which kernel it ran
    on."""
    m = re.search(r"TIER3-INSTALL-OK packages: (.*?);((?: \w+: [^;]*;)*) tree: (\S+)", line)
    if not m:
        raise RuntimeError("cannot read the installed tree from %r" % line)
    if m.group(1).strip():
        raise RuntimeError("the guest has packages installed that are not the run's tree: %s"
                           % m.group(1).strip())
    if m.group(3) != impl["tree_sha256"]:
        raise RuntimeError("the guest installed tree %s, the run's is %s"
                           % (m.group(3), impl["tree_sha256"]))
    fields = dict(re.findall(r" (\w+): ([^;]*);", m.group(2)), tree=m.group(3))
    if not fields.get("kernel", "").strip():
        raise RuntimeError("the guest reported no kernel version in %r" % line)
    return fields


# guest/setup's account of the image helper (its "images" field) once
# systemd-sysusers has made the helper's account from the tree's sysusers
# file: the helper, its mode, and the account's ids, neither of them 0, or,
# in a variant whose pre-login account is nobody ("image_account"), that
# setup removed the account again.
IMAGE_HELPER = {
    "vtium-image": re.compile(r"helper /usr/bin/vtium-image mode 755, account vtium-image "
                              r"uid [1-9]\d* gid [1-9]\d*"),
    "nobody": re.compile(r"helper /usr/bin/vtium-image mode 755, account vtium-image "
                         r"removed"),
}


def check_guest(variant, installed):
    """Raises when the guest's account (check_install) shows it is not the
    guest `variant` names: a variant whose console is built with images
    needs the image helper installed and its account made, or removed in
    one whose pre-login account is nobody (IMAGE_HELPER);
    under sysvinit, PID 1 must be init, systemd's /run/systemd/system must
    be absent, and a udev daemon must run, or none in a variant without
    udev."""
    v = VARIANTS[variant]
    if "images" in v.get("tree_features", ()):
        helper = installed.get("images", "not reported")
        account = v.get("image_account", "vtium-image")
        if not IMAGE_HELPER[account].fullmatch(helper):
            raise RuntimeError("the guest's image helper is %r; the variant needs "
                               "/usr/bin/vtium-image and the account vtium-image%s "
                               "(ADR-1101 D2)" % (helper, " removed" if account == "nobody"
                                                   else ""))
    if v.get("guest") != "sysvinit":
        return
    if installed.get("init", "").split()[:1] != ["init"] or installed.get("systemd") != "no":
        raise RuntimeError("the guest's PID 1 is %r and systemd's directory %s; the variant "
                           "runs sysvinit" % (installed.get("init"),
                                              "exists" if installed.get("systemd") == "yes"
                                              else "is not reported"))
    udev = installed.get("udev")
    if udev is None or (udev == "none") == v.get("udev", True):
        raise RuntimeError("the guest's udev daemon is %r; the variant runs %s" % (
            udev, "one" if v.get("udev", True) else "none"))


def build_disc(variant, work, impl):
    """The T3PKG disc: the tree as tree.tar, the variant's config, the
    screens, its guest's setup, and t3-console on the guest without
    systemd, the variant's sysctl settings if it has any, vtium built with
    its features if it names any, the test fonts if it draws with them, the
    frits it builds, the frits it installs to be generated at run time, a
    `helper-held` file if its helper never answers, a `pointer` file if it
    reads the cursor plane, the showcase's fonts in showcase-fonts and its
    pictures and palettes in screens/showcase (t3_showcase) if it draws the
    showcase, a `no-udev` file if it runs without a udev daemon, the test pictures
    (t3_pictures) in screens/pictures if it draws them, a `two-vts` file
    if it runs a second vtium on tty3 from the boot, a `udev-examples`
    file if it runs that check, whose guest installs the example hwdb
    fragment, its head file as `heads.conf` if it names one, and the agent
    and its unit if it runs it."""
    disc = os.path.join(work, "disc")
    shutil.rmtree(disc, ignore_errors=True)
    os.makedirs(disc)
    shutil.copy(impl["tar"], os.path.join(disc, "tree.tar"))
    shutil.copy(os.path.join(GUEST, VARIANTS[variant]["conf"]),
                os.path.join(disc, "vtium.conf"))
    shutil.copytree(os.path.join(GUEST, "screens"), os.path.join(disc, "screens"))
    shutil.copy(os.path.join(GUEST, "t3"), disc)
    if VARIANTS[variant].get("pictures"):
        t3_pictures.write(os.path.join(disc, "screens", "pictures"))
    if VARIANTS[variant].get("showcase"):
        t3_showcase.disc_fonts(disc)
        t3_showcase.disc_screens(os.path.join(disc, "screens"))
    if VARIANTS[variant].get("image_account") == "nobody":
        open(os.path.join(disc, "no-image-account"), "w").close()
    if VARIANTS[variant].get("rpc_pipefs"):
        open(os.path.join(disc, "rpc-pipefs"), "w").close()
    if VARIANTS[variant].get("guest") == "sysvinit":
        shutil.copy(os.path.join(GUEST_SYSVINIT, "setup"), disc)
        shutil.copy(os.path.join(GUEST_SYSVINIT, "t3-console"), disc)
    else:
        shutil.copy(os.path.join(GUEST, "setup"), disc)
    if not VARIANTS[variant].get("udev", True):
        open(os.path.join(disc, "no-udev"), "w").close()
    if VARIANTS[variant].get("two_vts"):
        open(os.path.join(disc, "two-vts"), "w").close()
    if "sysctl" in VARIANTS[variant]:
        with open(os.path.join(disc, "sysctl.conf"), "w") as f:
            f.write(VARIANTS[variant]["sysctl"])
    if "features" in VARIANTS[variant]:
        shutil.copy(impl["features"][VARIANTS[variant]["features"]], os.path.join(disc, "vtium"))
    if VARIANTS[variant].get("fonts"):
        shutil.copytree(impl["fonts"], os.path.join(disc, "fonts"))
    if VARIANTS[variant].get("frits"):
        os.makedirs(os.path.join(disc, "frits"))
        for frit in VARIANTS[variant]["frits"]:
            shutil.copy(os.path.join(GUEST, "frits", frit), os.path.join(disc, "frits"))
    if VARIANTS[variant].get("deferred_frits"):
        os.makedirs(os.path.join(disc, "deferred-frits"))
        for frit in VARIANTS[variant]["deferred_frits"]:
            shutil.copy(os.path.join(GUEST, "frits", frit),
                        os.path.join(disc, "deferred-frits"))
    if VARIANTS[variant].get("helper") == "held":
        open(os.path.join(disc, "helper-held"), "w").close()
    if VARIANTS[variant].get("pointer"):
        open(os.path.join(disc, "pointer"), "w").close()
    if "udev-examples" in VARIANTS[variant].get("checks", []):
        open(os.path.join(disc, "udev-examples"), "w").close()
    if "heads" in VARIANTS[variant]:
        shutil.copy(t3_heads.head_file(VARIANTS[variant]), os.path.join(disc, "heads.conf"))
    if VARIANTS[variant].get("agent"):
        for name in ("t3-agent", "t3-agent.service"):
            shutil.copy(os.path.join(GUEST, name), disc)
    iso = os.path.join(work, "t3pkg.iso")
    make_iso(disc, "T3PKG", iso)
    return iso


def console_displays(serial):
    """The console's "Display ... with backend [..] text renderer [..]" log
    lines."""
    with open(serial, "rb") as f:
        text = f.read().decode(errors="replace")
    return sorted({line[line.index("Display "):].strip()
                   for line in text.splitlines()
                   if "Display " in line and "text renderer [" in line})


def read_events(vm):
    """The lines the guest has written to its event port so far."""
    with open(vm.events, "rb") as f:
        return [e.strip() for e in f.read().decode(errors="replace").splitlines()]


def bench_figures(vm, caps, conf, images=True):
    """Each drawn benchmark screen's figures (t3_bench.measure), by name. In a
    variant without images, an image screen has no picture stream to measure
    (not_offered); in one with images, an image screen that found them not
    offered measured none, which is an "error"."""
    with open(vm.serial, "rb") as f:
        serial = f.read().decode(errors="replace")
    events = read_events(vm)
    out = {}
    for name in BENCHMARKS:
        if name in caps:
            line = next((e for e in events if e.startswith("T3-BENCH %s " % name)), None)
            refused = name in DRAWS_PICTURES and reported_not_offered(events, name)
            if not images and name in IMAGE_SCREENS:
                out[name] = not_offered(events, name, line)
            elif refused:
                out[name] = {"error": "the screen found images not offered (%s), so no "
                                      "picture stream was measured" % refused}
            else:
                out[name] = t3_bench.measure(serial, line, conf)
    return out


def not_offered(events, name, window):
    """The figures of image screen `name` in a variant without images: it
    must have found images not offered (T3-NO-SIXEL or T3-NO-KITTY, from
    sixel-common.inc and kitty-common.inc, after the screen before it ended)
    and sent no picture stream (no T3-BENCH window); either failing is an
    "error", which fails the case."""
    if window is not None:
        return {"error": "a build without images measured a picture stream: %r" % window}
    seen = reported_not_offered(events, name)
    if not seen:
        return {"error": "the screen did not report images as not offered"}
    return {"not_offered": seen}


def reported_not_offered(events, name):
    """The refusal screen `name` reported on the event port, after the screen
    before it ended and before it ended itself: T3-NO-SIXEL or T3-NO-KITTY
    (sixel-common.inc, kitty-common.inc) when the terminal offers no
    pictures, or T3-NO-PICTURES (img-common.inc) when the test pictures are
    not on the disc; None when it reported none."""
    seen = None
    for e in events:
        if e == "T3-DONE %s" % name:
            return seen
        if e.startswith("T3-DONE "):
            seen = None
        elif e in ("T3-NO-SIXEL", "T3-NO-KITTY", "T3-NO-PICTURES"):
            seen = e
    return None


def screen_replies(events, name):
    """What screen `name` reported on the event port (T3-REPLY,
    img-common.inc), in order, after the screen before it ended and before
    it ended itself."""
    seen = []
    for e in events:
        if e == "T3-DONE %s" % name:
            return seen
        if e.startswith("T3-DONE "):
            seen = []
        elif e.startswith("T3-REPLY "):
            seen.append(e[len("T3-REPLY "):])
    return []


def judge_replies(name, got):
    """Why the replies screen `name` reported (`got`) are wrong, against
    t3_evaluate.REPLIES, or None when they are the ones it must get."""
    want = REPLIES[name]
    if got == want:
        return None
    return "the screen reported %s; it must report %s" % (
        ", ".join(got) or "no reply", ", ".join(want))


def resident_kb(vm):
    """The console's resident memory in kB after each screen, by screen: the
    largest VmRSS of the vtium or kmscon processes guest/t3 found (one, on
    tty2, until the VT switch)."""
    out = {}
    for e in read_events(vm):
        fields = e.split()
        if len(fields) >= 3 and fields[0] == "T3-RSS":
            values = [int(v) for v in fields[2:] if v.isdigit()]
            if values:
                out[fields[1]] = max(values)
    return out


def resident(rss_kb, screens=SCREENS):
    """The minspec memory figures from `resident_kb`: the resident set after
    the first of `screens`, and the peak over `screens` with the screen it
    followed (s1-scroll, once its lines fill the scrollback, for the C
    reference's minimal build); the pointer case's readings are no screen.
    VmRSS is in KiB; None where nothing was read, as for a variant that
    draws no screen."""
    first = screens[0] if screens else None
    drawn = [s for s in screens if s in rss_kb]
    peak = max(drawn, key=rss_kb.get) if drawn else None
    return {"first_screen": first, "first_kib": rss_kb.get(first),
            "peak_kib": rss_kb.get(peak), "peak_screen": peak}


def console_startup_ms(serial):
    """Milliseconds from the first console process's first log line (its
    revision) to its "Display ... with backend" line, the display set up and
    its terminal drawing, by the kernel's timestamps on the console; None
    without both. The time before the console logs (exec and dynamic
    linking) is not in it."""
    with open(serial, "rb") as f:
        text = f.read().decode(errors="replace")
    first = re.search(r"\[\s*([0-9.]+)\] (kmscon|vtium)\[(\d+)\]: log: \w+ Revision", text)
    if not first:
        return None
    shown = re.search(r"\[\s*([0-9.]+)\] %s\[%s\]: (?:terminal: )?Display .* with backend"
                      % (first.group(2), first.group(3)), text)
    if not shown:
        return None
    return round((float(shown.group(1)) - float(first.group(1))) * 1000, 1)


def console_cell(serial):
    """The [width, height] of a character cell, from the "cell size WxH" log
    line, or None when the console logged none."""
    with open(serial, "rb") as f:
        m = re.search(rb"cell size (\d+)x(\d+)", f.read())
    return [int(m.group(1)), int(m.group(2))] if m else None


def run_variant(variant, timing, impl, log):
    """Boot one variant with the tree `impl` (t3_impl) and drive the
    scenario.

    Returns ({case: capture path}, metadata). A scenario that fails part way
    keeps the captures taken before the failure; metadata carries the error,
    and per-case errors under "errors". If the console drew with a renderer
    other than the one the variant tests, no capture is returned: they show
    the wrong path. `timing` gets the seconds from the start of the variant to the
    end of each phase.

    Each step waits for state it can observe, never for a fixed time: lines
    on the console, events the guest reports on its event port (guest/setup),
    and a screen that has changed and then settled. Every wait has a timeout
    that fails the case. On a slow host a capture taken too early can fail
    its case; it cannot pass it.
    """
    start = time.time()

    def phase(name):
        timing[name] = round(time.time() - start, 1)
        log("phase %s done at %.1f s" % (name, timing[name]))

    work = os.path.join(RUNDIR, variant)
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    iso = build_disc(variant, work, impl)
    disk = os.path.join(work, "run.qcow2")
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2",
                    "-b", base_of(variant)[0], disk], check=True)
    v = VARIANTS[variant]
    guest = guest_of(variant)
    vm = Qemu(work, [(disk, "qcow2", False), (iso, "raw", True)],
              v["device"], log, display=v["display"],
              capture=v.get("capture", "screendump"), cpus=v["cpus"], memory_mb=v["memory_mb"],
              extra=v.get("qemu", ()),
              heads=t3_devices.heads(v.get("outputs", []) + ([v["second"]] if "second" in v
                                                              else [])),
              agent=v.get("agent", False))
    caps, errors, notes, error, installed = {}, {}, {}, None, None
    try:
        line = wait_install(vm)
        log(line)
        installed = check_install(line, impl)
        check_guest(variant, installed)
        phase("install")
        for marker in guest["boot"]:
            vm.wait_serial(marker, timeout=120)
        phase("boot")
        # tty1 runs getty. Under systemd, switching to tty2 makes logind
        # spawn autovt@tty2, which is vtium@tty2 through the template link
        # that the setup's vtium console enable --all makes, and nothing
        # starts it earlier: a boot without the keypress has no console on
        # tty2.
        # Under sysvinit, the console the inittab started on tty2 takes it.
        vm.keys("ctrl", "alt", "f2")
        vm.wait_serial(guest["shown"], timeout=60)
        tmp = os.path.join(work, "wait.ppm")
        prompt = None
        if v["device"] is not None and v["renderer"] is not None:
            prompt = wait_stable(vm, tmp, settle=2.0, timeout=90, want=not_blank)
        if "second" in v:
            wait_stable(vm, os.path.join(work, "two-heads-start.ppm"), settle=2.0, timeout=90,
                        want=not_blank, output=t3_heads.second_output(v))
        if "first" in v:
            # The first screen is captured once the console has logged the
            # outcome it shows (t3_heads.LINES), and once settled after it.
            line = t3_heads.LINES[v["first"]]
            try:
                vm.wait_serial(line, timeout=10)
            except RuntimeError as e:
                errors[v["first"]] = "the console did not log its outcome: %s" % e
            else:
                path = os.path.join(work, v["first"] + ".ppm")
                wait_stable(vm, path, settle=2.0, timeout=90, want=not_blank)
                caps[v["first"]] = path
        if v.get("prelogin"):
            prompt = check_prelogin(variant, vm, work, prompt, caps, errors, log)
        phase("console")
        if "power" in v:
            press_power_keys(vm, v["power"], work, caps, errors, notes, log, phase)
        elif not v.get("login", True):
            run_checks(variant, vm, work, caps, errors, log, phase)
        else:
            draw_screens(variant, vm, work, prompt, caps, errors, log, phase)
    except Exception as e:  # reported per case; the captures so far still count
        error = str(e)
        log(error)
    finally:
        vm.stop()
        if os.path.exists(disk):
            os.unlink(disk)
    displays = console_displays(vm.serial)
    with open(os.path.join(GUEST, v["conf"])) as f:
        conf = [line.strip() for line in f if line.strip() and not line.startswith("#")]
    # A check on a guest with no display passes with no capture.
    passed = sorted(case for case, path in caps.items() if path is None)
    caps = {case: path for case, path in caps.items() if path is not None}
    rss = resident_kb(vm)
    meta = {"installed": installed, "kernel": installed and installed["kernel"],
            "passed": passed,
            "displays": displays, "errors": errors, "notes": notes, "device": v["device"],
            "qemu": list(v.get("qemu", ())), "config": conf,
            "cell": console_cell(vm.serial), "cpus": v["cpus"], "memory_mb": v["memory_mb"],
            "bench": bench_figures(vm, caps, conf, v.get("images", True)),
            "rss_kb": rss, "resident": resident(rss, screens_of(variant)),
            "startup_ms": console_startup_ms(vm.serial)}
    if "heads" in v:
        with open(t3_heads.head_file(v)) as f:
            meta["head_file"] = {"path": v["heads"], "policy": v["policy"],
                                 "lines": f.read().splitlines()}
    meta["console"] = console_revision(vm.serial)
    errors_all, drop = variant_errors(error, displays, v["renderer"], caps, rss,
                                      meta["console"], screens_of(variant),
                                      v.get("outputs", ()))
    if drop:
        caps = {}
    if errors_all:
        meta["error"] = "; ".join(errors_all)
    return caps, meta


def check_prelogin(variant, vm, work, prompt, caps, errors, log):
    """The prelogin case (t3_images): the agent writes two pictures to the
    login prompt's pty and reports the pty's owner and the image helpers'
    accounts, and the screen is captured once it differs from `prompt`.
    The capture is kept when t3_images.judge passes it; `errors` gets why
    otherwise. Returns the screen now shown, the login prompt the login
    starts from."""
    case = t3_images.CASE
    path = os.path.join(work, case + ".ppm")
    try:
        answer = vm.ask(t3_images.CASE)
        log("%s: the agent answered %s" % (case, "; ".join(answer)))
        shown = wait_stable(vm, path, want=lambda img: img.data != prompt.data)
    except RuntimeError as e:
        errors[case] = str(e)
        return wait_stable(vm, path)
    why = t3_images.judge(answer, shown, VARIANTS[variant].get("image_account",
                                                              "vtium-image"))
    if why:
        errors[case] = why
    else:
        caps[case] = path
    return shown


def draw_screens(variant, vm, work, prompt, caps, errors, log, phase):
    """Log in at the console's `prompt`, draw every screen and its keyed
    cases, then switch VTs away and back unless the variant switches none,
    and run its checks, filling `caps` with each case's capture and
    `errors` with each case's error."""
    v = VARIANTS[variant]
    outputs = v.get("outputs", [])
    tmp = os.path.join(work, "wait.ppm")
    vm.type("tester\n")
    # The user name echoes; then login asks for the password.
    wait_stable(vm, tmp, want=lambda img: img.data != prompt.data)
    vm.type("tester\n")
    try:
        line = vm.wait_event("T3-LOGIN", timeout=20)
    except RuntimeError:
        # PAM discards typed-ahead input when it turns echo off for the
        # password, so a password typed before the prompt is lost. Typing
        # it again is harmless otherwise: the shell runs it as a command,
        # and every screen clears before it draws.
        log("no login yet; typing the password again")
        vm.type("tester\n")
        line = vm.wait_event("T3-LOGIN", timeout=60)
    check_login(line)
    last = wait_stable(vm, tmp)
    # The capture before each output's next, which that one must differ from.
    lasts = {o["suffix"]: wait_stable(vm, tmp, output=t3_devices.capture(o)) for o in outputs}
    phase("login")
    for name in screens_of(variant):
        vm.type("t3 %s\n" % name)
        vm.wait_event("T3-DONE %s" % name,
                      timeout=BENCH_TIMEOUT if name in BENCHMARKS else SCREEN_TIMEOUT)
        path = os.path.join(work, name + ".ppm")
        last, why = capture_screen(vm, name, path, last)
        caps[name] = path
        if why:
            errors[name] = why
        capture_outputs(vm, outputs, work, name, caps, lasts)
        phase(name)
        # The keyed cases of this screen, each captured once the screen
        # has changed from the capture before it: a key that changes
        # nothing fails its case. An image screen that found images not
        # offered drew no picture for its keys to act on, so they are
        # not pressed and its keyed cases fail with the refusal; the
        # screens after it still run. In a variant with images, the
        # screen's own case fails with the refusal too, whatever its
        # golden, provisional or not.
        events = read_events(vm)
        refused = name in DRAWS_PICTURES and reported_not_offered(events, name)
        if refused and v.get("images", True):
            errors[name] = "drew no picture (%s) in a variant with images" % refused
        elif name in REPLIES and v.get("images", True):
            why = judge_replies(name, screen_replies(events, name))
            if why:
                errors[name] = "; ".join(filter(None, (errors.get(name), why)))
        if v.get("showcase"):
            why = t3_showcase.faults(events, name)
            if why:
                errors[name] = "; ".join(filter(None, [errors.get(name)] + why))
        for screen, case, chords in KEYED:
            if screen != name or case not in variant_cases(variant):
                continue
            if refused:
                errors[case] = ("not pressed: %s drew no picture (%s), and the keys act "
                                "on pictures" % (name, refused))
                continue
            for chord in chords:
                vm.keys(*chord)
            path = os.path.join(work, case + ".ppm")
            last = wait_stable(vm, path, want=lambda img, prev=last: img.data != prev.data)
            caps[case] = path
            capture_outputs(vm, outputs, work, case, caps, lasts)
            phase(case)
        for chord in KEYS_AFTER.get(name, []) if v.get("images", True) and not refused else []:
            vm.keys(*chord)
    if v.get("vt_switch", True):
        switch_vts(variant, vm, work, last, caps, errors, phase)
    if v.get("mirror"):
        check_mirror(variant, caps, errors)
    if v.get("pointer"):
        check_pointer(variant, vm, work, caps, errors, log)
        phase(t3_pointer.CASE)
    run_checks(variant, vm, work, caps, errors, log, phase)


def capture_screen(vm, name, path, last):
    """Capture screen `name` into `path` once it has settled, differs from
    `last`, the capture of the screen before it, so that capture is never
    taken for this one, and shows the pictures t3_evaluate.SHOWS lists for
    it, so an animation is captured on the frame it stops on and never on
    one it passes. Returns the capture and why the case fails, or None.

    A screen with pictures to show that settles without them, or never
    settles, gives the capture it ended on and the reason, so its case
    fails whatever its golden says and the screens after it still run.
    Any other screen that never gets there raises, as before."""
    def want(img):
        return img.data != last.data and not_shown(name, img) is None

    try:
        return wait_stable(vm, path, want=want), None
    except RuntimeError as e:
        if name not in SHOWS:
            raise
        img = t3img.read(path)
        return img, not_shown(name, img) or str(e)


def switch_vts(variant, vm, work, last, caps, errors, phase):
    """The VT switch away from tty2 and back, the vt-switch and vt-restore
    cases, `last` the screen tty2 showed before it."""
    v = VARIANTS[variant]
    outputs = v.get("outputs", [])
    tmp = os.path.join(work, "wait.ppm")
    lasts = {}
    # VT switch away and back. vt-switch: tty3, where logind starts a
    # second console at the switch, or the inittab a second console or a
    # getty at the boot, must show a screen (its login prompt). vt-restore:
    # back on tty2, the screen must be exactly as it was left. A switch
    # that does not visibly happen fails vt-switch, so vt-restore cannot
    # pass alone without testing anything.
    vm.keys("ctrl", "alt", "f3")
    path = os.path.join(work, "vt-switch.ppm")
    try:
        if v.get("guest") != "sysvinit" or v.get("two_vts"):
            vm.wait_serial("vt: using tty /dev/tty3", timeout=60)
        wait_stable(vm, path, settle=2.0, timeout=60,
                    want=lambda img: not_blank(img) and img.data != last.data)
        caps["vt-switch"] = path
    except RuntimeError as e:
        errors["vt-switch"] = "no new console screen on tty3: %s" % e
    phase("vt-switch")
    # The restored screen must differ from the one shown before the
    # switch back, so tty3's screen, drawn or blank, is never taken for
    # tty2's. A switch to tty3 that never happened fails vt-restore too.
    away = vm.screendump(tmp)
    for o in outputs:
        lasts[o["suffix"]] = vm.screendump(tmp, t3_devices.capture(o))
    vm.keys("ctrl", "alt", "f2")
    path = os.path.join(work, "vt-restore.ppm")
    restored = wait_stable(vm, path, want=lambda img: img.data != away.data)
    caps["vt-restore"] = path
    # The restored screen must also be the one tty2 showed before the
    # switch, pixel for pixel, whether or not its golden is reviewed yet: a
    # console that draws its text again and drops its pictures fails here.
    why = restore_differs(last, restored)
    if why:
        errors["vt-restore"] = why
    capture_outputs(vm, outputs, work, "vt-restore", caps, lasts)
    phase("vt-restore")


def restore_differs(before, after):
    """Why the screen `after` the switch back is not the screen `before` the
    switch, or None when the two are identical."""
    stats, _, _ = t3img.compare(after, before)
    if not stats["pixels"] and "size_mismatch" not in stats:
        return None
    return ("the screen after the switch back differs from the screen before the switch "
            "in %s pixels" % stats.get("pixels", "all"))


def run_checks(variant, vm, work, caps, errors, log, phase):
    """Run each of `variant`'s checks (CHECK_RUNS) in order."""
    v = VARIANTS[variant]
    for check in v.get("checks", []):
        CHECK_RUNS[check](v, vm, work, caps, errors, log)
        phase(check)


def capture_outputs(vm, outputs, work, case, caps, lasts):
    """Capture `case` from each of `outputs` after the first, once its
    screen has settled and differs from its capture before (`lasts`, by
    suffix, which takes the new one)."""
    for o in outputs:
        name = case + o["suffix"]
        path = os.path.join(work, name + ".ppm")
        lasts[o["suffix"]] = wait_stable(
            vm, path, output=t3_devices.capture(o),
            want=lambda img, prev=lasts[o["suffix"]]: img.data != prev.data)
        caps[name] = path


def check_mirror(variant, caps, errors):
    """The mirror check (t3_devices.CHECKS): every case with a golden that
    the first output drew, against the same case on each other output. Its
    capture is the last screen's on the last output, kept only when every
    capture matched; `errors` gets the cases that differ otherwise."""
    outputs = VARIANTS[variant]["outputs"]
    images, missing = {}, []
    for case in variant_cases(variant):
        if case in NO_GOLDEN or t3_devices.split(case)[1] or case not in caps:
            continue
        others = t3_devices.output_cases(case, outputs)
        missing += [c for c in others if c not in caps]
        images[case] = (t3img.read(caps[case]),
                        [(c, t3img.read(caps[c])) for c in others if c in caps])
    why = ("not captured on every output: %s" % ", ".join(missing) if missing
           else None if images else "no screen was captured")
    why = why or t3_devices.judge_mirror(images)
    if why:
        errors["mirror"] = why
    else:
        caps["mirror"] = caps[screens_of(variant)[-1] + outputs[-1]["suffix"]]


def check_udev_examples(_variant, vm, work, caps, errors, log):
    """The udev-examples check (t3_devices.CHECKS): the guest reads the
    example files with its udev (screens/udev-examples.sh), and the account
    it prints is judged. The capture is the screen the script left, kept
    when the account passed."""
    case = "udev-examples"
    try:
        lines = t3_devices.run_screen(vm, case, SCREEN_TIMEOUT)
        why = t3_devices.judge_udev(*t3_devices.parse_udev(lines))
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path)
        caps[case] = path
    except RuntimeError as e:
        errors[case] = str(e)


def check_keyboard_hotplug(_variant, vm, work, caps, errors, log):
    """The keyboard-hotplug check (t3_devices.CHECKS), after udev-examples,
    which reads the boot's USB keyboard. That keyboard is pulled out, so the
    console's keys reach the PS/2 keyboard, and screens/keyboard-hotplug.sh,
    which lists the keyboards and the PS/2 keyboard's interrupt count, runs
    twice on it, the count's control. A USB keyboard is plugged into the hub
    through QMP, bound to the first display's console so QMP's
    input-send-event types on it alone, and the script runs twice on it;
    then it is pulled out and the script runs once more on the console's.
    t3_devices.judge_hotplug judges the runs. The capture is the screen the
    last run left."""
    case = "keyboard-hotplug"
    try:
        vm.cmd("device_del", id=t3_devices.BOOT_ID)
        t3_devices.wait_qmp_event(vm, "DEVICE_DELETED", t3_devices.BOOT_ID, 30)
        control = [t3_devices.run_screen(vm, case, SCREEN_TIMEOUT) for _ in range(2)]
        since = t3_devices.serial_size(vm)
        vm.cmd("device_add", driver="usb-kbd", id=t3_devices.HOT_ID,
               bus=t3_devices.USB_BUS, port=t3_devices.HOT_PORT, display="video0")
        t3_devices.wait_serial_after(vm, t3_devices.KEYBOARD_LINE, since, 30)

        def type_hot(text):
            for ch in text:
                codes = char_qcodes(ch)
                for down in (True, False):
                    vm.cmd("input-send-event", device="video0",
                           events=t3_devices.key_events(codes, down))
                time.sleep(0.05)

        plugged = t3_devices.run_screen(vm, case, SCREEN_TIMEOUT, type_hot)
        typed = t3_devices.run_screen(vm, case, SCREEN_TIMEOUT, type_hot)
        with open(vm.serial, "rb") as f:
            took = [m.group(1) for m in t3_devices.KEYBOARD_LINE.finditer(
                f.read()[since:].decode(errors="replace"))]
        log("%s: vtium took %s after the plug" % (case, ", ".join(took)))
        vm.cmd("device_del", id=t3_devices.HOT_ID)
        t3_devices.wait_qmp_event(vm, "DEVICE_DELETED", t3_devices.HOT_ID, 30)
        pulled = t3_devices.run_screen(vm, case, SCREEN_TIMEOUT)
        why = t3_devices.judge_hotplug(control, plugged, typed, pulled, took)
        if why:
            raise RuntimeError(why)
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path)
        caps[case] = path
    except RuntimeError as e:
        errors[case] = str(e)
        log("%s: %s" % (case, e))


# The checks a variant names ("checks"), each run after the VT switch, or
# after the first screen when the console gives no login; each takes the
# variant's entry in VARIANTS first.
CHECK_RUNS = {"udev-examples": check_udev_examples,
              "keyboard-hotplug": check_keyboard_hotplug,
              **t3_heads.CHECK_RUNS}


def check_login(line):
    """Raises unless the login shell reported (T3-LOGIN, guest/setup) a
    pty: vtium's session is on one, and a login on a VT's own tty is a
    getty's, whose screens the kernel's console draws, as when the
    console was not on the VT the keys reached."""
    if not re.search(r"T3-LOGIN /dev/pts/\d+$", line.strip()):
        raise RuntimeError("the login was not vtium's: %r, not a pty" % line.strip())


def check_pointer(variant, vm, work, caps, errors, log):
    """The pointer case (t3_pointer): the pointer moved to each of
    t3_pointer.MOVES with QMP, the guest's DRM state read after each move and
    once the pointer hid, each reading judged. The capture, the screen the
    last reading printed, is kept only when every reading passed; `errors`
    gets the first reason otherwise."""
    case = t3_pointer.CASE
    m = re.search(r"xres=(\d+),yres=(\d+)", VARIANTS[variant]["device"])
    display = (int(m.group(1)), int(m.group(2)))
    cell = console_cell(vm.serial)
    path = os.path.join(work, case + ".ppm")
    try:
        if cell is None:
            raise RuntimeError("the console logged no cell size")
        for n, at in enumerate(t3_pointer.MOVES):
            vm.cmd("input-send-event", events=t3_pointer.move_events(*at))
            if n == 0:
                vm.wait_serial(t3_pointer.PLANE_LINE, timeout=30)
            want = t3_pointer.corner(at, display, cell)
            why = read_plane(vm, display, want, log)
            if why:
                raise RuntimeError("after the move to %s: %s" % (at, why))
        # The pointer hides HIDE_S after its last move; read until it has.
        deadline = time.time() + t3_pointer.HIDE_S + t3_pointer.HIDE_SLACK_S
        why = "not read"
        while why and time.time() < deadline:
            why = read_plane(vm, display, None, log)
        if why:
            raise RuntimeError("once the pointer hid: %s" % why)
        wait_stable(vm, path)
        caps[case] = path
    except RuntimeError as e:
        errors[case] = str(e)
        log("%s: %s" % (case, e))


def read_plane(vm, display, want, log):
    """Has the guest print its DRM state (screens/pointer.sh) and judges it
    against `want` (t3_pointer.judge): the reason it failed, or None."""
    with open(vm.events, errors="replace") as f:
        since = len(f.read())
    vm.type("t3 pointer\n")
    deadline = time.time() + SCREEN_TIMEOUT
    while True:
        with open(vm.events, errors="replace") as f:
            text = f.read()
        if "T3-DONE pointer\n" in text[since:]:
            break
        if time.time() > deadline:
            return "no T3-DONE pointer within %d s; see %s" % (SCREEN_TIMEOUT, rel(vm.events))
        time.sleep(0.2)
    state, why = t3_pointer.dump(text, since)
    if why:
        return why
    why = t3_pointer.judge(state, display, want)
    log("cursor plane, want %s: %s" % (want, why or "as wanted"))
    return why


# Seconds a power case may take: the shutdown, the suspend, the wake-up.
POWER_TIMEOUT = 180


def press_power_keys(vm, cases, work, caps, errors, notes, log, phase):
    """Press each power case's chord on the console (t3_power), in order.
    A case's capture is the screen before its key, or after the wake-up for
    suspend; it is kept when the console log after the key, and QEMU's
    SHUTDOWN event for a case that ends the VM, pass t3_power.verdict, and
    `errors` gets the reason otherwise; `notes` gets a known failure's
    note. A case that ends the VM is last; a case that fails leaves the
    next to run."""
    for case in cases:
        path = os.path.join(work, case + ".ppm")
        try:
            wait_stable(vm, path, want=not_blank)
            with open(vm.serial, "rb") as f:
                since = len(f.read())
            vm.keys(*t3_power.CHORDS[case])
            if case in t3_power.SUSPENDS:
                vm.wait_status("suspended", timeout=POWER_TIMEOUT)
                log("the guest is suspended; waking it")
                vm.cmd("system_wakeup")
                vm.wait_status("running", timeout=60)
                if case == "suspend":
                    vm.wait_serial("Finished System Suspend", timeout=POWER_TIMEOUT)
                # Drawn again after the wake-up: a blank screen fails the case.
                wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
            shutdown = None
            if case not in t3_power.SUSPENDS:
                exited = vm.wait_exit(POWER_TIMEOUT) is not None
                shutdown = vm.shutdown_reason() if exited else "none: still running"
            with open(vm.serial, "rb") as f:
                why, note = t3_power.verdict(case, f.read(), since, shutdown)
            if why:
                raise RuntimeError(why)
            caps[case] = path
            if note:
                notes[case] = note
        except (RuntimeError, OSError) as e:  # QMP gone with the VM is an OSError
            errors[case] = str(e)
            log("%s: %s" % (case, e))
        phase(case)


def variant_errors(error, displays, renderer, caps, rss, console, screens=SCREENS,
                   outputs=()):
    """Every reason a variant failed, the scenario's own `error` first, and
    whether its captures must be dropped so every case fails: when a console
    drew with another renderer than the variant tests, the captures show the
    wrong path, and on a guest with no display device (`renderer` None) when
    it set one up; when a variant with `outputs` beyond the first did not set up
    a display for each, one Display line a display, a capture of an output
    may show QEMU's screen rather than vtium's; when a drawn screen has no
    resident-memory reading (`rss`), the run lacks a figure measured on
    every run; and when the console (`console_revision`) is not vtium, the
    captures are another console's."""
    out, drop = [error] if error else [], False
    want = "text renderer [%s]" % renderer
    if renderer is None:
        if displays:
            out.append("the console set up a display on a guest with none: %s" % displays)
            drop = True
    elif not displays or not all(want in d for d in displays):
        out.append("the console did not draw with %s: %s" % (renderer, displays))
        drop = True
    names = sorted({m.group(1) for m in (re.search(r"Display (\S+) with", d) for d in displays)
                    if m})
    if outputs and len(names) != 1 + len(outputs):
        out.append("the console set up %d display(s) (%s), not one for each of the %d outputs"
                   % (len(names), ", ".join(names) or "none", 1 + len(outputs)))
        drop = True
    unread = [s for s in screens if s in caps and s not in rss]
    if unread:
        out.append("no resident memory reading (T3-RSS) after %s" % ", ".join(unread))
        drop = True
    if console is None:
        out.append("the console logged no Revision line, so nothing shows vtium drew")
        drop = True
    elif console["name"] != "vtium":
        out.append("the console that drew logged itself as %s %s, not vtium"
                   % (console["name"], console["revision"]))
        drop = True
    return out, drop


def console_revision(serial):
    """The console that ran, by its own account: the "NAME Revision REV" line
    its log opens with, the first on the console, as {"name", "revision"};
    None when it logged none."""
    with open(serial, "rb") as f:
        m = re.search(rb"\] [\w.-]+\[\d+\]: log: (\S+) Revision (\S+)", f.read())
    return {"name": m.group(1).decode(errors="replace"),
            "revision": m.group(2).decode(errors="replace")} if m else None


def consoles(metas):
    """The distinct consoles the variants' guests logged, as "NAME REVISION"."""
    return sorted({"%s %s" % (m["console"]["name"], m["console"]["revision"])
                   for m in metas.values() if m.get("console")})


def failure_meta(error):
    """A variant's metadata when it produced no captures, in the same shape
    as a variant that ran."""
    return {"installed": None, "displays": [], "errors": {}, "error": error, "timing": {},
            "resident": resident({}), "console": None}


def failed_variant(variant, error, log):
    log("%s: %s" % (variant, error))
    meta = failure_meta(error)
    return evaluate_variant(variant, {}, meta, log), meta


def variant_job(variant, impl, log):
    """Drive and evaluate one variant: (its case entries, its metadata).

    `log` is the run's logger; the variant's lines carry its name. A variant
    that cannot run is reported, not fatal: each of its cases fails with the
    error.
    """
    log = log.labelled(variant)
    timing = {}
    try:
        caps, meta = run_variant(variant, timing, impl, log)
    except Exception as e:
        log(str(e))
        caps, meta = {}, failure_meta(str(e))
    start = time.time()
    cases = evaluate_variant(variant, caps, meta, log)
    timing["evaluate"] = round(time.time() - start, 1)
    log("phase evaluate took %.1f s" % timing["evaluate"])
    meta["timing"] = timing
    return cases, meta
