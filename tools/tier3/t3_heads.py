"""Tier 3's head-file variants (ADR-1401): a head file installed at
/etc/vtium/heads.conf in the guest, root's, and what the guest's real KMS
device and systemd make of it, which unit tests cannot show.

A console that gives no login leaves the driver no shell in the guest, so
these variants run the agent (guest/t3-agent), a root program on the third
serial port that answers a few fixed requests on that same port, which only
root can open: the state of VT 2's units and the jobs systemd has queued, the closed policy's getty marker and the programs on tty2,
the key presses of the kiosk's second keyboard, read from its own evdev
node, and the failures it can cause vtium, a kill, a crash and a hangup. Each
check below waits on that state and sleeps no fixed time; the one that must
see vtium outlast display-wait waits for vtium's age, read in the guest.

The getty fallback (crates/vtium/install/kmsconvt-getty@.service) is the
part no unit test reaches: when vtium's unit on tty2 fails, its OnFailure= starts the fallback, which starts getty@tty2 unless
vtium's marker of a closed policy, /run/vtium/tty2.closed, exists. A check
knows the fallback ran by its condition's timestamp, which systemd sets
each time it checks the unit's condition, so the getty's absence under
closed is read from the condition systemd checked, never from a wait that
saw nothing.
"""

import os
import re
import time

import t3_devices
import t3img
from t3_common import GUEST, ROOT, rel
from t3_qemu import char_qcodes, wait_stable

# The guest's head files, beside the screens; a variant names one of these,
# or a shipped one under docs/heads.
HEADS = os.path.join(GUEST, "heads")

# The units of VT 2, as the agent reports them: vtium's, its fallback's and
# the getty the fallback starts.
VTIUM_UNIT = "kmsconvt@tty2.service"
FALLBACK_UNIT = "kmsconvt-getty@tty2.service"
GETTY_UNIT = "getty@tty2.service"
# The gettys the guest could start on tty2, whose last exit from their
# inactive state and restarts the agent reports: under closed each keeps
# the 0 and 0 of a unit that never started since boot. autovt@tty2 is not
# one, as in this guest it is vtium's unit.
GETTY_UNITS = (GETTY_UNIT, "serial-getty@tty2.service")

# The failures the getty-fallback check causes, each the agent's request and
# the Result= systemd gives vtium's unit for it: a kill, SIGKILL; a crash,
# SIGABRT, which a panic of vtium's minspec build also ends in, dumped by
# systemd-coredump where it runs; and a hangup of tty2, after which vtium
# exits 1 (crates/vtium/src/app.rs, Exit::Hangup).
FAILURES = {"kill": ("kill KILL", ("signal",)),
            "crash": ("kill ABRT", ("core-dump", "signal")),
            "hangup": ("hangup", ("exit-code",))}

# vtium's log line when no display came within display-wait under open
# (crates/vtium/src/app.rs).
NO_DISPLAY_LINE = re.compile(r"no display appeared within (\d+) s")
# The cell of each head's display in two-heads.conf, by connector: the cell
# each head's frit states, desk's 8x16 on the first output and panel's 8x8
# on the second.
TWO_HEADS = {"Virtual-1": "8x16", "Virtual-2": "8x8"}

# The second keyboard the kiosk check plugs into the hub's port 3.3, by the
# name the guest's udev gives it, and the line it types, each of whose keys
# its own evdev node must count as a press.
KIOSK_SECOND = t3_devices.HOT_NAME
KIOSK_LINE = "t3 cjk\n"

# How far past display-wait, in seconds, the held check reads vtium's state:
# the wait's FLIP_WAIT (crates/vtium/src/wait.rs) and as long again for the
# exit vtium would make under open.
HOLD_PAST_S = 2

# Seconds a check waits for a state it asks the agent for.
STATE_TIMEOUT = 60

# Where guest/setup installs the variant's head file, as the console's log
# names it.
GUEST_HEAD_FILE = "/etc/vtium/heads.conf"
# The console lines of the outcomes that run some of the file, partly broken
# under either policy, or the shipped default in its place (ADR-1401 D7). A
# file whose policy line vtium cannot read never shows either, as it is
# closed and broken and runs nothing.
RUN_LINES = ("config partly invalid, running what's valid",
              "invalid config skipped, using builtin")

