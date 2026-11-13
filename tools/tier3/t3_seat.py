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

The variant seat-seatd-wait also runs the two waits of a vtium that opens
the seat through seatd. Its check `seat-shown` starts vtium on tty4 while
tty2 is shown, and passes once that vtium opens the seat at the switch to
tty4, which tty0's `active` attribute reports, and draws its login prompt.
Its check `seat-busy` has another seatd client hold the seat on tty8 and
starts vtium there. It passes once vtium logs that the seat is busy, opens
the seat after the client ends, and draws its login prompt.

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
judged by its golden: the message in vtium's notice box on the boot display,
captured at the count's first value (first_count).
"""

import os
import re
import time

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
    "direct": "The console's log, for vtium built with libseat and logind in direct mode, "
              "vtium@.service, under logind with no LIBSEAT_BACKEND and no seatd (ADR-1400 D3). "
              "Correct: vtium logs that it is in no logind session of its own and takes the seat "
              "from seatd alone, and never opens the seat, so libseat's logind backend takes no "
              "session.",
    "seat-cgroup": "The control group, read from /proc/PID/cgroup, of vtium built with libseat "
                   "under logind, whose unit opens a PAM session of vtium's own (PAMName=vtium). "
                   "Correct, as measured on 2026-11-10: pam_systemd moves vtium out of its unit "
                   "into that session's scope, session-N.scope. The note gives the control group "
                   "of the shell logged in on vtium's console, which the check does not judge.",
    "login-session": "A user logged in at vtium's prompt on the libseat build under logind, "
                     "where vtium holds seat0 through its own PAM session and starts each "
                     "login in a systemd scope of its own (ADR-1400 D3). Correct: the shell "
                     "runs in a logind session of the user's own, with the user's uid and no "
                     "seat; its XDG_RUNTIME_DIR is /run/user/UID and it has no XDG_SEAT or "
                     "XDG_VTNR; user@UID.service is active; vtium's own session, the one vtium "
                     "leads, is still on seat0 and active, with the class greeter; and a line typed on the console's "
                     "keyboard after the login reaches the shell.",
    "two-logins": "A second login by the same user, on the head's second console on the same "
                  "VT, reached with key-console-next. Correct: two shells, each in a logind "
                  "session of its own with no seat, both sessions the user's; one "
                  "user@UID.service, active; and vtium's own session still on seat0 and "
                  "active. The capture is the second console's shell; key-console-previous "
                  "then shows the first console again.",
    "boot-claim": "The VTs after vtium console enable --direct --all and a second start of the "
                  "guest's userspace (systemctl soft-reboot), with nobody switching VTs "
                  "(ADR-1500 D7, \"Each claimed VT starts at boot\"). Correct: on each of VTs 1 "
                  "to 6, vtium@ttyN.service is active and a vtium in its control group holds "
                  "/dev/ttyN open, a different vtium on each VT, and the "
                  "first VT the kernel's VT_OPENQRY gives, which a display manager such as "
                  "SDDM takes for a new desktop session, is VT 7. Then the driver switches to "
                  "each of VTs 2 to 6 in turn, and each VT's own vtium, started at boot while "
                  "its VT was not shown, sets up the display and draws a screen that is not "
                  "blank. The log gives each vtium's resident memory.",
    "seat-shown": "vtium built with libseat on seatd, started on tty4 while tty2 is shown, "
                  "then shown with Ctrl+Alt+F4. Correct: the vtium on tty4 logs that it waits "
                  "for its VT to be shown, logs that VT 4 is shown and opens the seat at the "
                  "switch, which tty0's active attribute reports, and draws its login prompt.",
    "seat-busy": "vtium built with libseat on seatd, started on tty8 while another seatd client "
                 "holds the seat there. Correct: the vtium on tty8 logs that the seat is busy, "
                 "opens the seat once that client has ended, and draws its login prompt.",
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
# KERNEL_SHOWN). The box's last line counts the seconds down from it, "Press
# Esc to dismiss this message (15)", so the screen changes every second.
KMSG_SHOWN = 15
# How often the screen is captured while the case waits for the box, and
# how far apart the two captures that must agree are: both well inside the
# first second, so the capture shows the count at 15.
KMSG_POLL = 0.1

# The cases judged by a golden, and what each shows.
CAPTIONS = {
    "kernel-message": "The login prompt on tty2 after the kernel logged an error of its own, "
                      "tmpfs: Unknown parameter 'vtium_tier3_warning', at level err. Correct: "
                      "the message in vtium's notice box at the top of the boot display, with the "
                      "line Press Esc to dismiss this message (15) under it, the count at its "
                      "first value, drawn over the console; the console's screen around the box "
                      "unchanged.",
}

# vtium's lines (crates/vtium-platform/src/seat.rs), after the log's target.
SEAT_OPEN = "seat: the seat seat0 is open"
# seat.rs's SEATD_ONLY, which vtium logs when it pins libseat to seatd.
SEAT_DIRECT = ("seat: vtium is in no logind session of its own, so it takes the seat from "
               "seatd alone, and never from logind")
SEAT_KERNEL = "seat: no seat manager answered"
# seat.rs's lines while vtium waits for its VT to be shown, at the switch
# that shows it, and while another client holds the seat.
SEAT_WAITS = "seat: seatd listens: the seat opens once the VT is shown"
SEAT_SHOWN = "seat: VT %d is shown, so vtium opens the seat now"
SEAT_BUSY = "seat: the seat is busy, waiting for its previous client to let it go"
# The prefix systemd gives each line of vtium's on the console, which names
# its PID.
VTIUM_PID = re.compile(r"vtium\[(\d+)\]: ")
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


def vtium_line(text, message, pid=None):
    """The first line of the console log `text` in which a vtium, or the
    vtium of PID `pid`, logs `message`, as (line, PID), or None."""
    for line in text.splitlines():
        m = VTIUM_PID.search(line)
        if m and message in line[m.end():] and pid in (None, int(m.group(1))):
            return line.strip(), int(m.group(1))
    return None


def open_log(vm):
    """`vm`'s console log so far, as bytes."""
    with open(vm.serial, "rb") as f:
        return f.read()


