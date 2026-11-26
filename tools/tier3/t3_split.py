"""Tier 3's checks of vtium's privilege split (ADR-1400 D4, the first slice).

A console built with the feature split starts as root and runs everything
but the jobs that need root as its drawing process, under the account vtium
that the tree's sysusers file makes. The variant split-direct runs such a
tree in direct mode on the minspec machine, logs in, draws its screens and
switches VTs as 2d-potato-min does, and is judged by its goldens
(t3_evaluate.GOLDENS_OF). Then its checks ask the agent (guest/t3-agent),
which runs as root, about the two processes:

- drawer-status reads the drawing process's /proc/PID/status: it runs as
  the account vtium, with every capability set empty, NoNewPrivs set, and
  not dumpable, as the root process's child.
- split-switch switches to VT 1. A graphical program run as root, t3-probe
  take-master, must then become the card's master, so vtium has let the
  card go. A line typed on VT 1 must not reach the session on vtium's
  console, so vtium no longer reads the keyboard. Back on VT 2, a line typed
  in the session must reach it, and vtium must draw the console again.
- drawer-kill holds the unit so that systemd neither restarts it nor resets
  the VT, sends SIGKILL to the drawing process, and reads the VT once the
  unit stopped: the root process must have put the VT back in text mode,
  with the kernel switching VTs and a keyboard mode that is not K_OFF, and
  must have exited with a failure. Then the agent starts vtium again.

The fallback to a getty after the drawing process is killed is the
getty-fallback check's failure "drawer-kill" (t3_heads), which the variant
split-fallback runs.
"""

import os
import re
import time

import t3_heads
from t3_qemu import not_blank, wait_stable

STATUS = "drawer-status"
SWITCH = "split-switch"
KILL = "drawer-kill"

CHECKS = {
    STATUS: "vtium's drawing process runs as the account vtium, the child of vtium's root "
            "process, with no capabilities, NoNewPrivs set and not dumpable (ADR-1400 D4, "
            "\"The drawer's setup\")",
    SWITCH: "At a switch to another VT, vtium lets go of the card, so a graphical program "
            "run as root becomes its master, and a line typed there never reaches vtium's "
            "session; back on vtium's VT, the session reads what is typed and vtium draws "
            "again (ADR-1400 D4, \"What the broker does\")",
    KILL: "When vtium's drawing process is killed, vtium's root process puts the VT back in "
          "text mode, lets the kernel switch VTs again, gives back a keyboard mode other than "
          "K_OFF, and exits with a failure (ADR-1400 D4, \"When the drawer dies\")",
}

# An empty capability set as /proc/PID/status prints it.
NO_CAPS = "0000000000000000"
CAP_SETS = ("CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb")
# The keyboard modes the VT may come back in: the mode vtium found at start,
# which the guest's systemd-vconsole-setup leaves as K_UNICODE, or the
# kernel's K_XLATE. K_OFF is vtium's own while it runs.
KEYBOARD_BACK = ("K_UNICODE", "K_XLATE")
# The line split-switch types on VT 1, which writes to the event port if it
# reaches vtium's session, and the one it types back on VT 2.
LEAKED = "T3-LEAKED"
BACK = "T3-BACK"


def parse_drawer(lines):
    """The agent's answer to `drawer` as {"root": (PID, [uids]),
    "drawer": PID or None, "parent": PID, "status": {key: [values]},
    "proc_owner": the owner of /proc/PID/status as "UID:GID",
    "account": (uid, gid)}."""
    out = {"root": None, "drawer": None, "parent": None, "status": {},
           "proc_owner": None, "account": None}
    for line in lines:
        f = line.split()
        if f[:1] == ["root"] and len(f) >= 2:
            out["root"] = (f[1], f[2:])
        elif f[:1] == ["drawer"] and len(f) == 4 and f[2] == "parent":
            out["drawer"], out["parent"] = f[1], f[3]
        elif f[:1] == ["status"] and len(f) >= 2:
            out["status"][f[1].rstrip(":")] = f[2:]
        elif f[:1] == ["proc-owner"] and len(f) == 2:
            out["proc_owner"] = f[1]
        elif f[:2] == ["account", "vtium"] and len(f) == 4:
            out["account"] = (f[2], f[3])
    return out


