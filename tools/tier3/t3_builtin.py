"""Tier 3's case of vtium's built-in font (ADR-1204 D1).

The variant builtin-font starts vtium on the minspec machine with the
minimal configuration, as 2d-potato-min does. Once vtium has drawn its login
prompt, the agent (guest/t3-agent, request `bare`) takes from vtium what a
machine with nothing installed lacks. It stops vtium's unit and removes
/etc/vtium, which holds vtium.conf, the font sets and their built glyphs,
the folders where guest/setup installs font files for vtium, and the font
helper /usr/bin/vtium-config. Then it starts the unit again.

The case builtin-login is the login prompt that the new vtium draws with no
configuration, no font set, no font file and no font helper. The case
builtin-notice is the notice that vtium shows when the kernel logs an error,
which the agent raises with no key pressed (t3_seat.kernel_message): vtium
draws its notices in its built-in font, vtium built-in.

References:
- ADR-1204 D1: the built-in font, and the acceptance check this case runs.
- ADR-1401 D15: the notice vtium shows for a kernel message.
"""

import os

import t3_seat
from t3_qemu import not_blank, wait_stable

LOGIN = "builtin-login"
NOTICE = "builtin-notice"
CASES = [LOGIN, NOTICE]

# The paths the agent's `bare` removes, in the order it answers them.
BARE_PATHS = ["/etc/vtium", "/usr/local/share/tier3-fonts", "/usr/local/share/fonts/vtium-tier3",
              "/usr/local/share/fonts/vtium-showcase", "/usr/bin/vtium-config"]
# How long the agent may take to stop vtium, remove the paths and start
# vtium again, which systemd counts as started once vtium has sent READY=1.
BARE_TIMEOUT = 90
# The line each vtium logs once it has set up a display, which the new
# vtium must log after the agent's request.
DISPLAY = "with backend ["

CAPTIONS = {
    LOGIN: "The login prompt on tty2 after the agent stopped vtium, removed /etc/vtium "
           "(vtium.conf, the font sets and their built glyphs), the font files tier 3 "
           "installs for vtium and the font helper /usr/bin/vtium-config, and started vtium "
           "again (ADR-1204 D1). Correct: the login prompt drawn in the font vtium draws "
           "text in when nothing names one, which is still the compiled-in GNU Unifont at "
           "8×16 (card 5b of ADR-1204 replaces it with vtium built-in); the cursor's "
           "cell is masked, because with no vtium.conf the cursor blinks.",
    NOTICE: "The same login prompt after the kernel logged an error of its own, tmpfs: "
            "Unknown parameter 'vtium_tier3_warning', at level err. Correct: the message in "
            "vtium's notice box at the top of the display, with the line Press Esc to "
            "dismiss this message (15) under it, the count at its first value, drawn in "
            "vtium built-in's 8×16 strike, which is cut from Terminus, not in Unifont; the "
            "console's screen around the box unchanged.",
}


def parse_bare(lines):
    """The agent's answer to `bare` as a dict: "before" (vtium's PID before,
    or None), "paths" ({path: "gone" or "kept"}), "started" (whether systemd
    started the unit again), "state" (the unit's ActiveState then, or None)
    and "after" (the new vtium's PID, or None)."""
    out = {"before": None, "paths": {}, "started": False, "state": None, "after": None}
    for line in lines:
        words = line.split()
        if len(words) == 2 and words[0] == "before" and words[1].isdigit():
            out["before"] = int(words[1]) or None
        elif len(words) == 2 and words[0] in ("gone", "kept"):
            out["paths"][words[1]] = words[0]
        elif words[:1] == ["started"]:
            out["started"] = True
        elif len(words) == 3 and words[0] == "unit":
            out["state"] = words[2]
        elif len(words) == 3 and words[0] == "vtium" and words[1].isdigit():
            out["after"] = int(words[1])
    return out


def bare_verdict(answer):
    """Why the agent's parsed answer to `bare` (parse_bare) shows that it
    did not take away what it should, or that no new vtium runs, or None.
    """
    for path in BARE_PATHS:
        if answer["paths"].get(path) != "gone":
            return "the agent did not remove %s: %s" % (path, answer["paths"].get(path,
                                                                                  "no answer"))
    if not answer["started"] or answer["state"] != "active":
        return "vtium's unit did not start again: its state is %s" % answer["state"]
    if answer["after"] is None:
        return "vtium's unit runs no process after its start"
    if answer["after"] == answer["before"]:
        return "vtium's PID is %d before and after, so the agent started no new vtium" % (
            answer["after"])
    return None


def run(vm, work, caps, errors, log, phase):
    """The two cases of builtin-font, run on a vtium that shows its login
    prompt: the agent's `bare`, then the new vtium's login prompt
    (LOGIN), then its notice of a kernel message (NOTICE). A step that
    fails fails the cases after it with its error."""
    path = os.path.join(work, LOGIN + ".ppm")
    try:
        with open(vm.serial, "rb") as f:
            since = len(f.read())
        lines = vm.ask("bare", timeout=BARE_TIMEOUT)
        log("bare: %s" % "; ".join(lines))
        why = bare_verdict(parse_bare(lines))
        if why:
            raise RuntimeError(why)
        vm.wait_serial(DISPLAY, timeout=60, since=since)
        wait_stable(vm, path, settle=2.0, timeout=90, want=not_blank)
        caps[LOGIN] = path
    except (RuntimeError, OSError) as e:
        log("%s: %s" % (LOGIN, e))
        for case in CASES:
            errors[case] = str(e)
        return
    phase(LOGIN)
    t3_seat.kernel_message(vm, work, caps, errors, log, case=NOTICE)
    phase(NOTICE)
