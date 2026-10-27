"""Tier 3's power cases: the power keys pressed on vtium's VT, and the guest
seen to reboot, power off, or suspend and resume.

Each case is a check without a golden. Its capture is the screen vtium
drew before the key (after the resume, for suspend), and it passes when
the console log shows vtium asked for the action and the guest carried it
out the way the action promises. QEMU runs with -no-reboot, so a reboot
ends the VM as a power-off does; the kernel's last line tells them apart.
"""

import re

# The chords guest/kmscon-power.conf binds, as QEMU qcodes.
CHORDS = {
    "reboot": ("ctrl", "alt", "delete"),
    "reboot-hard": ("ctrl", "alt", "delete"),
    "power-off": ("ctrl", "alt", "end"),
    "suspend": ("ctrl", "alt", "home"),
}

# What each case shows and what correct looks like, for the report.
CHECKS = {
    "reboot": "grab-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 0. Correct: "
              "vtium asks init, systemd reaches its reboot target, stopping the services and "
              "unmounting the filesystems, and the kernel restarts.",
    "reboot-hard": "grab-reboot (Ctrl+Alt+Delete) with /proc/sys/kernel/ctrl-alt-del at 1. "
                   "Correct: vtium syncs and restarts the kernel at once, with no shutdown by "
                   "systemd.",
    "power-off": "grab-power-off (Ctrl+Alt+End). Correct: vtium asks init with SIGRTMIN+4, "
                 "systemd reaches its power-off target, stopping the services and unmounting "
                 "the filesystems, and the kernel powers down.",
    "suspend": "grab-suspend (Ctrl+Alt+Home), then QEMU's wake-up. Correct: vtium asks logind, "
               "the guest suspends to RAM, and after the wake-up vtium draws its screen again.",
}

# Lines each case requires on the console, in order, and lines it forbids.
# vtium's own line shows the key reached it; systemd's targets show the
# services stopped and the filesystems unmounted; the kernel's last line
# shows how the machine ended.
EXPECT = {
    "reboot": {
        "require": ["grab-reboot: asking init to reboot",
                    "Reached target Unmount All Filesystems",
                    "Reached target System Reboot",
                    "reboot: Restarting system"],
        "forbid": ["reboot: Power down"],
    },
    "reboot-hard": {
        "require": ["grab-reboot: restarting the kernel at once",
                    "reboot: Restarting system"],
        "forbid": ["Reached target System Reboot", "Reached target Unmount All Filesystems",
                   "reboot: Power down"],
    },
    "power-off": {
        "require": ["grab-power-off: asking init to power off",
                    "Reached target Unmount All Filesystems",
                    "Reached target System Power Off",
                    "reboot: Power down"],
        "forbid": ["reboot: Restarting system"],
    },
    "suspend": {
        "require": ["grab-suspend: asking logind to suspend",
                    "Reached target Sleep",
                    "Finished System Suspend"],
        "forbid": ["reboot: "],
    },
}

# CSI and OSC sequences, which systemd's coloured status lines carry.
_ESCAPES = re.compile(r"\x1b\[[0-9;?!]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def judge(case, serial, since=0):
    """Why `case` failed, from the console log `serial` (bytes) after offset
    `since`, where the key was pressed; None when it passed. Each required
    line must appear after the one before it, and no forbidden line may
    appear."""
    text = _ESCAPES.sub("", serial[since:].decode(errors="replace"))
    lines = text.splitlines()
    want = EXPECT[case]
    at = 0
    for marker in want["require"]:
        found = next((i for i in range(at, len(lines)) if marker in lines[i]), None)
        if found is None:
            return "no %r on the console after the key%s" % (
                marker, "" if at == 0 else ", after %r" % lines[at - 1].strip())
        at = found + 1
    for marker in want["forbid"]:
        if any(marker in line for line in lines):
            return "%r on the console after the key" % marker
    return None