def log_size(vm):
    """The bytes of `vm`'s console log so far, from which a wait reads."""
    return len(open_log(vm))


def wait_vtium_line(vm, message, since, timeout, pid=None):
    """The first whole line after byte `since` of `vm`'s console log in which
    a vtium, or the vtium of PID `pid`, logs `message`, as (line, PID).
    Raises RuntimeError when none comes within `timeout` seconds."""
    deadline = time.time() + timeout
    while True:
        data = open_log(vm)[since:]
        found = vtium_line(data[:data.rfind(b"\n") + 1].decode(errors="replace"), message, pid)
        if found:
            return found
        if vm.proc.poll() is not None or time.time() > deadline:
            who = "vtium" if pid is None else "vtium %d" % pid
            raise RuntimeError("%s never logged %r within %d s" % (who, message, timeout))
        time.sleep(0.2)


def check_seat_shown(v, vm, work, caps, errors, log):
    """The seat-shown check (CHECKS): vtium started on tty4 while tty2 is
    shown waits for its VT, and opens the seat at the switch to tty4."""
    case = "seat-shown"
    try:
        since = log_size(vm)
        log("%s: %s" % (case, "; ".join(vm.ask("seat-wait"))))
        _, pid = wait_vtium_line(vm, SEAT_WAITS, since, 60)
        log("%s: vtium %d waits for VT 4 to be shown" % (case, pid))
        vm.keys("ctrl", "alt", "f4")
        wait_vtium_line(vm, SEAT_SHOWN % 4, since, 60, pid)
        wait_vtium_line(vm, SEAT_OPEN, since, 60, pid)
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    except RuntimeError as e:
        errors[case] = str(e)
        return
    log("%s: vtium %d opened the seat at the switch to VT 4" % (case, pid))
    caps[case] = path


