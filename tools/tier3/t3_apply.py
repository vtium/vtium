"""Tier 3's check of vtium-config's privileged step, run as root.

`vtium-config apply DIR` is the step that installs an install bundle into
the system's directories, run as root through sudo. Its check copies each
font set the bundle writes into a private folder under /run, and it writes
font sets only into a frits' directory that passes the console's own
ownership walk. Unit tests run apply as an ordinary user, so only a guest
shows the root path.

The variant apply-root runs on the minspec machine with the minimal
configuration, as 2d-potato-min does. Once vtium shows its login prompt,
the agent's request `apply-root` (guest/t3-agent) builds a bundle in a
folder of root's own. Its MANIFEST writes the new font set
fonts/t3-apply.vtfrit, the built-in Unscii-8 alone in its 8x8 cell, and a
vtium.conf that names the set at its top level, with the installed
vtium.conf's SHA-256. The agent runs /usr/bin/vtium-config apply on it with
no options, so the targets are the system's. apply's check runs
/usr/bin/vtium, and root may not name another console, so for this one
install the agent moves the console aside and puts a small program in its
place. The program writes down each copy of a font set the check is given,
with the owner and mode of the folder that holds the copy, and then runs the
console with the same arguments. The agent then puts the console back.
Next it builds a second bundle for the set t3-open, opens the frits'
directory to every user with chmod 0777, runs apply again, and gives the
directory its mode back. Last it stops vtium's unit on tty2 and starts it
again, so the new vtium reads the new vtium.conf.

The check passes when the first apply exits 0 and wrote both files, the set
is root's with mode 644, and vtium.conf names it. Its check must have read
one copy of the set, in a folder /run/vtium-config-apply-* that is root's
with mode 700, and /run must hold none of apply's private folders
afterwards. So the check tells a folder made under /run and removed apart
from one never made there. The console must be back as it was. The second
apply must exit with a failure in the walk's words, say that nothing was
installed, write neither file and leave nothing in /run, and the directory
must have its mode back.

The new vtium must log that its font is the frit t3-apply, that its cell
is 8x8, and that the font helper generated the set's glyphs at 8x8. It must
log no warning about the frit and no failure of the font helper, which
would leave it drawing its built-in font. Last it must draw a screen that
is not blank. The capture is that screen, judged by no golden.

References:
- ADR-1403 D8, "What `install` does": apply's check reads root's copies of the bundle's font sets from a private folder under /run, and apply checks the frits' directory with the console's walk before it writes there.
- ADR-1401 D10: the ownership check that every installed font set and the folders above it pass.
"""

import os
import re

from t3_qemu import not_blank, wait_stable

CASE = "apply-root"

CHECKS = {
    CASE: "As root, vtium-config apply installs a bundle that writes the font set t3-apply and "
          "a vtium.conf naming it: the set is root's with mode 644, no private folder of "
          "apply's is left in /run, and vtium started again draws with the set. A second "
          "bundle into the frits' directory at mode 0777 is refused, writes nothing and "
          "leaves nothing in /run (ADR-1403 D8)",
}