CHECKS = {
    "no-login": "A line typed on the console's keyboard, which reaches the guest (the PS/2 "
                "keyboard's interrupt count rises). Correct: the screen as it was, no login "
                "on any keyboard, vtium still running on tty2 and the closed policy's "
                "marker /run/vtium/tty2.closed written.",
    "getty-fallback": "vtium on tty2 failed by each of the variant's failures (a kill, a "
                      "crash, a hangup), with vtium started again after each, and the "
                      "unit's getty fallback read once systemd has checked its condition. "
                      "Correct under closed: vtium's unit failed, the fallback skipped on "
                      "its condition, the marker still there, and no getty started on tty2 "
                      "since boot and no login session on it, read once the fallback ran, "
                      "again before vtium starts again and once more at the end. Correct "
                      "under open: the fallback ran, getty@tty2 runs a getty on tty2 and no "
                      "marker exists. The capture is tty2 after the last failure.",
    "display-held": "No display device under a closed policy: vtium on tty2 read past "
                    "display-wait. Correct: the same vtium still running, no line that no "
                    "display appeared, the marker written and no getty on tty2 (ADR-1401 "
                    "D8).",
    "two-heads": "A valid open head file with two heads, one on each output of the card, "
                 "each with its own profile's font: the second output, the panel, read "
                 "after the first head, desk, took the login and drew its screens. "
                 "Correct: desk's display in 8x16 cells and panel's in 8x8; panel's "
                 "screen, its own login prompt, as it was before the login, and not "
                 "desk's.",
    "kiosk": "A closed kiosk whose one keyboard, in the hub's port 3.2, claims its head, "
             "logged in and drawing cjk through it. Its Ctrl+Alt+F3, read once the USB "
             "controller's interrupt count has risen, then the sgr screen drawn through it; "
             "then a line typed on a second USB keyboard plugged into port 3.3 through QMP, "
             "read once its own evdev node counted each of its keys. Correct: the VT shown "
             "stays 2 after the chord and the claimed keyboard still reaches the session; "
             "the guest lists the second keyboard, and the screen stays as it was while it "
             "types.",
    "policy-line": "A head file whose policy line holds a value vtium does not know, "
                   "`policy = opne`, read from the console's log once its panic screen "
                   "settled. Correct: the console logged that line's error with the head "
                   "file's path and the line's number, and logged no outcome that runs some "
                   "of the file or the shipped default (ADR-1401 D7). The capture is the screen "
                   "when the log was read.",
    "display-exit": "No display device under an open policy, the control of display-held. "
                    "Correct: vtium runs when the check starts, then logs that no display "
                    "appeared within display-wait and fails, and the fallback starts a getty "
                    "on tty2.",
}

# The console line of the outcome each first screen shows (ADR-1401 D7,
# D9), which vtium also logs: a first screen is captured only once the
# console has logged it.
LINES = {"panic": "vtium: config invalid, policy closed: input off",
         "policy-panic": "vtium: config invalid, policy closed: input off",
         "partly-notice": "config partly invalid, running what's valid",
         "no-display-notice": "vtium: no display for any head"}

# The cases judged by a golden that no screen script draws: the console's
# first screen, before any key.
FIRST = {
    "panic": "The broken closed head file's panic screen on the boot display: `vtium: "
             "config invalid, policy closed: input off`, then that its errors leave no "
             "head that can run, each error with its line of /etc/vtium/heads.conf, and "
             "how to read the full report (ADR-1401 D7). Correct: the report alone, with "
             "no login prompt and no shell.",
    "policy-panic": "The panic screen on the boot display of a head file whose policy line "
                    "vtium cannot read, `policy = opne`, though the rest of the file would "
                    "run a head with every keyboard roaming (ADR-1401 D3, D7). Correct: "
                    "`vtium: config invalid, policy closed: input off`, then that no head "
                    "can run, then the policy line's error with its line of "
                    "/etc/vtium/heads.conf, listed first, before the error of the line "
                    "above it, and how to read the full report; no login prompt and no "
                    "shell.",
    "partly-notice": "The partly broken open head file's notice at the top of the head's "
                     "screen: `config partly invalid, running what's valid` and what to "
                     "do next (ADR-1401 D7). It covers the rows of the login prompt, "
                     "which takes keys as before. Correct: the notice over a screen "
                     "otherwise blank.",
    "no-display-notice": "A closed head file whose one head matches no display: the "
                         "display guard lights the guest's display for the notice "
                         "`vtium: no display for any head` alone, with what to do next "
                         "(ADR-1401 D9). Correct: the notice, with no login prompt.",
}