def check_seat_busy(v, vm, work, caps, errors, log):
    """The seat-busy check (CHECKS): another seatd client holds the seat on
    tty8 while vtium starts there, and vtium opens the seat once that
    client has ended."""
    case = "seat-busy"
    try:
        since = log_size(vm)
        answer = vm.ask("seat-hold", timeout=60)
        log("%s: %s" % (case, "; ".join(answer)))
        if not any(line.startswith("holding the seat") for line in answer):
            raise RuntimeError("no client held the seat: %s" % "; ".join(answer))
        _, pid = wait_vtium_line(vm, SEAT_BUSY, since, 60)
        # vtium tries the seat again for SEAT_OPENS_MAX tries, 5 s, and
        # cannot open it while the client holds it.
        released = log_size(vm)
        if vtium_line(open_log(vm)[since:released].decode(errors="replace"), SEAT_OPEN, pid):
            raise RuntimeError("vtium %d opened the seat while another client held it" % pid)
        log("%s: %s" % (case, "; ".join(vm.ask("seat-release"))))
        wait_vtium_line(vm, SEAT_OPEN, released, 30, pid)
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    except RuntimeError as e:
        errors[case] = str(e)
        return
    log("%s: vtium %d opened the seat once the other client ended" % (case, pid))
    caps[case] = path


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


def direct_verdict(serial):
    """Why the direct check fails on the console log `serial` (bytes), or
    None when it passes: vtium said it is in no logind session of its own
    and pinned libseat to seatd, and never opened the seat."""
    lines = serial.decode(errors="replace").splitlines()
    if any(SEAT_OPEN in line for line in lines):
        return "vtium opened the seat in direct mode"
    if not any(SEAT_DIRECT in line for line in lines):
        return "the console never logged %r" % SEAT_DIRECT
    return None


def check_direct(v, vm, work, caps, errors, log):
    """The direct check on `vm`'s console log, its capture the screen once
    the screens are drawn, as check_seat's."""
    with open(vm.serial, "rb") as f:
        why = direct_verdict(f.read())
    if why:
        errors["direct"] = why
        return
    path = os.path.join(work, "direct.ppm")
    wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    caps["direct"] = path


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
        first_count(vm, path, shown)
        caps["kernel-message"] = path
    except (RuntimeError, OSError) as e:
        errors["kernel-message"] = str(e)
        log("kernel-message: %s" % e)


def first_count(vm, path, shown):
    """Capture the screen at the first count of vtium's notice box: the first
    capture that differs from `shown`, the screen before the message, and
    equals the capture KMSG_POLL seconds after it, so neither falls on the
    count's first tick. The box counts down once a second from KMSG_SHOWN,
    so this capture shows its first value. Raises when no two such captures
    agree within the time vtium shows the message."""
    deadline = time.time() + KMSG_SHOWN - 2
    while time.time() < deadline:
        img = vm.screendump(path)
        if img.data == shown.data or not not_blank(img):
            time.sleep(KMSG_POLL)
            continue
        time.sleep(KMSG_POLL)
        again = vm.screendump(path + ".again")
        os.unlink(path + ".again")
        if again.data == img.data:
            return img
    raise RuntimeError("the notice box with the kernel message never held still for %.1f s"
                       % KMSG_POLL)


def parse_cgroups(answer):
    """The agent's answer to `cgroups` as (vtium's control group or None,
    [each shell's control group])."""
    vtium, shells = None, []
    for line in answer:
        fields = line.split(" ", 2)
        if len(fields) == 3 and fields[0] == "vtium":
            vtium = fields[2]
        elif len(fields) == 3 and fields[0] == "shell":
            shells.append(fields[2])
    return vtium, shells


def cgroup_verdict(vtium, shells):
    """Why the seat-cgroup check fails, or None: vtium in a logind session
    scope, out of its unit, and one shell logged in."""
    scope = (vtium or "").rsplit("/", 1)[-1]
    if not (scope.startswith("session-") and scope.endswith(".scope")):
        return "vtium runs in %s, not in a session scope" % vtium
    if len(shells) != 1:
        return "want one shell logged in, the agent found %d: %s" % (len(shells), shells)
    return None


