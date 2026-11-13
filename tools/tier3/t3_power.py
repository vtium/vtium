"""Tier 3's power cases: the power keys pressed on vtium's VT, and the guest
seen to reboot, power off, or suspend and resume.

Each case is a check without a golden. Its capture is the screen vtium
drew before the key (after the resume, for suspend), and it passes when the console log shows
vtium asked for the action and the guest carried it out the way the action
promises, with no kernel panic. QEMU runs with -no-reboot, so a reboot ends
the VM as a power-off does: QEMU's SHUTDOWN event tells them apart,
"guest-reset" from "guest-shutdown". On the busybox boot, whose PID 1 does
not take the power-off signal, the case passes when vtium warned of that at
its start and the guest stays up after the key (STAYS_UP).
"""

import re

# The chords guest/vtium-power.conf binds, as QEMU qcodes.
CHORDS = {
    "reboot": ("ctrl", "alt", "delete"),
    "reboot-hard": ("ctrl", "alt", "delete"),
    "power-off": ("ctrl", "alt", "end"),
    "power-off-after-s3": ("ctrl", "alt", "end"),
    "suspend": ("ctrl", "alt", "home"),
    "suspend-sysfs": ("ctrl", "alt", "home"),
    "reboot-sysvinit": ("ctrl", "alt", "delete"),
    "power-off-sysvinit": ("ctrl", "alt", "end"),
    "power-off-busybox": ("ctrl", "alt", "end"),
}
# The cases that suspend: QMP sees the guest suspended and wakes it.
SUSPENDS = ("suspend", "suspend-sysfs")

# What each case shows and what correct looks like, for the report.
CHECKS = {
    "reboot": "key-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 0. Correct: "
              "vtium asks init, systemd reaches its reboot target, stopping the services and "
              "unmounting the filesystems, and the kernel restarts.",
    "reboot-hard": "key-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 1. "
                   "Correct: vtium syncs and restarts the kernel at once, with no shutdown by "
                   "systemd.",
    "power-off": "key-power-off (Ctrl+Alt+End) on a guest that has not slept. Correct: vtium "
                 "gives no warning at its start, asks init with SIGRTMIN+4, systemd reaches its "
                 "power-off target, stopping the services and unmounting the filesystems, and "
                 "the machine powers off.",
    "power-off-after-s3": "key-power-off (Ctrl+Alt+End) after a suspend and resume. vtium's "
                          "part is power-off's; the known failure (KNOWN) is the guest's ACPI "
                          "power-off after S3, which does not take, so the kernel panics.",
    "suspend": "key-suspend (Ctrl+Alt+Home) in vtium built with logind, then QEMU's wake-up. "
               "Correct: logind takes the request and systemd suspends the guest to RAM through "
               "its sleep target; after the wake-up vtium draws its screen again.",
    "suspend-sysfs": "key-suspend (Ctrl+Alt+Home) in the minspec build, without logind, then "
                     "QEMU's wake-up. Correct: vtium writes mem to /sys/power/state, the guest "
                     "suspends to RAM with no sleep target of systemd's, and after the wake-up "
                     "vtium draws its screen again.",
    "power-off-sysvinit": "key-power-off (Ctrl+Alt+End) on the guest without systemd, whose "
                          "PID 1, sysvinit, catches SIGRTMIN+4 with a handler, where systemd "
                          "blocks it and reads it from a signalfd. Correct: vtium "
                          "gives no warning at its start, sends the signal, sysvinit switches to "
                          "runlevel 0, stopping the services and unmounting the filesystems, and "
                          "the machine powers off.",
    "reboot-sysvinit": "key-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 0 "
                       "on the guest without systemd. Correct: vtium sends PID 1 SIGINT, "
                       "sysvinit runs its inittab's ctrlaltdel entry, shutdown -r, which "
                       "switches to runlevel 6, stopping the services and unmounting the "
                       "filesystems, and the kernel restarts.",
    "power-off-busybox": "key-power-off (Ctrl+Alt+End) on the guest without systemd booted "
                         "with busybox init, which neither catches nor blocks SIGRTMIN+4. "
                         "Correct: vtium warns at its start that the key will not work, sends "
                         "the signal, the kernel drops it, and the guest stays up: a login "
                         "on vtium's console after the key works.",
}

