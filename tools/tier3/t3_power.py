"""Tier 3's power cases: the power keys pressed on vtium's VT, and the guest
seen to reboot, power off, or suspend and resume.

Each case is a check without a golden. Its capture is the screen vtium
drew before the key (after the resume, for suspend), and it passes when the console log shows
vtium asked for the action and the guest carried it out the way the action
promises, with no kernel panic. QEMU runs with -no-reboot, so a reboot ends
the VM as a power-off does: QEMU's SHUTDOWN event tells them apart,
"guest-reset" from "guest-shutdown".
"""

import re

# The chords guest/kmscon-power.conf binds, as QEMU qcodes.
CHORDS = {
    "reboot": ("ctrl", "alt", "delete"),
    "reboot-hard": ("ctrl", "alt", "delete"),
    "power-off": ("ctrl", "alt", "end"),
    "power-off-after-s3": ("ctrl", "alt", "end"),
    "suspend": ("ctrl", "alt", "home"),
    "suspend-sysfs": ("ctrl", "alt", "home"),
    "reboot-sysvinit": ("ctrl", "alt", "delete"),
    "power-off-sysvinit": ("ctrl", "alt", "end"),
}
# The cases that suspend: QMP sees the guest suspended and wakes it.
SUSPENDS = ("suspend", "suspend-sysfs")

# What each case shows and what correct looks like, for the report.
CHECKS = {
    "reboot": "grab-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 0. Correct: "
              "vtium asks init, systemd reaches its reboot target, stopping the services and "
              "unmounting the filesystems, and the kernel restarts.",
    "reboot-hard": "grab-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 1. "
                   "Correct: vtium syncs and restarts the kernel at once, with no shutdown by "
                   "systemd.",
    "power-off": "grab-power-off (Ctrl+Alt+End) on a guest that has not slept. Correct: vtium "
                 "gives no warning at its start, asks init with SIGRTMIN+4, systemd reaches its "
                 "power-off target, stopping the services and unmounting the filesystems, and "
                 "the machine powers off.",
    "power-off-after-s3": "grab-power-off (Ctrl+Alt+End) after a suspend and resume. vtium's "
                          "part is power-off's; the known failure (KNOWN) is the guest's ACPI "
                          "power-off after S3, which does not take, so the kernel panics.",
    "suspend": "grab-suspend (Ctrl+Alt+Home) in vtium built with logind, then QEMU's wake-up. "
               "Correct: logind takes the request and systemd suspends the guest to RAM through "
               "its sleep target; after the wake-up vtium draws its screen again.",
    "suspend-sysfs": "grab-suspend (Ctrl+Alt+Home) in the minspec build, without logind, then "
                     "QEMU's wake-up. Correct: vtium writes mem to /sys/power/state, the guest "
                     "suspends to RAM with no sleep target of systemd's, and after the wake-up "
                     "vtium draws its screen again.",
    "power-off-sysvinit": "grab-power-off (Ctrl+Alt+End) on the guest without systemd, whose "
                          "PID 1, sysvinit, catches SIGRTMIN+4 as systemd does. Correct: vtium "
                          "gives no warning at its start, sends the signal, sysvinit switches to "
                          "runlevel 0, stopping the services and unmounting the filesystems, and "
                          "the machine powers off.",
    "reboot-sysvinit": "grab-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 0 "
                       "on the guest without systemd. Correct: vtium sends PID 1 SIGINT, "
                       "sysvinit runs its inittab's ctrlaltdel entry, shutdown -r, which "
                       "switches to runlevel 6, stopping the services and unmounting the "
                       "filesystems, and the kernel restarts.",
}

# The start of each of vtium's warnings at its start that a bound
# grab-power-off may power nothing off: PID 1 does not catch SIGRTMIN+4, or
# /proc/1/status does not say.
POWER_OFF_WARNING = "grab-power-off is bound"

POWER_OFF = {
    "before": {"forbid": [POWER_OFF_WARNING]},
    "require": ["grab-power-off: asking init to power off",
                "Reached target Unmount All Filesystems",
                "Reached target System Power Off",
                "reboot: Power down"],
    "forbid": ["reboot: Restarting system"],
    "shutdown": "guest-shutdown",
}

# Lines each case requires on the console after its key, in order, lines it
# requires in any order, lines it forbids, lines it requires or forbids
# before the key ("before"), and the reason of QEMU's SHUTDOWN event it
# requires (None: the guest runs on). vtium's own line shows the key
# reached it; systemd's targets, or sysvinit's runlevel and its scripts'
# lines, show the services stopped and the filesystems unmounted; the
# kernel's last line and QEMU's event show how the machine ended. A kernel
# panic fails every case.
EXPECT = {
    "reboot": {
        "require": ["grab-reboot: asking init to reboot",
                    "Reached target Unmount All Filesystems",
                    "Reached target System Reboot",
                    "reboot: Restarting system"],
        "forbid": ["reboot: Power down"],
        "shutdown": "guest-reset",
    },
    "reboot-hard": {
        "require": ["grab-reboot: restarting the kernel at once",
                    "reboot: Restarting system"],
        "forbid": ["Reached target System Reboot", "Reached target Unmount All Filesystems",
                   "Switching to runlevel", "reboot: Power down"],
        "shutdown": "guest-reset",
    },
    "reboot-sysvinit": {
        "require": ["grab-reboot: asking init to reboot",
                    "INIT: Switching to runlevel: 6",
                    "Will now restart.",
                    "reboot: Restarting system"],
        "forbid": ["reboot: Power down", "Reached target"],
        "shutdown": "guest-reset",
    },
    "power-off-sysvinit": {
        "before": {"forbid": [POWER_OFF_WARNING]},
        "require": ["grab-power-off: asking init to power off",
                    "INIT: Switching to runlevel: 0",
                    "Will now halt.",
                    "reboot: Power down"],
        "forbid": ["reboot: Restarting system", "Reached target"],
        "shutdown": "guest-shutdown",
    },
    "power-off": POWER_OFF,
    "power-off-after-s3": POWER_OFF,
    "suspend": {
        "require": ["grab-suspend: suspending",
                    "Reached target Sleep",
                    "Finished System Suspend"],
        # logind answers while systemd starts its sleep target, so the
        # console takes the two lines in either order.
        "anywhere": ["grab-suspend: logind took the request"],
        "forbid": ["reboot: ", "writing mem to /sys/power/state"],
        "shutdown": None,
    },
    "suspend-sysfs": {
        "require": ["grab-suspend: suspending",
                    "grab-suspend: writing mem to /sys/power/state"],
        "forbid": ["reboot: ", "Reached target Sleep"],
        "shutdown": None,
    },
}

PANIC = "Kernel panic"

# Cases that fail today for a reason outside vtium, each with the lines
# that show it and why. Such a case passes with a note while it fails that
# way, and fails once it no longer does, so its entry goes with the fix.
KNOWN = {
    "power-off-after-s3": {
        "shows": ["grab-power-off: asking init to power off",
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
    if shutdown != want["shutdown"]:
        return "QEMU's SHUTDOWN reason is %r, not %r" % (shutdown, want["shutdown"])
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