def check_cgroup(v, vm, work, caps, errors, log):
    """The seat-cgroup check (CHECKS), on the agent's answer `cgroups`, its
    capture the screen once it passed."""
    answer = vm.ask("cgroups")
    log("seat-cgroup: %s" % "; ".join(answer))
    vtium, shells = parse_cgroups(answer)
    why = cgroup_verdict(vtium, shells)
    if why:
        errors["seat-cgroup"] = why
        return
    path = os.path.join(work, "seat-cgroup.ppm")
    wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    caps["seat-cgroup"] = path


def parse_logins(answer):
    """The agent's answer to `logins` as {"vtium": pid or None, "sessions":
    {id: {"User", "Seat", "VTNr", "Class", "Active", "TTY", "Leader"}},
    "shells": [{"pid", "uid", "cgroup", "env"}], "users": {uid: state}}."""
    out = {"vtium": None, "sessions": {}, "shells": [], "users": {}}
    for line in answer:
        word, _, rest = line.partition(" ")
        fields = rest.split()
        if word == "vtium" and fields and fields[0] != "0":
            out["vtium"] = fields[0]
        elif word == "session" and fields:
            # loginctl prints an empty property as "Seat=", which stays
            out["sessions"][fields[0]] = dict(f.split("=", 1) for f in fields[1:] if "=" in f)
        elif word == "shell" and len(fields) >= 3:
            env = dict(f.split("=", 1) for f in fields[3:] if "=" in f)
            out["shells"].append({"pid": fields[0], "uid": fields[1], "cgroup": fields[2],
                                  "env": env})
        elif word == "user" and len(fields) == 2:
            out["users"][fields[0]] = fields[1]
    return out


def session_of(shell):
    """The logind session id of the scope `shell` runs in, or None outside
    every session's scope."""
    scope = shell["cgroup"].rsplit("/", 1)[-1]
    if scope.startswith("session-") and scope.endswith(".scope"):
        return scope[len("session-"):-len(".scope")]
    return None


def login_verdict(logins, shells=1):
    """Why the login-session check, or with `shells` 2 the two-logins check,
    fails on the agent's answer `logins` (parse_logins), or None."""
    if len(logins["shells"]) != shells:
        return "want %d shell(s) logged in, the agent found %d" % (shells, len(logins["shells"]))
    seen = set()
    for shell in logins["shells"]:
        uid = shell["uid"]
        sid = session_of(shell)
        session = logins["sessions"].get(sid or "")
        if session is None or session.get("User") != uid:
            return ("the shell %s of uid %s runs in %s, which is no logind session of its "
                    "user's own" % (shell["pid"], uid, shell["cgroup"]))
        if sid in seen:
            return "two shells share the session %s" % sid
        seen.add(sid)
        if session.get("Seat"):
            return "the user's session %s is on the seat %s, not on none" % (sid, session["Seat"])
        for name in ("XDG_SEAT", "XDG_VTNR"):
            if name in shell["env"]:
                return "the shell %s has %s=%s" % (shell["pid"], name, shell["env"][name])
        if shell["env"].get("XDG_RUNTIME_DIR") != "/run/user/%s" % uid:
            return ("the shell %s's XDG_RUNTIME_DIR is %r"
                    % (shell["pid"], shell["env"].get("XDG_RUNTIME_DIR")))
        if logins["users"].get(uid) != "active":
            return "user@%s.service is %s" % (uid, logins["users"].get(uid))
    own = [(sid, s) for sid, s in logins["sessions"].items()
           if logins["vtium"] is not None and s.get("Leader") == logins["vtium"]]
    if len(own) != 1:
        return "vtium %s leads %d logind sessions, not 1" % (logins["vtium"], len(own))
    sid, session = own[0]
    if session.get("Seat") != "seat0" or session.get("Active") != "yes":
        return "vtium's own session %s lost the seat: %s" % (sid, session)
    # vtium.pam opens a greeter's session, which logind's
    # StopIdleSessionSec= never stops (ADR-1400 D3).
    if session.get("Class") != "greeter":
        return "vtium's own session %s has the class %s, not greeter" % (sid, session.get("Class"))
    return None