def parse_state(lines):
    """The agent's answer to `state` (guest/t3-agent) as {"units": {unit:
    {"active", "result", "condition", "checked"}}, "gettys": {unit: (left
    inactive at, restarts)}, "queued", "timers": {unit: state}, "sessions":
    {id: tty}, "marker", "tty2", "logins", "fgconsole", "i8042", "xhci",
    "kbds", "keys", "vtium": (pid, age) or None}; a field the answer lacks
    is None, but "kbds", the keyboards the guest lists, which is empty, and
    the readings by unit or session, which are empty."""
    out = {"units": {}, "gettys": {}, "timers": {}, "sessions": {}, "marker": None,
           "tty2": None, "logins": None, "fgconsole": None, "i8042": None, "xhci": None,
           "kbds": [], "keys": None, "queued": [], "vtium": None}
    for line in lines:
        f = line.split()
        if not f:
            continue
        if f[0] == "unit" and len(f) == 6:
            out["units"][f[1]] = {"active": f[2], "result": f[3], "condition": f[4],
                                  "checked": int(f[5]) if f[5].isdigit() else None}
        elif f[0] == "marker" and len(f) == 2:
            out["marker"] = f[1] == "yes"
        elif f[0] == "tty2":
            out["tty2"] = f[1:]
        elif f[0] == "getty" and len(f) == 4 and f[2].isdigit() and f[3].isdigit():
            out["gettys"][f[1]] = (int(f[2]), int(f[3]))
        elif f[0] == "timer" and len(f) == 3:
            out["timers"][f[1]] = f[2]
        elif f[0] == "session" and len(f) in (2, 3):
            out["sessions"][f[1]] = f[2] if len(f) == 3 else ""
        elif (f[0] in ("logins", "fgconsole", "i8042", "xhci", "keys") and len(f) == 2
              and f[1].isdigit()):
            out[f[0]] = int(f[1])
        elif f[0] == "kbd" and len(f) == 2:
            out["kbds"].append(f[1])
        elif f[0] == "job" and len(f) >= 2:
            out["queued"].append(" ".join(f[1:]))
        elif f[0] == "vtium" and len(f) == 3:
            out["vtium"] = (int(f[1]), float(f[2]))
    return out


def unit(state, name):
    """The agent's reading of unit `name`, {} when it gave none."""
    return state["units"].get(name, {})


def getty_on_tty2(state):
    """Whether a getty or a login runs on tty2."""
    return any(p in ("agetty", "login") for p in state["tty2"] or ())


def new_jobs(state, before):
    """The jobs queued in `state` for a unit that had none in `before`. The
    guest's boot never ends, as systemd-time-wait-sync waits for a network
    the guest lacks, so its jobs stay queued throughout and only a job that
    came since `before` is the scenario's."""
    known = {job.split()[0] for job in before["queued"]}
    return sorted(job for job in state["queued"] if job.split()[0] not in known)


def new_timers(state, before):
    """The timers waiting in `state` that were not in `before`: a start
    delayed by a timer waits there, not as a job."""
    return sorted(t for t, sub in state["timers"].items()
                  if sub == "waiting" and t not in before["timers"])