def judge_drawer(d):
    """Why the parsed answer `d` (parse_drawer) shows a drawing process that
    is not the one ADR-1400 D4 sets up; None when it is."""
    if d["root"] is None or d["root"][1] != ["0"] * 4:
        return "vtium's root process on tty2 does not run as root: %s" % (d["root"],)
    if d["drawer"] is None:
        return "vtium on tty2 runs no drawing process, so it runs as one process"
    if d["parent"] != d["root"][0]:
        return "the drawing process %s is the child of %s, not of the root process %s" % (
            d["drawer"], d["parent"], d["root"][0])
    if d["account"] is None or not all(x.isdigit() and x != "0" for x in d["account"]):
        return "the guest has no account vtium of its own ids: %s" % (d["account"],)
    uid, gid = d["account"]
    st = d["status"]
    if st.get("Uid") != [uid] * 4:
        return "the drawing process's user ids are %s, not the account vtium's %s" % (
            st.get("Uid"), uid)
    if st.get("Gid") != [gid] * 4:
        return "the drawing process's group ids are %s, not the account vtium's %s" % (
            st.get("Gid"), gid)
    if set(st.get("Groups") or ()) - {gid}:
        return "the drawing process keeps the supplementary groups %s" % st.get("Groups")
    held = ["%s %s" % (c, " ".join(st.get(c) or ["missing"])) for c in CAP_SETS
            if st.get(c) != [NO_CAPS]]
    if held:
        return "the drawing process holds capabilities: %s" % ", ".join(held)
    if st.get("NoNewPrivs") != ["1"]:
        return "the drawing process's NoNewPrivs is %s, not 1" % st.get("NoNewPrivs")
    if d["proc_owner"] != "0:0":
        return ("/proc/%s/status belongs to %s, so the drawing process is dumpable; the kernel "
                "gives the files under /proc/PID of a process that is not dumpable to root"
                % (d["drawer"], d["proc_owner"]))
    return None


def parse_vt(lines):
    """The fields of t3-probe vt-state's answer in `lines`, as {name: value}
    with kd, kb and switch; and the unit's line as "unit": [words]."""
    out = {}
    for line in lines:
        if line.startswith("unit "):
            out["unit"] = line.split()[1:]
        m = re.search(r"ROOTCHECK vt tty2: (.*)$", line)
        if m:
            out.update(dict(re.findall(r"(\w+)=(\S+)", m.group(1))))
    return out


def judge_vt_running(vt):
    """Why the VT `vt` (parse_vt) while vtium runs is not the VT a running
    vtium holds; None when it is. This is the reference the state after the
    kill is read against."""
    got = (vt.get("kd"), vt.get("kb"), vt.get("switch"))
    if got != ("KD_GRAPHICS", "K_OFF", "VT_PROCESS"):
        return "while vtium runs, tty2 is %s, not KD_GRAPHICS, K_OFF and VT_PROCESS" % (got,)
    return None


def judge_vt_after(vt):
    """Why the unit and the VT after the drawing process's kill (parse_vt)
    are not what ADR-1400 D4's "When the drawer dies" says; None when they
    are. systemctl prints ExecMainCode 1 for a process that exited, so the
    root process must have exited, with a status other than 0."""
    unit = vt.get("unit") or []
    if len(unit) != 5 or unit[0] not in ("failed", "inactive") or unit[3] != "1" or unit[4] == "0":
        return ("after the drawing process's kill vtium's unit is %s; its root process must "
                "exit with a failure" % " ".join(unit or ["not reported"]))
    if vt.get("kd") != "KD_TEXT":
        return "after the drawing process's kill tty2's display mode is %s, not KD_TEXT" % (
            vt.get("kd"))
    if vt.get("switch") != "VT_AUTO":
        return "after the drawing process's kill tty2's switch mode is %s, not VT_AUTO" % (
            vt.get("switch"))
    if vt.get("kb") not in KEYBOARD_BACK:
        return ("after the drawing process's kill tty2's keyboard mode is %s, not the mode "
                "vtium found at start" % vt.get("kb"))
    return None