def ask_logins(vm, log, case):
    """The agent's answer to `logins`, logged under `case`, parsed."""
    answer = vm.ask("logins")
    for line in answer:
        log("%s: %s" % (case, line))
    return parse_logins(answer)


def check_login_session(v, vm, work, caps, errors, log):
    """The login-session check (CHECKS), on the agent's answer `logins`, then
    a line typed in the shell."""
    case = "login-session"
    why = login_verdict(ask_logins(vm, log, case)) or typed_after_login(vm, log, "typed")
    if why:
        errors[case] = why
        return
    path = os.path.join(work, case + ".ppm")
    wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    caps[case] = path


def typed_after_login(vm, log, what):
    """Why a line typed on the console's keyboard does not reach the shell
    shown, or None: vtium still has the keyboard once the user's session
    exists, so it still holds the seat. The shell writes its id to
    /tmp/t3-answer-`what`, which the agent's `ptys` reads."""
    import t3_heads
    vm.type("echo $$ > /tmp/t3-answer-%s\n" % what)
    deadline = time.time() + 20
    while time.time() < deadline:
        answers = t3_heads.ask_ptys(vm)["answers"]
        if answers.get(what):
            log("%s: the shell %s answered the line typed" % (what, answers[what]))
            return None
        time.sleep(0.5)
    return "a line typed after the login never reached the shell: vtium lost the keyboard"


def logins_settled(logins, shells):
    """Whether the agent's answer `logins` shows `shells` shells, each in a
    session logind lists. A login's shell can appear in its session's scope
    before logind lists that session, and the agent reads the sessions
    before the shells, so one answer can show the shell and not yet its
    session."""
    return (len(logins["shells"]) >= shells
            and all((session_of(s) or "") in logins["sessions"] for s in logins["shells"]))


def check_two_logins(v, vm, work, caps, errors, log):
    """The two-logins check (CHECKS): the user logs in again on the head's
    second console, which key-console-next shows, then the agent's answer
    `logins` is judged and key-console-previous shows the first console."""
    case = "two-logins"
    tmp = os.path.join(work, "wait.ppm")
    before = wait_stable(vm, tmp)
    vm.keys("ctrl", "meta_l", "right")
    prompt = wait_stable(vm, tmp, want=lambda img: img.data != before.data)
    vm.type("tester\n")
    wait_stable(vm, tmp, want=lambda img: img.data != prompt.data)
    vm.type("tester\n")
    logins = None
    deadline = time.time() + 60
    retyped = False
    while time.time() < deadline:
        logins = ask_logins(vm, log, case)
        if logins_settled(logins, 2):
            break
        if not retyped and time.time() > deadline - 40:
            # PAM drops a password typed before it turned echo off, as in
            # the scenario's own login.
            log("%s: no second login yet; typing the password again" % case)
            vm.type("tester\n")
            retyped = True
        time.sleep(1)
    why = login_verdict(logins, shells=2) or typed_after_login(vm, log, "second")
    if not why:
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
        caps[case] = path
    vm.keys("ctrl", "meta_l", "left")
    if why:
        errors[case] = why


# The VTs vtium console enable --direct --all claims on the guest, which has
# no display manager: VTs 1 to 6, the VTs logind manages by default.
CLAIMED_VTS = range(1, 7)
# How long the boot-claim check asks the agent again for vtium to hold the
# claimed VTs: systemd starts vtium's units at boot, and each vtium opens its
# VT just after its start.
BOOT_CLAIM_TIMEOUT = 90