def judge_no_getty(state, before):
    """Why the agent's `state` shows a getty on tty2 under closed: one
    that runs, one that ran since boot, a login session on tty2, or a
    timer that came since `before`; None when it shows none."""
    getty = unit(state, GETTY_UNIT)
    if getty.get("active") != "inactive" or getty_on_tty2(state):
        return "a getty runs on tty2 under the closed policy: %s, %s" % (
            getty.get("active"), " ".join(state["tty2"] or ()) or "nothing")
    for name in GETTY_UNITS:
        reading = state["gettys"].get(name)
        if reading is None:
            return "the agent gave no reading of %s" % name
        if reading != (0, 0):
            return ("%s started under the closed policy: it left its inactive state at "
                    "%.1f s and restarted %d times" % (name, reading[0] / 1e6, reading[1]))
    on_tty2 = sorted(s for s, tty in state["sessions"].items() if tty == "tty2")
    if on_tty2:
        return "login session %s is on tty2 under the closed policy" % ", ".join(on_tty2)
    if new_timers(state, before):
        return "a timer waits to start a unit: %s" % ", ".join(new_timers(state, before))
    return None


def fallback_ran(state, checked, before):
    """Whether systemd has checked the fallback's condition since the
    timestamp `checked`, and has no job queued that came since `before`, so
    no unit is about to start or stop and the units' states are final."""
    return (unit(state, FALLBACK_UNIT).get("checked") not in (None, 0, checked)
            and not new_jobs(state, before))


def judge_running(state, closed):
    """Why vtium is not holding tty2 as a console that gives no login
    should: running, on the VT shown, with the marker under `closed`; None
    when it is."""
    if unit(state, VTIUM_UNIT).get("active") != "active" or state["vtium"] is None:
        return "vtium does not run on tty2: %s, PID %s" % (
            unit(state, VTIUM_UNIT).get("active"), state["vtium"])
    if state["fgconsole"] != 2:
        return "the VT shown is %s, not 2" % state["fgconsole"]
    if closed and state["marker"] is not True:
        return "the closed policy's marker /run/vtium/tty2.closed is missing"
    if getty_on_tty2(state):
        return "a getty runs on tty2: %s" % " ".join(state["tty2"])
    return None


def judge_no_login(before, after, same_screen, logged_in):
    """Why the no-login check failed (CHECKS), from the agent's state before
    and after the line was typed, whether the screen stayed as it was, and
    whether a login reported itself on the event port; None when it
    passed."""
    if before["i8042"] is None or after["i8042"] is None or after["i8042"] <= before["i8042"]:
        return ("the PS/2 keyboard's interrupt count did not rise (%s, then %s), so the line "
                "never reached the guest" % (before["i8042"], after["i8042"]))
    why = judge_running(after, closed=True)
    if why:
        return why
    if logged_in:
        return "a login reported itself on the event port"
    if not same_screen:
        return "the screen changed while the line was typed"
    if after["logins"] != 0:
        return "%s login program(s) run, though no keyboard reaches a head" % after["logins"]
    return None


def judge_fallback(closed, failure, before, state):
    """Why a failure's fallback is wrong (CHECKS "getty-fallback"), from the
    agent's state before the failure and once the fallback's condition was
    checked again; None when it is right."""
    vt, fallback, getty = (unit(state, u) for u in (VTIUM_UNIT, FALLBACK_UNIT, GETTY_UNIT))
    results = FAILURES[failure][1]
    if new_jobs(state, before):
        return "after the %s systemd still has jobs queued: %s" % (
            failure, ", ".join(new_jobs(state, before)))
    if vt.get("active") != "failed" or vt.get("result") not in results:
        return "after the %s vtium's unit is %s with result %s, not failed with %s" % (
            failure, vt.get("active"), vt.get("result"), " or ".join(results))
    if fallback.get("checked") in (None, 0, unit(before, FALLBACK_UNIT).get("checked")):
        return "after the %s systemd never checked the fallback's condition" % failure
    if closed:
        if fallback.get("condition") != "no":
            return "after the %s the fallback's condition held, under the closed policy" % failure
        if state["marker"] is not True:
            return "after the %s the closed policy's marker is gone" % failure
        why = judge_no_getty(state, before)
        return "after the %s %s" % (failure, why) if why else None
    if fallback.get("condition") != "yes":
        return "after the %s the fallback skipped the getty, under the open policy" % failure
    if state["marker"] is not False:
        return "the closed policy's marker exists under the open policy"
    if getty.get("active") != "active" or not getty_on_tty2(state):
        return "after the %s no getty runs on tty2 under the open policy: %s, %s" % (
            failure, getty.get("active"), " ".join(state["tty2"] or ()) or "nothing")
    return None


