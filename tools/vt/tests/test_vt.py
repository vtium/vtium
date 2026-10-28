#!/usr/bin/env python3
"""Tests the VT review tools (tools/vt, docs/vt-review.md) without a VT, sudo
or logind. Each case gets a temporary directory of stand-ins named by
VTIUM_VT_STANDINS: systemctl, loginctl, setpriv, getent and sudo record their
calls, "shells" stands for /etc/shells, chvt and fgconsole record that they were reached (no tool may switch a VT),
"active" stands for the VT on screen, and a stand-in vtium binary records its
arguments and exits, crashes, waits for a signal or runs its session. The
stand-ins also lead PATH, so a command the tools reach by name finds one.

    python3 -I tools/vt/tests/test_vt.py [-v] [TestCase.test_name ...]
"""

import fcntl
import os
import pty
import select
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
VT = os.path.join(ROOT, "tools", "vt")
RUN_VT = os.path.join(VT, "run-vt.sh")
VT_SH = os.path.join(VT, "vt.sh")
GUIDE_SH = os.path.join(VT, "checks", "guide.sh")
DEMO_SH = os.path.join(VT, "demo.sh")
FEEDBACK_SH = os.path.join(VT, "feedback.sh")
USER = subprocess.run(["id", "-un"], capture_output=True, text=True, check=True).stdout.strip()
# script(1) as run-vt.sh finds it, on the PATH it pins.
SCRIPT = next(os.path.join(d, "script") for d in ("/usr/sbin", "/usr/bin", "/sbin", "/bin")
              if os.access(os.path.join(d, "script"), os.X_OK))

# Each call, one line: the command's name and its arguments.
RECORD = 'printf "%s\\n" "${0##*/} $*" >>"$CALLS"\n'

SYSTEMCTL = "#!/bin/bash\n" + RECORD + """
# is-active --quiet UNIT succeeds for the units in $ACTIVE_UNITS.
if [ "$1" = is-active ]; then
	case " ${ACTIVE_UNITS-} " in *" $3 "*) exit 0 ;; esac
	exit 3
fi
[ "$1" != start ] || exit "${START_STATUS:-${SYSTEMCTL_STATUS:-0}}"
exit "${SYSTEMCTL_STATUS:-0}"
"""

# list-sessions call k prints word k of $SEQ (the last word past the end): -
# no session, F a failure, else sessions split at commas, each N (one on VT N,
# active), N:STATE, or either with ! (show-session fails for it). Each id
# carries its VT and state, which show-session reads back.
LOGINCTL = "#!/bin/bash\n" + RECORD + """
case $1 in
list-sessions)
	n=$(( $(cat "$COUNT" 2>/dev/null || echo 0) + 1 )); echo "$n" >"$COUNT"
	read -ra seq <<<"${SEQ:--}"; w=${seq[n - 1]:-${seq[-1]}}
	case $w in
	-) ;;
	F) echo "Failed to list sessions" >&2; exit 1 ;;
	*) IFS=, read -ra ss <<<"$w"
		for s in "${ss[@]}"; do
			bad=; [[ $s != *! ]] || bad=_x; s=${s%!}
			vt=${s%%:*}; st=active; [[ $s != *:* ]] || st=${s#*:}
			echo "c${vt}_${st}${bad} 1000 tester seat0"
		done ;;
	esac ;;
show-session)
	for id; do :; done; id=${id#c}
	[[ $id != *_x ]] || { echo "Failed to get session" >&2; exit 1; }
	case "$*" in
	*--value*VTNr* | *VTNr*--value*) echo "${id%%_*}" ;;
	*VTNr*) printf 'VTNr=%s\\nState=%s\\n' "${id%%_*}" "${id#*_}" ;;
	*) printf 'tester\\ntty\\n' ;;
	esac ;;
esac
"""

# Drops nothing: runs what follows --, as setpriv would after dropping to the
# user.
SETPRIV = "#!/bin/bash\n" + RECORD + """
while [ $# -gt 0 ] && [ "$1" != -- ]; do shift; done
shift
exec "$@"
"""

# passwd NAME prints $PASSWD when it is set (an empty one: no such user),
# else NAME's entry with the caller's uid and gid, $LOGIN_HOME and $LOGIN_SHELL.
GETENT = "#!/bin/bash\n" + RECORD + """
[ "$1" = passwd ] || exit 2
if [ -n "${PASSWD+x}" ]; then
	[ -n "$PASSWD" ] || exit 2
	printf '%s\\n' "$PASSWD"
	exit
fi
printf '%s:x:%s:%s::%s:%s\\n' "$2" "$(id -u)" "$(id -g)" "$LOGIN_HOME" "$LOGIN_SHELL"
"""

# The operator's login shell: prints how it was started, as whom and where,
# the environment the run gave it, then the line it reads, so a transcript
# shows each.
LOGIN_SHELL = """#!/bin/bash
printf 'login shell: %s as %s in %s\\n' "$*" "$(id -un)" "$PWD"
printf 'env: %s\\n' "LANG=${LANG-}" "LC_TIME=${LC_TIME-}" "XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR-}" \\
	"DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS-}"
read -r line || true
printf 'read: %s\\n' "$line"
"""

SUDO = "#!/bin/bash\n" + RECORD + """
[ "$1" != -v ] || exit "${SUDO_STATUS:-0}"
exec "$@"
"""

FORBIDDEN = "#!/bin/bash\n" + RECORD + "exit 1\n"

# The stand-in vtium: records its arguments, prints $BIN_LOG, then as
# $BIN_MODE says: exit (with $BIN_STATUS), wait (until a signal, recording
# it), or session (runs the argv after --, with $BIN_STDIN as its input).
BIN = """#!/bin/bash
echo $$ >"$BIN_PID"
# The run's lock is root's; vtium, and the login and shell it starts, never
# hold it.
for f in /proc/$$/fd/*; do
	case $(readlink "$f") in *vt.lock) echo "vtium inherited the lock" >&2; exit 97 ;; esac
done
printf '%s\\n' "$@" >"$BIN_ARGS"
printf '%b' "${BIN_LOG:-}"
case ${BIN_MODE:-exit} in
exit) exit "${BIN_STATUS:-0}" ;;
wait)
	trap 'echo TERM >>"$BIN_SIGNALS"; exit 0' TERM
	while :; do sleep 0.05; done ;;
session)
	while [ $# -gt 0 ] && [ "$1" != -- ]; do shift; done
	shift
	exec "$@" <"$BIN_STDIN" ;;
esac
"""


def write(path, text, mode=0o755):
    with open(path, "w") as f:
        f.write(text)
    os.chmod(path, mode)


def read(path):
    try:
        with open(path) as f:
            return f.read()
    except FileNotFoundError:
        return ""