def parse_vts(lines):
    """The agent's answer to `vts` (guest/t3-agent): {VT: [(pid, name, VmRSS
    in kB or None, the unit of its control group or None)]} for each
    process that holds /dev/ttyN open, {VT: {unit: active state}} for
    vtium's two units on each VT, and the VT that `fgconsole
    --next-available` gave, or None."""
    held, units, free = {}, {}, None
    for line in lines:
        words = line.split()
        if words[:1] == ["vt"] and len(words) >= 4:
            rss = int(words[4]) if len(words) > 4 and words[4].isdigit() else None
            unit = words[5] if len(words) > 5 else None
            held.setdefault(int(words[1]), []).append((int(words[2]), words[3], rss, unit))
        elif words[:1] == ["unit"] and len(words) >= 3:
            units.setdefault(int(words[1]), {})[words[2]] = words[3] if len(words) > 3 else ""
        elif words[:1] == ["next-available"]:
            free = int(words[1]) if len(words) > 1 and words[1].isdigit() else None
    return held, units, free


def unit_holders(held, units, n):
    """The vtium processes that hold VT `n` open from the control group of
    vtium's unit on that VT, while that unit is active."""
    own = {"vtium@tty%d.service" % n, "vtium-seat@tty%d.service" % n}
    active = {u for u, state in units.get(n, {}).items() if u in own and state == "active"}
    return [pid for pid, name, _, unit in held.get(n, []) if name == "vtium" and unit in active]


def boot_claim_verdict(held, units, free):
    """Why the boot-claim check fails on the parsed `vts` answer, or None:
    a VT of CLAIMED_VTS that no vtium of that VT's own active unit holds
    open, one vtium holding two of them, or a first free VT other than the
    first after them."""
    missing = [n for n in CLAIMED_VTS if not unit_holders(held, units, n)]
    if missing:
        return "no vtium of its own active unit holds VT %s open: %s" % (
            ", ".join(map(str, missing)),
            "; ".join("VT %d: %s" % (n, ", ".join(
                "%s %d in %s" % (name, pid, unit) for pid, name, _, unit in held.get(n, []))
                or "nothing") for n in missing))
    pids = [unit_holders(held, units, n)[0] for n in CLAIMED_VTS]
    if len(set(pids)) != len(pids):
        return "one vtium holds several VTs open: %s" % " ".join(map(str, pids))
    if free != CLAIMED_VTS[-1] + 1:
        return "the first free VT is %s, not VT %d" % (free, CLAIMED_VTS[-1] + 1)
    return None


# The line a vtium logs once it has set up a display. A vtium started at boot
# on a VT that is not shown drops DRM master after it opens the card, and
# sets up its display only once its VT is shown.
DISPLAY_LINE = "vtium[%d]: Display "


def check_boot_claim(v, vm, work, caps, errors, log):
    """The boot-claim check (CHECKS), on the agent's answer `vts`, asked
    again until vtium holds every claimed VT or BOOT_CLAIM_TIMEOUT passes,
    then on each VT from 2 to 6 once it is shown. It keeps a capture of
    each VT as evidence, which no golden judges."""
    case = "boot-claim"
    deadline = time.time() + BOOT_CLAIM_TIMEOUT
    while True:
        answer = vm.ask("vts")
        held, units, free = parse_vts(answer)
        why = boot_claim_verdict(held, units, free)
        if why is None or time.time() > deadline:
            break
        time.sleep(0.5)
    for line in answer:
        log("%s: %s" % (case, line))
    rss = [r for n in CLAIMED_VTS for _, name, r, _ in held.get(n, []) if name == "vtium" and r]
    if rss:
        log("%s: resident memory of each idle vtium, kB: %s" % (case, " ".join(map(str, rss))))
    if why:
        errors[case] = why
        return
    # Each vtium started at boot draws its own VT once it is shown.
    for n in CLAIMED_VTS[1:]:
        pid = unit_holders(held, units, n)[0]
        vm.keys("ctrl", "alt", "f%d" % n)
        try:
            vm.wait_serial(DISPLAY_LINE % pid, timeout=60)
            wait_stable(vm, os.path.join(work, "%s-vt%d.ppm" % (case, n)), settle=2.0,
                        timeout=60, want=not_blank)
        except RuntimeError as e:
            errors[case] = "VT %d: %s" % (n, e)
            return
        log("%s: VT %d shown, drawn by vtium %d" % (case, n, pid))
    caps[case] = None