def judge_held(first, state, serial, wait_s):
    """Why vtium did not hold tty2 past display-wait (`wait_s`) with no
    display under closed (CHECKS "display-held"), from the agent's first
    reading of vtium, its reading once vtium was older than the wait, and
    the console log; None when it held."""
    why = judge_running(state, closed=True)
    if why:
        return why
    if state["vtium"][0] != first["vtium"][0]:
        return "vtium's PID changed from %d to %d: it ended and started again" % (
            first["vtium"][0], state["vtium"][0])
    if state["vtium"][1] < wait_s + HOLD_PAST_S:
        return "vtium was read %.1f s after its start, before display-wait ran out" % (
            state["vtium"][1])
    m = NO_DISPLAY_LINE.search(serial)
    if m:
        return "vtium logged %r under the closed policy" % m.group(0)
    if unit(state, FALLBACK_UNIT).get("checked"):
        return "the getty fallback ran"
    return None


def judge_exit(first, state, wait_s, logged):
    """Why vtium did not end at display-wait (`wait_s`) with no display under
    open (CHECKS "display-exit"), from the agent's first state and its state
    once the fallback ran, and the wait vtium logged it gave up after
    (`logged`); None when it did."""
    if first["vtium"] is None:
        return ("vtium was not running when the check read the boot's jobs, so the "
                "fallback's may be among them")
    if logged != wait_s:
        return "vtium gave up after %s s, not the display-wait of %d s" % (logged, wait_s)
    if new_jobs(state, first):
        return "systemd still has jobs queued: %s" % ", ".join(new_jobs(state, first))
    vt, fallback, getty = (unit(state, u) for u in (VTIUM_UNIT, FALLBACK_UNIT, GETTY_UNIT))
    if vt.get("active") != "failed" or vt.get("result") != "exit-code":
        return "vtium's unit is %s with result %s, not failed with exit-code" % (
            vt.get("active"), vt.get("result"))
    if fallback.get("condition") != "yes" or not fallback.get("checked"):
        return "the fallback did not run: condition %s" % fallback.get("condition")
    if state["marker"] is not False:
        return "the closed policy's marker exists under the open policy"
    if getty.get("active") != "active" or not getty_on_tty2(state):
        return "no getty runs on tty2: %s, %s" % (getty.get("active"),
                                                  " ".join(state["tty2"] or ()) or "nothing")
    return None


def judge_policy_line(serial, lines, policy_line):
    """Why the console's log `serial` does not show a head file whose
    `lines` hold `policy_line`, a policy line vtium cannot read, as closed
    and broken (CHECKS "policy-line"); None when it does."""
    if policy_line not in lines:
        raise ValueError("the head file holds no line %r" % policy_line)
    n = lines.index(policy_line) + 1
    value = policy_line.split("=", 1)[1].strip()
    error = '%s:%d: `policy` cannot be "%s"' % (GUEST_HEAD_FILE, n, value)
    if error not in serial:
        return "the console logged no error %r for the policy line" % error
    for line in RUN_LINES:
        if line in serial:
            return ("the console logged %r, an outcome that runs some of the file or the "
                    "shipped default" % line)
    return None


def check_policy_line(variant, vm, work, caps, errors, log):
    """The policy-line check (CHECKS), on the console's log once the first
    screen has settled."""
    case = "policy-line"
    try:
        with open(head_file(variant)) as f:
            lines = f.read().splitlines()
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path)
        why = judge_policy_line(serial_after(vm, 0), lines, variant["policy_line"])
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = path
    except (RuntimeError, ValueError) as e:
        errors[case] = str(e)


