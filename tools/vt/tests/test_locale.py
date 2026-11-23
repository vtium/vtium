#!/usr/bin/env python3
"""Tests the locale mode scripts in tools/vt/locale. Each script is sourced by
a shell on a pseudo-terminal that answers as vtium does, and runs the real
vtium-config, which switches the font set with OSC 50 and checks the font
set's name against the catalog in crates/vtium-config/frits. The tests check
what each script prints, that it sets LANG and the font set and changes
nothing else in the shell or on the terminal, and that it changes nothing at
all when vtium does not take the font set.

The tests need the vtium-config binary. VTIUM_CONFIG_BIN names it, and
otherwise the tests take target/debug/vtium-config, which
`cargo build -p vtium-config --bin vtium-config` builds.

    python3 -I -B tools/vt/tests/test_locale.py [-v] [TestCase.test_name ...]

References:
- ADR-1511 D3: what a locale mode script switches, and that one of the
  languages is written right to left.
"""

import os
import pty
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

# The longest any test waits on another process. It only makes a broken test fail instead of hanging.
WAIT_BOUND = 60

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
LOCALE = os.path.join(ROOT, "tools", "vt", "locale")
CATALOG = os.path.join(ROOT, "crates", "vtium-config", "frits")
VTIUM_CONFIG = os.environ.get("VTIUM_CONFIG_BIN") or os.path.join(
    os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target"), "debug", "vtium-config")

# Each script: the language, the font set from vtium's catalog that covers its
# script, LANG, and the xkb layout the script names, which vtium cannot switch
# while it runs. Arabic and Hebrew are written right to left.
SCRIPTS = {
    "de.sh": ("German", "terminus", "de_DE.UTF-8", "de"),
    "ja.sh": ("Japanese", "noto-cjk-jp", "ja_JP.UTF-8", "jp"),
    "hi.sh": ("Hindi", "noto-indic", "hi_IN.UTF-8", "in"),
    "ar.sh": ("Arabic", "noto-arabic", "ar_EG.UTF-8", "ara"),
    "he.sh": ("Hebrew", "noto-hebrew", "he_IL.UTF-8", "il"),
}
RIGHT_TO_LEFT = {"ar.sh", "he.sh"}

# The shells a script is sourced in: sh always, and the others where this
# machine has them. busybox's ash is the demo image's shell.
SHELLS = [s for s in (["sh"], ["bash"], ["zsh"], ["dash"], ["busybox", "ash"]) if shutil.which(s[0])]

# What the shell records before and after it sources a script: its exported
# variables, its options, aliases and traps, its directory and umask, and
# every shell variable and function that `set` lists. Each variable that a
# shell changes by itself between two reads is left out.
SNAP = r"""
snap() {
	echo '## env'; env | LC_ALL=C sort | grep -v '^_='
	echo '## options'; set -o
	echo '## aliases'; alias
	echo '## traps'; trap
	echo '## cwd'; pwd
	echo '## umask'; umask
	echo '## vars'; set | grep -v -E '^(_|RANDOM|SECONDS|EPOCHSECONDS|EPOCHREALTIME|LINENO|BASH_COMMAND)='
}
"""

# The queries vtium-config sends to learn that the terminal is vtium and
# which font it shows. Anything else it sends is a switch.
XTVERSION = b"\x1b[>q"
DA1 = b"\x1b[c"
FONT_QUERY = re.compile(rb"\x1b\]50;\?(?:\x07|\x1b\\)")
FONT_SWITCH = re.compile(rb"\x1b\]50;([^?\x07\x1b][^\x07\x1b]*)(?:\x07|\x1b\\)")


def expected_output(script):
    language, frit, lang, layout = SCRIPTS[script]
    return (f"set\t{frit}\n"
            f"{language} mode: LANG is {lang}, and the font set is {frit}.\n"
            "The keyboard layout stays as it was, because vtium sets the layout when it starts.\n"
            f"For the {language} layout, set xkb-layout = {layout} in vtium.conf and start vtium again.\n")


class Terminal:
    """A shell on a pseudo-terminal that answers as vtium does: XTVERSION
    with vtium's name when VTIUM, DA1 always, and OSC 50's query with the
    font in use. An OSC 50 switch to a name in TAKES changes the font in use,
    and a switch to any other name leaves it as it was."""

    def __init__(self, argv, env, vtium=True, takes=None, font="unifont"):
        path = shutil.which(argv[0])
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(path, argv, env)
        self.vtium = vtium
        self.takes = takes
        self.font = font
        self.seen = b""
        self.answered = 0

    def answer(self):
        while True:
            rest = self.seen[self.answered:]
            found = []
            for q in (XTVERSION, DA1):
                at = rest.find(q)
                if at >= 0:
                    found.append((at, at + len(q), q))
            for pattern in (FONT_QUERY, FONT_SWITCH):
                m = pattern.search(rest)
                if m:
                    found.append((m.start(), m.end(), m))
            if not found:
                return
            _, end, what = min(found, key=lambda f: f[0])
            self.answered += end
            if what == XTVERSION:
                if self.vtium:
                    os.write(self.fd, b"\x1bP>|vtium(0.1.0)\x1b\\")
            elif what == DA1:
                os.write(self.fd, b"\x1b[?62;22c")
            elif what.re is FONT_QUERY:
                os.write(self.fd, b"\x1b]50;" + self.font.encode() + b"\x1b\\")
            else:
                name = what.group(1).decode()
                if self.takes is None or name in self.takes:
                    self.font = name

    def finish(self):
        end = time.monotonic() + WAIT_BOUND
        while time.monotonic() < end:
            if select.select([self.fd], [], [], 0.05)[0]:
                try:
                    data = os.read(self.fd, 4096)
                except OSError:
                    data = b""
                self.seen += data
                self.answer()
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                return os.waitstatus_to_exitcode(status)
        os.kill(self.pid, signal.SIGKILL)
        os.waitpid(self.pid, 0)
        raise AssertionError(f"the shell did not exit; the terminal saw {self.seen!r}")

    def switches(self):
        """The font names of the OSC 50 switches the terminal received."""
        return [m.group(1).decode() for m in FONT_SWITCH.finditer(self.seen)]

    def others(self):
        """What the terminal received besides vtium-config's queries and its
        switches."""
        rest = FONT_SWITCH.sub(b"", FONT_QUERY.sub(b"", self.seen))
        return rest.replace(XTVERSION, b"").replace(DA1, b"")


class Case(unittest.TestCase):
    def setUp(self):
        if not os.access(VTIUM_CONFIG, os.X_OK):
            self.fail(f"no vtium-config at {VTIUM_CONFIG}: build it with cargo build -p vtium-config --bin vtium-config, or name it with VTIUM_CONFIG_BIN")
        self.tmp = tempfile.mkdtemp(prefix="vtium-locale-")
        # vtium-config on PATH reads the catalog of the checkout, as an
        # installed one reads the frits directory that vtium.conf names.
        bin_dir = os.path.join(self.tmp, "bin")
        os.mkdir(bin_dir)
        wrapper = os.path.join(bin_dir, "vtium-config")
        with open(wrapper, "w", encoding="utf-8") as f:
            f.write(f"#!/bin/sh\nexec '{VTIUM_CONFIG}' --frits '{CATALOG}' \"$@\"\n")
        os.chmod(wrapper, 0o755)
        with open(os.path.join(self.tmp, "snap.sh"), "w", encoding="utf-8") as f:
            f.write(SNAP)
        self.env = {"PATH": bin_dir + ":/usr/bin:/bin", "HOME": self.tmp, "TERM": "xterm-vtium", "LANG": "C.UTF-8"}

    def tearDown(self):
        subprocess.run(["rm", "-rf", "--", self.tmp], check=True)

    def source(self, script, shell=("sh",), env=None, **terminal):
        """Sources SCRIPT in SHELL on a Terminal. Returns the exit status of
        the `.` command, what the script printed on its standard output and
        on its standard error, the shell's state before and after, and the
        Terminal."""
        t = self.tmp
        command = (f". '{t}/snap.sh'; snap >'{t}/before'; "
                   f". '{os.path.join(LOCALE, script)}' >'{t}/out' 2>'{t}/err'; "
                   f"echo $? >'{t}/status'; snap >'{t}/after'")
        term = Terminal([*shell, "-c", command], {**self.env, **(env or {})}, **terminal)
        self.assertEqual(term.finish(), 0)

        def read(name):
            with open(os.path.join(t, name), encoding="utf-8") as f:
                return f.read()
        return int(read("status")), read("out"), read("err"), read("before"), read("after"), term

    def changed(self, before, after):
        """The lines of AFTER that BEFORE lacks, and those it lost."""
        b, a = before.splitlines(), after.splitlines()
        return sorted(set(a) - set(b)), sorted(set(b) - set(a))

    def test_the_scripts_are_the_planned_set(self):
        self.assertEqual(sorted(os.listdir(LOCALE)), sorted(SCRIPTS))
        self.assertTrue(RIGHT_TO_LEFT <= set(SCRIPTS))

    def test_each_script_switches_the_font_set_and_lang_and_nothing_else(self):
        for shell in SHELLS:
            for script, (_, frit, lang, _) in SCRIPTS.items():
                with self.subTest(shell=shell[-1], script=script):
                    status, out, err, before, after, term = self.source(script, shell)
                    self.assertEqual((status, err), (0, ""))
                    self.assertEqual(out, expected_output(script))
                    self.assertEqual(term.switches(), [frit])
                    self.assertEqual(term.font, frit)
                    self.assertEqual(term.others(), b"")
                    added, lost = self.changed(before, after)
                    self.assertEqual([line for line in lost if "LANG" not in line], [])
                    self.assertEqual([line for line in added if "LANG" not in line], [])
                    self.assertIn(f"LANG={lang}", added)
                    self.assertIn("LANG=C.UTF-8", lost)

    def test_lang_stays_when_vtium_keeps_its_font(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                status, out, err, before, after, term = self.source(script, takes=())
                self.assertEqual(status, 1)
                self.assertEqual(before, after)
                self.assertEqual(term.font, "unifont")
                self.assertRegex(out, r"^failed\t")
                language = SCRIPTS[script][0]
                self.assertEqual(err, f"{language} mode failed: vtium did not switch to the font set {SCRIPTS[script][1]}, so LANG stays as it was.\n")

    def test_lang_stays_and_nothing_is_switched_in_another_terminal(self):
        status, out, err, before, after, term = self.source("ja.sh", vtium=False)
        self.assertEqual(status, 1)
        self.assertEqual(before, after)
        self.assertEqual(term.switches(), [])
        self.assertIn("not a vtium console", out)
        self.assertIn("Japanese mode failed", err)

    def test_lang_stays_without_vtium_config(self):
        # A PATH with only the commands the snapshot runs, and no vtium-config.
        bare = os.path.join(self.tmp, "bare")
        os.mkdir(bare)
        for tool in ("env", "sort", "grep"):
            os.symlink(shutil.which(tool), os.path.join(bare, tool))
        status, _, err, before, after, term = self.source("hi.sh", env={"PATH": bare})
        self.assertEqual(status, 1)
        self.assertEqual(before, after)
        self.assertEqual(term.seen, b"")
        self.assertTrue(err.endswith("Hindi mode failed: vtium did not switch to the font set noto-indic, so LANG stays as it was.\n"), err)

    def test_a_set_lc_all_is_named_and_left_as_it_is(self):
        status, out, _, before, after, _ = self.source("he.sh", env={"LC_ALL": "C"})
        self.assertEqual(status, 0)
        self.assertEqual(out, expected_output("he.sh") + "LC_ALL is set, and it overrides LANG. Unset LC_ALL to use the Hebrew locale.\n")
        self.assertIn("LC_ALL=C", after)
        added, _ = self.changed(before, after)
        self.assertEqual([line for line in added if "LANG" not in line], [])


if __name__ == "__main__":
    unittest.main()