def check_status(v, vm, work, caps, errors, log):
    """The drawer-status check (CHECKS)."""
    answer = vm.ask("drawer")
    for line in answer:
        log("%s: %s" % (STATUS, line))
    why = judge_drawer(parse_drawer(answer))
    if why:
        errors[STATUS] = why
        return
    caps[STATUS] = None


def judge_master(answer):
    """Why the agent's answer to `take-master` shows no graphical program
    that became the card's master while VT 1 was shown; None when it did."""
    lines = [line.partition("ROOTCHECK split-switch ")[2] or line for line in answer]
    if not any(line.startswith("master: yes") for line in lines):
        return "the graphical program never became the card's master: %s" % "; ".join(lines)
    if "shown 1" not in lines:
        return "the VT shown changed while the graphical program ran: %s" % "; ".join(lines)
    return None


def check_switch(v, vm, work, caps, errors, log):
    """The split-switch check (CHECKS)."""
    try:
        first = t3_heads.ask_state(vm)
        switch = t3_heads.parse_switch(vm.ask("chvt 1"))
        why = t3_heads.judge_switch(first, switch, 1)
        if why:
            raise RuntimeError(why)
        answer = vm.ask("take-master", timeout=60)
        for line in answer:
            log("%s: %s" % (SWITCH, line))
        why = judge_master(answer)
        if why:
            raise RuntimeError(why)
        vm.type("echo %s > /dev/ttyS1\n" % LEAKED)
        switch = t3_heads.parse_switch(vm.ask("chvt 2"))
        why = t3_heads.judge_switch(first, switch, 2)
        if why:
            raise RuntimeError(why)
        vm.type("echo %s > /dev/ttyS1\n" % BACK)
        vm.wait_event(BACK, timeout=30)
        with open(vm.events, errors="replace") as f:
            if LEAKED in f.read():
                raise RuntimeError("the line typed on VT 1 reached vtium's session")
        path = os.path.join(work, SWITCH + ".ppm")
        wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    except RuntimeError as e:
        errors[SWITCH] = str(e)
        return
    caps[SWITCH] = path


def check_kill(v, vm, work, caps, errors, log):
    """The drawer-kill check (CHECKS). Whatever the check finds, the agent
    takes its drop-in away again and starts vtium; once the VT passed, the
    new vtium must run as two processes again."""
    held = False
    try:
        running = parse_vt(vm.ask("vt-now"))
        log("%s: while vtium runs: %s" % (KILL, running))
        why = judge_vt_running(running)
        if why:
            raise RuntimeError(why)
        log("%s: %s" % (KILL, " ".join(vm.ask("hold-vt"))))
        held = True
        answer = vm.ask("kill-drawer KILL")
        log("%s: %s" % (KILL, " ".join(answer)))
        if not any(line.startswith("sent KILL") for line in answer):
            raise RuntimeError("the agent killed no drawing process: %s" % " ".join(answer))
        after = vm.ask("vt-after", timeout=60)
        for line in after:
            log("%s: %s" % (KILL, line))
        why = judge_vt_after(parse_vt(after))
        if why:
            raise RuntimeError(why)
        since = t3_heads.serial_size(vm)
        held = False
        log("%s: %s" % (KILL, " ".join(vm.ask("unhold-vt"))))
        deadline = time.time() + t3_heads.STATE_TIMEOUT
        while "vtium runs as two processes" not in t3_heads.serial_after(vm, since):
            if time.time() > deadline:
                raise RuntimeError("vtium did not start again as two processes after the kill")
            time.sleep(0.2)
    except RuntimeError as e:
        errors[KILL] = str(e)
        return
    finally:
        if held:
            log("%s: %s" % (KILL, " ".join(vm.ask("unhold-vt"))))
    caps[KILL] = None


CHECK_RUNS = {STATUS: check_status, SWITCH: check_switch, KILL: check_kill}