def judge_kiosk(chord, plugged=None, same_screen=True):
    """Why the kiosk check failed (CHECKS "kiosk"), from the agent's
    states before and after the claimed keyboard's Ctrl+Alt+F3 (`chord`)
    and before and after the second keyboard's line (`plugged`, None while
    it has not typed), each pair read once the screen settled, and whether
    the screen stayed as it was while the second keyboard typed; None when
    it passed. The chord is known to reach the guest by the USB
    controller's interrupt count, which both keyboards share, and the
    second keyboard's line by the presses its own evdev node counted."""
    before, after = chord
    if before["xhci"] is None or after["xhci"] is None or after["xhci"] <= before["xhci"]:
        return ("the USB controller's interrupt count did not rise for Ctrl+Alt+F3 "
                "(%s, then %s)" % (before["xhci"], after["xhci"]))
    why = judge_running(after, closed=True)
    if why:
        return "after Ctrl+Alt+F3: %s" % why
    if plugged is None:
        return None
    if plugged[1]["keys"] != len(KIOSK_LINE):
        return ("the second keyboard's evdev node counted %s key presses, not the %d of "
                "its line" % (plugged[1]["keys"], len(KIOSK_LINE)))
    why = judge_running(plugged[1], closed=True)
    if why:
        return "after the second keyboard's line: %s" % why
    if KIOSK_SECOND not in plugged[0]["kbds"]:
        return "plugged in, the guest lists %s, not %s" % (plugged[0]["kbds"], KIOSK_SECOND)
    if not same_screen:
        return "the screen changed while the second keyboard typed"
    return None


def check_kiosk(variant, vm, work, caps, errors, log):
    """The kiosk check (CHECKS), after the scenario logged in and drew cjk
    through the claimed keyboard. The claimed keyboard types last before
    the second keyboard is plugged in: QEMU sends its keys to the keyboard
    that took input last, which once the second is pulled out need not be
    the claimed one. The capture is the screen after the second keyboard's
    line."""
    case = "kiosk"
    tmp = os.path.join(work, "wait.ppm")
    try:
        shown = wait_stable(vm, tmp)
        before = ask_state(vm)
        vm.keys("ctrl", "alt", "f3")
        wait_state(vm, lambda s: (s["xhci"] or 0) > (before["xhci"] or 0),
                   "the chord reaching the guest")
        wait_stable(vm, tmp)
        chord = (before, ask_state(vm))
        why = judge_kiosk(chord)
        if why:
            raise RuntimeError(why)
        # the claimed keyboard still reaches the session after its chord
        t3_devices.run_screen(vm, "sgr", STATE_TIMEOUT)
        drawn = wait_stable(vm, tmp, want=lambda i: i.data != shown.data)
        vm.cmd("device_add", driver="usb-kbd", id=t3_devices.HOT_ID, bus=t3_devices.USB_BUS,
               port=t3_devices.HOT_PORT, display="video0")
        listed = wait_state(vm, lambda s: KIOSK_SECOND in s["kbds"], "the second keyboard listed")
        answer = vm.ask("watch")
        log("%s: %s" % (case, " ".join(answer)))
        if not any(line.startswith("watching ") for line in answer):
            raise RuntimeError("the agent cannot read the second keyboard: %s"
                               % " ".join(answer))
        for ch in KIOSK_LINE:
            for down in (True, False):
                vm.cmd("input-send-event", device="video0",
                       events=t3_devices.key_events(char_qcodes(ch), down))
            time.sleep(0.05)  # QEMU's bounded input queue, as Qemu.keys
        wait_state(vm, lambda s: (s["keys"] or 0) >= len(KIOSK_LINE),
                   "the second keyboard's keys reaching its evdev node")
        path = os.path.join(work, case + ".ppm")
        img = wait_stable(vm, path)
        plugged = (listed, ask_state(vm))
        vm.cmd("device_del", id=t3_devices.HOT_ID)
        t3_devices.wait_qmp_event(vm, "DEVICE_DELETED", t3_devices.HOT_ID, 30)
        why = judge_kiosk(chord, plugged, img.data == drawn.data)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = path
    except RuntimeError as e:
        errors[case] = str(e)


def display_wait(variant):
    """The display-wait in seconds `variant`'s kmscon.conf sets."""
    with open(os.path.join(GUEST, variant["conf"])) as f:
        m = re.search(r"^display-wait=(\d+)$", f.read(), re.M)
    if not m:
        raise ValueError("the configuration sets no display-wait in seconds")
    return int(m.group(1))


