"""Tier 3's head variants (ADR-1401, ADR-1403): heads, rules and looks
installed as the sections of /etc/vtium/vtium.conf in the guest, root's, and
what the guest's real KMS device and systemd make of them, which unit tests
cannot show. A variant names its head file, the sections and the `version`
and `policy` lines above them; the guest's vtium.conf is the variant's
configuration with the head file below it (guest_conf).

A console that gives no login leaves the driver no shell in the guest, so
these variants run the agent (guest/t3-agent), a root program on the third
serial port that answers a few fixed requests on that same port, which only
root can open: the state of VT 2's units, what systemd logged of vtium's
unit and its fallback, and the jobs systemd has queued, the closed policy's
getty marker and the programs on tty2, the key presses of the kiosk's
second keyboard, read from its own evdev node, and the failures it can
cause vtium, a kill, a crash, a hangup and kills past the unit's start
limit. Each check below waits on that state and sleeps no fixed time; the
one that must see vtium outlast display-wait waits for vtium's age, read in
the guest.

The getty fallback (vtium-getty@.service, which tools/vt/units.sh generates
from crates/vtium/install/vtium-getty@.service) and the unit's restarts
are the part no unit test reaches. When vtium's unit on tty2 fails,
systemd starts vtium again after 100 ms (Restart=on-failure), up to
systemd's default limit of 5 starts in 10 s, and each failure's
OnFailure= starts the fallback, which starts getty@tty2 unless vtium's
marker of a closed policy, /run/vtium/tty2.closed, exists (ADR-1401 D9).
Under open the getty's Conflicts= then stops vtium's unit; under closed
vtium comes back, and past the limit the unit stays failed. A check knows
the fallback ran by the journal, which logs each time the fallback skipped
itself on its condition or started the getty, so the getty's absence under
closed is read from the condition systemd checked, never from a wait that
saw nothing. The journal also keeps each failure's result, which a restart
clears from the unit's Result=.
"""

import os
import re
import time

import t3_devices
import t3img
from t3_common import GUEST, ROOT, rel
from t3_qemu import char_qcodes, wait_stable

# The guest's head files, beside the screens; a variant names one of these,
# or a shipped one under docs/heads. Each is the flow part of a vtium.conf:
# its `version` and `policy` lines, then its sections.
HEADS = os.path.join(GUEST, "heads")

# The units of VT 2, as the agent reports them: vtium's, its fallback's and
# the getty the fallback starts.
VTIUM_UNIT = "vtium@tty2.service"
# vtium's unit in seat mode, which the agent reports in VTIUM_UNIT's place on
# a guest in seat mode (guest/setup, ADR-1400 D3).
SEAT_UNIT = "vtium-seat@tty2.service"
FALLBACK_UNIT = "vtium-getty@tty2.service"
GETTY_UNIT = "getty@tty2.service"
# The gettys the guest could start on tty2, whose last exit from their
# inactive state and restarts the agent reports: under closed each keeps
# the 0 and 0 of a unit that never started since boot. autovt@tty2 is not
# one, as in this guest it is vtium's unit.
GETTY_UNITS = (GETTY_UNIT, "serial-getty@tty2.service")

# The failures the getty-fallback check causes, each the agent's request and
# the result systemd logs for vtium's unit: a kill, SIGKILL; a crash,
# SIGABRT, which a panic of vtium's minspec build also ends in, dumped by
# systemd-coredump where it runs; and a hangup of tty2, after which vtium
# exits 1 (crates/vtium/src/app.rs, Exit::Hangup).
FAILURES = {"kill": ("kill KILL", ("signal",)),
            "crash": ("kill ABRT", ("core-dump", "signal")),
            "hangup": ("hangup", ("exit-code",))}

# The first systemd that starts a unit's OnFailure= at each failure that its
# Restart= restarts, as the default RestartMode=normal passes through the
# failed state (systemd.service(5), systemd's NEWS for 254). Older ones
# start it only once the unit stops restarting, which the checks below do
# not expect, so they refuse a guest older than this.
SYSTEMD_ONFAILURE_EACH = 254
# systemd's default start limit, which vtium's unit keeps (ADR-1401 D9):
# DefaultStartLimitBurst=5 starts in DefaultStartLimitIntervalSec=10s
# (systemd-system.conf(5)). From a cleared count, the start-limit check
# kills the running vtium and each of the 5 the unit starts after it, and
# systemd refuses the sixth start: 6 kills.
START_LIMIT_BURST = 5
# Under open the most kills the start-limit check may send: the first, and
# a second when the unit started vtium again after its 100 ms before the
# getty's start stopped the unit. A third means the getty did not take the
# VT at the first failure.
OPEN_LIMIT_KILLS = 2
# The kernel console's cell on the guest's 640x480 display, the font the
# kernel draws a VT in once vtium has left it: past the start limit the VT
# is blank but for the cursor in its first cell.
KERNEL_CELL = (8, 16)

