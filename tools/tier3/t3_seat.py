"""Tier 3's seat and kernel-panic cases.

The seat variants run vtium built with libseat (ADR-1400's L2 layer,
vtium/vtium#34) on the minspec machine, with the seat manager the variant
names: seatd, which guest/setup starts, or logind. guest/setup holds libseat
to that backend with LIBSEAT_BACKEND, so a seat that opens through another
backend fails the variant. The variants log in, draw screens and switch VTs
through the seat as 2d-potato-min does, and are judged by its goldens
(t3_evaluate.GOLDENS_OF). Their check `seat` passes when the console's log
shows that the seat opened, that each keyboard opened through the seat
manager, and that vtium never fell back to the kernel's VT.

The panic variant crashes the guest's kernel with `echo c >
/proc/sysrq-trigger`, through the agent (guest/t3-agent), while vtium shows
its login prompt. Its check `kernel-panic` keeps the screen that the panic leaves
on the display: it is the evidence of what a person sees at a kernel panic
while vtium holds the VT (ADR-1401 D15). The check judges nothing of the
picture, and passes once the console shows the kernel's panic and the
screen has settled after it.

The kernel-message variant makes the guest's kernel log an error while vtium
shows its login prompt, through the agent's request `kmsg`, and captures the
screen once vtium has drawn it (ADR-1401 D15). The case `kernel-message` is
judged by its golden: the message in vtium's notice box on the boot display.
"""

import os

from t3_qemu import not_blank, wait_stable

# The kernel's line at a crash that sysrq asked for (kernel/panic.c, with
# the message of drivers/tty/sysrq.c's sysrq_handle_crash).
PANIC_LINE = "Kernel panic - not syncing: sysrq triggered crash"
# How long the console may take to show the panic after the request.
PANIC_TIMEOUT = 60

CHECKS = {
    "seat": "The console's log, for vtium built with libseat on the seat manager the variant "
            "names. Correct: the seat seat0 opens, every keyboard vtium uses opens through "
            "the seat manager, and vtium never falls back to the kernel's VT.",
    "kernel-panic": "The display after echo c > /proc/sysrq-trigger, while vtium shows its login "
             "prompt on tty2. It judges nothing of the picture: the capture is the evidence "
             "of what a person sees at a kernel panic while vtium holds the VT. Correct: the "
             "console shows the kernel's panic, and the screen settles after it.",
}

# The kernel's line for the agent's request `kmsg`: a mount of tmpfs with an
# option tmpfs does not know, which fs/fs_context.c logs at level err when
# the caller passed no log of its own, as mount(2) does.
KMSG_LINE = "tmpfs: Unknown parameter 'vtium_tier3_warning'"
# How long vtium shows a kernel message (crates/vtium/src/head.rs,
# KERNEL_SHOWN); the capture must settle within it.
KMSG_SHOWN = 15

# The cases judged by a golden, and what each shows.
CAPTIONS = {
    "kernel-message": "The login prompt on tty2 after the kernel logged an error of its own, "
                      "tmpfs: Unknown parameter 'vtium_tier3_warning', at level err. Correct: "
                      "the message, alone, in vtium's notice box at the top of the boot display, "
                      "drawn over the console; the console's screen around the box unchanged.",
}

# vtium's lines (crates/vtium-platform/src/seat.rs), after the log's target.
SEAT_OPEN = "seat: the seat seat0 is open"
SEAT_KERNEL = "seat: no seat manager answered"
INPUT_USED = "input: using /dev/input/"
INPUT_THROUGH = "through the seat manager"


def seat_verdict(serial):
    """Why the seat check fails on the console log `serial` (bytes), or None
    when it passes: the seat opened, vtium never ran on the kernel's VT, and
    each keyboard or pointer vtium used opened through the seat manager
    first."""
    text = serial.decode(errors="replace")
    lines = text.splitlines()
    if any(SEAT_KERNEL in line for line in lines):
        return "vtium ran on the kernel's VT: no seat manager answered"
    if not any(SEAT_OPEN in line for line in lines):
        return "the console never logged %r" % SEAT_OPEN
    used = [line.split(INPUT_USED, 1)[1].split()[0] for line in lines if INPUT_USED in line]
    if not used:
        return "the console used no keyboard"
    through = {line.split("opened ", 1)[1].split()[0] for line in lines
               if "seat: opened /dev/input/" in line and INPUT_THROUGH in line}
    direct = sorted({"/dev/input/" + node for node in used} - through)
    if direct:
        return "these devices did not open through the seat manager: %s" % ", ".join(direct)
    return None


def check_seat(v, vm, work, caps, errors, log):
    """The seat check on `vm`'s console log, its capture the screen once the
    screens are drawn: `caps` gets it when the check passes, and `errors`
    the reason otherwise. It runs after the VT switch, as each check does
    (t3_scenario.CHECK_RUNS)."""
    with open(vm.serial, "rb") as f:
        why = seat_verdict(f.read())
    if why:
        errors["seat"] = why
        return
    path = os.path.join(work, "seat.ppm")
    wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    caps["seat"] = path


def panic_verdict(serial, since):
    """Why the panic check fails on the console log `serial` (bytes) after
    the byte offset `since`, or None when the kernel panicked as asked."""
    if PANIC_LINE not in serial[since:].decode(errors="replace"):
        return "the console never showed %r" % PANIC_LINE
    return None


def crash(vm, work, caps, errors, notes, log):
    """Crash the guest's kernel while vtium shows its VT, and keep the
    screen the panic leaves (CHECKS "kernel-panic"). `notes` says whether that
    screen differs from vtium's last frame."""
    before = os.path.join(work, "panic-before.ppm")
    path = os.path.join(work, "kernel-panic.ppm")
    try:
        shown = wait_stable(vm, before, settle=2.0, timeout=90, want=not_blank)
        with open(vm.serial, "rb") as f:
            since = len(f.read())
        log("crashing the guest's kernel through the agent")
        vm.tell("panic")
        vm.wait_serial(PANIC_LINE, timeout=PANIC_TIMEOUT)
        with open(vm.serial, "rb") as f:
            why = panic_verdict(f.read(), since)
        if why:
            raise RuntimeError(why)
        after = wait_stable(vm, path, settle=2.0, timeout=PANIC_TIMEOUT)
        caps["kernel-panic"] = path
        notes["kernel-panic"] = ("the screen after the panic is vtium's last frame, unchanged"
                          if after.data == shown.data else
                          "the screen after the panic differs from vtium's last frame")
    except (RuntimeError, OSError) as e:
        errors["kernel-panic"] = str(e)
        log("kernel-panic: %s" % e)


def kernel_message(vm, work, caps, errors, log):
    """Make the guest's kernel log an error while vtium shows its VT, and
    capture the screen once it has changed from the login prompt and
    settled, within the time vtium shows the message (CAPTIONS
    "kernel-message")."""
    before = os.path.join(work, "kernel-message-before.ppm")
    path = os.path.join(work, "kernel-message.ppm")
    try:
        shown = wait_stable(vm, before, settle=2.0, timeout=90, want=not_blank)
        answer = vm.ask("kmsg")
        log("kmsg: %s" % "; ".join(answer))
        vm.wait_serial(KMSG_LINE, timeout=30)
        wait_stable(vm, path, settle=1.0, timeout=KMSG_SHOWN - 2,
                    want=lambda img: not_blank(img) and img.data != shown.data)
        caps["kernel-message"] = path
    except (RuntimeError, OSError) as e:
        errors["kernel-message"] = str(e)
        log("kernel-message: %s" % e)
