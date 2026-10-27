"""Tier 3's variants and the scenario each one runs: install the tree, log
in, draw the screens, switch VTs, then evaluate every case."""

import os
import re
import shutil
import subprocess
import time

import t3_bench
from t3_common import BASE, GUEST, RUNDIR, rel
from t3_evaluate import BENCHMARKS, CASES, IMAGE_SCREENS, KEYED, KEYS_AFTER, SCREENS, evaluate
from t3_qemu import Qemu, make_iso, not_blank, wait_stable

# Display variants. Each boots its own guest from the base image, with its
# own vCPUs and memory, and names the text renderer its console must log:
# bbulk for the 2D variants, gltex on virgl for the GL one, which the C
# reference drew with and vtium does not yet. Each draws every screen, the
# image ones included (t3_evaluate.IMAGE_SCREENS).
VARIANTS = {
    "2d-1280x800": {
        "device": "virtio-vga,xres=1280,yres=800,edid=on",
        "display": "none",
        "conf": "kmscon.conf",
        "renderer": "bbulk",
        "cpus": 4,
        "memory_mb": 4096,
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
    },
    "2d-potato-min-psf": {
        "device": "virtio-vga,xres=1366,yres=768,edid=on",
        "display": "none",
        "conf": "kmscon-minimal-psf.conf",
        "renderer": "bbulk",
        "cpus": 1,
        "memory_mb": 512,
        "images": False,
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
    },
    # The 7680x2160 display with DejaVu Sans Mono at 32 points, a 19x38 cell,
    # near the 20x40 of Monoid at 30 the operator runs: pictures sized from
    # the cell (sixel) are checked at a cell other than the 10x19 of the rest.
    "2d-7680x2160-font32": {
        "device": "virtio-vga,xres=7680,yres=2160,edid=on,max_hostmem=1G",
        "display": "none",
        "conf": "kmscon-font32.conf",
        "renderer": "bbulk",
        "cpus": 4,
        "memory_mb": 4096,
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
    },
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


def uses_virgl(variant):
    return VARIANTS[variant]["device"].startswith("virtio-vga-gl")


def variant_cases(variant):
    """The cases of `variant`'s report: every case, but for a variant without
    images the keyed cases of the image screens, whose keys act on pictures."""
    if VARIANTS[variant].get("images", True):
        return CASES
    skipped = {case for screen, case, _ in KEYED if screen in IMAGE_SCREENS}
    return [c for c in CASES if c not in skipped]


def evaluate_variant(variant, caps, meta, log):
    """The report entries of `variant`'s cases from their captures."""
    return [evaluate(variant, case, caps.get(case), meta, log) for case in variant_cases(variant)]


def check_install(line, impl):
    """The guest's account of what it installed (guest/setup's
    TIER3-INSTALL-OK line), as metadata. Raises when the guest holds a
    kmscon, libtsm or vtium package, which the base image never installs, or
    unpacked a tree other than the run's: either would let another build's
    results stand for this one's."""
    m = re.search(r"TIER3-INSTALL-OK packages: (.*?); tree: (\S+)", line)
    if not m:
        raise RuntimeError("cannot read the installed tree from %r" % line)
    if m.group(1).strip():
        raise RuntimeError("the guest has packages installed that are not the run's tree: %s"
                           % m.group(1).strip())
    if m.group(2) != impl["tree_sha256"]:
        raise RuntimeError("the guest installed tree %s, the run's is %s"
                           % (m.group(2), impl["tree_sha256"]))
    return {"tree": m.group(2)}


def build_disc(variant, work, impl):
    """The T3PKG disc: the tree as tree.tar, the variant's config, the
    screens and guest/setup."""
    disc = os.path.join(work, "disc")
    shutil.rmtree(disc, ignore_errors=True)
    os.makedirs(disc)
    shutil.copy(impl["tar"], os.path.join(disc, "tree.tar"))
    shutil.copy(os.path.join(GUEST, VARIANTS[variant]["conf"]),
                os.path.join(disc, "kmscon.conf"))
    shutil.copytree(os.path.join(GUEST, "screens"), os.path.join(disc, "screens"))
    shutil.copy(os.path.join(GUEST, "t3"), disc)
    shutil.copy(os.path.join(GUEST, "setup"), disc)
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
            refused = name in IMAGE_SCREENS and reported_not_offered(events, name)
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
    """The refusal screen `name` reported on the event port, T3-NO-SIXEL or
    T3-NO-KITTY (sixel-common.inc, kitty-common.inc), after the screen before
    it ended and before it ended itself; None when it reported none."""
    seen = None
    for e in events:
        if e == "T3-DONE %s" % name:
            return seen
        if e.startswith("T3-DONE "):
            seen = None
        elif e in ("T3-NO-SIXEL", "T3-NO-KITTY"):
            seen = e
    return None


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


def resident(rss_kb):
    """The minspec memory figures from `resident_kb`: the resident set after
    the first screen, and the peak over every screen with the screen it
    followed (s1-scroll, once its lines fill the scrollback, for the C
    reference's minimal build). VmRSS is in KiB; None where nothing was read."""
    first = SCREENS[0]
    peak = max(rss_kb, key=rss_kb.get) if rss_kb else None
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
                    "-b", BASE, disk], check=True)
    v = VARIANTS[variant]
    vm = Qemu(work, [(disk, "qcow2", False), (iso, "raw", True)],
              v["device"], log, display=v["display"],
              capture=v.get("capture", "screendump"), cpus=v["cpus"], memory_mb=v["memory_mb"])
    caps, errors, error, installed = {}, {}, None, None
    try:
        line = vm.wait_serial("TIER3-INSTALL-OK", timeout=180)
        log(line)
        installed = check_install(line, impl)
        phase("install")
        # The VT switch spawns the console only once logind is up and the
        # boot has reached its login prompts; systemd reports both on the
        # console.
        vm.wait_serial("Started User Login Management", timeout=120)
        vm.wait_serial("Reached target Login Prompts", timeout=120)
        phase("boot")
        # tty1 runs getty; switching to tty2 makes logind spawn autovt@tty2,
        # which is kmsconvt@tty2. Nothing starts it earlier: a boot without
        # the keypress has no console on tty2.
        vm.keys("ctrl", "alt", "f2")
        vm.wait_serial("vt: using tty /dev/tty2", timeout=60)
        tmp = os.path.join(work, "wait.ppm")
        prompt = wait_stable(vm, tmp, settle=2.0, timeout=90, want=not_blank)
        phase("console")
        vm.type("tester\n")
        # The user name echoes; then login asks for the password.
        wait_stable(vm, tmp, want=lambda img: img.data != prompt.data)
        vm.type("tester\n")
        try:
            vm.wait_event("T3-LOGIN", timeout=20)
        except RuntimeError:
            # PAM discards typed-ahead input when it turns echo off for the
            # password, so a password typed before the prompt is lost. Typing
            # it again is harmless otherwise: the shell runs it as a command,
            # and every screen clears before it draws.
            log("no login yet; typing the password again")
            vm.type("tester\n")
            vm.wait_event("T3-LOGIN", timeout=60)
        last = wait_stable(vm, tmp)
        phase("login")
        for name in SCREENS:
            vm.type("t3 %s\n" % name)
            vm.wait_event("T3-DONE %s" % name,
                          timeout=BENCH_TIMEOUT if name in BENCHMARKS else SCREEN_TIMEOUT)
            path = os.path.join(work, name + ".ppm")
            # Each screen differs from the one before it, so a capture of the
            # previous screen is never taken for this one.
            last = wait_stable(vm, path, want=lambda img, prev=last: img.data != prev.data)
            caps[name] = path
            phase(name)
            # The keyed cases of this screen, each captured once the screen
            # has changed from the capture before it: a key that changes
            # nothing fails its case. An image screen that found images not
            # offered drew no picture for its keys to act on, so they are
            # not pressed and its keyed cases fail with the refusal; the
            # screens after it still run.
            refused = name in IMAGE_SCREENS and reported_not_offered(read_events(vm), name)
            for screen, case, chords in KEYED:
                if screen != name or case not in variant_cases(variant):
                    continue
                if refused:
                    errors[case] = ("not pressed: %s found images not offered (%s), and the "
                                    "keys act on pictures" % (name, refused))
                    continue
                for chord in chords:
                    vm.keys(*chord)
                path = os.path.join(work, case + ".ppm")
                last = wait_stable(vm, path, want=lambda img, prev=last: img.data != prev.data)
                caps[case] = path
                phase(case)
            for chord in KEYS_AFTER.get(name, []) if v.get("images", True) and not refused else []:
                vm.keys(*chord)
        # VT switch away and back. vt-switch: logind starts a second console
        # on tty3, which must draw a screen (its login prompt). vt-restore:
        # back on tty2, the screen must be exactly as it was left. A switch
        # that does not visibly happen fails vt-switch, so vt-restore cannot
        # pass alone without testing anything.
        vm.keys("ctrl", "alt", "f3")
        path = os.path.join(work, "vt-switch.ppm")
        try:
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
        vm.keys("ctrl", "alt", "f2")
        path = os.path.join(work, "vt-restore.ppm")
        wait_stable(vm, path, want=lambda img: img.data != away.data)
        caps["vt-restore"] = path
        phase("vt-restore")
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
    rss = resident_kb(vm)
    meta = {"installed": installed,
            "displays": displays, "errors": errors, "device": v["device"], "config": conf,
            "cell": console_cell(vm.serial), "cpus": v["cpus"], "memory_mb": v["memory_mb"],
            "bench": bench_figures(vm, caps, conf, v.get("images", True)),
            "rss_kb": rss, "resident": resident(rss),
            "startup_ms": console_startup_ms(vm.serial)}
    meta["console"] = console_revision(vm.serial)
    errors_all, drop = variant_errors(error, displays, v["renderer"], caps, rss,
                                      meta["console"])
    if drop:
        caps = {}
    if errors_all:
        meta["error"] = "; ".join(errors_all)
    return caps, meta


def variant_errors(error, displays, renderer, caps, rss, console):
    """Every reason a variant failed, the scenario's own `error` first, and
    whether its captures must be dropped so every case fails: when a console
    drew with another renderer than the variant tests, the captures show the
    wrong path; when a drawn screen has no resident-memory reading (`rss`),
    the run lacks a figure measured on every run; and when the console
    (`console_revision`) is not vtium, the captures are another console's."""
    out, drop = [error] if error else [], False
    want = "text renderer [%s]" % renderer
    if not displays or not all(want in d for d in displays):
        out.append("the console did not draw with %s: %s" % (renderer, displays))
        drop = True
    unread = [s for s in SCREENS if s in caps and s not in rss]
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