def head_file(variant):
    """The path of the head file `variant` installs, which its "heads"
    names from the repository's root."""
    return os.path.join(ROOT, variant["heads"])


def ask_state(vm):
    return parse_state(vm.ask("state"))


def wait_state(vm, want, what, timeout=STATE_TIMEOUT):
    """The agent's state once `want(state)` holds, asked again until then."""
    deadline = time.time() + timeout
    while True:
        state = ask_state(vm)
        if want(state):
            return state
        if time.time() > deadline:
            raise RuntimeError("%s not within %d s; the last state: %s" % (what, timeout, state))
        time.sleep(0.5)


def serial_size(vm):
    with open(vm.serial, "rb") as f:
        return len(f.read())


def serial_after(vm, since):
    with open(vm.serial, "rb") as f:
        return f.read()[since:].decode(errors="replace")


def check_no_login(variant, vm, work, caps, errors, log):
    """The no-login check (CHECKS): the screen and the agent's state, a
    line typed on the console's keyboard, then the same once the guest
    took its keys. The capture is the screen after the line."""
    case = "no-login"
    try:
        tmp = os.path.join(work, "wait.ppm")
        shown = wait_stable(vm, tmp)
        before = ask_state(vm)
        since = os.path.getsize(vm.events)
        vm.type("tester\n")
        # Seven keys pressed and released: the kernel took at least seven
        # of the PS/2 keyboard's interrupts. The state judged is read once
        # the screen has settled after them.
        wait_state(vm, lambda s: (s["i8042"] or 0) >= (before["i8042"] or 0) + 7,
                   "the typed keys reaching the guest")
        path = os.path.join(work, case + ".ppm")
        img = wait_stable(vm, path)
        after = ask_state(vm)
        with open(vm.events, errors="replace") as f:
            logged_in = "T3-LOGIN" in f.read()[since:]
        why = judge_no_login(before, after, img.data == shown.data, logged_in)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = path
    except RuntimeError as e:
        errors[case] = str(e)


def check_getty_fallback(variant, vm, work, caps, errors, log):
    """The getty-fallback check (CHECKS): each of the variant's failures
    in turn, judged by judge_fallback once systemd has checked the
    fallback's condition again, then vtium started again for the next.
    Under closed, the agent's readings of the gettys count from boot, so
    a getty that started and ended between two reads still shows; they
    are judged again just before vtium starts again after each failure,
    and once more when vtium runs on tty2 after the last."""
    case = "getty-fallback"
    closed = variant["policy"] == "closed"
    try:
        first = None
        for failure in variant["failures"]:
            before = wait_state(vm, lambda s: judge_running(s, closed) is None,
                                "vtium running on tty2")
            first = first or before
            checked = unit(before, FALLBACK_UNIT).get("checked")
            answer = vm.ask(FAILURES[failure][0])
            log("%s: %s: %s" % (case, failure, " ".join(answer)))

            # Under open a fallback that ran ends the wait once its getty
            # runs, and one that skipped itself at once, to be judged.
            def fell_back(s):
                done = (unit(s, VTIUM_UNIT).get("active") == "failed"
                        and fallback_ran(s, checked, before))
                return done and (closed or getty_on_tty2(s)
                                 or unit(s, FALLBACK_UNIT).get("condition") == "no")

            state = wait_state(vm, fell_back, "the fallback after the %s" % failure)
            why = judge_fallback(closed, failure, before, state)
            log("%s: %s: %s" % (case, failure, why or "as wanted"))
            if why:
                raise RuntimeError(why)
            if variant["device"] is not None:
                path = os.path.join(work, case + ".ppm")
                wait_stable(vm, path)
                caps[case] = path
            if closed:
                why = judge_no_getty(ask_state(vm), before)
                if why:
                    raise RuntimeError("after the %s, before vtium started again: %s"
                                       % (failure, why))
            since = serial_size(vm)
            vm.ask("start")
            deadline = time.time() + STATE_TIMEOUT
            while "vt: using tty /dev/tty2" not in serial_after(vm, since):
                if time.time() > deadline:
                    raise RuntimeError("vtium did not start again on tty2 after the %s; see %s"
                                       % (failure, rel(vm.serial)))
                time.sleep(0.2)
        if closed:
            end = wait_state(vm, lambda s: judge_running(s, closed) is None,
                             "vtium running on tty2 after the last failure")
            why = judge_no_getty(end, first)
            log("%s: at the end: %s" % (case, why or "as wanted"))
            if why:
                raise RuntimeError("at the end: %s" % why)
        if variant["device"] is None:
            caps[case] = None
    except RuntimeError as e:
        caps.pop(case, None)
        errors[case] = str(e)