# The words that open each of vtium's warnings at its start that a bound
# key-power-off may power nothing off: PID 1 neither catches SIGRTMIN+4 nor
# blocks it ("will not work"), or /proc/1/status does not say ("may not
# work"). vtium's check-config notices (crates/vtium/src/notice.rs) word them.
POWER_OFF_WARNING = "the key-power-off key"
# The warning that PID 1 neither catches nor blocks the signal, which names
# the signal's number as the guest's libc numbers SIGRTMIN+4.
WILL_NOT_WORK = POWER_OFF_WARNING + " will not work"
_WILL_NOT_WORK = re.compile(re.escape(WILL_NOT_WORK) + r": .*\(signal (\d+),")
# The kernel's trace of a signal sent to PID 1, tracefs's signal_generate
# event, which guest-sysvinit/setup reads onto the console for a disc with
# trace-pid1-signals. Its res field is the kernel's verdict on the signal:
# 0 delivered or queued, 1 dropped (TRACE_SIGNAL_IGNORED in the kernel's
# include/trace/events/signal.h). The kernel drops a signal that PID 1
# neither catches nor blocks.
SIGNAL_TRACE = "signal_generate: "
_SIGNAL_TRACE = re.compile(r"signal_generate: sig=(\d+) errno=-?\d+ code=-?\d+ comm=.* "
                           r"pid=(\d+) grp=\d+ res=(\d+)")

POWER_OFF = {
    "before": {"forbid": [POWER_OFF_WARNING]},
    "require": ["key-power-off: asking init to power off",
                "Reached target Unmount All Filesystems",
                "Reached target System Power Off",
                "reboot: Power down"],
    "forbid": ["reboot: Restarting system"],
    "shutdown": "guest-shutdown",
}

# Lines each case requires on the console after its key, in order, lines it
# requires in any order, lines it forbids, lines it requires or forbids
# before the key ("before"), whether the kernel must drop the signal the
# warning before the key names ("dropped"), and the reason of QEMU's
# SHUTDOWN event it requires (None: the guest runs on). vtium's own line shows the key
# reached it; systemd's targets, or sysvinit's runlevel and its scripts'
# lines, show the services stopped and the filesystems unmounted; the
# kernel's last line and QEMU's event show how the machine ended. A kernel
# panic fails every case.
EXPECT = {
    "reboot": {
        "require": ["key-reboot: asking init to reboot",
                    "Reached target Unmount All Filesystems",
                    "Reached target System Reboot",
                    "reboot: Restarting system"],
        "forbid": ["reboot: Power down"],
        "shutdown": "guest-reset",
    },
    "reboot-hard": {
        "require": ["key-reboot: restarting the kernel at once",
                    "reboot: Restarting system"],
        "forbid": ["Reached target System Reboot", "Reached target Unmount All Filesystems",
                   "Switching to runlevel", "reboot: Power down"],
        "shutdown": "guest-reset",
    },
    "reboot-sysvinit": {
        "require": ["key-reboot: asking init to reboot",
                    "INIT: Switching to runlevel: 6",
                    "Will now restart.",
                    "reboot: Restarting system"],
        "forbid": ["reboot: Power down", "Reached target"],
        "shutdown": "guest-reset",
    },
    "power-off-sysvinit": {
        "before": {"forbid": [POWER_OFF_WARNING]},
        "require": ["key-power-off: asking init to power off",
                    "INIT: Switching to runlevel: 0",
                    "Will now halt.",
                    "reboot: Power down"],
        "forbid": ["reboot: Restarting system", "Reached target"],
        "shutdown": "guest-shutdown",
    },
    "power-off-busybox": {
        "before": {"require": [WILL_NOT_WORK]},
        "require": ["key-power-off: asking init to power off"],
        "dropped": True,
        "forbid": ["The system is going down", "Requesting system", "reboot: Power down",
                   "reboot: Restarting system"],
        "shutdown": None,
    },
    "power-off": POWER_OFF,
    "power-off-after-s3": POWER_OFF,
    "suspend": {
        "require": ["key-suspend: suspending",
                    "Reached target Sleep",
                    "Finished System Suspend"],
        # logind answers while systemd starts its sleep target, so the
        # console takes the two lines in either order.
        "anywhere": ["key-suspend: logind took the request"],
        "forbid": ["reboot: ", "writing mem to /sys/power/state"],
        "shutdown": None,
    },
    "suspend-sysfs": {
        "require": ["key-suspend: suspending",
                    "key-suspend: writing mem to /sys/power/state"],
        "forbid": ["reboot: ", "Reached target Sleep"],
        "shutdown": None,
    },
}

PANIC = "Kernel panic"

# The cases after whose key the guest stays up, and does not suspend: for
# each, the lines the driver waits for after the key before it logs in on
# vtium's console, vtium's line that it sent the signal and the kernel's
# trace of the signal. A login that works shows the guest stayed up.
STAYS_UP = {"power-off-busybox": ["key-power-off: asking init to power off", SIGNAL_TRACE]}