class Stage:
    """A temporary directory of stand-ins, and the environment that names it."""

    def __init__(self, tmp):
        self.tmp = tmp
        self.standins = os.path.join(tmp, "standins")
        os.mkdir(self.standins)
        for name, text in (("systemctl", SYSTEMCTL), ("loginctl", LOGINCTL), ("setpriv", SETPRIV),
                           ("getent", GETENT), ("sudo", SUDO), ("cargo", "#!/bin/bash\n" + RECORD),
                           ("chvt", FORBIDDEN), ("fgconsole", FORBIDDEN)):
            write(os.path.join(self.standins, name), text)
        write(os.path.join(self.standins, "active"), "tty7\n", 0o644)
        self.home = os.path.join(tmp, "home")
        os.mkdir(self.home)
        self.login_shell = os.path.join(tmp, "login-shell")
        write(self.login_shell, LOGIN_SHELL)
        write(os.path.join(self.standins, "shells"),
              f"# Pathnames of valid login shells.\n/bin/sh\n/bin/bash\n{self.login_shell}\n", 0o644)
        self.bin = os.path.join(tmp, "vtium")
        write(self.bin, BIN)
        self.dir = os.path.join(tmp, "vt")
        self.calls = os.path.join(tmp, "calls")
        self.bin_args = os.path.join(tmp, "bin-args")
        self.signals = os.path.join(tmp, "signals")

    def env(self, **extra):
        env = dict(os.environ)
        for k in list(env):
            if k.startswith(("VTIUM_", "GUIDE", "DEMO_", "SUDO_", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS")):
                del env[k]
        env.update(
            PATH=self.standins + os.pathsep + env["PATH"],
            VTIUM_VT_STANDINS=self.standins,
            VTIUM_VT_DIR=self.dir,
            CALLS=self.calls,
            COUNT=os.path.join(self.tmp, "count"),
            BIN_ARGS=self.bin_args,
            BIN_SIGNALS=self.signals,
            BIN_PID=os.path.join(self.tmp, "bin-pid"),
            ACTIVE_UNITS="kmsconvt@tty8.service",
            LOGIN_HOME=self.home,
            LOGIN_SHELL="/bin/bash",
            SHELL="/bin/bash",
        )
        env.update(extra)
        return env

    def runtime_dir(self, mode=0o700, bus=True):
        """The stand-in /run/user/UID, with a socket for the user's bus."""
        run = os.path.join(self.standins, "run-user", str(os.getuid()))
        os.makedirs(run)
        os.chmod(run, mode)
        if bus:
            sock = socket.socket(socket.AF_UNIX)
            sock.bind(os.path.join(run, "bus"))
            sock.close()
        return run

    def locale(self, text):
        write(os.path.join(self.standins, "locale.conf"), text, 0o644)

    def active(self, tty):
        write(os.path.join(self.standins, "active"), tty + "\n", 0o644)

    def calls_of(self, name):
        return [l.split(" ", 1)[1] if " " in l else "" for l in read(self.calls).splitlines()
                if l.split(" ", 1)[0] == name]

    def changes(self):
        """systemctl's calls that change anything: stop and start."""
        return [c for c in self.calls_of("systemctl") if not c.startswith("is-active")]


class Case(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.s = Stage(self._tmp.name)

    def tearDown(self):
        # A vtium left running by a failed restore is stopped here, so it
        # outlives no test.
        pid = read(os.path.join(self.s.tmp, "bin-pid")).strip()
        if pid:
            try:
                os.kill(int(pid), signal.SIGKILL)
            except ProcessLookupError:
                pass
        # No path of any tool switches a VT.
        self.assertEqual(self.s.calls_of("chvt") + self.s.calls_of("fgconsole"), [])
        self._tmp.cleanup()

    def run_vt(self, *args, timeout=20, **env):
        return subprocess.run(["bash", RUN_VT, f"--bin={self.s.bin}", *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=timeout, cwd=self.s.tmp,
                              start_new_session=True)

    def vt_sh(self, *args, timeout=20, **env):
        return subprocess.run(["bash", VT_SH, *args], env=self.s.env(VTIUM_VT_BIN=self.s.bin, **env),
                              capture_output=True, text=True, timeout=timeout, cwd=self.s.tmp,
                              start_new_session=True)

    def refused(self, p, why):
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(why, p.stderr)


class RunVt(Case):
    """run-vt.sh, the part that runs as root, in its test mode."""

    def test_dry_run_prints_the_changes_and_makes_none(self):
        p = self.run_vt("--dry-run", "8", "--", "--font-size=30")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("+ " + os.path.join(self.s.standins, "systemctl") + " stop kmsconvt@tty8.service", p.stdout)
        self.assertIn(f"+ {self.s.bin} --vt=tty8 --no-switchvt --configdir {VT}/config --oneshot --font-size=30 --login --",
                      p.stdout)
        self.assertIn(" start kmsconvt@tty8.service", p.stdout)
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args), "the dry run ran vtium")

    def test_a_vt_a_session_is_on_is_refused(self):
        for seq in ("8", "8:online", "5,8:closing"):
            with self.subTest(seq=seq):
                os.unlink(self.s.calls) if os.path.exists(self.s.calls) else None
                for path in (os.path.join(self.s.tmp, "count"),):
                    if os.path.exists(path):
                        os.unlink(path)
                p = self.run_vt("8", SEQ=seq)
                self.refused(p, "is on VT 8; pick another VT")
                self.assertEqual(self.s.changes(), [])
                self.assertFalse(os.path.exists(self.s.bin_args))

    def test_the_vt_on_screen_is_refused(self):
        self.s.active("tty8")
        p = self.run_vt("8")
        self.refused(p, "VT 8 is on screen now")
        self.assertEqual(self.s.changes(), [])

    def test_unknown_sessions_are_refused(self):
        self.refused(self.run_vt("8", SEQ="F"), "loginctl could not list the sessions")
        os.unlink(os.path.join(self.s.tmp, "count"))
        # Unreadable, and still in the second list: its VT is unknown.
        self.refused(self.run_vt("8", SEQ="5!"), "loginctl could not list the sessions")
        self.assertEqual(self.s.changes(), [])

    def test_a_session_gone_between_list_and_query_is_skipped(self):
        p = self.run_vt("8", SEQ="5! -")
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_a_session_on_another_vt_is_no_bar(self):
        p = self.run_vt("8", SEQ="2,7")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def test_the_unit_comes_back_after_a_normal_end(self):
        p = self.run_vt("8", ACTIVE_UNITS="getty@tty8.service autovt@tty8.service")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.s.changes(), ["stop getty@tty8.service autovt@tty8.service",
                                            "start getty@tty8.service autovt@tty8.service"])
        self.assertIn("manual restore: sudo systemctl start getty@tty8.service autovt@tty8.service", p.stdout)

    def test_the_unit_comes_back_after_a_crash(self):
        p = self.run_vt("8", BIN_STATUS="134")
        self.assertEqual(p.returncode, 134)
        self.assertIn("vtium exited with status 134", p.stderr)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def interrupted(self, sig, status):
        proc = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", "8"], env=self.s.env(BIN_MODE="wait"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp)
        end = time.monotonic() + 10
        while not os.path.exists(self.s.bin_args) and time.monotonic() < end:
            time.sleep(0.02)
        self.assertTrue(os.path.exists(self.s.bin_args), "vtium never started")
        proc.send_signal(sig)
        out, err = proc.communicate(timeout=15)
        self.assertEqual(proc.returncode, status, out + err)
        self.assertEqual(read(self.s.signals), "TERM\n", "vtium was not stopped with SIGTERM")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def test_the_unit_comes_back_when_stdout_is_closed(self):
        # The terminal the run was started from closes, then hangs up: every
        # write the restore makes fails.
        errors = open(os.path.join(self.s.tmp, "stderr"), "w")
        proc = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", "8"], env=self.s.env(BIN_MODE="wait"),
                                stdout=subprocess.PIPE, stderr=errors, cwd=self.s.tmp)
        errors.close()
        end = time.monotonic() + 10
        while not os.path.exists(self.s.bin_args) and time.monotonic() < end:
            time.sleep(0.02)
        self.assertTrue(os.path.exists(self.s.bin_args), "vtium never started")
        proc.stdout.close()
        proc.send_signal(signal.SIGHUP)
        proc.wait(timeout=15)
        err = read(os.path.join(self.s.tmp, "stderr"))
        self.assertEqual(proc.returncode, 129, err)
        self.assertEqual(read(self.s.signals), "TERM\n", "vtium was left running")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def test_one_run_at_a_time(self):
        first = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", "8"], env=self.s.env(BIN_MODE="wait"),
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=self.s.tmp)
        try:
            end = time.monotonic() + 10
            while not os.path.exists(self.s.bin_args) and time.monotonic() < end:
                time.sleep(0.02)
            os.unlink(self.s.bin_args)
            p = self.run_vt("9", ACTIVE_UNITS="getty@tty9.service")
            self.refused(p, "another VT review run is in progress")
            self.assertFalse(os.path.exists(self.s.bin_args), "a second vtium ran")
            self.assertNotIn("stop getty@tty9.service", self.s.changes())
        finally:
            first.send_signal(signal.SIGTERM)
            first.wait(timeout=15)
        # Released when the first run ends.
        self.assertEqual(self.run_vt("9").returncode, 0)

    def test_the_unit_comes_back_after_ctrl_c(self):
        self.interrupted(signal.SIGINT, 130)

    def test_the_unit_comes_back_after_sigterm(self):
        self.interrupted(signal.SIGTERM, 143)

    def test_the_unit_comes_back_after_a_hangup(self):
        self.interrupted(signal.SIGHUP, 129)

    def test_no_active_unit_stops_and_starts_none(self):
        p = self.run_vt("8", ACTIVE_UNITS="")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.s.changes(), [])
        self.assertIn("no console unit was running on VT 8", p.stdout)

    def test_a_failed_restart_names_the_manual_restore(self):
        p = self.run_vt("8", SYSTEMCTL_STATUS="1")
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("restore it with: sudo systemctl start kmsconvt@tty8.service", p.stderr)
        # Only the start fails, after vtium ended well.
        p = self.run_vt("8", START_STATUS="1")
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("restore it with: sudo systemctl start kmsconvt@tty8.service", p.stderr)
        self.assertTrue(os.path.exists(self.s.bin_args))

    def test_the_vt_number(self):
        for vt in ("1", "13", "05", "3x"):
            with self.subTest(vt=vt):
                p = self.run_vt(vt)
                self.assertNotEqual(p.returncode, 0)
                self.assertEqual(self.s.changes(), [])

    def test_unsafe_vtium_options_are_refused(self):
        for opts in (["--login"], ["--lo"], ["--vt=tty3"], ["--configdir=/tmp"], ["--oneshot"],
                     ["--font-name", "x"], ["x"], ["--asciicast=/tmp/a"], ["--xkb-keymap=/etc/shadow"],
                     ["--seats=all"], ["--no-switchvt"], ["--login=/bin/sh"],
                     # Two allowed names joined by a space, as the lists hold them.
                     ["--sb-size palette=1"], ["--verbose debug"], ["--Mode=1"],
                     ["--grab-reboot=<Ctrl>x"], ["--grab-power-off=<Ctrl>x"]):
            with self.subTest(opts=opts):
                self.refused(self.run_vt("--dry-run", "8", "--", *opts), "is refused")
        for size in ("5G", "4097M", "4194305K", "4294967297", "-1", "1T"):
            with self.subTest(size=size):
                self.refused(self.run_vt("--dry-run", "8", "--", f"--image-storage-limit={size}"),
                             "takes auto or a size up to 4G")

    def test_the_operators_options_are_taken(self):
        opts = ["--font-name=Monoid Nerd Font Mono", "--font-size=30", "--mode=7680x2160",
                "--no-use-original-mode", "--hwaccel", "--image-storage-limit=4G",
                "--image-storage-limit=auto", "--sync-timeout=500", "--dpms-timeout=15",
                "--grab-paste=<Ctrl><Shift>v", "--pointer-hide-timeout=5",
                "--pointer-hide-while-typing", "--no-pointer-hide-while-typing",
                "--pointer-char=+"]
        p = self.run_vt("--dry-run", "8", "--", *opts)
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_the_binary_must_be_safe_to_run_as_root(self):
        os.chmod(self.s.bin, 0o775)
        self.refused(self.run_vt("8"), "writable by group or others")
        os.chmod(self.s.bin, 0o757)
        self.refused(self.run_vt("8"), "writable by group or others")
        os.chmod(self.s.bin, 0o755)
        p = subprocess.run(["bash", RUN_VT, "--bin=" + os.path.join(self.s.tmp, "none"), "8"],
                           env=self.s.env(), capture_output=True, text=True, timeout=20)
        self.refused(p, "no vtium binary at")
        p = subprocess.run(["bash", RUN_VT, "8"], env=self.s.env(), capture_output=True, text=True, timeout=20)
        self.refused(p, "--bin=PATH names the vtium binary")
        self.assertEqual(self.s.changes(), [])

    def test_path_is_pinned_before_any_command(self):
        # Without stand-ins, a command planted on the caller's PATH is never
        # run. VT 13 is refused before anything is asked of the system.
        plant = os.path.join(self.s.tmp, "plant")
        os.mkdir(plant)
        for name in ("readlink", "dirname", "id", "tr", "cat"):
            write(os.path.join(plant, name), "#!/bin/sh\necho planted >>" + os.path.join(self.s.tmp, "planted") + "\nexit 1\n")
        for standins in (False, True):
            with self.subTest(standins=standins):
                env = self.s.env(PATH=plant + os.pathsep + os.environ["PATH"])
                if not standins:
                    del env["VTIUM_VT_STANDINS"]
                p = subprocess.run(["bash", RUN_VT, f"--bin={self.s.bin}", "--dry-run", "13"], env=env,
                                   capture_output=True, text=True, timeout=20, start_new_session=True)
                self.refused(p, "VT must be a number from 2 to 12")
                self.assertFalse(os.path.exists(os.path.join(self.s.tmp, "planted")))

    def test_an_unreadable_vt_on_screen_is_refused(self):
        for text in ("", "ttyX", "tty", "7 8"):
            with self.subTest(active=text):
                self.s.active(text)
                self.refused(self.run_vt("8"), "the VT on screen is unknown")
        self.assertEqual(self.s.changes(), [])

    def test_the_label_takes_a_builds_characters_only(self):
        self.refused(self.run_vt("--label=a$(id)", "8"), "--label takes")
        self.refused(self.run_vt("--label=a\nb", "8"), "--label takes")

    def test_the_demo_runs_as_the_user_with_its_label(self):
        p = self.run_vt("--label=dev build abc", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        at = argv.index("--")
        uid, gid = os.getuid(), os.getgid()
        self.assertEqual(argv[at + 1:], [os.path.join(self.s.standins, "setpriv"), f"--reuid={uid}", f"--regid={gid}",
                                         "--init-groups", "--inh-caps=-all", "--reset-env", "--", "/usr/bin/env",
                                         "DEMO_LABEL=vtium dev build abc", "/bin/bash",
                                         os.path.join(VT, "demo.sh")])

    def test_a_guide_runs_as_the_user_with_its_vt_and_steps(self):
        guide = os.path.join(VT, "checks", "start.guide")
        p = self.run_vt(f"--guide={os.path.relpath(guide, self.s.tmp)}", "--steps=2,1", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertIn("--oneshot", argv)
        self.assertEqual(argv[argv.index("/usr/bin/env"):],
                         ["/usr/bin/env", "GUIDE_VT=8", "GUIDE_DESKTOP=7", "GUIDE_STEPS=2,1",
                          "/bin/bash",
                          os.path.join(VT, "checks", "guide.sh"), guide])
        for opts, why in ((["--steps=1;2"], "--steps takes step numbers"),
                          (["--steps=0"], "--steps takes step numbers"),
                          (["--login"], "with no login")):
            with self.subTest(opts=opts):
                self.refused(self.run_vt(f"--guide={guide}", *opts, "8"), why)
        self.refused(self.run_vt("--steps=1", "8"), "--steps goes with --guide")

    def test_a_login_ends_at_the_first_logout(self):
        p = self.run_vt("--login", "8", BIN_MODE="wait", SEQ="- - 8 8 8:closing", timeout=30)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("vt-login: session ended", p.stdout)
        self.assertEqual(read(self.s.signals), "TERM\n")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])
        argv = read(self.s.bin_args).splitlines()
        self.assertNotIn("--oneshot", argv)
        self.assertEqual(argv[argv.index("--"):], ["--", "/bin/login", "-p"])

    def test_a_login_vtium_ending_on_its_own_is_a_failure(self):
        p = self.run_vt("--login", "8", SEQ="-")
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("vtium exited with status 0", p.stderr)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])


    def test_a_shell_runs_as_the_user_with_the_shell_passwd_names(self):
        t = os.path.join(self.s.tmp, "shell.transcript")
        p = self.run_vt("--shell", f"--transcript={t}", "8", LOGIN_SHELL=self.s.login_shell)
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertIn("--oneshot", argv)
        uid, gid = os.getuid(), os.getgid()
        self.assertEqual(argv[argv.index("--") + 1:],
                         [os.path.join(self.s.standins, "setpriv"), f"--reuid={uid}", f"--regid={gid}", "--init-groups",
                          "--inh-caps=-all", "--reset-env", "--", "/usr/bin/env",
                          "-C", self.s.home, SCRIPT, "-q", "-f", "-c", f"exec {self.s.login_shell} -l", t])
        self.assertIn(f"with {USER}'s shell", p.stdout)
        self.assertEqual(self.s.calls_of("getent"), [f"passwd {USER}"])
        # Without a transcript, the shell is the session itself.
        p = self.run_vt("--shell", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertEqual(argv[argv.index("/usr/bin/env"):],
                         ["/usr/bin/env", "-C", self.s.home, "/bin/bash", "-l"])

    def shell_env(self, **env):
        """The words between the shell's env and the shell itself."""
        p = self.run_vt("--shell", "8", **env)
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        return argv[argv.index(self.s.home) + 1:-2]

    def test_a_shell_gets_the_locale_and_the_users_runtime_directory(self):
        self.assertEqual(self.shell_env(), [])
        self.s.locale('# The system locale.\nLANG=de_DE.UTF-8\nLC_TIME="en_GB.UTF-8"\nLC_PAPER=a b\n'
                      'LANGUAGE=de\nLC_MONETARY=$(id)\n')
        run = self.s.runtime_dir()
        self.assertEqual(self.shell_env(), ["LANG=de_DE.UTF-8", "LC_TIME=en_GB.UTF-8", f"XDG_RUNTIME_DIR={run}",
                                            f"DBUS_SESSION_BUS_ADDRESS=unix:path={run}/bus"])
        # No bus socket: the runtime directory alone.
        os.unlink(os.path.join(run, "bus"))
        write(os.path.join(run, "bus"), "", 0o600)
        self.assertEqual(self.shell_env()[2:], [f"XDG_RUNTIME_DIR={run}"])
        # A runtime directory open to others, or a symlink to one: neither.
        os.chmod(run, 0o755)
        self.assertEqual(self.shell_env()[2:], [])
        os.chmod(run, 0o700)
        os.rename(run, run + ".real")
        os.symlink(run + ".real", run)
        self.assertEqual(self.shell_env()[2:], [])

    def test_a_home_directory_not_the_users_is_refused(self):
        a_file = os.path.join(self.s.tmp, "a-file")
        write(a_file, "", 0o644)
        for home in (os.path.join(self.s.tmp, "none"), a_file, "/", "home"):
            with self.subTest(home=home):
                self.refused(self.run_vt("--shell", "8", LOGIN_HOME=home), "home directory")
                self.refused(self.run_vt("--shell", "--dry-run", "8", LOGIN_HOME=home), "home directory")
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_a_shell_not_in_etc_shells_is_refused(self):
        listed_missing = "/usr/bin/no-such-shell"
        with open(os.path.join(self.s.standins, "shells"), "a") as f:
            f.write(listed_missing + "\n/bin/sh;id\nbash\n")
        for shell, why in (("/bin/zsh-not-listed", "is not in " + os.path.join(self.s.standins, "shells")),
                           ("", "has no login shell"),
                           ("bash", "is not an absolute path"),
                           ("# Pathnames of valid login shells.", "is not an absolute path"),
                           ("/bin/sh;id", "is not an absolute path"),
                           (listed_missing, "is not an executable file")):
            with self.subTest(shell=shell):
                self.refused(self.run_vt("--shell", "8", LOGIN_SHELL=shell), why)
                self.refused(self.run_vt("--shell", "--dry-run", "8", LOGIN_SHELL=shell), why)
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_the_session_never_runs_as_root(self):
        for mode in (["--shell"], [], ["--login"]):
            with self.subTest(mode=mode):
                self.refused(self.run_vt(*mode, "8", SUDO_USER="root"), "never as root")
                self.refused(self.run_vt(*mode, "8", PASSWD="toor:x:0:0::/root:/bin/bash"), "never as root")
                self.refused(self.run_vt(*mode, "--dry-run", "8", PASSWD="toor:x:0:0::/root:/bin/bash"),
                             "never as root")
        self.refused(self.run_vt("--shell", "8", PASSWD=""), "no such user")
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_the_shell_takes_no_other_session(self):
        guide = os.path.join(VT, "checks", "start.guide")
        for opts, why in ((["--shell", "--login"], "--shell runs your shell"),
                          (["--shell", f"--guide={guide}"], "--shell runs your shell"),
                          (["--transcript=/tmp/x"], "--transcript goes with --shell")):
            with self.subTest(opts=opts):
                self.refused(self.run_vt(*opts, "8"), why)
        self.assertEqual(self.s.changes(), [])

    def test_a_transcript_the_user_cannot_write_is_refused(self):
        self.refused(self.run_vt("--shell", "--transcript=" + os.path.join(self.s.tmp, "none", "t"), "8"),
                     "cannot write the transcript")
        os.symlink("/etc/passwd", os.path.join(self.s.tmp, "link"))
        self.refused(self.run_vt("--shell", "--transcript=" + os.path.join(self.s.tmp, "link"), "8"),
                     "cannot write the transcript")
        self.assertEqual(self.s.changes(), [])


class VtSh(Case):
    """vt.sh, the desktop side behind the make targets."""

    def picked(self, p):
        line = next((l for l in p.stdout.splitlines() if l.startswith("+ ")), "")
        words = line.split()
        return words[words.index("--") - 1] if "--" in words else None

    def test_the_first_free_vt_is_picked(self):
        for active, seq, want in (("tty7", "-", "5"), ("tty5", "-", "6"), ("tty5", "6", "4"),
                                  ("tty7", "5,6,4", "3"), ("tty3", "5,6,4", "2"), ("tty1", "2,7", "5")):
            with self.subTest(active=active, seq=seq):
                self.s.active(active)
                if os.path.exists(os.path.join(self.s.tmp, "count")):
                    os.unlink(os.path.join(self.s.tmp, "count"))
                p = self.vt_sh(VTIUM_VT_DRY_RUN="1", SEQ=seq)
                self.assertEqual(self.picked(p), want, p.stdout + p.stderr)

    def test_every_vt_busy_names_each_and_why(self):
        self.s.active("tty5")
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", SEQ="6,4,3,2")
        self.refused(p, "no free VT")
        for want in ["VT 5: it is on screen"] + [f"VT {v}: logind session c{v}_active" for v in (6, 4, 3, 2)]:
            self.assertIn(want, p.stderr)

    def test_vt_given_is_refused_when_busy(self):
        self.refused(self.vt_sh("7"), "VT 7 refused: it is on screen")
        self.refused(self.vt_sh(SEQ="4", VTIUM_VT_NR="4"), "VT 4 refused: logind session c4_active")
        self.assertEqual(self.s.calls_of("sudo"), [])

    def test_demo_args_are_split_and_never_run(self):
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", VTIUM_VT_NR="4",
                       VTIUM_VT_ARGS="""--font-name="Monoid Nerd Font Mono" --x='a b' ; touch planted $HOME""")
        line = next(l for l in p.stdout.splitlines() if l.startswith("+ "))
        self.assertTrue(line.endswith("4 -- --font-name=Monoid\\ Nerd\\ Font\\ Mono --x=a\\ b \\; touch planted \\$HOME"),
                        line)
        self.assertFalse(os.path.exists(os.path.join(self.s.tmp, "planted")))
        # run-vt.sh's dry run, which follows, refuses the stray words.
        self.assertIn("is refused", p.stderr)
        self.refused(self.vt_sh(VTIUM_VT_ARGS='--font-name="Monoid'), "cannot be split")

    def test_a_dry_run_runs_no_sudo_and_changes_nothing(self):
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("Switch to VT 5 now: Ctrl-Alt-F5    (Ctrl-Alt-F7 returns to this desktop)", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_a_guide_that_fails_its_check_stops_before_sudo(self):
        bad = os.path.join(self.s.tmp, "bad.guide")
        for text in ("title: t\nstep: s\ndo: press Logo+Enter\nexpect: x\n", "step: s\ndo: x\nexpect: y\n",
                     "title: t\nstep: s\ndo: x\n"):
            with self.subTest(text=text):
                write(bad, text, 0o644)
                self.refused(self.vt_sh(VTIUM_VT_SESSION="guide", VTIUM_VT_GUIDE=bad), "is refused (above)")
        self.refused(self.vt_sh(VTIUM_VT_SESSION="guide", VTIUM_VT_GUIDE="start", VTIUM_VT_STEPS="3"),
                     "is refused (above)")
        self.refused(self.vt_sh(VTIUM_VT_SESSION="guide", VTIUM_VT_GUIDE="nonesuch"), "the guides in tools/vt/checks: blank start vt wave-a")
        self.refused(self.vt_sh(VTIUM_VT_SESSION="guide"), "GUIDE=NAME names the guide")
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])

    def test_a_guide_is_make_vt_checks(self):
        self.refused(self.vt_sh(VTIUM_VT_GUIDE="start"), "go with make vt-check")

    def test_the_log_directory_must_be_the_users_own(self):
        os.symlink(self.s.tmp, self.s.dir)
        self.refused(self.vt_sh(), "is a symlink, or not a directory of yours")
        p = self.vt_sh(VTIUM_VT_DIR="/usr")
        self.refused(p, "/usr is a symlink, or not a directory of yours")
        # Another user could read or write the answers in a directory open to them.
        open_dir = os.path.join(self.s.tmp, "open")
        os.mkdir(open_dir)
        os.chmod(open_dir, 0o755)
        self.refused(self.vt_sh(VTIUM_VT_DIR=open_dir), "is a symlink, or not a directory of yours")
        self.assertEqual(self.s.calls_of("sudo"), [])

    def test_a_crash_is_reported_and_the_unit_restored(self):
        p = self.vt_sh(BIN_STATUS="101", BIN_LOG="thread 'main' panicked at src/main.rs:1:1\\n",
                       ACTIVE_UNITS="kmsconvt@tty5.service")
        self.assertEqual(p.returncode, 101, p.stdout + p.stderr)
        self.assertIn("health: 1 'panicked at' lines in vtium's log: vtium panicked", p.stdout)
        self.assertIn("run-vt.sh exited with status 101", p.stderr)
        self.assertEqual(self.s.calls_of("sudo")[0], "-v")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])

    def test_ctrl_c_restores_the_unit_and_still_prints_the_summary(self):
        proc = subprocess.Popen(["bash", VT_SH], env=self.s.env(VTIUM_VT_BIN=self.s.bin, BIN_MODE="wait",
                                                                ACTIVE_UNITS="kmsconvt@tty5.service"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp,
                                start_new_session=True)
        end = time.monotonic() + 10
        while not os.path.exists(self.s.bin_args) and time.monotonic() < end:
            time.sleep(0.02)
        # Ctrl-C reaches the terminal's whole foreground process group.
        os.killpg(proc.pid, signal.SIGINT)
        out, err = proc.communicate(timeout=15)
        self.assertEqual(proc.returncode, 130, out + err)
        self.assertIn("vt: interrupted", err)
        self.assertIn("vtium log: ", out)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])

    def test_a_shell_dry_run_names_its_log_and_transcript(self):
        p = self.vt_sh(VTIUM_VT_SESSION="shell", VTIUM_VT_DRY_RUN="1")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        line = next(l for l in p.stdout.splitlines() if l.startswith("+ "))
        self.assertRegex(line, r" --shell --transcript=" + self.s.dir + r"/vt-[0-9T]+-tty5-XXXXXX\.transcript 5 --$")
        self.assertRegex(p.stdout, r"vtium's log would go to " + self.s.dir + r"/vt-[0-9T]+-tty5-XXXXXX\.log; "
                         r"the shell's transcript to " + self.s.dir + r"/vt-[0-9T]+-tty5-XXXXXX\.transcript")
        # run-vt.sh's dry run names the shell it would run.
        self.assertIn(f" {SCRIPT} -q -f -c exec\\ /bin/bash\\ -l {self.s.dir}/vt-", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_a_shell_ended_with_ctrl_c_or_a_crash_restores_the_unit(self):
        proc = subprocess.Popen(["bash", VT_SH], env=self.s.env(VTIUM_VT_BIN=self.s.bin, BIN_MODE="wait",
                                                                VTIUM_VT_SESSION="shell",
                                                                ACTIVE_UNITS="kmsconvt@tty5.service"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp,
                                start_new_session=True)
        end = time.monotonic() + 10
        while not os.path.exists(self.s.bin_args) and time.monotonic() < end:
            time.sleep(0.02)
        os.killpg(proc.pid, signal.SIGINT)
        out, err = proc.communicate(timeout=15)
        self.assertEqual(proc.returncode, 0, out + err)
        self.assertIn("vt-shell: ended with Ctrl-C", out)
        self.assertRegex(out, r"vtium log: " + self.s.dir + r"/vt-[0-9T]+-tty5-\w{6}\.log\n"
                         r"transcript: " + self.s.dir + r"/vt-[0-9T]+-tty5-\w{6}\.transcript\n")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])
        p = self.vt_sh(VTIUM_VT_SESSION="shell", BIN_STATUS="101", ACTIVE_UNITS="kmsconvt@tty5.service")
        self.assertEqual(p.returncode, 101, p.stdout + p.stderr)
        self.assertIn("transcript: " + self.s.dir, p.stdout)

    def test_the_start_guide_runs_and_its_log_reaches_the_caller(self):
        # Enter to begin; step 1: Enter, y, a note; step 2: Enter, the shell's
        # two lines, n and no note; Enter to end.
        answers = os.path.join(self.s.tmp, "answers")
        write(answers, "\n\ny" "all drawn\n" "\necho hello vtium\nexit\n" "n\n" "\n", 0o644)
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-check", "GUIDE=start",
                            f"VTIUM_BIN={self.s.bin}", "VT=6"],
                           env=self.s.env(BIN_MODE="session", BIN_STDIN=answers, ACTIVE_UNITS="getty@tty6.service"),
                           capture_output=True, text=True, timeout=30)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = read(os.path.join(self.s.dir, "vt-check.log"))
        self.assertRegex(log, r"=== vtium starts and runs a shell, .*, guide " + os.path.join(VT, "checks", "start.guide"))
        self.assertIn("-- step 1 of 2: The first screen\n  answer: yes  note: all drawn\n", log)
        self.assertIn("-- step 2 of 2: A shell prompt\n  answer: no\n=== done", log)
        # The caller prints what the run added, and where the log is.
        self.assertIn("  answer: yes  note: all drawn", p.stdout)
        self.assertIn("results: " + os.path.join(self.s.dir, "vt-check.log"), p.stdout)
        self.assertEqual(self.s.changes(), ["stop getty@tty6.service", "start getty@tty6.service"])

    def test_the_demo_runs_and_its_answers_reach_the_caller(self):
        # demo.sh reads its keys from the terminal make runs in.
        env = self.s.env(BIN_MODE="session", BIN_STDIN="/dev/null", TERM="linux", ACTIVE_UNITS="kmsconvt@tty5.service")
        d = Pty(["/usr/bin/env", "make", "--no-print-directory", "-C", ROOT, "vt-demo", f"VTIUM_BIN={self.s.bin}"], env)
        d.wait_for(b"Switch to VT 5 now")
        d.wait_for(b"press a key for the first section", timeout=20)
        d.send(b" ")
        d.page()
        d.wait_for(b"1 right  2 wrong")
        d.send(b"1")
        d.wait_for(b"q exit")
        d.send(b"q")
        d.wait_for(b"results: " + os.path.join(self.s.dir, "feedback.tsv").encode())
        self.assertEqual(d.finish(), 0)
        seen = d.seen.decode(errors="replace")
        self.assertRegex(seen, r"label +vtium dev build ")
        self.assertIn("10-sgr                    right", seen)
        self.assertEqual(self.s.calls_of("sudo")[0], "-v")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])

    def test_the_shell_runs_and_its_paths_reach_the_caller(self):
        typed = os.path.join(self.s.tmp, "typed")
        write(typed, "ls -l\n", 0o644)
        self.s.locale("LANG=de_DE.UTF-8\n")
        self.s.runtime_dir_path = self.s.runtime_dir()
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-shell", f"VTIUM_BIN={self.s.bin}", "VT=6"],
                           env=self.s.env(BIN_MODE="session", BIN_STDIN=typed, LOGIN_SHELL=self.s.login_shell,
                                          ACTIVE_UNITS="getty@tty6.service"),
                           capture_output=True, text=True, timeout=30)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = os.path.join(self.s.dir, "vt-") + r"[0-9T]+-tty6-\w{6}"
        self.assertRegex(p.stdout, r"health: 0 'panicked at' lines in vtium's log\n"
                                   f"vtium log: {log}\\.log\ntranscript: {log}\\.transcript\n")
        transcript = p.stdout.split("transcript: ")[1].split("\n")[0]
        self.assertEqual(stat.S_IMODE(os.stat(transcript).st_mode), 0o600)
        text = read(transcript).replace("\r", "")
        self.assertIn(f"login shell: -l as {USER} in {self.s.home}\n", text)
        self.assertIn("read: ls -l\n", text)
        self.assertIn("env: LANG=de_DE.UTF-8\n", text)
        self.assertIn(f"env: XDG_RUNTIME_DIR={self.s.runtime_dir_path}\n", text)
        self.assertEqual(self.s.calls_of("sudo")[0], "-v")
        self.assertEqual(self.s.changes(), ["stop getty@tty6.service", "start getty@tty6.service"])


