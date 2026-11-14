"""Tier 3's display hand-over (ADR-1401 D16): a program in vtium's console
hands the display to a graphical program and takes it back, with no VT
switch.

The variant hand-over runs the minspec tree in direct mode on the minspec
machine, logs in, draws its screens and switches VTs as 2d-potato-min does,
and is judged by its goldens (t3_evaluate.GOLDENS_OF). Then its check
`hand-over` types `t3 hand-over` in the logged-in shell. The script
(guest/screens/hand-over.sh) turns on focus reporting (mode 1004) and sends
setBackground, so vtium lets go of the card. The agent (guest/t3-agent),
which runs as root, then plays the graphical program with the probe
(tools/tier3/probes, `t3-probe take-master`): the probe opens the card,
becomes its master once vtium has let it go, draws one frame, and lets the
card go again. Once the probe ends, the agent makes the marker the script
waits for, and the script sends setForeground, so vtium takes the card back.

The check passes when the probe became the card's master and drew, the VT
shown stayed 2, the script received a focus-out (CSI O) and then a focus-in
(CSI I), vtium logged that it handed the display over and that the
hand-over ended, and vtium draws the console again: the screen after the
hand-over is not one color. The capture is that screen, judged by no golden.
"""

import os

import t3_rootchecks
from t3_qemu import not_blank, wait_stable

CASE = "hand-over"

CHECKS = {
    CASE: "A program in vtium's console sends setBackground, a graphical program run as root "
          "takes the card and lets it go, and the program sends setForeground: with no VT "
          "switch, vtium sends the program a focus-out and then a focus-in, and draws the "
          "console again (ADR-1401 D5 and D16)",
}

# The focus reports the script must receive, in order, as od prints them.
FOCUS_OUT = "1b5b4f"
FOCUS_IN = "1b5b49"

# vtium's log lines at the hand-over and at its end (src/app/hand_over.rs).
HANDED = "so vtium hands the display to a graphical program"
ENDED = "so the hand-over of the display ends"


def build_probe():
    """Builds t3-probe, as tier3-rootchecks does, and returns its path."""
    return t3_rootchecks.build_probe(lambda _line: None)


def probe_verdict(answer):
    """Why the agent's answer to `hand-over` shows no graphical program that
    held the card, or that the VT changed, or None."""
    lines = [line.partition("ROOTCHECK hand-over ")[2] or line for line in answer]
    if not any(line.startswith("master: yes") for line in lines):
        return "the probe never became the card's master: %s" % "; ".join(lines)
    if not any(line.startswith("drew: connector") for line in lines):
        return "the probe could not draw on the card: %s" % "; ".join(lines)
    if "shown 2" not in lines:
        return "the VT shown changed during the hand-over: %s" % "; ".join(lines)
    return None


def focus_verdict(line):
    """Why the script's T3-FOCUS line shows other reports than a focus-out
    and then a focus-in, or None."""
    words = line.split()
    got = words[1:3] if len(words) >= 3 else words[1:]
    if got != [FOCUS_OUT, FOCUS_IN]:
        return ("the session received %s, not a focus-out (%s) and then a focus-in (%s)"
                % (" then ".join(got) or "nothing", FOCUS_OUT, FOCUS_IN))
    return None


def check_hand_over(v, vm, work, caps, errors, log):
    """The hand-over check (CHECKS)."""
    try:
        vm.type("t3 hand-over\n")
        vm.wait_event("T3-HANDOVER sent", timeout=30)
        vm.wait_serial(HANDED, timeout=30)
        answer = vm.ask(CASE, timeout=60)
        for line in answer:
            log("%s: %s" % (CASE, line))
        why = probe_verdict(answer)
        line = vm.wait_event("T3-FOCUS", timeout=60)
        log("%s: %s" % (CASE, line))
        why = why or focus_verdict(line[line.index("T3-FOCUS"):])
        vm.wait_event("T3-DONE hand-over", timeout=30)
        vm.wait_serial(ENDED, timeout=30)
        path = os.path.join(work, CASE + ".ppm")
        wait_stable(vm, path, settle=2.0, timeout=60, want=not_blank)
    except RuntimeError as e:
        errors[CASE] = str(e)
        return
    if why:
        errors[CASE] = why
        return
    caps[CASE] = path


CHECK_RUNS = {CASE: check_hand_over}