# How long the agent may take for both installs, each of which runs the
# console's check, and for stopping and starting vtium's unit.
APPLY_TIMEOUT = 180
# The line the new vtium logs once it has chosen the installed set
# (crates/vtium/src/app.rs).
FONT_LINE = "the font is the frit t3-apply"
# The line the new vtium logs once the font helper has generated the set's
# glyphs in the set's own 8x8 cell (crates/vtium/src/helper/answers.rs), and the
# cell its display line names (crates/vtium/src/app.rs).
GLYPHS_LINE = "the font's glyphs at 8x8 are generated"
CELL = "cell size 8x8"
# The start of the line vtium logs when the font helper cannot draw the set
# and vtium falls back to its built-in font (crates/vtium/src/helper/process.rs).
HELPER_FAILED = "the font helper could not draw the font"
# A copy of the set that apply's check reads: its path, and the owner and
# mode of the folder that holds it, as the agent's program writes them down.
CHECKED_COPY = re.compile(r"/run/vtium-config-apply-[^/ ]+/t3-apply\.vtfrit root 700")
# Terminal escape sequences, left out of the console log before it is read.
ESCAPES = re.compile(r"\x1b\[[0-9;?!]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
# The start of apply's refusal of a frits' directory the console would
# refuse, after the directory's path.
OPEN_REFUSAL = "so the install writes none there: other users can change"
# The frits' directory in the guest.
FRITS = "/etc/vtium/fonts"


def parse_apply(lines):
    """The agent's answer to `apply-root` as a dict: for each of the two
    runs, "apply" and "open", a dict of "said" (apply's output lines),
    "checked" (each copy apply's check read, as "PATH OWNER MODE"),
    "console" ("kept" when the console is back as it was), "status",
    "frit" ("OWNER MODE" or "none"), "left" and "conf" (counts),
    each None when the answer lacks it; then "was" and "mode" (the frits'
    directory's mode before and once restored), "before", "started", "state" and
    "after", as t3_builtin.parse_bare reads them."""
    runs = {k: {"said": [], "checked": [], "console": None, "status": None, "frit": None,
                "left": None, "conf": None}
            for k in ("apply", "open")}
    out = {"apply": runs["apply"], "open": runs["open"], "was": None, "mode": None, "before": None,
           "started": False, "state": None, "after": None}
    for line in lines:
        words = line.split()
        if len(words) >= 2 and words[0] in runs:
            run, key, rest = runs[words[0]], words[1], line.split(None, 2)[2:]
            rest = rest[0] if rest else ""
            if key == "said":
                run["said"].append(rest)
            elif key == "checked":
                run["checked"].append(rest)
            elif key in ("status", "left", "conf") and rest.isdigit():
                run[key] = int(rest)
            elif key in ("frit", "console"):
                run[key] = rest
            elif key in ("was", "mode") and words[0] == "open":
                out[key] = rest
        elif len(words) == 2 and words[0] == "before" and words[1].isdigit():
            out["before"] = int(words[1]) or None
        elif words[:1] == ["started"]:
            out["started"] = True
        elif len(words) == 3 and words[0] == "unit":
            out["state"] = words[2]
        elif len(words) == 3 and words[0] == "vtium" and words[1].isdigit():
            out["after"] = int(words[1])
    return out


def apply_verdict(answer):
    """Why the agent's parsed answer to `apply-root` (parse_apply) shows an
    install as root that failed or left something behind, a refusal that
    did not happen, or no new vtium, or None."""
    ok, bad = answer["apply"], answer["open"]
    said = "; ".join(ok["said"]) or "nothing"
    if ok["status"] != 0:
        return "apply as root exited %s: %s" % (ok["status"], said)
    lines = [" ".join(line.split()) for line in ok["said"]]
    for path in (FRITS + "/t3-apply.vtfrit", "/etc/vtium/vtium.conf"):
        if "wrote %s" % path not in lines:
            return "apply as root did not write %s: %s" % (path, said)
    if ok["frit"] != "root 644":
        return "the installed font set is %s, not root's with mode 644" % ok["frit"]
    if ok["conf"] != 1:
        return "the installed vtium.conf names t3-apply on %s lines, not 1" % ok["conf"]
    if len(ok["checked"]) != 1 or not CHECKED_COPY.fullmatch(ok["checked"][0]):
        return ("apply's check did not read one copy of t3-apply in a folder of root's "
                "under /run with mode 700: it read %s" % ("; ".join(ok["checked"]) or "none"))
    if ok["left"] != 0:
        return "apply as root left %s private folders in /run" % ok["left"]
    if ok["console"] != "kept":
        return "the agent did not put the console back as it was after apply: %s" % ok["console"]
    said = "; ".join(bad["said"]) or "nothing"
    if bad["status"] in (None, 0):
        return ("apply wrote into a frits' directory at mode 0777 (exit %s): %s"
                % (bad["status"], said))
    if not any(OPEN_REFUSAL in line for line in bad["said"]):
        return "apply's refusal of a frits' directory at mode 0777 is not the walk's: %s" % said
    if "Nothing was installed." not in bad["said"]:
        return "the refused install does not say that nothing was installed: %s" % said
    if bad["frit"] != "none":
        return "the refused install left the font set t3-open, %s" % bad["frit"]
    if bad["conf"] != 0:
        return "the refused install changed vtium.conf to name t3-open"
    if bad["left"] != 0:
        return "the refused install left %s private folders in /run" % bad["left"]
    if answer["was"] is None or answer["mode"] != answer["was"]:
        return "the frits' directory has mode %s afterwards, not its mode before, %s" % (
            answer["mode"], answer["was"])
    if not answer["started"] or answer["state"] != "active":
        return "vtium's unit did not start again: its state is %s" % answer["state"]
    if answer["after"] is None:
        return "vtium's unit runs no process after its start"
    if answer["after"] == answer["before"]:
        return "vtium's PID is %d before and after, so the agent started no new vtium" % (
            answer["after"])
    return None


def font_verdict(text, pid):
    """Why the console log `text`, from the agent's request on, shows that
    the new vtium, PID `pid`, does not draw with the set t3-apply in its
    8x8 cell, or None. Only that vtium's lines count. A line about the
    frit's text that the font helper sent, or one that says the font helper
    failed or restarts, fails the check, as does a line that says vtium
    draws its built-in font."""
    prefix = "vtium[%d]: " % pid
    said = [line.split(prefix, 1)[1].strip()
            for line in ESCAPES.sub("", text).splitlines() if prefix in line]
    for line in said:
        if (line.startswith("the frit ") or line.startswith("the font helper ")
                or "vtium draws its built-in font" in line
                or "vtium uses its built-in font" in line):
            return "the new vtium does not draw with the set t3-apply: %s" % line
    for want, what in ((FONT_LINE, "that its font is the frit t3-apply"),
                       (CELL, "a cell of 8x8"),
                       (GLYPHS_LINE, "that the font helper generated the set's glyphs at 8x8")):
        if not any(want in line for line in said):
            return "the new vtium, PID %d, logged no line saying %s" % (pid, what)
    return None


def check_apply_root(v, vm, work, caps, errors, log):
    """The apply-root check (CHECKS): the agent's `apply-root`, judged by
    apply_verdict, then the new vtium's lines that it draws with the set,
    judged by font_verdict, and its screen once settled."""
    path = os.path.join(work, CASE + ".ppm")
    try:
        with open(vm.serial, "rb") as f:
            since = len(f.read())
        answer = vm.ask(CASE, timeout=APPLY_TIMEOUT)
        for line in answer:
            log("%s: %s" % (CASE, line))
        parsed = parse_apply(answer)
        why = apply_verdict(parsed)
        if why:
            errors[CASE] = why
            return
        vm.wait_serial(FONT_LINE, timeout=60, since=since)
        vm.wait_serial(GLYPHS_LINE, timeout=60, since=since, fail=HELPER_FAILED)
        wait_stable(vm, path, settle=2.0, timeout=90, want=not_blank)
        with open(vm.serial, "rb") as f:
            f.seek(since)
            why = font_verdict(f.read().decode(errors="replace"), parsed["after"])
        if why:
            errors[CASE] = why
            return
    except (RuntimeError, OSError) as e:
        errors[CASE] = str(e)
        return
    caps[CASE] = path


CHECK_RUNS = {CASE: check_apply_root}