# vtium's log line when no display came within display-wait under open
# (crates/vtium/src/app.rs).
NO_DISPLAY_LINE = re.compile(r"no display appeared within (\d+) s")
# The cell of each head's display in two-heads.conf, by connector: the cell
# the frit of each head's look states, desk's 8x16 on the first output and
# panel's 8x8 on the second.
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

# Where guest/setup installs the variant's vtium.conf, as the console's log
# names it.
GUEST_CONF = "/etc/vtium/vtium.conf"
# The console lines of the outcomes that run some of the file, partly broken
# under either policy, or the shipped default in its place (ADR-1401 D7). A
# file whose policy line vtium cannot read never shows either, as it is
# closed and broken and runs nothing.
RUN_LINES = ("vtium.conf has errors: running the parts without errors.",
              "vtium.conf not used: running the built-in configuration.")

CHECKS = {
    "no-login": "A line typed on the console's keyboard, which reaches the guest (the PS/2 "
                "keyboard's interrupt count rises). Correct: the screen as it was, no login "
                "on any keyboard, vtium still running on tty2 and the closed policy's "
                "marker /run/vtium/tty2.closed written.",
    "getty-fallback": "vtium on tty2 failed by each of the variant's failures (a kill, a "
                      "crash, a hangup), and the unit's getty fallback read once systemd "
                      "has checked its condition. Correct under closed: systemd logged the "
                      "failure's result and started vtium again, which draws its first "
                      "screen again, the fallback skipped once on its condition, the marker "
                      "still there, and no getty started on tty2 since boot and no login "
                      "session on it, read after each failure and once more at the end. "
                      "Correct under open: the fallback ran, the getty's start stopped "
                      "vtium's unit, getty@tty2 runs a getty on tty2 and no marker exists; "
                      "vtium is started again by hand for the next failure. The capture is "
                      "tty2 after the last failure.",
    "start-limit": "vtium on tty2 killed by SIGKILL each time its unit starts it, from a "
                   "cleared start count, until the unit stops restarting, then started once "
                   "more; then the count cleared and vtium started by hand. Correct under "
                   "closed: systemd starts vtium 5 times and refuses the sixth start, the "
                   "unit failed with start-limit-hit and refusing the next start, the "
                   "fallback skipped on its condition, the marker there, no getty started "
                   "on tty2 since boot, and the VT blank but for the kernel console's "
                   "cursor; started by hand, vtium draws its first screen again. Correct "
                   "under open: at most 2 kills, the first and one more if the unit started "
                   "vtium again before the getty's start stopped it, as the getty takes the "
                   "VT at the first failure. The capture is tty2 once the unit "
                   "stopped restarting (ADR-1401 D9).",
    "display-held": "No display device under a closed policy: vtium on tty2 read past "
                    "display-wait. Correct: the same vtium still running, no line that no "
                    "display appeared, the marker written and no getty on tty2 (ADR-1401 "
                    "D8).",
    "session-survives": "A shell logged in on vtium's console, which vtium keeps in "
                        "systemd's file-descriptor store, then vtium on tty2 restarted by its "
                        "unit, as after an upgrade, and then killed by SIGKILL, as a crash, "
                        "under a closed policy, where no getty takes the VT. Correct after "
                        "each: a new vtium on tty2 holds the shell's pty master and says it "
                        "took the session back; the same shell, by its process id, still "
                        "leads its session on the same pty, at the same size; and a line "
                        "typed on the console's keyboard reaches that shell, which writes its "
                        "own process id where the agent reads it. The capture is the screen "
                        "after the last line (ADR-1401 D13).",
    "two-heads": "A valid open vtium.conf with two heads, one on each output of the card, "
                 "each with its own look's font: the second output, the panel, read "
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
    "policy-line": "A vtium.conf whose policy line holds a value vtium does not know, "
                   "`policy = opne`, read from the console's log once its panic screen "
                   "settled. Correct: the console logged that line's error with the path "
                   "/etc/vtium/vtium.conf and the line's number, and logged no outcome that runs some "
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
LINES = {"panic": "vtium.conf has errors and counts as closed: all input is off.",
         "policy-panic": "vtium.conf has errors and counts as closed: all input is off.",
         "partly-notice": "vtium.conf has errors: running the parts without errors.",
         "no-display-notice": "No head has a connected display."}

# The cases judged by a golden that no screen script draws: the console's
# first screen, before any key.
FIRST = {
    "panic": "The panic screen on the boot display of a vtium.conf whose closed flow is "
             "broken: `vtium.conf has errors and counts as closed: all input is off.`, then that its "
             "errors leave no head that can run, each error with its line of "
             "/etc/vtium/vtium.conf, and how to read the full report (ADR-1401 D7). Correct: the report alone, with "
             "no login prompt and no shell.",
    "policy-panic": "The panic screen on the boot display of a vtium.conf whose policy "
                    "line vtium cannot read, `policy = opne`, below a line naming a "
                    "version vtium does not know, `version = 2` (ADR-1401 D3, D7). "
                    "Correct: `vtium.conf has errors and counts as closed: all input is off.`, then that "
                    "no head can run, then the policy line's error with its line of "
                    "/etc/vtium/vtium.conf, listed first, before the error of the version "
                    "line above it, and how to read the full report; no login prompt and "
                    "no shell.",
    "partly-notice": "The notice of a vtium.conf whose open flow is partly broken, at "
                     "the top of the head's screen: `vtium.conf has errors: running the "
                     "parts without errors.` and what to "
                     "do next (ADR-1401 D7). It covers the rows of the login prompt, "
                     "which takes keys as before. Correct: the notice over a screen "
                     "otherwise blank.",
    "no-display-notice": "A closed vtium.conf whose one head matches no display: the "
                         "display guard lights the guest's display for the notice "
                         "`No head has a connected display.` alone, with what to do next "
                         "(ADR-1401 D9). Correct: the notice, with no login prompt.",
}


def parse_state(lines):
    """The agent's answer to `state` (guest/t3-agent) as {"systemd": its
    version, "units": {unit: {"active", "result", "condition", "checked",
    "sub", "restarts"}}, "journal": [word], "gettys": {unit: (left inactive
    at, restarts)}, "queued", "timers": {unit: state}, "sessions": {id:
    tty}, "marker", "tty2", "logins", "fgconsole", "i8042", "xhci", "kbds",
    "keys", "vtium": (pid, age) or None}; a field the answer lacks is None,
    but "kbds", the keyboards the guest lists, which is empty, and the
    readings by unit or session, which are empty. The journal's words, in
    the order systemd logged them this boot, are failed:RESULT, restart:N,
    started and stopped for vtium's unit, and skipped and fell-back for its
    fallback."""
    out = {"units": {}, "gettys": {}, "timers": {}, "sessions": {}, "marker": None,
           "tty2": None, "logins": None, "fgconsole": None, "i8042": None, "xhci": None,
           "kbds": [], "keys": None, "queued": [], "vtium": None, "systemd": None, "journal": None}
    for line in lines:
        f = line.split()
        if not f:
            continue
        if f[0] == "unit" and len(f) == 6:
            out["units"].setdefault(f[1], {}).update(
                {"active": f[2], "result": f[3], "condition": f[4],
                 "checked": int(f[5]) if f[5].isdigit() else None})
        elif f[0] == "restarts" and len(f) == 4:
            out["units"].setdefault(f[1], {}).update(
                {"sub": f[2], "restarts": int(f[3]) if f[3].isdigit() else None})
        elif f[0] == "journal":
            out["journal"] = f[1:]
        elif f[0] == "systemd" and len(f) == 2 and f[1].isdigit():
            out["systemd"] = int(f[1])
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
    """The agent's reading of unit `name`, {} when it gave none. vtium's
    unit is VTIUM_UNIT, or SEAT_UNIT on a guest in seat mode."""
    units = state["units"]
    if name == VTIUM_UNIT and name not in units:
        return units.get(SEAT_UNIT, {})
    return units.get(name, {})


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


def journal_since(state, before):
    """The words of the journal of vtium's unit and its fallback (guest/
    t3-agent) logged since `before`, or None when `before`'s words are not
    where `state`'s begin."""
    old, new = before["journal"] or [], state["journal"] or []
    return new[len(old):] if new[:len(old)] == old else None


def vtium_words(words):
    """The words of `words` that vtium's unit logged: its failures, restarts,
    starts and stops, without the fallback's."""
    return [w for w in words if w not in ("skipped", "fell-back")]


def judge_systemd(state):
    """Why the guest's systemd is not one the checks of vtium's restarts can
    judge; None when it is."""
    if state["systemd"] is None:
        return "the agent gave no version of systemd"
    if state["systemd"] < SYSTEMD_ONFAILURE_EACH:
        return ("the guest runs systemd %d, which starts OnFailure= only once a unit stops "
                "restarting; these checks expect %d or later" % (state["systemd"],
                                                                 SYSTEMD_ONFAILURE_EACH))
    return None


def restart_settled(state, before):
    """Whether a failure since `before` has run its course: systemd queues no
    job that came since, the fallback ran after the failure, and vtium's unit
    either runs a vtium that was not running before or no longer waits to
    restart."""
    words = journal_since(state, before)
    if words is None:
        return True
    failed = [i for i, w in enumerate(words) if w.startswith("failed:")]
    if new_jobs(state, before) or not failed:
        return False
    if not {"skipped", "fell-back"} & set(words[failed[0]:]):
        return False
    vt = unit(state, VTIUM_UNIT)
    back = (vt.get("active") == "active" and "started" in words[failed[0]:]
            and state["vtium"] is not None
            and state["vtium"][0] != (before["vtium"] or (None,))[0])
    return back or vt.get("active") in ("failed", "inactive")


def judge_restart(failure, before, state):
    """Why vtium's unit did not come back from a failure under closed as
    ADR-1401 D9 says (CHECKS "getty-fallback"), from the agent's state before
    the failure and once it settled; None when it did."""
    words = journal_since(state, before)
    if words is None:
        return "the journal before the %s is not where the journal after it begins" % failure
    if new_jobs(state, before):
        return "after the %s systemd still has jobs queued: %s" % (
            failure, ", ".join(new_jobs(state, before)))
    if "fell-back" in words:
        return "after the %s the fallback started a getty, under the closed policy" % failure
    n = (unit(before, VTIUM_UNIT).get("restarts") or 0) + 1
    wanted = [["failed:" + r, "restart:%d" % n, "started"] for r in FAILURES[failure][1]]
    if vtium_words(words) not in wanted:
        return "after the %s systemd logged %s for vtium's unit, not %s" % (
            failure, " ".join(vtium_words(words)) or "nothing", " or ".join(
                " ".join(w) for w in wanted))
    if words.count("skipped") != 1:
        return "after the %s the fallback skipped itself %d times, not once" % (
            failure, words.count("skipped"))
    vt = unit(state, VTIUM_UNIT)
    if vt.get("active") != "active" or state["vtium"] is None:
        return "after the %s vtium does not run on tty2: %s, PID %s" % (
            failure, vt.get("active"), state["vtium"])
    if unit(state, FALLBACK_UNIT).get("condition") != "no":
        return "after the %s the fallback's condition held, under the closed policy" % failure
    if state["marker"] is not True:
        return "after the %s the closed policy's marker is gone" % failure
    why = judge_no_getty(state, before)
    return "after the %s %s" % (failure, why) if why else None


def judge_fallback(failure, before, state):
    """Why a failure's fallback under open is wrong (CHECKS
    "getty-fallback"), from the agent's state before the failure and once it
    settled; None when it is right."""
    words = journal_since(state, before)
    if words is None:
        return "the journal before the %s is not where the journal after it begins" % failure
    if new_jobs(state, before):
        return "after the %s systemd still has jobs queued: %s" % (
            failure, ", ".join(new_jobs(state, before)))
    ours = vtium_words(words)
    if not ours or ours[0] not in ["failed:" + r for r in FAILURES[failure][1]]:
        return "after the %s systemd logged %s for vtium's unit, not a failure with %s" % (
            failure, " ".join(ours) or "nothing", " or ".join(FAILURES[failure][1]))
    if "skipped" in words or "fell-back" not in words:
        return "after the %s the fallback skipped the getty, under the open policy" % failure
    vt = unit(state, VTIUM_UNIT)
    if ours[-1] != "stopped" or vt.get("active") != "inactive" or state["vtium"] is not None:
        return ("after the %s the getty's start did not stop vtium's unit: it logged %s and "
                "is %s" % (failure, " ".join(ours), vt.get("active")))
    if state["marker"] is not False:
        return "the closed policy's marker exists under the open policy"
    getty = unit(state, GETTY_UNIT)
    if getty.get("active") != "active" or not getty_on_tty2(state):
        return "after the %s no getty runs on tty2 under the open policy: %s, %s" % (
            failure, getty.get("active"), " ".join(state["tty2"] or ()) or "nothing")
    return None


def judge_limit(closed, answer, before, state):
    """Why kills of each vtium its unit starts did not end as ADR-1401 D9
    says (CHECKS "start-limit"), from the agent's answer to `limit`, its
    state before, with the unit's start count cleared, and once the unit
    stopped restarting; None when they did."""
    words = journal_since(state, before)
    if words is None:
        return "the journal before the kills is not where the journal after them begins"
    if new_jobs(state, before):
        return "after the kills systemd still has jobs queued: %s" % ", ".join(
            new_jobs(state, before))
    kills = len([line for line in answer if line.startswith("sent KILL to ")])
    vt = unit(state, VTIUM_UNIT)
    ours = vtium_words(words)
    if not closed:
        # the getty takes the VT at the first failure and stops vtium's unit
        if not 1 <= kills <= OPEN_LIMIT_KILLS or "failed:start-limit-hit" in ours:
            return ("under the open policy the getty did not take the VT at the first failure: "
                    "%d kills, not 1 or %d, and systemd logged %s" % (
                        kills, OPEN_LIMIT_KILLS, " ".join(ours)))
        return judge_fallback("kill", before, state)
    if kills != START_LIMIT_BURST + 1:
        return ("vtium's unit stopped restarting after %d kills, not the %d that systemd's "
                "default limit of %d starts allows" % (kills, START_LIMIT_BURST + 1,
                                                        START_LIMIT_BURST))
    wanted = sum((["failed:signal", "restart:%d" % n, "started"]
                  for n in range(1, START_LIMIT_BURST + 1)), [])
    wanted += ["failed:signal", "restart:%d" % (START_LIMIT_BURST + 1),
               "failed:start-limit-hit", "failed:start-limit-hit"]
    if ours != wanted:
        return "systemd logged %s for vtium's unit, not %s" % (" ".join(ours), " ".join(wanted))
    if "start refused" not in answer:
        return "systemd did not refuse the start that followed the start limit"
    if (vt.get("active"), vt.get("result")) != ("failed", "start-limit-hit") or state["vtium"]:
        return "past the start limit vtium's unit is %s with result %s, PID %s" % (
            vt.get("active"), vt.get("result"), state["vtium"])
    tripped = words.index("failed:start-limit-hit")
    if "fell-back" in words or "skipped" not in words[tripped:]:
        return ("past the start limit the fallback did not skip itself: systemd logged %s"
                % " ".join(words))
    if unit(state, FALLBACK_UNIT).get("condition") != "no":
        return "past the start limit the fallback's condition held, under the closed policy"
    if state["marker"] is not True:
        return "past the start limit the closed policy's marker is gone"
    why = judge_no_getty(state, before)
    return "past the start limit %s" % why if why else None


def kernel_blank(img):
    """Whether `img` is blank but for the first cell of the kernel console,
    where it draws the cursor of a VT that runs nothing."""
    bg = img.data[-3:]
    w, h = KERNEL_CELL
    row = img.width * 3
    for y in range(img.height):
        line = img.data[y * row:(y + 1) * row]
        if y < h:
            line = line[w * 3:]
        if line.count(bg) * 3 != len(line):
            return False
    return True


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
    if (unit(state, FALLBACK_UNIT).get("checked")
            or {"skipped", "fell-back"} & set(state["journal"] or ())):
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
    words = journal_since(state, first)
    if words is None:
        return "the journal of the boot is not where the journal after the exit begins"
    ours = vtium_words(words)
    if ours[:1] != ["failed:exit-code"]:
        return "systemd logged %s for vtium's unit, not a failure with exit-code" % (
            " ".join(ours) or "nothing")
    getty = unit(state, GETTY_UNIT)
    if "fell-back" not in words:
        return "the fallback did not start the getty: systemd logged %s" % " ".join(words)
    if ours[-1] != "stopped" or unit(state, VTIUM_UNIT).get("active") != "inactive":
        return "the getty's start did not stop vtium's unit: it logged %s and is %s" % (
            " ".join(ours), unit(state, VTIUM_UNIT).get("active"))
    if state["marker"] is not False:
        return "the closed policy's marker exists under the open policy"
    if getty.get("active") != "active" or not getty_on_tty2(state):
        return "no getty runs on tty2: %s, %s" % (getty.get("active"),
                                                  " ".join(state["tty2"] or ()) or "nothing")
    return None


def judge_policy_line(serial, lines, policy_line):
    """Why the console's log `serial` does not show a vtium.conf whose
    `lines` hold `policy_line`, a policy line vtium cannot read, as closed
    and broken (CHECKS "policy-line"); None when it does."""
    if policy_line not in lines:
        raise ValueError("the vtium.conf holds no line %r" % policy_line)
    n = lines.index(policy_line) + 1
    value = policy_line.split("=", 1)[1].strip()
    error = '%s:%d: `policy` cannot be "%s"' % (GUEST_CONF, n, value)
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
        lines = guest_conf(variant).splitlines()
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
    """The display-wait in seconds `variant`'s configuration sets."""
    with open(os.path.join(GUEST, variant["conf"])) as f:
        m = re.search(r"^display-wait=(\d+)$", f.read(), re.M)
    if not m:
        raise ValueError("the configuration sets no display-wait in seconds")
    return int(m.group(1))


def head_file(variant):
    """The path of the head file `variant` installs, which its "heads"
    names from the repository's root."""
    return os.path.join(ROOT, variant["heads"])


def guest_conf(variant):
    """The vtium.conf guest/setup installs for `variant`: its configuration,
    vtium's own keys, then its head file, if it names one, whose `version`
    and `policy` lines join the top level and whose sections come below it
    (ADR-1403 D1)."""
    with open(os.path.join(GUEST, variant["conf"])) as f:
        text = f.read()
    if "heads" in variant:
        if text and not text.endswith("\n"):
            text += "\n"
        with open(head_file(variant)) as f:
            text += f.read()
    return text


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


def parse_ptys(lines):
    """The agent's answer to `ptys` (guest/t3-agent) as {"ptys": {pid: (pty,
    size, command)}, "masters": (vtium's pid, the pty masters it has open)
    or None, "answers": {what: text}}."""
    out = {"ptys": {}, "masters": None, "answers": {}}
    for line in lines:
        f = line.split()
        if f[:1] == ["pty"] and len(f) == 5 and f[1].isdigit():
            out["ptys"][int(f[1])] = (f[2], f[3], f[4])
        elif f[:1] == ["masters"] and len(f) == 3 and f[1].isdigit() and f[2].isdigit():
            out["masters"] = (int(f[1]), int(f[2]))
        elif f[:1] == ["answer"] and len(f) >= 2:
            out["answers"][f[1]] = " ".join(f[2:])
    return out


def ask_ptys(vm):
    return parse_ptys(vm.ask("ptys"))


def shell_of(ptys):
    """The process id of the one shell that leads a session on a pty in the
    agent's answer `ptys`, or None when there is not exactly one."""
    shells = [pid for pid, (_, _, command) in ptys["ptys"].items() if command == "bash"]
    return shells[0] if len(shells) == 1 else None


def prompt_of(ptys):
    """The process id of the one login prompt that leads a session on a pty
    in the agent's answer `ptys`, or None when there is not exactly one."""
    prompts = [pid for pid, (_, _, command) in ptys["ptys"].items() if command == "login"]
    return prompts[0] if len(prompts) == 1 else None


def judge_survived(before, after, vtium, took_back):
    """Why the sessions in `before` did not survive the restart that left
    `after` (the agent's answers to `ptys`) with vtium's new process id
    `vtium`, and `took_back`, whether the new vtium logged that it took a
    session back; None when they survived. The sessions are the logged-in
    shell and the login prompt nobody has used, which lives in vtium's
    unit until someone logs in."""
    shell, prompt = shell_of(before), prompt_of(before)
    if shell is None:
        return "no one shell leads a session on a pty before the restart: %s" % before["ptys"]
    if prompt is None:
        return "no one login prompt leads a session on a pty before the restart: %s" % (
            before["ptys"])
    if not took_back:
        return "the new vtium did not log that it took a session back"
    for what, pid in (("shell", shell), ("login prompt", prompt)):
        if pid not in after["ptys"]:
            return "the %s %d is gone after the restart: %s" % (what, pid, after["ptys"])
        (pty, size, _), (pty_after, size_after, _) = before["ptys"][pid], after["ptys"][pid]
        if pty_after != pty:
            return "the %s %d leads its session on %s, not %s" % (what, pid, pty_after, pty)
        if size_after != size:
            return "the %s's pty is %s after the restart, not %s" % (what, size_after, size)
    if after["masters"] is None or after["masters"][0] != vtium or after["masters"][1] < 1:
        return "the new vtium %d holds no pty master: %s" % (vtium, after["masters"])
    return None


def check_session_survives(variant, vm, work, caps, errors, log):
    """The session-survives check (CHECKS), after the scenario logged in: a
    restart of vtium's unit, then a kill, each followed by a line typed in
    the shell. The new vtium is known by the agent's state, its log since
    the restart in the serial file, and its process id."""
    case = "session-survives"
    closed = variant["policy"] == "closed"
    try:
        for what, request in (("restart", "restart"), ("crash", "kill KILL")):
            first = wait_state(vm, lambda s: judge_running(s, closed) is None,
                               "vtium running on tty2")
            before = ask_ptys(vm)
            since = serial_size(vm)
            answer = vm.ask(request)
            log("%s: %s: %s" % (case, what, " ".join(answer)))
            state = wait_state(vm, lambda s: judge_running(s, closed) is None
                               and s["vtium"][0] != first["vtium"][0],
                               "a new vtium on tty2 after the %s" % what)
            pid = state["vtium"][0]
            deadline = time.time() + STATE_TIMEOUT
            line = "vtium[%d]: head console: console 1 takes back the session" % pid
            while line not in serial_after(vm, since) and time.time() < deadline:
                time.sleep(0.2)
            after = ask_ptys(vm)
            why = judge_survived(before, after, pid, line in serial_after(vm, since))
            log("%s: %s: %s" % (case, what, why or "the session survived"))
            if why:
                raise RuntimeError("after the %s: %s" % (what, why))
            vm.type("echo $$ > /tmp/t3-answer-%s\n" % what)
            shell = str(shell_of(before))
            deadline = time.time() + STATE_TIMEOUT
            while ask_ptys(vm)["answers"].get(what) != shell:
                if time.time() > deadline:
                    raise RuntimeError("after the %s the shell %s did not answer the line typed: "
                                       "%s" % (what, shell, ask_ptys(vm)["answers"]))
                time.sleep(0.5)
        path = os.path.join(work, case + ".ppm")
        wait_stable(vm, path)
        caps[case] = path
        # A stop is no restart: systemd closes the stored masters, and the
        # kernel hangs the sessions up.
        before = ask_ptys(vm)
        answer = vm.ask("stop")
        log("%s: stop: %s" % (case, " ".join(answer)))
        deadline = time.time() + STATE_TIMEOUT
        while True:
            after = ask_ptys(vm)
            why = judge_stopped(before, after)
            if why is None or time.time() > deadline:
                break
            time.sleep(0.5)
        log("%s: stop: %s" % (case, why or "the sessions ended"))
        if why:
            raise RuntimeError("after the stop: %s" % why)
    except RuntimeError as e:
        # A case with a capture and no golden passes on the capture, so a
        # failure after the capture drops it.
        caps.pop(case, None)
        errors[case] = str(e)


def judge_stopped(before, after):
    """Why the sessions in `before` did not end with the stop of vtium's
    unit that left `after` (the agent's answers to `ptys`); None when they
    ended."""
    left = sorted(pid for pid in (shell_of(before), prompt_of(before))
                  if pid is not None and pid in after["ptys"])
    if left:
        return "sessions still run after vtium's unit stopped: %s" % left
    if shell_of(before) is None:
        return "no shell led a session before the stop: %s" % before["ptys"]
    return None


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


def wait_restarted(variant, vm, work, path, pid, since, what):
    """vtium on tty2 started again in place of the vtium of PID `pid`, where
    the variant has a first screen: its log since `since` in the serial file
    shows the new vtium took its display, and the screen once settled, saved
    at `path`, is the variant's first screen."""
    deadline = time.time() + STATE_TIMEOUT
    while True:
        lines = re.findall(r"vtium\[(\d+)\]: Display ", serial_after(vm, since))
        if any(int(p) != pid for p in lines):
            break
        if time.time() > deadline:
            raise RuntimeError("the vtium started again after the %s took no display; see %s"
                               % (what, rel(vm.serial)))
        time.sleep(0.2)
    first = t3img.read(os.path.join(work, variant["first"] + ".ppm"))
    try:
        wait_stable(vm, path, want=lambda i: i.data == first.data)
    except RuntimeError as e:
        raise RuntimeError("after the %s vtium did not draw its first screen again: %s"
                           % (what, e))


def check_getty_fallback(variant, vm, work, caps, errors, log):
    """The getty-fallback check (CHECKS): each of the variant's failures in
    turn, judged once it has run its course. Under closed systemd starts
    vtium again, judged by judge_restart; the agent's readings of the
    gettys count from boot, so a getty that started and ended between two
    reads still shows, and they are judged once more when vtium runs on
    tty2 after the last failure. Under open, judged by judge_fallback, the
    agent starts vtium again for the next."""
    case = "getty-fallback"
    closed = variant["policy"] == "closed"
    try:
        first = None
        for failure in variant["failures"]:
            before = wait_state(vm, lambda s: judge_running(s, closed) is None,
                                "vtium running on tty2")
            why = judge_systemd(before)
            if why:
                raise RuntimeError(why)
            first = first or before
            since = serial_size(vm)
            answer = vm.ask(FAILURES[failure][0])
            log("%s: %s: %s" % (case, failure, " ".join(answer)))
            state = wait_state(vm, lambda s: restart_settled(s, before) and (
                closed or getty_on_tty2(s) or "skipped" in (journal_since(s, before) or ())),
                "the %s running its course" % failure)
            log("%s: %s: systemd logged %s" % (case, failure,
                                                " ".join(journal_since(state, before) or ())))
            if closed:
                why = judge_restart(failure, before, state)
            else:
                why = judge_fallback(failure, before, state)
            log("%s: %s: %s" % (case, failure, why or "as wanted"))
            if why:
                raise RuntimeError(why)
            path = os.path.join(work, case + ".ppm")
            if closed and "first" in variant:
                wait_restarted(variant, vm, work, path, before["vtium"][0], since, failure)
                caps[case] = path
            elif variant["device"] is not None:
                wait_stable(vm, path)
                caps[case] = path
            if not closed:
                start_again(vm, failure)
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


def start_again(vm, what):
    """vtium's unit on tty2 started again by the agent, once vtium logs that
    it uses tty2."""
    since = serial_size(vm)
    vm.ask("start")
    deadline = time.time() + STATE_TIMEOUT
    while "vt: using tty /dev/tty2" not in serial_after(vm, since):
        if time.time() > deadline:
            raise RuntimeError("vtium did not start again on tty2 after the %s; see %s"
                               % (what, rel(vm.serial)))
        time.sleep(0.2)


def check_start_limit(variant, vm, work, caps, errors, log):
    """The start-limit check (CHECKS): from a cleared start count, the agent
    kills each vtium the unit starts until the unit stops restarting, judged
    by judge_limit once that has run its course; then, with the count
    cleared again, vtium started by hand. The capture is tty2 once the unit
    stopped restarting."""
    case = "start-limit"
    closed = variant["policy"] == "closed"
    try:
        # An earlier check that failed may have left vtium's unit failed,
        # or a getty on tty2: the count is cleared and vtium started first,
        # so this check judges the limit on its own.
        log("%s: %s" % (case, " ".join(vm.ask("reset-failed"))))
        if ask_state(vm)["vtium"] is None:
            start_again(vm, "earlier checks")
        first = wait_state(vm, lambda s: judge_running(s, closed) is None,
                           "vtium running on tty2")
        why = judge_systemd(first)
        if why:
            raise RuntimeError(why)
        before = wait_state(vm, lambda s: unit(s, VTIUM_UNIT).get("restarts") == 0,
                            "vtium's start count cleared")
        answer = vm.ask("limit")
        log("%s: %s" % (case, " ".join(answer)))
        state = wait_state(vm, lambda s: restart_settled(s, before) and (
            closed or getty_on_tty2(s) or "skipped" in (journal_since(s, before) or ())),
            "the kills running their course")
        log("%s: systemd logged %s" % (case, " ".join(journal_since(state, before) or ())))
        why = judge_limit(closed, answer, before, state)
        log("%s: %s" % (case, why or "as wanted"))
        if why:
            raise RuntimeError(why)
        path = os.path.join(work, case + ".ppm")
        if closed:
            try:
                wait_stable(vm, path, want=kernel_blank)
            except RuntimeError as e:
                raise RuntimeError("past the start limit tty2 is not blank: %s" % e)
        else:
            wait_stable(vm, path)
        # Started by hand once its count is cleared, as ADR-1401 D9 tells
        # the operator, vtium draws its first screen again.
        vm.ask("reset-failed")
        since = serial_size(vm)
        start_again(vm, "start limit")
        end = wait_state(vm, lambda s: judge_running(s, closed) is None,
                         "vtium running on tty2 after the start limit")
        if closed and "first" in variant:
            wait_restarted(variant, vm, work, os.path.join(work, "start-limit-end.ppm"), -1,
                           since, "start limit")
            why = judge_no_getty(end, first)
            if why:
                raise RuntimeError("started again by hand: %s" % why)
        caps[case] = path
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
        state = wait_state(vm, lambda s: restart_settled(s, first) and (
            getty_on_tty2(s) or "skipped" in (journal_since(s, first) or ())),
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
              "start-limit": check_start_limit,
              "display-held": check_display_held, "display-exit": check_display_exit,
              "two-heads": check_two_heads, "kiosk": check_kiosk,
              "policy-line": check_policy_line, "session-survives": check_session_survives}