# Cases that fail today for a reason outside vtium, each with the lines
# that show it and why. Such a case passes with a note while it fails that
# way, and fails once it no longer does, so its entry goes with the fix.
KNOWN = {
    "power-off-after-s3": {
        "shows": ["key-power-off: asking init to power off",
                  "Reached target System Power Off",
                  "reboot: Power down",
                  "Kernel panic - not syncing: Attempted to kill init!"],
        "why": "after an S3 resume, through logind or /sys/power/state alike, the guest "
               "kernel logs entering S5 and 'reboot: Power down', and QEMU's q35 machine does "
               "not power off: 10 s later reboot(2) returns to systemd-shutdown, which exits, "
               "and the kernel panics as PID 1 is gone. systemd stopped the services and "
               "unmounted the filesystems first. vtium's part, SIGRTMIN+4 to PID 1, is "
               "power-off's, which passes on a guest that has not slept.",
    },
}

# CSI and OSC sequences, which systemd's colored status lines carry.
_ESCAPES = re.compile(r"\x1b\[[0-9;?!]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def _lines(serial, since):
    return _ESCAPES.sub("", serial[since:].decode(errors="replace")).splitlines()


def judge(case, serial, since=0, shutdown=None):
    """Why `case` failed, from the console log `serial` (bytes) after offset
    `since`, where the key was pressed, and the reason of QEMU's SHUTDOWN
    event, `shutdown`; None when it passed. Each line required before the
    key must appear before `since`, and none it forbids there; after it,
    each required line must appear after the one before it, each "anywhere"
    line somewhere, no forbidden line and no kernel panic may appear, and
    a "dropped" case needs the kernel's trace, after the key, of the signal
    vtium's warning before the key names, sent to PID 1 and dropped, and
    QEMU's event must be the one the case requires."""
    lines = _lines(serial, since)
    want = EXPECT[case]
    before = _ESCAPES.sub("", serial[:since].decode(errors="replace")).splitlines()
    for marker in want.get("before", {}).get("require", []):
        if not any(marker in line for line in before):
            return "no %r on the console before the key" % marker
    for marker in want.get("before", {}).get("forbid", []):
        if any(marker in line for line in before):
            return "%r on the console before the key" % marker
    at = 0
    for marker in want["require"]:
        found = next((i for i in range(at, len(lines)) if marker in lines[i]), None)
        if found is None:
            return "no %r on the console after the key%s" % (
                marker, "" if at == 0 else ", after %r" % lines[at - 1].strip())
        at = found + 1
    for marker in want.get("anywhere", []):
        if not any(marker in line for line in lines):
            return "no %r on the console after the key" % marker
    for marker in want["forbid"] + [PANIC]:
        if any(marker in line for line in lines):
            return "%r on the console after the key" % marker
    if want.get("dropped"):
        why = _dropped(before, lines)
        if why:
            return why
    if shutdown != want["shutdown"]:
        return "QEMU's SHUTDOWN reason is %r, not %r" % (shutdown, want["shutdown"])
    return None


def _dropped(before, lines):
    """Why the kernel's trace in `lines` does not show the signal that
    vtium's warning in `before` names sent to PID 1 and dropped; None when
    it does."""
    warned = next((m for m in map(_WILL_NOT_WORK.search, before) if m), None)
    if warned is None:
        return "vtium's warning before the key names no signal"
    signal = warned.group(1)
    traces = [m for m in map(_SIGNAL_TRACE.search, lines)
              if m and m.group(1) == signal and m.group(2) == "1"]
    if not traces:
        return "no trace of signal %s sent to PID 1 after the key" % signal
    delivered = next((m for m in traces if m.group(3) != "1"), None)
    if delivered:
        return ("the kernel delivered signal %s to PID 1 (res=%s), so PID 1 caught or "
                "blocked it" % (signal, delivered.group(3)))
    return None


def verdict(case, serial, since=0, shutdown=None):
    """(why the case failed or None, a note or None): judge's verdict, but a
    KNOWN case passes with a note while its known failure shows, and fails
    once it passes or fails some other way."""
    why = judge(case, serial, since, shutdown)
    known = KNOWN.get(case)
    if known is None:
        return why, None
    if why is None:
        return ("%s no longer fails as KNOWN says; remove its entry" % case), None
    lines = _lines(serial, since)
    missing = [m for m in known["shows"] if not any(m in line for line in lines)]
    if missing:
        return "%s (and not as KNOWN says: no %r)" % (why, missing[0]), None
    return None, "known: " + known["why"]