def check_display_held(variant, vm, work, caps, errors, log):
    """The display-held check (CHECKS): vtium's state first, then once vtium
    is older than display-wait and FLIP_WAIT past it."""
    case = "display-held"
    try:
        wait_s = display_wait(variant)
        first = wait_state(vm, lambda s: s["vtium"] is not None, "vtium running on tty2")
        state = wait_state(vm, lambda s: s["vtium"] is None
                           or s["vtium"][1] >= wait_s + HOLD_PAST_S,
                           "vtium older than display-wait", timeout=wait_s + STATE_TIMEOUT)
        why = judge_held(first, state, serial_after(vm, 0), wait_s)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = None
    except RuntimeError as e:
        errors[case] = str(e)


def check_display_exit(variant, vm, work, caps, errors, log):
    """The display-exit check (CHECKS): vtium's line that no display came,
    then the agent's state once the fallback has run."""
    case = "display-exit"
    try:
        wait_s = display_wait(variant)
        # The boot's own jobs, read while the console waits, which
        # judge_exit checks vtium still did; a job queued since then is the
        # scenario's.
        first = ask_state(vm)
        line = vm.wait_serial("no display appeared within", timeout=wait_s + STATE_TIMEOUT)
        logged = int(NO_DISPLAY_LINE.search(line).group(1))
        state = wait_state(vm, lambda s: fallback_ran(s, 0, first) and (
            getty_on_tty2(s) or unit(s, FALLBACK_UNIT).get("condition") == "no"),
            "the fallback's getty on tty2")
        why = judge_exit(first, state, wait_s, logged)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = None
    except RuntimeError as e:
        errors[case] = str(e)


def judge_two_heads(displays, start, end, desk):
    """Why the two heads are not two (CHECKS "two-heads"), from the
    console's Display lines, the second output's capture before the login
    and after the scenario, and the first output's last capture; None when
    they are."""
    cells = {}
    for d in displays:
        m = re.search(r"Display (\S+) with .* cell size (\d+x\d+)", d)
        if m:
            cells[m.group(1)] = m.group(2)
    if cells != TWO_HEADS:
        return "the displays and their cells are %s, not %s" % (cells, TWO_HEADS)
    if end.data != start.data:
        return "the second output's screen changed while the first head was typed into"
    if end.data == desk.data:
        return "the second output shows the first head's screen"
    return None


def check_two_heads(variant, vm, work, caps, errors, log):
    """The two-heads check (CHECKS): the second output captured once
    settled, against its capture before the login (the run's
    two-heads-start.ppm) and the first output's last capture."""
    case = "two-heads"
    try:
        start = t3img.read(os.path.join(work, "two-heads-start.ppm"))
        desk = t3img.read(os.path.join(work, "vt-restore.ppm"))
        path = os.path.join(work, case + ".ppm")
        end = wait_stable(vm, path, output=second_output(variant))
        with open(vm.serial, "rb") as f:
            text = f.read().decode(errors="replace")
        displays = [l[l.index("Display "):] for l in text.splitlines()
                    if "Display " in l and "text renderer [" in l]
        why = judge_two_heads(displays, start, end, desk)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        caps[case] = path
    except (RuntimeError, OSError) as e:
        errors[case] = str(e)


def second_output(variant):
    """QMP screendump's arguments for `variant`'s second output."""
    return {k: variant["second"][k] for k in ("device", "head")}


CHECK_RUNS = {"no-login": check_no_login, "getty-fallback": check_getty_fallback,
              "display-held": check_display_held, "display-exit": check_display_exit,
              "two-heads": check_two_heads, "kiosk": check_kiosk,
              "policy-line": check_policy_line}