class Make(Case):
    """The Makefile's guards, which act before anything is built."""

    def make(self, *args, **env):
        return subprocess.run(["make", "--no-print-directory", "-C", ROOT, *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=30, start_new_session=True)

    def test_help_lists_the_targets(self):
        p = self.make("help")
        for target in ("minspec", "test-tools", "vt-demo", "vt-login", "vt-check", "vt-shell"):
            self.assertIn(target, p.stdout)

    def test_the_binary_is_never_taken_from_the_environment(self):
        p = self.make("vt-demo", "DRY_RUN=1", VTIUM_BIN=self.s.bin)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_BIN is set in the environment", p.stderr)
        self.assertEqual(self.s.calls_of("sudo") + self.s.calls_of("loginctl"), [])

    def test_the_default_binary_is_the_minspec_build(self):
        # VTIUM_MINSPEC points the default at the stand-in, which the stand-in
        # cargo "builds".
        p = self.make("vt-demo", "DRY_RUN=1", f"VTIUM_MINSPEC={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), ["build --profile minspec -p vtium"])
        self.assertIn(f"--bin={self.s.bin} ", p.stdout)

    def test_another_binary_is_run_as_it_is(self):
        p = self.make("vt-demo", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), [])

    def test_vt_check_without_a_guide_fails_before_the_build(self):
        p = self.make("vt-check")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("vt-check: GUIDE=NAME, a guide in tools/vt/checks: blank start vt wave-a.", p.stderr)
        self.assertNotIn("cargo", p.stdout)

    def test_a_guide_reaches_only_vt_check(self):
        # One exported in the shell does not turn vt-demo into a guided run.
        p = self.make("vt-demo", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", GUIDE="start", GUIDE_STEPS="9")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertNotIn("--guide", p.stdout)
        p = self.make("vt-check", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", "GUIDE=start", "GUIDE_STEPS=2")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"--guide={os.path.join(VT, 'checks', 'start.guide')} --steps=2 5 --", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])


class Pty:
    """A program on a pseudo-terminal, read up to each prompt before it is
    answered."""

    def __init__(self, argv, env):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(argv[0], argv, env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        # out is what is left after the last wait_for; seen is everything.
        self.out = b""
        self.seen = b""

    def pump(self, timeout):
        if select.select([self.fd], [], [], max(timeout, 0))[0]:
            try:
                data = os.read(self.fd, 4096)
            except OSError:
                data = b""
            if not data:
                return False
            self.out += data
            self.seen += data
        return True

    def wait_for(self, text, timeout=10):
        end = time.monotonic() + timeout
        while text not in self.out:
            if time.monotonic() > end or not self.pump(end - time.monotonic()):
                raise AssertionError(f"no {text!r} in:\n{self.out[-2000:].decode(errors='replace')}")
        self.out = self.out[self.out.index(text) + len(text):]

    def send(self, data):
        os.write(self.fd, data)

    def page(self):
        """Waits for demo.sh's cursor position query and answers it with a
        row the footer fits below."""
        self.wait_for(b"\x1b[6n")
        self.send(b"\x1b[14;1R")

    def finish(self, timeout=10):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                while self.pump(0.05):
                    pass
                return os.waitstatus_to_exitcode(status)
            self.pump(0.05)
        os.kill(self.pid, signal.SIGKILL)
        os.waitpid(self.pid, 0)
        raise AssertionError("did not exit")


class Guide(Case):
    """guide.sh: the check of a guide, and a run on a pseudo-terminal."""

    def check(self, text, steps=""):
        path = os.path.join(self.s.tmp, "t.guide")
        write(path, text, 0o644)
        return subprocess.run(["bash", GUIDE_SH, "--check", path], env=self.s.env(GUIDE_STEPS=steps),
                              capture_output=True, text=True, timeout=10)

    def test_every_guide_in_the_tree_passes_its_check(self):
        checks = os.path.join(VT, "checks")
        guides = sorted(f for f in os.listdir(checks) if f.endswith(".guide"))
        self.assertIn("start.guide", guides)
        for g in guides:
            with self.subTest(guide=g):
                p = subprocess.run(["bash", GUIDE_SH, "--check", os.path.join(checks, g)],
                                   capture_output=True, text=True, timeout=10)
                self.assertEqual(p.returncode, 0, p.stderr)

    def test_broken_guides_are_refused(self):
        ok = "title: T\nstep: S\ndo: d\nexpect: e\n"
        for text, why in ((ok.replace("do: d", "do: hold Logo"), "name the Super (Windows) key, not Logo"),
                          (ok.replace("title: T\n", ""), "no title"),
                          ("title: T\n", "no steps"),
                          (ok.replace("expect: e\n", ""), "needs an expect: line, and a do: or run: line"),
                          (ok.replace("do: d\n", ""), "needs an expect: line, and a do: or run: line"),
                          (ok + "run:\n", "run: needs the command"),
                          (ok + "run: true\nrun: false\n", "one run per step"),
                          (ok + "run: true\nprobe: shell\n", "runs a command or a probe, not both"),
                          (ok + "probe: shell\nrun: true\n", "runs a command or a probe, not both"),
                          (ok + "probe: nonesuch\n", "probe mode 'nonesuch' is not one of: shell focus"),
                          (ok + "probe: shell\nprobe: focus\n", "one probe per step"),
                          (ok + "intro: late\n", "intro lines go before the first step"),
                          (ok + "color: red\n", "unknown key 'color'"),
                          (ok + "not a key line\n", "not a 'key: text' line")):
            with self.subTest(why=why):
                p = self.check(text)
                self.refused(p, why)
        self.refused(self.check(ok, steps="2"), "'2' is not a step of this guide (1 to 1)")
        self.assertEqual(self.check(ok, steps="1").returncode, 0)
        # A step whose command is all it needs, with no do: line.
        # The Logo rule reads the prose, not a command's words.
        p = self.check("title: T\nstep: S\nrun: ls /usr/share/logo\nexpect: e\nstep: U\ndo: d\nexpect: e\n")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(p.stdout, "T: 2 steps\n   1  run          S\n   2  (no probe)   U\n")

    def guide_env(self, **extra):
        env = {"PATH": os.environ["PATH"], "TERM": "linux", "VTIUM_VT_DIR": self.s.dir,
               "GUIDE_VT": "6", "GUIDE_DESKTOP": "7"}
        env.update(extra)
        return env

    def test_the_answers_log_refuses_a_directory_not_the_users_own(self):
        guide = os.path.join(VT, "checks", "start.guide")
        os.symlink(self.s.tmp, self.s.dir)
        open_dir = os.path.join(self.s.tmp, "open")
        os.mkdir(open_dir)
        os.chmod(open_dir, 0o750)
        for env, why in ((self.guide_env(), b"is a symlink, or not a directory of yours"),
                         (self.guide_env(VTIUM_VT_DIR="/usr"), b"/usr is a symlink, or not a directory of yours"),
                         (self.guide_env(VTIUM_VT_DIR=open_dir), b"open is a symlink, or not a directory of yours")):
            with self.subTest(dir=env["VTIUM_VT_DIR"]):
                g = Pty(["/bin/bash", GUIDE_SH, guide], env)
                g.wait_for(why)
                g.wait_for(b"Press Enter to end.")
                g.send(b"\r")
                self.assertEqual(g.finish(), 1)
        self.assertEqual(sorted(os.listdir(self.s.tmp)), sorted(["standins", "vtium", "vt", "open", "home", "login-shell"]))
        self.assertEqual(os.listdir(open_dir), [])

    def test_a_run_on_a_terminal(self):
        guide = os.path.join(self.s.tmp, "t.guide")
        write(guide, "title: Test guide\nintro: VT {vt}, desktop {desktop}.\n"
                     "step: Look\ndo: look\nexpect: it is there\n"
                     "step: Skipped\nprobe: decarm\ndo: nothing\nexpect: nothing\n"
                     "step: Shell\nprobe: shell\ndo: type\nexpect: the prompt\n"
                     "step: Probe\nprobe: focus\ndo: switch to F{desktop}\nexpect: FOCUS OUT\n"
                     "step: Not run\ndo: x\nexpect: y\n", 0o644)
        g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env())
        g.wait_for(b"VT 6, desktop 7.")
        g.wait_for(b"Press Enter to begin.")
        g.send(b"\r")
        g.wait_for(b"Press Enter when done")
        g.send(b"\r")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        g.send(b"x")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        g.send(b"y")
        g.wait_for(b"Note (Enter for none): ")
        g.send(b"fine\r")
        g.wait_for(b"Step 2 of 5: Skipped")
        g.send(b"s\r")
        g.wait_for(b"Press Enter to start the shell")
        g.send(b"\r")
        g.wait_for(b"vtium-check$ ")
        g.send(b"echo hello $((6 * 7))\r")
        g.wait_for(b"hello 42")
        g.wait_for(b"vtium-check$ ")
        g.send(b"exit\r")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        g.send(b"n")
        g.wait_for(b"Note (Enter for none): ")
        g.send(b"\r")
        g.wait_for(b"Press Enter to start the probe")
        g.send(b"\r")
        g.wait_for(b"focus on; q quits")
        g.send(b"\x1b[O")
        g.wait_for(b"FOCUS OUT")
        g.send(b"q")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        g.send(b"y")
        g.wait_for(b"Note (Enter for none): ")
        g.send(b"\r")
        g.wait_for(b"Step 5 of 5: Not run")
        g.send(b"q\r")
        self.assertEqual(g.finish(), 0)
        log = read(os.path.join(self.s.dir, "vt-check.log")).splitlines()
        self.assertRegex(log[0], r"^=== Test guide, \S+, guide " + guide + r", tty /dev/pts/\d+, steps all$")
        self.assertEqual(log[1:7], ["-- step 1 of 5: Look", "  answer: yes  note: fine",
                                    "-- step 2 of 5: Skipped", "  answer: skip  (before the probe)",
                                    "-- step 3 of 5: Shell", "  answer: no"])
        self.assertEqual(log[7], "-- step 4 of 5: Probe")
        self.assertIn("  [focus] focus on; q quits", log)
        self.assertTrue(any("FOCUS OUT" in l for l in log), log)
        self.assertEqual(log[-3:-1], ["-- step 5 of 5: Not run", "  ended here"])
        self.assertRegex(log[-1], r"^=== ended at step 5, ")
        self.assertEqual(stat.S_IMODE(os.stat(self.s.dir).st_mode), 0o700)

    def test_a_run_step_runs_its_command_on_the_guides_terminal(self):
        guide = os.path.join(self.s.tmp, "t.guide")
        ran = "printf 'ran %s on %s\\n' $((6 * 7)) \"$(tty)\"; sleep 2"
        write(guide, "title: Run guide\n"
                     f"step: Ran\nrun: {ran}\ndo: keep still\nexpect: ran 42\n"
                     "step: Failed\nrun: echo VT {vt}; exit 3\nexpect: VT 6\n"
                     "step: Stopped\nrun: echo started; sleep 30; echo not reached\nexpect: started\n"
                     "step: Quit\nrun: echo begun; sleep 30\nexpect: begun\n", 0o644)
        g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env())
        g.wait_for(b"Press Enter to begin.")
        g.send(b"\r")
        # The step shows the command before it runs, under Run:.
        g.wait_for(b"Step 1 of 4: Ran")
        g.wait_for(b"Run:    " + ran.encode())
        g.wait_for(b"Do:     keep still")
        g.wait_for(b"Expect: ran 42")
        g.wait_for(b"Press Enter to run it (")
        self.assertNotIn(b"ran 42 on", g.seen)
        g.send(b"\r")
        # Enter runs it, shown as at a shell prompt, on the guide's own terminal.
        g.wait_for(b"vtium-check$ " + ran.encode())
        g.wait_for(b"ran 42 on /dev/pts/")
        g.wait_for(b"\r\n")
        tty = g.seen.split(b"ran 42 on ")[1].split(b"\r\n")[0].decode()
        # A key pressed while the command runs is not drawn on the picture,
        # and answers nothing.
        g.send(b"yz")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        self.assertNotIn(b"yz", g.seen)
        g.send(b"n")
        g.wait_for(b"Note (Enter for none): ")
        g.send(b"\r")
        g.wait_for(b"Step 2 of 4: Failed")
        g.wait_for(b"Run:    echo VT 6; exit 3")
        g.wait_for(b"Press Enter to run it (")
        g.send(b"\r")
        g.wait_for(b"VT 6")
        g.wait_for(b"The command exited with status 3.")
        g.wait_for(b"[y]es [n]o [s]kip: ")
        g.send(b"s")
        g.wait_for(b"Note (Enter for none): ")
        g.send(b"\r")
        # Ctrl-C stops the command, and Ctrl-\ the program it runs, as at a
        # prompt; the checks go on, the step asks with the status, and the
        # keys echo again.
        for k, title, out, key, status in ((3, "Stopped", b"started", b"\x03", 130),
                                           (4, "Quit", b"begun", b"\x1c", 131)):
            g.wait_for(f"Step {k} of 4: {title}".encode())
            g.wait_for(b"Press Enter to run it (")
            g.send(b"\r")
            g.wait_for(out + b"\r\n")
            # The key goes in once sleep runs: bash ignores Ctrl-\, so one
            # sent between echo and sleep would reach no program.
            end = time.monotonic() + 10
            while subprocess.run(["pgrep", "-s", str(g.pid), "-x", "sleep"], capture_output=True).returncode:
                self.assertLess(time.monotonic(), end, "sleep never started")
                time.sleep(0.02)
            g.send(key)
            g.wait_for(f"The command exited with status {status}.".encode())
            g.wait_for(b"[y]es [n]o [s]kip: ")
            g.send(b"n")
            g.wait_for(b"Note (Enter for none): ")
            g.send(b"cut short\r")
            g.wait_for(b"cut short")
        self.assertNotIn(b"\r\nnot reached", g.seen)
        g.wait_for(b"Press Enter to end the run.")
        g.send(b"\r")
        self.assertEqual(g.finish(), 0)
        log = read(os.path.join(self.s.dir, "vt-check.log")).splitlines()
        self.assertRegex(log[0], r"^=== Run guide, \S+, guide " + guide + ", tty " + tty + ", steps all$")
        self.assertEqual(log[1:16], ["-- step 1 of 4: Ran", "  run: " + ran, "  answer: no",
                                     "-- step 2 of 4: Failed", "  run: echo VT 6; exit 3",
                                     "  command exited with status 3", "  answer: skip",
                                     "-- step 3 of 4: Stopped", "  run: echo started; sleep 30; echo not reached",
                                     "  command exited with status 130", "  answer: no  note: cut short",
                                     "-- step 4 of 4: Quit", "  run: echo begun; sleep 30",
                                     "  command exited with status 131", "  answer: no  note: cut short"])
        self.assertRegex(log[16], r"^=== done ")


class Demo(Case):
    """demo.sh and feedback.sh on a pseudo-terminal."""

    def test_a_page_rated_reaches_feedback(self):
        env = {"PATH": os.environ["PATH"], "TERM": "linux", "VTIUM_VT_DIR": self.s.dir,
               "DEMO_LABEL": "vtium dev build test"}
        d = Pty(["/bin/bash", DEMO_SH], env)
        d.wait_for(b"press a key for the first section")
        d.send(b" ")
        d.page()
        d.wait_for(b"1 right  2 wrong")
        d.send(b"c")
        d.wait_for(b"> ")
        d.send(b"red is orange\r")
        d.wait_for(b"> ")
        d.send(b".\r")
        d.wait_for(b"Enter no rating")
        d.send(b"2")
        d.wait_for(b"r review again from page 1  q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        tsv = read(os.path.join(self.s.dir, "feedback.tsv")).splitlines()
        self.assertEqual(tsv[0], "time\trun\tlabel\tterm\tsize\tsection\trating\tnote")
        fields = tsv[1].split("\t")
        self.assertEqual(fields[2:], ["vtium dev build test", "linux", "24x80", "10-sgr", "wrong", "red is orange"])
        p = subprocess.run(["bash", FEEDBACK_SH], env=self.s.env(), capture_output=True, text=True, timeout=10)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("10-sgr                    wrong\n    red is orange\n", p.stdout)

    def test_the_demo_records_nothing_in_a_directory_not_the_users_own(self):
        os.symlink(self.s.tmp, self.s.dir)
        env = {"PATH": os.environ["PATH"], "TERM": "linux", "VTIUM_VT_DIR": self.s.dir}
        d = Pty(["/bin/bash", DEMO_SH], env)
        d.wait_for(b"is a symlink, or not a directory of yours")
        d.wait_for(b"press a key for the first section")
        d.send(b" ")
        d.page()
        d.wait_for(b"1 right  2 wrong")
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        self.assertEqual(d.finish(), 0)
        self.assertFalse(os.path.exists(os.path.join(self.s.tmp, "feedback.tsv")))

    def test_every_section_has_its_guidance_and_fits_80x25(self):
        sections = os.path.join(VT, "sections")
        for f in sorted(os.listdir(sections)):
            with self.subTest(section=f):
                text = read(os.path.join(sections, f))
                self.assertIn("# look-for: ", text)
                self.assertIn("# wrong-looks-like: ", text)
                out = subprocess.run(["bash", os.path.join(sections, f)], capture_output=True, text=True,
                                     timeout=10).stdout
                self.assertLessEqual(len(out.splitlines()), 17)


if __name__ == "__main__":
    unittest.main()
