#!/usr/bin/env python3
"""Tests the VT review tools (tools/vt, docs/vt-review.md) without a VT, sudo
or logind. Each case gets a temporary directory of stand-ins named by
VTIUM_VT_STANDINS: systemctl, loginctl, setpriv, getent, sudo and
systemd-sysusers record their calls, "shells" stands for /etc/shells, chvt and fgconsole record that they were reached (no tool may switch a VT),
"active" stands for the VT on screen, and a stand-in vtium binary records its
arguments and exits, crashes, waits for a signal or runs its session. The
stand-ins also lead PATH, so a command the tools reach by name finds one.

    python3 -I tools/vt/tests/test_vt.py [-v] [TestCase.test_name ...]
"""

import fcntl
import hashlib
import itertools
import os
import pty
import re
import select
import shutil
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

# The tests start the tools as a terminal starts them, with the signals a
# terminal sends at their defaults. A launcher can leave one of them ignored:
# nohup ignores SIGHUP, and a shell ignores SIGINT and SIGQUIT in a job it
# starts in the background. Every process the tests start inherits that, and
# bash cannot trap a signal that was ignored when it started, so run-vt.sh
# would go on waiting for vtium through a signal a test sends it, and the test
# would wait for an end that never comes. So the tests put these signals back
# to their defaults before they start anything.
for _sig in (signal.SIGHUP, signal.SIGINT, signal.SIGQUIT, signal.SIGTERM):
    if signal.getsignal(_sig) == signal.SIG_IGN:
        signal.signal(_sig, signal.default_int_handler if _sig == signal.SIGINT else signal.SIG_DFL)

# The longest any test waits on another process. It only makes a broken test fail instead of hanging.
WAIT_BOUND = 120
# The same for a run of make, which may build with cargo on a loaded machine.
BUILD_BOUND = 1800

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
# The cargo target directory the Makefile builds in, as it names it: the
# caller's CARGO_TARGET_DIR, or target. make gates gives each gate its own.
TARGET = os.environ.get("CARGO_TARGET_DIR") or "target"
VT = os.path.join(ROOT, "tools", "vt")
RUN_VT = os.path.join(VT, "run-vt.sh")
VT_INSTALL = os.path.join(VT, "vt-install.sh")
VT_SH = os.path.join(VT, "vt.sh")
UNITS_SH = os.path.join(VT, "units.sh")
GUIDE_SH = os.path.join(VT, "checks", "guide.sh")
ACCEPT_SH = os.path.join(VT, "accept.sh")
FEEDBACK_SH = os.path.join(VT, "feedback.sh")
# The sysroot cache tools/arm/sysroot keeps, which every checkout shares, as
# the caller's environment names it: VTIUM_ARM_SYSROOTS, else under
# XDG_CACHE_HOME or HOME. Stage.env names it to the tests' builds, because it
# moves HOME and drops every VTIUM_ variable.
SYSROOTS = os.path.dirname(subprocess.run([os.path.join(ROOT, "tools", "arm", "sysroot"), "dir", "amd64"],
                                          capture_output=True, text=True, check=True).stdout.strip())
# The real systemctl, outside the stand-ins, for the links under a root.
REAL_SYSTEMCTL = shutil.which("systemctl")
REAL_ANALYZE = shutil.which("systemd-analyze")
USER = subprocess.run(["id", "-un"], capture_output=True, text=True, check=True).stdout.strip()
# script(1) as run-vt.sh finds it, on the PATH it pins.
SCRIPT = next(os.path.join(d, "script") for d in ("/usr/sbin", "/usr/bin", "/sbin", "/bin")
              if os.access(os.path.join(d, "script"), os.X_OK))

# Each call, one line: the command's name and its arguments.
RECORD = 'printf "%s\\n" "${0##*/} $*" >>"$CALLS"\n'

# The minspec target reads the stand-in binary's dynamic section for DT_RELR and its version needs for GLIBC_.
READELF = "#!/bin/bash\n" + RECORD + 'echo " 0x0000000000000024 (RELR) 0x3238  Name: GLIBC_2.34"\n'

SYSTEMCTL = "#!/bin/bash\n" + RECORD + """
# is-active --quiet UNIT succeeds for the units in $ACTIVE_UNITS.
if [ "$1" = is-active ]; then
	case " ${ACTIVE_UNITS-} " in *" $3 "*) exit 0 ;; esac
	exit 3
fi
# show -p Id --value autovt@ttyN.service prints the unit the alias names:
# N=UNIT in $AUTOVT_IS, else the name itself, as for a unit with no alias.
if [ "$1" = show ]; then
	for u; do :; done
	n=${u#autovt@tty}; n=${n%.service}
	for e in ${AUTOVT_IS-}; do
		[ "${e%%=*}" != "$n" ] || { echo "${e#*=}"; exit 0; }
	done
	echo "$u"
	exit 0
fi
# Each stop and start appends to $HOLDS whether the caller holds a stand-in
# VT (a file ttyN among the stand-ins) open at that moment: "stop held" or
# "start free".
case $1 in
stop | start)
	h=free
	for f in /proc/$PPID/fd/*; do
		case $(readlink "$f") in "$VTIUM_VT_STANDINS"/tty[0-9]*) h=held ;; esac
	done
	echo "$1 $h" >>"${HOLDS:-/dev/null}" ;;
esac
# The verb $FAIL_VERB fails.
[ "$1" != "${FAIL_VERB-}" ] || exit 1
# The verbs that read and write unit links run the real systemctl offline,
# on the tree under $SYSTEMD_ROOT, as it would on the machine's /etc.
case $1 in
enable | disable | add-wants | is-enabled) exec "$REAL_SYSTEMCTL" --root="$SYSTEMD_ROOT" "$@" ;;
esac
# Under a seeded root, stop, start and restart fail for a unit with no unit
# file there, as the real systemctl does ("Unit ... not found").
if [ -d "$SYSTEMD_ROOT/etc/systemd/system" ]; then
	case $1 in
	stop | start | restart)
		for u in "${@:2}"; do
			f=$u; [[ $u != *@*.service ]] || f=${u%%@*}@.service
			[ -e "$SYSTEMD_ROOT/etc/systemd/system/$f" ] || [ -e "$SYSTEMD_ROOT/usr/lib/systemd/system/$f" ] ||
				{ echo "Failed to $1 $u: Unit $u not found." >&2; exit 5; }
		done ;;
	esac
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
	# The run lists the sessions after it has checked the files it copies and before it copies
	# them, so a test that sets SWAP_FROM puts a file in a checked file's place in between.
	[ -z "${SWAP_FROM:-}" ] || mv -f -- "$SWAP_FROM" "$SWAP_TO"
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

# Drops no privilege: runs what follows --, as setpriv would after dropping
# to the user. With --reset-env it drops vtium's own variables, as the real
# reset does, so only what the run carries across reaches the session; the
# tests' own variables stay, which the stand-ins read.
SETPRIV = "#!/bin/bash\n" + RECORD + """
# Whether it holds a stand-in VT (a file ttyN among the stand-ins) open: a
# line "held" or "free" in $SETPRIV_HOLD.
h=free
for f in /proc/$$/fd/*; do
	case $(readlink "$f") in "$VTIUM_VT_STANDINS"/tty[0-9]*) h=held ;; esac
done
echo "$h" >>"${SETPRIV_HOLD:-/dev/null}"
while [ $# -gt 0 ] && [ "$1" != -- ]; do [ "$1" != --reset-env ] || unset VTIUM_HEAD VTIUM_CONSOLE; shift; done
shift
# Hooks for the tests of the reading, each named by the path its command reads:
# RFORGE is a report the stand-in writes in place of the real one (for RFORGE_FOR, else for every
# read); RSUBST_FOR gives that read the contents and report of the file RSUBST_DATA; RLINK_FOR is
# made a symlink to RLINK_TARGET just before it is read.
if [ -n "${RLINK_FOR:-}" ] && [[ " $* " == *" $RLINK_FOR "* ]]; then ln -sf -- "$RLINK_TARGET" "$RLINK_FOR"; fi
if [ -n "${RFORGE:-}" ] && [[ -z "${RFORGE_FOR:-}" || " $* " == *" $RFORGE_FOR "* ]]; then
	echo "$RFORGE" >&3; echo x >&4; exit 0
fi
if [ -n "${RSUBST_FOR:-}" ] && [[ " $* " == *" $RSUBST_FOR "* ]]; then
	/usr/bin/stat -L -c "file %u %a %h %F" "$RSUBST_DATA" >&3; cat "$RSUBST_DATA" >&4; exit 0
fi
# RSWAP_AFTER names a file whose read by the user is the command being run: once it ends, the
# file at RSWAP_TO is replaced by RSWAP_FROM, so a test can change a file between the read and
# the checks that follow.
if [ -z "${RSWAP_AFTER:-}" ] || [[ " $* " != *" $RSWAP_AFTER "* ]]; then exec "$@"; fi
"$@"; status=$?
mv -f -- "$RSWAP_FROM" "$RSWAP_TO"
exit $status
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
printf 'terminfo: %s\\n' "$([ -f "${TERMINFO-}/x/xterm-vtium" ] && echo xterm-vtium || echo none)"
[ ! -f "${TERMINFO-}/x/xterm-vtium" ] ||
	printf 'terminfo modes: %s\\n' "$(stat -c %a "$TERMINFO" "$TERMINFO/x" "$TERMINFO/x/xterm-vtium" | paste -sd' ')"
printf 'env: %s\\n' "LANG=${LANG-}" "LC_TIME=${LC_TIME-}" "XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR-}" \\
	"DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS-}" "VTIUM_HEAD=${VTIUM_HEAD-}" \\
	"VTIUM_CONSOLE=${VTIUM_CONSOLE-}"
read -r line || true
printf 'read: %s\\n' "$line"
"""

# cat-config systemd/logind.conf prints $LOGIND_CONF: none, by default, so
# logind's ReserveVT= is its default, 6.
ANALYZE = "#!/bin/bash\n" + RECORD + """
[ "$1 $2" = "cat-config systemd/logind.conf" ] || exit 2
printf '%b' "${LOGIND_CONF-}"
"""

# Runs what follows as the real sudo would run it as root: VTIUM_STANDIN_ROOT
# marks it, for the stand-ins that, as their real programs do, refuse to run
# as anyone else.
SUDO = "#!/bin/bash\n" + RECORD + """
[ "$1" != -v ] || exit "${SUDO_STATUS:-0}"
export VTIUM_STANDIN_ROOT=1
exec "$@"
"""

# Checks what the real systemd-sysusers checks before it creates anything:
# it runs as root alone (under the stand-in sudo), it opens each file it is
# given, and each line of one is blank, a comment, or a u, g, m or r line
# (sysusers.d(5)).
SYSUSERS = "#!/bin/bash\n" + RECORD + """
[ -n "${VTIUM_STANDIN_ROOT-}" ] || { echo "systemd-sysusers: Need to be root." >&2; exit 1; }
for f; do
	[ -f "$f" ] && [ -r "$f" ] || { echo "systemd-sysusers: Failed to open '$f'" >&2; exit 1; }
	while IFS= read -r l || [ -n "$l" ]; do
		[[ -z ${l//[[:space:]]/} || $l == '#'* || $l =~ ^[ugmr][[:space:]] ]] ||
			{ echo "systemd-sysusers: $f: unknown line: $l" >&2; exit 1; }
	done <"$f"
done
"""

FORBIDDEN = "#!/bin/bash\n" + RECORD + "exit 1\n"

# The stand-in vtium: records its arguments, prints $BIN_LOG, then as
# $BIN_MODE says: exit (with $BIN_STATUS), wait (until a signal, recording
# it), linger (exits, leaving a sleep that never ends and holds its
# output open, as a process vtium started might), or session (runs the argv after --, with $BIN_STDIN as its input, as
# vtium runs its first head's first console: VTIUM_HEAD=console and
# VTIUM_CONSOLE=1 in its environment, and a word that is $VTIUM_HEAD or
# $VTIUM_CONSOLE replaced by its value).
BIN = """#!/bin/bash
# Built to start its glyph helper at /usr/local/bin/vtium-dev-config, as make
# vt-install builds it.
echo $$ >"$BIN_PID"
# The run's lock is root's; vtium, and the login and shell it starts, never
# hold it.
for f in /proc/$$/fd/*; do
	case $(readlink "$f") in
	*vt.lock) echo "vtium inherited the lock" >&2; exit 97 ;;
	"$VTIUM_VT_STANDINS"/tty[0-9]*) echo "vtium inherited the run's hold on its VT" >&2; exit 97 ;;
	*/run-vt-read.*) echo "vtium inherited a file the run read" >&2; exit 97 ;;
	esac
done
# Whether the run that started it holds its VT open: "held" or "free" in
# $BIN_HOLD.
h=free
for f in /proc/$PPID/fd/*; do
	case $(readlink "$f") in "$VTIUM_VT_STANDINS"/tty[0-9]*) h=held ;; esac
done
echo "$h" >"${BIN_HOLD:-/dev/null}"
printf '%s\\n' "$@" >"$BIN_ARGS"
# The stage is gone when the run ends; a test that wants to read it names a directory for a copy.
[ -z "${BIN_STAGE_COPY:-}" ] || cp -r "$VTIUM_VT_STANDINS/run-vtium-vt/." "$BIN_STAGE_COPY"
printf '%b' "${BIN_LOG:-}"
case ${BIN_MODE:-exit} in
exit) exit "${BIN_STATUS:-0}" ;;
wait)
	trap 'echo TERM >>"$BIN_SIGNALS"; exit 0' TERM
	# Signals are recorded from here on; a test that sends one waits for this file.
	: >"${BIN_READY:-/dev/null}"
	while :; do sleep 0.05; done ;;
linger)
	sleep infinity &
	exit 0 ;;
session)
	while [ $# -gt 0 ] && [ "$1" != -- ]; do shift; done
	shift
	export VTIUM_HEAD=console VTIUM_CONSOLE=1
	argv=()
	for w; do
		case $w in
		'$VTIUM_HEAD') argv+=("$VTIUM_HEAD") ;;
		'$VTIUM_CONSOLE') argv+=("$VTIUM_CONSOLE") ;;
		*) argv+=("$w") ;;
		esac
	done
	exec "${argv[@]}" <"$BIN_STDIN" ;;
esac
"""


# The lines of vtium's log at which run-vt.sh --debug copies the DRM state:
# the source file that logs each, its format string there, word for word,
# the values a log fills it with, and what the line says of a display.
DISPLAY_LINES = (
    ("crates/vtium-platform/src/display/drm2d.rs", "display {name} at {}x{}", ("HDMI-A-2", "800", "480"), "came"),
    ("crates/vtium-platform/src/display/drm2d.rs", "display {} is gone", ("HDMI-A-2",), "went"),
    ("crates/vtium/src/app.rs", "a display{of} came: the session is drawn again at {cols}x{rows}",
     (" of head panel", "100", "30"), "came"),
    ("crates/vtium-platform/src/display/drm2d.rs", "display {}, left dark, is gone", ("DP-8",), "went"),
    ("crates/vtium/src/app.rs", "a display{of} came: the session is drawn again at {cols}x{rows}",
     ("", "100", "30"), "came"),
    ("crates/vtium-platform/src/display/drm2d.rs", "display {}: the first flip after its modeset is done",
     ("HDMI-A-2",), "modeset"),
)


def logged(fmt, values):
    """The line `fmt` logs with `values`, each placeholder filled in turn."""
    parts = fmt.replace("}", "{").split("{")
    return "".join(parts[i] if i % 2 == 0 else values[i // 2] for i in range(len(parts)))


def body(lines):
    """A shipped unit's lines from its first section on: the generated unit
    replaces the opening comment, which names the C reference's units."""
    i = next(i for i, l in enumerate(lines) if l.startswith("["))
    return lines[i:]


def generated(src_lines, name, exec_path, conflicts):
    """The shipped vtium@.service's lines as units.sh changes them for
    NAME with EXEC_PATH and each --conflicts unit: ExecStart= and OnFailure=
    renamed for NAME, a Conflicts= and an After= line for each unit just above
    OnFailure=, and the shipped DefaultInstance= replaced, at the end of
    [Install], by one that names no VT, which systemctl refuses (ADR-1500 D3,
    D4)."""
    out = []
    for line in body(src_lines):
        if line.startswith("DefaultInstance="):
            continue
        if line.startswith("OnFailure="):
            for c in conflicts:
                out += [f"Conflicts={c}", f"After={c}"]
        out.append({"ExecStart=/usr/bin/vtium --vt=%I --no-switchvt": f"ExecStart={exec_path} --vt=%I --no-switchvt",
                    "OnFailure=vtium-getty@%i.service": f"OnFailure={name}-getty@%i.service"}.get(line, line))
    return out + [f"# No VT: systemctl refuses it, so enabling {name}@.service with no VT",
                  f"# named fails and links nothing. Name the VT: {name}@tty3.service.",
                  f"DefaultInstance=name-a-VT/{name}@ttyN"]


def write(path, text, mode=0o755):
    with open(path, "w") as f:
        f.write(text)
    os.chmod(path, mode)


# The words run-vt.sh puts after setpriv, which vtium fills in with the head
# and the console: they carry VTIUM_HEAD and VTIUM_CONSOLE across
# --reset-env.
CARRY = ["/bin/sh", "-c",
         'h=$1 c=$2; shift 2; exec /usr/bin/env VTIUM_HEAD="$h" VTIUM_CONSOLE="$c" "$@"',
         "sh", "$VTIUM_HEAD", "$VTIUM_CONSOLE"]


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
                           ("getent", GETENT), ("sudo", SUDO), ("systemd-analyze", ANALYZE),
                           ("cargo", "#!/bin/bash\n" + RECORD),
                           ("systemd-sysusers", SYSUSERS),
                           ("readelf", READELF),
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
        # The stand-in glyph helper, which vt-install copies and never runs.
        self.helper = os.path.join(tmp, "vtium-config")
        write(self.helper, "#!/bin/sh\n# the stand-in vtium-config\n")
        # The stand-in image helper, which vt-install copies and never runs.
        self.image = os.path.join(tmp, "vtium-image")
        write(self.image, "#!/bin/sh\n# the stand-in vtium-image\n")
        self.dir = os.path.join(tmp, "vt")
        self.passes = os.path.join(tmp, "passes")
        self.caller_home = os.path.join(tmp, "caller-home")
        os.mkdir(self.caller_home, 0o700)
        self.calls = os.path.join(tmp, "calls")
        self.bin_args = os.path.join(tmp, "bin-args")
        self.signals = os.path.join(tmp, "signals")
        self.bin_ready = os.path.join(tmp, "bin-ready")
        self.holds = os.path.join(tmp, "holds")
        self.bin_hold = os.path.join(tmp, "bin-hold")

    def env(self, **extra):
        env = dict(os.environ)
        # An enclosing make's command line, such as make test-tools
        # FEATURES=images, reaches the make these run through MAKEFLAGS, so
        # it is kept out.
        for k in list(env):
            if k.startswith(("VTIUM_", "GUIDE", "DEMO_", "ACCEPT_", "SUDO_", "XDG_RUNTIME_DIR", "XDG_STATE_HOME",
                             "DBUS_SESSION_BUS_ADDRESS")) or \
                    k in ("MAKEFLAGS", "MFLAGS"):
                del env[k]
        env.update(
            PATH=self.standins + os.pathsep + env["PATH"],
            VTIUM_VT_STANDINS=self.standins,
            VTIUM_VT_DIR=self.dir,
            # the acceptance passes, which vt.sh hands the session
            ACCEPT_DIR=self.passes,
            # A home of the tests' own, so nothing a test runs can write under
            # the real one, such as the passes directory's default.
            HOME=self.caller_home,
            # The caller's sysroot cache, which the moved HOME would otherwise
            # replace with an empty one, where every build's sysroot fetch
            # would download each package from the Debian mirror again.
            VTIUM_ARM_SYSROOTS=SYSROOTS,
            CALLS=self.calls,
            COUNT=os.path.join(self.tmp, "count"),
            BIN_ARGS=self.bin_args,
            BIN_SIGNALS=self.signals,
            BIN_READY=self.bin_ready,
            BIN_PID=os.path.join(self.tmp, "bin-pid"),
            HOLDS=self.holds,
            BIN_HOLD=self.bin_hold,
            ACTIVE_UNITS="kmsconvt@tty8.service",
            REAL_SYSTEMCTL=REAL_SYSTEMCTL or "/nonexistent/systemctl",
            SYSTEMD_ROOT=os.path.join(self.standins, "root"),
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

    def cards(self):
        """The stand-in /sys/kernel/debug/dri: cards 0 and 1 with their state,
        a render node with none, and card 0 again under its bus id."""
        dri = os.path.join(self.standins, "debugfs-dri")
        for card, text in (("0", "crtc[51]: crtc-0\n\tactive=1\n"), ("1", "crtc[80]: crtc-0\n\tactive=0\n")):
            os.makedirs(os.path.join(dri, card))
            write(os.path.join(dri, card, "state"), text, 0o400)
        os.makedirs(os.path.join(dri, "128"))
        os.symlink("0", os.path.join(dri, "0000:03:00.0"))
        return dri

    def calls_of(self, name):
        return [l.split(" ", 1)[1] if " " in l else "" for l in read(self.calls).splitlines()
                if l.split(" ", 1)[0] == name]

    def changes(self):
        """systemctl's calls that change anything: all but is-active, is-enabled and show."""
        return [c for c in self.calls_of("systemctl") if not c.startswith(("is-active", "is-enabled", "show"))]


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

    def run_vt(self, *args, timeout=WAIT_BOUND, **env):
        return subprocess.run(["bash", RUN_VT, f"--bin={self.s.bin}", *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=timeout, cwd=self.s.tmp,
                              start_new_session=True)

    def vt_sh(self, *args, timeout=WAIT_BOUND, **env):
        return subprocess.run(["bash", VT_SH, *args], env=self.s.env(VTIUM_VT_BIN=self.s.bin, **env),
                              capture_output=True, text=True, timeout=timeout, cwd=self.s.tmp,
                              start_new_session=True)

    def wait_ready(self):
        """Waits until the stand-in vtium in wait mode has installed its signal trap: a signal sent
        before that would kill it without a record. The bound only fails a broken test."""
        end = time.monotonic() + WAIT_BOUND
        while not os.path.exists(self.s.bin_ready) and time.monotonic() < end:
            time.sleep(0.02)
        self.assertTrue(os.path.exists(self.s.bin_ready), "vtium never started waiting")

    def refused(self, p, why):
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(why, p.stderr)


class RunVt(Case):
    """run-vt.sh, the part that runs as root, in its test mode."""

    def test_dry_run_prints_the_changes_and_makes_none(self):
        p = self.run_vt("--dry-run", "8", "--", "--font-size=30")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("+ " + os.path.join(self.s.standins, "systemctl") + " stop kmsconvt@tty8.service", p.stdout)
        self.assertIn(f"+ {self.s.standins}/run-vtium-vt/vtium --vt=tty8 --no-switchvt --configdir {self.s.standins}/run-vtium-vt/config --oneshot "
                      f"--font-frits={self.s.standins}/run-vtium-vt/frits --font-size=30 --login --",
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
                self.refused(p, "is on VT 8; log out there, or pick another VT")
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
        p = self.run_vt("8", ACTIVE_UNITS="getty@tty8.service")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.s.changes(), ["stop getty@tty8.service", "start getty@tty8.service"])
        self.assertIn("manual restore: sudo systemctl start getty@tty8.service", p.stdout)

    def test_autovt_is_stopped_and_started_as_the_unit_it_names(self):
        # logind's autovt@tty8 is the getty there, or a unit no other name
        # covers: each is stopped and started once, under its own name.
        for active, autovt, want in (("getty@tty8.service autovt@tty8.service", "8=getty@tty8.service",
                                      "getty@tty8.service"),
                                     ("autovt@tty8.service", "8=console@tty8.service", "console@tty8.service")):
            with self.subTest(autovt=autovt):
                open(self.s.calls, "w").close()
                p = self.run_vt("8", ACTIVE_UNITS=active, AUTOVT_IS=autovt)
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assertEqual(self.s.changes(), [f"stop {want}", f"start {want}"])
                self.assertIn(f"manual restore: sudo systemctl start {want}\n", p.stdout)

    def test_the_development_vt_is_borrowed_and_given_back_as_it_was(self):
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl: is-enabled reads the links through systemctl --root")
        VtSh.installed(self, 8)
        VtSh.enabled(self, "vtium-dev", 8)
        write(os.path.join(self.s.standins, "root", "usr", "lib", "systemd", "system", "getty@.service"),
              "[Service]\nExecStart=-/sbin/agetty %I\n", 0o644)
        dev = {"AUTOVT_IS": "8=vtium-dev@tty8.service"}
        # vtium-dev runs: it is stopped, and started again after a normal end
        # and after a crash. The getty its fallback started instead: only the
        # getty. Nothing runs: nothing is stopped or started.
        for active, status, want in (("vtium-dev@tty8.service autovt@tty8.service", "0", ["vtium-dev@tty8.service"]),
                                     ("vtium-dev@tty8.service autovt@tty8.service", "134", ["vtium-dev@tty8.service"]),
                                     ("getty@tty8.service", "0", ["getty@tty8.service"]),
                                     ("", "0", [])):
            with self.subTest(active=active, status=status):
                open(self.s.calls, "w").close()
                p = self.run_vt("8", ACTIVE_UNITS=active, BIN_STATUS=status, **dev)
                self.assertEqual(p.returncode, int(status), p.stdout + p.stderr)
                self.assertTrue(os.path.exists(self.s.bin_args), "vtium did not run")
                self.assertEqual(self.s.changes(), [f"{v} {u}" for v in ("stop", "start") for u in want])
                if want:
                    self.assertIn(f"manual restore: sudo systemctl start {want[0]}\n", p.stdout)
                    self.assertIn(f"when the acceptance pass ends, {want[0]} is started again on VT 8", p.stdout)
                else:
                    self.assertIn("nothing to restore afterwards", p.stdout)

    def test_a_login_inside_the_development_vtium_is_refused(self):
        VtSh.installed(self, 8)
        p = self.run_vt("8", SEQ="8", ACTIVE_UNITS="vtium-dev@tty8.service autovt@tty8.service",
                        AUTOVT_IS="8=vtium-dev@tty8.service")
        self.refused(p, "is on VT 8; log out there, or pick another VT")
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))

    def test_the_vt_is_held_open_from_the_stop_until_the_units_start_again(self):
        # logind starts autovt@tty8 at a switch to VT 8 only while nothing
        # holds the VT open, so the run holds it from the moment its units have
        # stopped, and lets go before they start again, whose start hangs the
        # VT up and may give it back to the kernel. vtium does not inherit it.
        for status in ("0", "134"):
            with self.subTest(status=status):
                for f in (self.s.holds, self.s.bin_hold):
                    open(f, "w").close()
                p = self.run_vt("8", BIN_STATUS=status)
                self.assertEqual(p.returncode, int(status), p.stdout + p.stderr)
                self.assertEqual(read(self.s.holds), "stop free\nstart free\n")
                self.assertEqual(read(self.s.bin_hold), "held\n")
        for f in (self.s.calls, self.s.holds):
            open(f, "w").close()
        os.unlink(self.s.bin_args)
        self.interrupted(signal.SIGTERM, 143)
        self.assertEqual(read(self.s.holds), "stop free\nstart free\n")
        p = self.run_vt("--dry-run", "8")
        self.assertIn(f"(run-vt.sh holds {self.s.standins}/tty8 open from here until the units start again,", p.stdout)

    def test_loginds_reserved_vt_is_refused(self):
        # logind starts autovt@ttyN on its reserved VT at every switch to it,
        # even while the VT is held open, and that console would take the VT
        # from the run.
        why = "VT {0} refused: it is logind's reserved VT (ReserveVT= in logind.conf)"
        for vt, conf, refused in (("6", "", True), ("6", "[Login]\n#ReserveVT=6\n", True),
                                  ("6", "[Login]\nReserveVT=0\n", False), ("8", "[Login]\nReserveVT = 8\n", True),
                                  ("8", "[Login]\nReserveVT=8\n[Login]\nReserveVT=2\n", False)):
            with self.subTest(vt=vt, conf=conf):
                open(self.s.calls, "w").close()
                p = self.run_vt("--dry-run", vt, LOGIND_CONF=conf)
                if refused:
                    self.refused(p, why.format(vt))
                else:
                    self.assertEqual(p.returncode, 0, p.stderr)
                self.assertEqual(self.s.changes(), [])

    def test_the_unit_comes_back_after_a_crash(self):
        p = self.run_vt("8", BIN_STATUS="134")
        self.assertEqual(p.returncode, 134)
        self.assertIn("vtium exited with status 134", p.stderr)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def interrupted(self, sig, status):
        proc = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", "8"], env=self.s.env(BIN_MODE="wait"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp)
        self.wait_ready()
        proc.send_signal(sig)
        out, err = proc.communicate(timeout=WAIT_BOUND)
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
        self.wait_ready()
        proc.stdout.close()
        proc.send_signal(signal.SIGHUP)
        proc.wait(timeout=WAIT_BOUND)
        err = read(os.path.join(self.s.tmp, "stderr"))
        self.assertEqual(proc.returncode, 129, err)
        self.assertEqual(read(self.s.signals), "TERM\n", "vtium was left running")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def test_one_run_at_a_time(self):
        first = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", "8"], env=self.s.env(BIN_MODE="wait"),
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=self.s.tmp)
        try:
            self.wait_ready()
            os.unlink(self.s.bin_args)
            p = self.run_vt("9", ACTIVE_UNITS="getty@tty9.service")
            self.refused(p, "another VT review run is in progress")
            self.assertFalse(os.path.exists(self.s.bin_args), "a second vtium ran")
            self.assertNotIn("stop getty@tty9.service", self.s.changes())
        finally:
            first.send_signal(signal.SIGTERM)
            first.wait(timeout=WAIT_BOUND)
        # Released when the first run ends.
        self.assertEqual(self.run_vt("9").returncode, 0)

    def test_the_unit_comes_back_after_ctrl_c(self):
        self.interrupted(signal.SIGINT, 130)

    def test_the_unit_comes_back_after_sigterm(self):
        self.interrupted(signal.SIGTERM, 143)

    def test_the_unit_comes_back_after_a_hangup(self):
        self.interrupted(signal.SIGHUP, 129)

    def test_the_signal_tests_hold_when_their_launcher_ignored_the_signals(self):
        # nohup starts the tests with SIGHUP ignored, and a shell's background
        # job with SIGINT and SIGQUIT ignored. The tests that send those
        # signals still see run-vt.sh stop vtium and start the unit again.
        tests = [f"RunVt.test_the_unit_comes_back_{t}" for t in ("after_a_hangup", "when_stdout_is_closed",
                                                                 "after_ctrl_c", "after_sigterm")]
        ignore = lambda: [signal.signal(s, signal.SIG_IGN) for s in (signal.SIGHUP, signal.SIGINT, signal.SIGQUIT)]
        p = subprocess.run([sys.executable, "-I", os.path.abspath(__file__), *tests], preexec_fn=ignore,
                           capture_output=True, text=True, timeout=WAIT_BOUND * (len(tests) + 1))
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"Ran {len(tests)} tests", p.stderr)

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

    def test_a_vt_the_packaged_vtium_holds_is_refused(self):
        packaged = "the packaged vtium@tty8 holds it; sudo systemctl disable --now vtium@tty8 frees it"
        switched = ("logind starts the packaged vtium@tty8 there when the VT is switched to "
                    "(autovt@tty8 is vtium@tty8); vtium console status shows what holds each VT")
        for setup, env, why in ((lambda: None, {"ACTIVE_UNITS": "vtium@tty8.service"}, packaged),
                                (lambda: VtSh.enabled(self, "vtium", 8), {}, packaged),
                                (lambda: None, {"AUTOVT_IS": "8=vtium@tty8.service"}, switched)):
            with self.subTest(env=env, why=why):
                shutil.rmtree(os.path.join(self.s.standins, "root"), ignore_errors=True)
                setup()
                open(self.s.calls, "w").close()
                p = self.run_vt("--dry-run", "8", **env)
                self.refused(p, f"VT 8 refused: {why}")
                self.assertEqual(self.s.changes(), [])

    def test_the_operators_options_are_taken(self):
        opts = ["--font-name=Monoid Nerd Font Mono", "--font-size=30", "--mode=7680x2160",
                "--no-use-original-mode", "--hwaccel", "--image-storage-limit=4G",
                "--image-storage-limit=auto", "--sync-timeout=500", "--dpms-timeout=15",
                "--grab-paste=<Ctrl><Shift>V", "--pointer-hide-timeout=5",
                "--pointer-hide-while-typing", "--no-pointer-hide-while-typing",
                "--pointer-char=+", "--osd=full", "--osd-timeout=never", "--osd-timeout=10",
                "--focus-mark=always", "--display-buffers=1", "--display-buffers=2", "--display-buffers=3"]
        p = self.run_vt("--dry-run", "8", "--", *opts)
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_display_buffers_takes_one_two_or_three(self):
        # vtium runs as root here, so a value it would refuse stops the run
        # before any unit is stopped.
        for value in ("0", "4", "two", "", "3x"):
            with self.subTest(value=value):
                self.refused(self.run_vt("--dry-run", "8", "--", f"--display-buffers={value}"),
                             "--display-buffers takes 1, 2 or 3 here")

    def test_the_binary_must_be_safe_to_run_as_root(self):
        os.chmod(self.s.bin, 0o775)
        self.refused(self.run_vt("8"), "writable by group or others")
        os.chmod(self.s.bin, 0o757)
        self.refused(self.run_vt("8"), "writable by group or others")
        os.chmod(self.s.bin, 0o755)
        p = subprocess.run(["bash", RUN_VT, "--bin=" + os.path.join(self.s.tmp, "none"), "8"],
                           env=self.s.env(), capture_output=True, text=True, timeout=WAIT_BOUND)
        self.refused(p, "no vtium binary at")
        p = subprocess.run(["bash", RUN_VT, "8"], env=self.s.env(), capture_output=True, text=True, timeout=WAIT_BOUND)
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
                                   capture_output=True, text=True, timeout=WAIT_BOUND, start_new_session=True)
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

    def test_the_acceptance_pass_runs_as_the_user_with_its_label(self):
        p = self.run_vt("--label=dev build abc", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        at = argv.index("--")
        uid, gid = os.getuid(), os.getgid()
        self.assertEqual(argv[at + 1:], [os.path.join(self.s.standins, "setpriv"), f"--reuid={uid}", f"--regid={gid}",
                                         "--init-groups", "--inh-caps=-all", "--reset-env", "--", *CARRY,
                                         "/usr/bin/env", f"TERMINFO={self.s.standins}/run-vtium-vt/terminfo",
                                         "ACCEPT_LABEL=vtium dev build abc", "ACCEPT_VT=8", f"ACCEPT_BINARY={self.s.bin}",
                                         "ACCEPT_FEATURES=none", f"ACCEPT_VTIUM_CONF={self.s.standins}/run-vtium-vt/config/vtium.conf",
                                         "/bin/bash", os.path.join(VT, "accept.sh")])

    def test_the_acceptance_pass_reads_the_blink_style_vtium_is_given(self):
        # The run's config, which --configdir gives vtium, and the blink
        # options among vtium's, in their order, so the last of each wins.
        p = self.run_vt("8", "--", "--blink-text=off", "--no-blink", "--blink", "--blink-text=full")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        words = [a for a in argv if a.startswith(("ACCEPT_VTIUM_CONF=", "ACCEPT_BLINK"))]
        self.assertEqual(words, [f"ACCEPT_VTIUM_CONF={self.s.standins}/run-vtium-vt/config/vtium.conf", "ACCEPT_BLINK_TEXT=off",
                                 "ACCEPT_BLINK=off", "ACCEPT_BLINK=on", "ACCEPT_BLINK_TEXT=full"])

    def test_a_file_swapped_in_after_the_check_is_never_copied(self):
        # The run checks vtium, the glyph helper, the image helper and the frits, then copies them as root.
        # A file put in their place in between must not reach the stage: the copy is of the file
        # that was checked.
        fonts = os.path.join(self.s.standins, "vtium-fonts")
        os.mkdir(fonts)
        frit = os.path.join(fonts, "system.vtfrit")
        write(frit, "font=/usr/share/fonts/checked.ttf\n")
        os.chmod(frit, 0o644)
        for what, path, staged in (("vtium", self.s.bin, "vtium"), ("glyph helper", self.s.helper, "vtium-config"),
                                   ("image helper", self.s.image, "vtium-image"),
                                   ("frit", frit, "frits/system.vtfrit")):
            with self.subTest(what=what):
                checked = read(path)
                swapped = os.path.join(self.s.tmp, "swapped")
                write(swapped, "#!/bin/bash\n# the swapped-in file\n", 0o755)
                seen = os.path.join(self.s.tmp, "stage-copy")
                shutil.rmtree(seen, ignore_errors=True)
                os.mkdir(seen)
                p = self.run_vt(f"--helper={self.s.helper}", f"--image={self.s.image}", "8",
                                SWAP_FROM=swapped, SWAP_TO=path, BIN_STAGE_COPY=seen)
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertIn("swapped-in file", read(path), "the test did not swap the file")
                self.assertEqual(read(os.path.join(seen, staged)), checked)
                write(path, checked, 0o755 if what != "frit" else 0o644)

    def test_the_checks_are_made_on_what_was_read(self):
        # The owner and mode checks read the file the user's process opened, not the path, which a
        # swap after the read can change. A good file read and then replaced by a bad one passes,
        # and a bad file read and then replaced by a good one is refused.
        fonts = os.path.join(self.s.standins, "vtium-fonts")
        os.mkdir(fonts)
        frit = os.path.join(fonts, "system.vtfrit")
        write(frit, "font=/usr/share/fonts/x.ttf\n", 0o755)
        for what, path, args in (("vtium", self.s.bin, []), ("glyph helper", self.s.helper, [f"--helper={self.s.helper}"]),
                                 ("image helper", self.s.image, [f"--image={self.s.image}"]), ("frit", frit, [])):
            for first, then, ok in ((0o755, 0o777, True), (0o775, 0o755, False)):
                with self.subTest(what=what, first=oct(first)):
                    swapped = os.path.join(self.s.tmp, "swapped")
                    write(swapped, read(path), then)
                    os.chmod(path, first)
                    p = self.run_vt(*args, "--dry-run", "8", RSWAP_AFTER=path, RSWAP_FROM=swapped, RSWAP_TO=path)
                    self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), then, "the test did not swap the file")
                    if ok:
                        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    else:
                        self.refused(p, "is writable by group or others")
                    os.chmod(path, 0o755)

    def read_dirs(self):
        return [n for n in os.listdir(self.s.standins) if n.startswith("run-vt-read.")]

    def test_a_report_that_is_not_numbers_is_refused_and_run_by_no_one(self):
        # Root reads the owner, mode and link count the user's process reports as numbers, so a report
        # that holds shell syntax is refused before anything evaluates it.
        pwned = os.path.join(self.s.tmp, "pwned")
        for report in (f"file 0 0)+x[$(touch {pwned})]+(0 1 regular file", "file x 755 1 regular file",
                       f"file 0 755 1$(touch {pwned}) regular file", "file 0 7555555 1 regular file"):
            with self.subTest(report=report):
                p = self.run_vt("--dry-run", "8", RFORGE=report)
                self.refused(p, "is not an owner, mode and link count")
                self.assertFalse(os.path.exists(pwned))

    def test_a_file_that_is_not_regular_is_refused(self):
        fonts = os.path.join(self.s.standins, "vtium-fonts")
        os.mkdir(fonts)
        frit = os.path.join(fonts, "system.vtfrit")
        write(frit, "font=/usr/share/fonts/x.ttf\n", 0o644)
        report = f"file {os.getuid()} 755 1 directory"
        for what, path, args in (("vtium", self.s.bin, []), ("glyph helper", self.s.helper, [f"--helper={self.s.helper}"]),
                                 ("image helper", self.s.image, [f"--image={self.s.image}"])):
            with self.subTest(what=what):
                p = self.run_vt(*args, "--dry-run", "8", RFORGE=report, RFORGE_FOR=path)
                self.refused(p, f"{path} is not a regular file")

    def test_a_symlink_put_at_a_path_just_before_it_is_read_is_refused(self):
        fonts = os.path.join(self.s.standins, "vtium-fonts")
        os.mkdir(fonts)
        frit = os.path.join(fonts, "system.vtfrit")
        write(frit, "font=/usr/share/fonts/x.ttf\n", 0o644)
        other = os.path.join(self.s.tmp, "other")
        write(other, "#!/bin/bash\n", 0o755)
        for what, path, args in (("vtium", self.s.bin, []), ("glyph helper", self.s.helper, [f"--helper={self.s.helper}"]),
                                 ("image helper", self.s.image, [f"--image={self.s.image}"]), ("frit", frit, [])):
            with self.subTest(what=what):
                kept = read(path)
                p = self.run_vt(*args, "--dry-run", "8", RLINK_FOR=path, RLINK_TARGET=other)
                self.refused(p, "is a symlink")
                os.unlink(path)
                write(path, kept, 0o755)

    def test_tic_compiles_what_was_read(self):
        # The terminfo source is read as the user like the other files, and tic compiles that copy.
        ti = os.path.join(ROOT, "crates", "vtium", "install", "xterm-vtium.ti")
        other = os.path.join(self.s.tmp, "other.ti")
        write(other, "xterm-vtium|substituted description,\n\tam,\n", 0o644)
        seen = os.path.join(self.s.tmp, "stage-copy")
        os.mkdir(seen)
        p = self.run_vt("8", RSUBST_FOR=ti, RSUBST_DATA=other, BIN_STAGE_COPY=seen)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        compiled = [os.path.join(d, f) for d, _, fs in os.walk(os.path.join(seen, "terminfo")) for f in fs]
        self.assertEqual(len(compiled), 1)
        with open(compiled[0], "rb") as f:
            self.assertIn(b"substituted description", f.read())

    def test_the_directory_the_files_are_read_into_is_removed(self):
        self.assertEqual(self.run_vt("--dry-run", "8").returncode, 0)
        self.assertEqual(self.read_dirs(), [])
        self.assertEqual(self.run_vt("8").returncode, 0)
        self.assertEqual(self.read_dirs(), [])
        os.chmod(self.s.helper, 0o775)
        self.refused(self.run_vt(f"--helper={self.s.helper}", "8"), "is writable by group or others")
        self.assertEqual(self.read_dirs(), [])
        os.chmod(self.s.helper, 0o755)
        open(self.s.calls, "w").close()
        self.interrupted(signal.SIGTERM, 143)
        self.assertEqual(self.read_dirs(), [])

    def test_a_run_starts_the_helper_built_with_it_from_a_copy_of_roots(self):
        helper = self.s.helper
        p = self.run_vt(f"--helper={helper}", "--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        stage = os.path.join(self.s.standins, "run-vtium-vt")
        for line in (f"+ rm -rf -- {stage}", f"+ install -d -m 0755 -- {stage} {stage}/terminfo",
                     f"+ install -m 0755 -- {helper} {stage}/vtium-config"):
            self.assertIn(line, p.stdout)
        self.assertIn(f" --font-helper={stage}/vtium-config ", p.stdout)
        # the system's frits and the acceptance pass's, staged for OSC 50 (ADR-1501 D6)
        self.assertIn(f" --font-frits={stage}/frits ", p.stdout)
        for name in ("acceptance-bitmap", "acceptance-outline"):
            self.assertRegex(p.stdout, rf"\+ install -m 0644 -- \S+/tools/vt/sections/frits/{name}\.vtfrit "
                                       rf"{re.escape(stage)}/frits/{name}\.vtfrit\n")
        self.assertFalse(os.path.exists(stage))
        # A real run: vtium gets the copy's path, and the copy, the compiled
        # entry and their directory are gone once the run ends.
        p = self.run_vt(f"--helper={helper}", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertIn(f"--font-helper={stage}/vtium-config", argv)
        self.assertNotIn("--term=kmscon", argv)
        self.assertFalse(os.path.exists(stage))
        # Without --helper, vtium starts the helper it was built with.
        p = self.run_vt("8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertFalse([a for a in read(self.s.bin_args).splitlines() if a.startswith("--font-helper")])

    def test_a_run_with_pictures_gives_vtium_a_copy_of_its_image_helper(self):
        # The acceptance pass's helper pages need the image helper built with
        # the console; the run stages it as it stages the glyph helper.
        image = self.s.image
        p = self.run_vt(f"--image={image}", "--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        stage = os.path.join(self.s.standins, "run-vtium-vt")
        self.assertIn(f"+ install -m 0755 -- {image} {stage}/vtium-image\n", p.stdout)
        self.assertIn(f" --image-helper={stage}/vtium-image ", p.stdout)
        p = self.run_vt(f"--image={image}", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"--image-helper={stage}/vtium-image", read(self.s.bin_args).splitlines())
        self.assertFalse(os.path.exists(stage))
        # Without --image, vtium starts none of the run's.
        p = self.run_vt("8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertFalse([a for a in read(self.s.bin_args).splitlines() if a.startswith("--image-helper")])

    def test_vtium_reads_the_reviews_config_from_a_copy_of_roots(self):
        # vtium reads a vtium.conf only when root owns it and no one else can
        # write it or a directory above it (ADR-1403 D3), so the run copies
        # the review's config, read as the user, into the stage, a directory
        # of root's, and names that directory as --configdir. The checkout's
        # own directory is never vtium's config directory.
        source = os.path.join(VT, "config", "vtium.conf")
        stage = os.path.join(self.s.standins, "run-vtium-vt")
        p = self.run_vt("--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"+ install -d -m 0755 -- {stage} {stage}/terminfo {stage}/frits {stage}/config\n",
                      p.stdout)
        self.assertIn(f"+ install -m 0644 -- {source} {stage}/config/vtium.conf\n", p.stdout)
        self.assertIn(f" --configdir {stage}/config ", p.stdout)
        self.assertNotIn(f"--configdir {VT}/config", p.stdout)
        self.assertFalse(os.path.exists(stage))
        # A real run: the copy holds the checkout's file, mode 0644, and is
        # gone once the run ends.
        seen = os.path.join(self.s.tmp, "stage-copy")
        os.mkdir(seen)
        p = self.run_vt("8", BIN_STAGE_COPY=seen)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        argv = read(self.s.bin_args).splitlines()
        at = argv.index("--configdir")
        self.assertEqual(argv[at + 1], f"{stage}/config")
        copy = os.path.join(seen, "config", "vtium.conf")
        self.assertEqual(read(copy), read(source))
        self.assertEqual(stat.S_IMODE(os.stat(copy).st_mode), 0o644)
        self.assertFalse(os.path.exists(stage))

    def test_an_image_helper_another_user_could_have_written_is_refused(self):
        for mode in (0o775, 0o757):
            with self.subTest(mode=oct(mode)):
                os.chmod(self.s.image, mode)
                self.refused(self.run_vt(f"--image={self.s.image}", "--dry-run", "8"),
                             "is writable by group or others")
        os.chmod(self.s.image, 0o755)
        self.refused(self.run_vt(f"--image={self.s.image}-missing", "--dry-run", "8"), "no image helper at")

    def test_a_frit_the_run_copies_as_root_is_a_file_of_its_own(self):
        # The run copies the system's frits as root into a directory everyone
        # reads, so a symlink or a second hard link, which could name a file
        # the user may not read, is refused, and so is a frit another user
        # could have written.
        fonts = os.path.join(self.s.standins, "vtium-fonts")
        os.mkdir(fonts)
        stage = os.path.join(self.s.standins, "run-vtium-vt")
        frit = os.path.join(fonts, "system.vtfrit")
        write(frit, "font=/usr/share/fonts/x.ttf\n")
        p = self.run_vt("--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"+ install -m 0644 -- {frit} {stage}/frits/system.vtfrit\n", p.stdout)
        self.assertRegex(p.stdout, rf"\+ install -m 0644 -- \S+/tools/vt/sections/frits/acceptance-bitmap\.vtfrit {re.escape(stage)}/frits/acceptance-bitmap\.vtfrit\n")
        secret = os.path.join(self.s.tmp, "secret")
        write(secret, "not the user's to read\n")
        link = os.path.join(fonts, "link.vtfrit")
        os.symlink(secret, link)
        self.refused(self.run_vt("--dry-run", "8"), "is a symlink")
        os.unlink(link)
        os.link(secret, link)
        self.refused(self.run_vt("--dry-run", "8"), "has 2 hard links")
        os.unlink(link)
        os.chmod(frit, 0o664)
        self.refused(self.run_vt("--dry-run", "8"), "is writable by group or others")
        os.chmod(frit, 0o644)
        # A system frit named as one of the acceptance pass's is refused:
        # OSC 50 would reach whichever was copied last.
        same = os.path.join(fonts, "acceptance-outline.vtfrit")
        write(same, "font=/usr/share/fonts/y.ttf\n")
        self.refused(self.run_vt("--dry-run", "8"), "are both named acceptance-outline.vtfrit")
        os.unlink(same)
        self.assertFalse(os.path.exists(stage))

    def test_a_helper_another_user_could_have_written_is_refused(self):
        for mode in (0o775, 0o757):
            with self.subTest(mode=oct(mode)):
                os.chmod(self.s.helper, mode)
                self.refused(self.run_vt(f"--helper={self.s.helper}", "--dry-run", "8"),
                             "is writable by group or others")
        os.chmod(self.s.helper, 0o755)
        self.refused(self.run_vt(f"--helper={self.s.helper}-missing", "--dry-run", "8"), "no glyph helper at")

    def test_the_staged_files_are_readable_whatever_the_umask(self):
        # The run stages as root under the caller's umask; the session, which
        # runs as the user, must read the entry even when that umask is 077.
        t = os.path.join(self.s.tmp, "shell.transcript")
        typed = os.path.join(self.s.tmp, "typed")
        write(typed, "\n", 0o644)
        env = self.s.env(BIN_MODE="session", BIN_STDIN=typed, LOGIN_SHELL=self.s.login_shell)
        p = subprocess.run(["bash", "-c", 'umask 077; exec bash "$@"', "bash", RUN_VT, f"--bin={self.s.bin}",
                            f"--helper={self.s.helper}", "--shell", f"--transcript={t}", "8"],
                           env=env, capture_output=True, text=True, timeout=WAIT_BOUND, cwd=self.s.tmp, start_new_session=True)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        text = read(t).replace("\r", "")
        self.assertIn("terminfo: xterm-vtium\n", text)
        self.assertIn("terminfo modes: 755 755 644\n", text)

    def test_a_login_reads_the_systems_entry_or_runs_as_kmscon(self):
        # /bin/login passes no TERMINFO, so with no xterm-vtium entry in the
        # system's terminfo the run names TERM=kmscon and says why.
        p = self.run_vt("--login", "--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(" --term=kmscon ", p.stdout)
        self.assertIn("The system has no xterm-vtium terminfo entry", p.stdout)
        entry = os.path.join(self.s.standins, "terminfo", "x")
        os.makedirs(entry)
        write(os.path.join(entry, "xterm-vtium"), "", 0o644)
        p = self.run_vt("--login", "--dry-run", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertNotIn("--term=kmscon", p.stdout)
        self.assertNotIn("The system has no xterm-vtium terminfo entry", p.stdout)

    def test_a_guide_runs_as_the_user_with_its_vt_and_steps(self):
        guide = os.path.join(VT, "checks", "start.guide")
        p = self.run_vt(f"--guide={os.path.relpath(guide, self.s.tmp)}", "--steps=2,1", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertIn("--oneshot", argv)
        self.assertEqual(argv[argv.index("/usr/bin/env"):],
                         ["/usr/bin/env", f"TERMINFO={self.s.standins}/run-vtium-vt/terminfo",
                          "GUIDE_VT=8", "GUIDE_DESKTOP=7", "GUIDE_STEPS=2,1",
                          "/bin/bash",
                          os.path.join(VT, "checks", "guide.sh"), guide])
        for opts, why in ((["--steps=1;2"], "--steps takes step numbers"),
                          (["--steps=0"], "--steps takes step numbers"),
                          (["--login"], "with no login")):
            with self.subTest(opts=opts):
                self.refused(self.run_vt(f"--guide={guide}", *opts, "8"), why)
        self.refused(self.run_vt("--steps=1", "8"), "--steps goes with --guide")

    def test_a_login_ends_at_the_first_logout(self):
        p = self.run_vt("--login", "8", BIN_MODE="wait", SEQ="- - 8 8 8:closing", timeout=WAIT_BOUND)
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
                          "--inh-caps=-all", "--reset-env", "--", *CARRY, "/usr/bin/env",
                          "-C", self.s.home, f"TERMINFO={self.s.standins}/run-vtium-vt/terminfo",
                          SCRIPT, "-q", "-f", "-c", f"exec {self.s.login_shell} -l", t])
        self.assertIn(f"with {USER}'s shell", p.stdout)
        self.assertEqual(self.s.calls_of("getent"), [f"passwd {USER}"])
        # Without a transcript, the shell is the session itself.
        p = self.run_vt("--shell", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        self.assertEqual(argv[argv.index("/usr/bin/env"):],
                         ["/usr/bin/env", "-C", self.s.home, f"TERMINFO={self.s.standins}/run-vtium-vt/terminfo",
                          "/bin/bash", "-l"])

    def shell_env(self, **env):
        """The words between the shell's env and the shell itself."""
        p = self.run_vt("--shell", "8", **env)
        self.assertEqual(p.returncode, 0, p.stderr)
        argv = read(self.s.bin_args).splitlines()
        return argv[argv.index(self.s.home) + 1:-3]

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

    def test_debug_copies_the_drm_state_at_each_display_change_and_at_the_end(self):
        dri = self.s.cards()
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        changes = [logged(fmt, values) for _, fmt, values, _ in DISPLAY_LINES]
        others = ["preparing the modeset of HDMI-A-2 at 800x480",
                  "the last display of head panel is gone: the session runs on at 100x30 until the next comes",
                  "not a display HDMI-A-2 is gone", "display HDMI-A-2 at 800x480 again"]
        lines = [changes[0], others[0], changes[1], others[1], *changes[2:], *others[2:]]
        p = self.run_vt(f"--debug={drm}", "8", BIN_LOG="".join(l + "\\n" for l in lines))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("--debug", read(self.s.bin_args).splitlines())
        copies = [f"{n:03d}-{what}" for n, (*_, what) in enumerate(DISPLAY_LINES, 1)]
        names = [f"{n}-{c}.state" for n in (*copies, "end") for c in "01"]
        self.assertEqual(sorted(os.listdir(drm)), names)
        for name in names:
            with self.subTest(name=name):
                self.assertEqual(read(os.path.join(drm, name)), read(os.path.join(dri, name[-7], "state")))
                self.assertEqual(stat.S_IMODE(os.stat(os.path.join(drm, name)).st_mode), 0o600)
        # Every line of vtium's reaches the log, each copy named after the
        # line that made it.
        out = p.stdout.splitlines()
        self.assertEqual([l for l in out if l in lines], lines)
        for line, copy in zip(changes, copies):
            self.assertEqual(out[out.index(line) + 1],
                             f"run-vt: the DRM state, in {drm}: {copy}-0.state {copy}-1.state")
        # vtium's output ends with vtium, so the copy at the end waits for its
        # last line, and not for the timeout.
        self.assertNotIn("still holds its log open", p.stdout)
        # The state at the end is copied before the console unit starts again.
        calls = read(self.s.calls).splitlines()
        end = max(i for i, c in enumerate(calls) if c.startswith("setpriv ") and "end-1.state" in c)
        self.assertLess(end, calls.index("systemctl start kmsconvt@tty8.service"))

    def test_no_process_run_as_the_user_inherits_the_hold(self):
        # Root's read-write descriptor of the VT would let the user's process,
        # or anyone who traces it, write to the VT and take it. The copies of
        # the DRM state run as the user, during the run and at its end.
        self.s.cards()
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        held = os.path.join(self.s.tmp, "setpriv-hold")
        p = self.run_vt(f"--debug={drm}", "8", BIN_LOG=logged(*DISPLAY_LINES[0][1:3]) + "\\n", SETPRIV_HOLD=held)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("end-0.state", os.listdir(drm))
        # The checks before the hold, a copy at the display line and one at
        # the end, two cards each: none holds the VT.
        self.assertEqual(read(held).split(), ["free"] * len(read(held).split()))
        self.assertGreaterEqual(len(read(held).split()), 4)

    def test_the_watched_lines_are_the_ones_vtium_logs(self):
        # A line reworded in the code would never make a copy again.
        for path, fmt, _, _ in DISPLAY_LINES:
            with self.subTest(fmt=fmt):
                self.assertIn(f'"{fmt}"', read(os.path.join(ROOT, path)))

    def test_debug_stops_the_watcher_when_something_holds_vtiums_output(self):
        self.s.cards()
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        proc = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", f"--debug={drm}", "8"],
                                env=self.s.env(BIN_MODE="linger", BIN_LOG="display HDMI-A-2 is gone\\n"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp,
                                start_new_session=True)
        try:
            # The output closes after the 5 s grace, long before the sleep
            # ends.
            out, err = proc.communicate(timeout=WAIT_BOUND)
            self.assertEqual(proc.returncode, 0, out + err)
            self.assertIn("run-vt: something vtium started still holds its log open", out)
            self.assertEqual(sorted(os.listdir(drm)), ["001-went-0.state", "001-went-1.state",
                                                       "end-0.state", "end-1.state"])
            self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])
            # Of the run's processes, only the stand-in's sleep is left.
            left = []
            for pid in filter(str.isdigit, os.listdir("/proc")):
                try:
                    with open(f"/proc/{pid}/stat") as f:
                        text = f.read()
                except OSError:
                    continue
                if int(text[text.rindex(")") + 2:].split()[3]) == proc.pid:
                    left.append(text[text.index("(") + 1:text.rindex(")")])
            self.assertEqual(left, ["sleep"])
        finally:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.kill()
            proc.communicate()

    def test_debug_copies_the_state_at_the_end_of_an_interrupted_run(self):
        self.s.cards()
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        proc = subprocess.Popen(["bash", RUN_VT, f"--bin={self.s.bin}", f"--debug={drm}", "8"],
                                env=self.s.env(BIN_MODE="wait", BIN_LOG="display HDMI-A-2 is gone\\n"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=self.s.tmp)
        end = time.monotonic() + WAIT_BOUND
        while not os.path.exists(os.path.join(drm, "001-went-1.state")) and time.monotonic() < end:
            time.sleep(0.02)
        self.assertTrue(os.path.exists(os.path.join(drm, "001-went-1.state")), "no copy at the display line")
        self.wait_ready()
        proc.send_signal(signal.SIGTERM)
        out, err = proc.communicate(timeout=WAIT_BOUND)
        self.assertEqual(proc.returncode, 143, out + err)
        self.assertEqual(read(self.s.signals), "TERM\n", "vtium was not stopped with SIGTERM")
        self.assertEqual(sorted(os.listdir(drm)), ["001-went-0.state", "001-went-1.state", "end-0.state", "end-1.state"])
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty8.service", "start kmsconvt@tty8.service"])

    def test_debug_with_no_card_says_so(self):
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        p = self.run_vt(f"--debug={drm}", "8", BIN_LOG="display HDMI-A-2 is gone\\n")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        dri = os.path.join(self.s.standins, "debugfs-dri")
        self.assertEqual(p.stdout.count(f"run-vt: no card's DRM state to copy under {dri}\n"), 2, p.stdout)
        self.assertEqual(os.listdir(drm), [])

    def test_debug_keeps_the_fifo_out_of_the_callers_tmpdir(self):
        # Root makes the FIFO's directory where it alone can write, so a
        # TMPDIR the caller kept through sudo is never used.
        self.s.cards()
        drm = os.path.join(self.s.tmp, "drm")
        os.mkdir(drm, 0o700)
        p = self.run_vt(f"--debug={drm}", "8", BIN_LOG="display HDMI-A-2 is gone\\n",
                        TMPDIR=os.path.join(self.s.tmp, "nowhere"))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("001-went-0.state", os.listdir(drm))

    def test_debug_needs_a_directory_the_user_can_write(self):
        os.symlink(self.s.tmp, os.path.join(self.s.tmp, "link"))
        for where in ("none", "link"):
            with self.subTest(where=where):
                self.refused(self.run_vt("--debug=" + os.path.join(self.s.tmp, where), "8"),
                             "cannot write the directory")
        self.refused(self.run_vt("--debug=", "8"), "--debug needs the directory")
        self.assertEqual(self.s.changes(), [])
        self.assertFalse(os.path.exists(self.s.bin_args))
        # A dry run names the option it would give vtium.
        p = self.run_vt("--dry-run", "--debug=/nowhere", "8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"+ {self.s.standins}/run-vtium-vt/vtium --vt=tty8 --no-switchvt --configdir {self.s.standins}/run-vtium-vt/config --oneshot "
                      f"--font-frits={self.s.standins}/run-vtium-vt/frits --debug --login --",
                      p.stdout)

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
        # VT 6 is logind's reserved VT by default, which no run takes.
        for active, seq, conf, want in (("tty7", "-", "", "5"), ("tty5", "-", "", "4"),
                                        ("tty5", "-", "[Login]\nReserveVT=0\n", "6"), ("tty5", "6", "", "4"),
                                        ("tty7", "5,6,4", "", "3"), ("tty3", "5,6,4", "", "2"),
                                        ("tty1", "2,7", "", "5")):
            with self.subTest(active=active, seq=seq, conf=conf):
                self.s.active(active)
                if os.path.exists(os.path.join(self.s.tmp, "count")):
                    os.unlink(os.path.join(self.s.tmp, "count"))
                p = self.vt_sh(VTIUM_VT_DRY_RUN="1", SEQ=seq, LOGIND_CONF=conf)
                self.assertEqual(self.picked(p), want, p.stdout + p.stderr)

    def test_every_vt_busy_names_each_and_why(self):
        self.s.active("tty5")
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", SEQ="6,4,3,2")
        self.refused(p, "no free VT")
        for want in ["VT 5: it is on screen", "VT 6: it is logind's reserved VT"] + \
                [f"VT {v}: logind session c{v}_active" for v in (4, 3, 2)]:
            self.assertIn(want, p.stderr)

    def installed(self, vt):
        """The stand-in manifest make vt-install writes for VT vt."""
        state = os.path.join(self.s.standins, "root", "var", "lib", "vtium-dev-install")
        os.makedirs(state, exist_ok=True)
        write(os.path.join(state, f"tty{vt}"), f"vt={vt}\ngetty-enabled=yes\ngetty-active=yes\n", 0o644)

    def enabled(self, name, vt):
        """NAME@ttyVT enabled under the stand-ins' root, as systemctl enable
        leaves it: the unit file in /usr/lib and its link in getty.target's
        wants."""
        root = os.path.join(self.s.standins, "root")
        lib = os.path.join(root, "usr", "lib", "systemd", "system")
        wants = os.path.join(root, "etc", "systemd", "system", "getty.target.wants")
        os.makedirs(lib, exist_ok=True)
        os.makedirs(wants, exist_ok=True)
        write(os.path.join(lib, f"{name}@.service"),
              "[Service]\nExecStart=/usr/bin/vtium --vt=%I\n\n[Install]\nWantedBy=getty.target\n", 0o644)
        os.symlink(f"/usr/lib/systemd/system/{name}@.service", os.path.join(wants, f"{name}@tty{vt}.service"))

    def test_the_development_vt_is_picked(self):
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl: is-enabled reads the links through systemctl --root")
        # The development VT is the one with a manifest of make vt-install's
        # whose vtium-dev@ttyN is enabled; with only one of the two, the run
        # takes the first free VT.
        self.installed(3)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1")
        self.assertEqual(self.picked(p), "5", p.stdout + p.stderr)
        shutil.rmtree(os.path.join(self.s.standins, "root"))
        self.enabled("vtium-dev", 3)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1")
        self.assertEqual(self.picked(p), "5", p.stdout + p.stderr)
        self.installed(3)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", ACTIVE_UNITS="vtium-dev@tty3.service autovt@tty3.service",
                       AUTOVT_IS="3=vtium-dev@tty3.service")
        self.assertEqual(self.picked(p), "3", p.stdout + p.stderr)
        self.assertIn("vt: VT 3 is the development VT (make vt-install VT=3); the run borrows it and gives it "
                      "back as it found it", p.stdout)
        self.assertIn(" stop vtium-dev@tty3.service", p.stdout)
        self.assertIn(" start vtium-dev@tty3.service", p.stdout)
        # VT= still names another; two development VTs: the first.
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", VTIUM_VT_NR="4")
        self.assertEqual(self.picked(p), "4", p.stdout + p.stderr)
        self.installed(2)
        self.enabled("vtium-dev", 2)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1")
        self.assertEqual(self.picked(p), "2", p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])

    def test_a_development_vt_with_a_login_is_refused(self):
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl: is-enabled reads the links through systemctl --root")
        self.installed(3)
        self.enabled("vtium-dev", 3)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", SEQ="3")
        self.refused(p, "the development VT 3 is refused: logind session c3_active (tester tty) is on it; "
                        "log out there, or name another VT with VT=N")
        self.assertEqual(self.s.calls_of("sudo"), [])

    def test_a_vt_the_packaged_vtium_holds_is_never_borrowed(self):
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl: is-enabled reads the links through systemctl --root")
        why = "VT 5 refused: the packaged vtium@tty5 holds it; sudo systemctl disable --now vtium@tty5 frees it"
        # Enabled, it holds the VT at the VT's next start; running, it holds
        # it now; named by autovt@tty5, logind starts it at a switch to VT 5.
        self.enabled("vtium", 5)
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1")
        self.assertEqual(self.picked(p), "4", p.stdout + p.stderr)
        self.refused(self.vt_sh(VTIUM_VT_NR="5"), why)
        shutil.rmtree(os.path.join(self.s.standins, "root"))
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", ACTIVE_UNITS="vtium@tty5.service")
        self.assertEqual(self.picked(p), "4", p.stdout + p.stderr)
        self.refused(self.vt_sh(VTIUM_VT_NR="5", ACTIVE_UNITS="vtium@tty5.service"), why)
        switched = {"AUTOVT_IS": "5=vtium@tty5.service 4=vtium@tty4.service 3=vtium-dev@tty3.service"}
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", **switched)
        self.assertEqual(self.picked(p), "3", p.stdout + p.stderr)
        self.refused(self.vt_sh(VTIUM_VT_NR="5", **switched),
                     "VT 5 refused: logind starts the packaged vtium@tty5 there when the VT is switched to "
                     "(autovt@tty5 is vtium@tty5); vtium console status shows what holds each VT")
        # Every VT the packaged vtium's: no free VT, each named.
        p = self.vt_sh(VTIUM_VT_DRY_RUN="1", AUTOVT_IS=" ".join(f"{v}=vtium@tty{v}.service" for v in (2, 3, 4, 5)))
        self.refused(p, "no free VT")
        for v in (2, 3, 4, 5):
            self.assertIn(f"VT {v}: logind starts the packaged vtium@tty{v} there", p.stderr)
        self.assertEqual(self.s.calls_of("sudo"), [])

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
        self.refused(self.vt_sh(VTIUM_VT_SESSION="guide", VTIUM_VT_GUIDE="nonesuch"), "the guides in tools/vt/checks: blank chords daily restart start token vt wave-a")
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
        self.wait_ready()
        # Ctrl-C reaches the terminal's whole foreground process group.
        os.killpg(proc.pid, signal.SIGINT)
        out, err = proc.communicate(timeout=WAIT_BOUND)
        self.assertEqual(proc.returncode, 130, out + err)
        self.assertIn("vt: interrupted", err)
        self.assertIn("vtium log: ", out)
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])

    def test_a_home_directory_with_a_space_keeps_the_acceptance_passes(self):
        # With no ACCEPT_DIR and no XDG_STATE_HOME, vt.sh keeps the passes under $HOME/.local/state.
        home = os.path.join(self.s.tmp, "a home & more")
        os.mkdir(home)
        p = self.vt_sh(HOME=home, ACCEPT_DIR="", ACTIVE_UNITS="kmsconvt@tty5.service")
        passes = os.path.join(home, ".local", "state", "vtium", "acceptance")
        self.assertNotIn("--passes takes", p.stderr)
        self.assertIn(f"ACCEPT_DIR={passes}", read(self.s.bin_args).splitlines())
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])
        # A control character is still refused, before anything stops.
        open(self.s.calls, "w").close()
        p = self.run_vt("--passes=/tmp/a\nb", "--dry-run", "8")
        self.refused(p, "--passes takes an absolute path without control characters")
        self.refused(self.run_vt("--passes=relative", "--dry-run", "8"), "--passes takes an absolute path")

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
        self.wait_ready()
        os.killpg(proc.pid, signal.SIGINT)
        out, err = proc.communicate(timeout=WAIT_BOUND)
        self.assertEqual(proc.returncode, 0, out + err)
        self.assertIn("vt-shell: ended with Ctrl-C", out)
        self.assertRegex(out, r"vtium log: " + self.s.dir + r"/vt-[0-9T]+-tty5-\w{6}\.log\n"
                         r"transcript: " + self.s.dir + r"/vt-[0-9T]+-tty5-\w{6}\.transcript\n")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])
        p = self.vt_sh(VTIUM_VT_SESSION="shell", BIN_STATUS="101", ACTIVE_UNITS="kmsconvt@tty5.service")
        self.assertEqual(p.returncode, 101, p.stdout + p.stderr)
        self.assertIn("transcript: " + self.s.dir, p.stdout)

    def test_vt_debug_copies_the_drm_state_beside_the_log(self):
        self.s.cards()
        p = self.vt_sh(VTIUM_VT_DEBUG="1", VTIUM_VT_SESSION="shell", BIN_LOG="display HDMI-A-2 is gone\\n")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = os.path.join(self.s.dir, "vt-") + r"[0-9T]+-tty5-\w{6}"
        self.assertRegex(p.stdout, f"vtium log: {log}\\.log\ntranscript: {log}\\.transcript\n"
                                   f"DRM state: {log}\\.drm\n")
        drm = p.stdout.split("DRM state: ")[1].split("\n")[0]
        self.assertEqual(stat.S_IMODE(os.stat(drm).st_mode), 0o700)
        self.assertEqual(sorted(os.listdir(drm)), ["001-went-0.state", "001-went-1.state", "end-0.state", "end-1.state"])
        text = read(drm[:-len(".drm")] + ".log")
        self.assertIn(f"display HDMI-A-2 is gone\nrun-vt: the DRM state, in {drm}: 001-went-0.state 001-went-1.state\n",
                      text)
        self.assertIn(" --debug ", text)
        self.refused(self.vt_sh(VTIUM_VT_DEBUG="yes"), "VTIUM_VT_DEBUG=yes: 1 or nothing")

    def test_a_vt_debug_dry_run_names_the_drm_directory(self):
        p = self.vt_sh(VTIUM_VT_DEBUG="1", VTIUM_VT_DRY_RUN="1")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        line = next(l for l in p.stdout.splitlines() if l.startswith("+ "))
        self.assertRegex(line, r" --debug=" + self.s.dir + r"/vt-[0-9T]+-tty5-XXXXXX\.drm ")
        self.assertRegex(p.stdout, r"the DRM state would be copied into " + self.s.dir + r"/vt-[0-9T]+-tty5-XXXXXX\.drm")
        self.assertIn("/run-vtium-vt/frits --debug --login -- ", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])

    def test_the_start_guide_runs_and_its_log_reaches_the_caller(self):
        # Enter to begin; step 1: Enter, y, a note; step 2: Enter, the shell's
        # two lines, n and no note; Enter to end.
        answers = os.path.join(self.s.tmp, "answers")
        write(answers, "\n\ny" "all drawn\n" "\necho hello vtium\nexit\n" "n\n" "\n", 0o644)
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-check", "GUIDE=start",
                            f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}", "VT=4"],
                           env=self.s.env(BIN_MODE="session", BIN_STDIN=answers, ACTIVE_UNITS="getty@tty4.service"),
                           capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = read(os.path.join(self.s.dir, "vt-check.log"))
        self.assertRegex(log, r"=== vtium starts and runs a shell, .*, guide " + os.path.join(VT, "checks", "start.guide"))
        self.assertIn("-- step 1 of 2: The first screen\n  answer: yes  note: all drawn\n", log)
        self.assertIn("-- step 2 of 2: A shell prompt\n  answer: no\n=== done", log)
        # The caller prints what the run added, and where the log is.
        self.assertIn("  answer: yes  note: all drawn", p.stdout)
        self.assertIn("results: " + os.path.join(self.s.dir, "vt-check.log"), p.stdout)
        self.assertEqual(self.s.changes(), ["stop getty@tty4.service", "start getty@tty4.service"])

    def test_the_acceptance_pass_runs_and_its_pass_reaches_the_caller(self):
        # accept.sh reads its keys from the terminal make runs in. The
        # binary carries the line only a build with pictures carries, so the
        # pass records it with images.
        write(self.s.bin, BIN + "# cannot open the image helper {name}: {e}\n")
        env = self.s.env(BIN_MODE="session", BIN_STDIN="/dev/null", TERM="linux", ACTIVE_UNITS="kmsconvt@tty5.service")
        d = Pty(["/usr/bin/env", "make", "--no-print-directory", "-C", ROOT, "vt-accept", f"VTIUM_BIN={self.s.bin}",
                 f"VTIUM_CONFIG_BIN={self.s.helper}", f"VTIUM_IMAGE_BIN={self.s.image}"], env)
        d.wait_for(b"Switch to VT 5 now")
        d.wait_for(b"press a key to start the pass", timeout=WAIT_BOUND)
        d.send(b" ")
        d.page()
        d.wait_for(b"y yes  n no")
        d.send(b"y")
        d.page()
        d.wait_for(b"y yes  n no")
        d.send(b"q")
        d.wait_for(b"q exit")
        d.send(b"q")
        d.wait_for(b"results: " + self.s.passes.encode())
        self.assertEqual(d.finish(), 0)
        seen = d.seen.decode(errors="replace")
        self.assertRegex(seen, r"label +vtium dev build ")
        self.assertRegex(seen, r"vt +5\r\n")
        self.assertRegex(seen, r"features +images\r\n")
        self.assertIn("10-sgr                    yes", seen)
        records = [n for n in os.listdir(self.s.passes) if n.endswith(".md")]
        self.assertEqual(len(records), 1)
        self.assertIn(f"- binary: {self.s.bin}\n", read(os.path.join(self.s.passes, records[0])))
        self.assertEqual(self.s.calls_of("sudo")[0], "-v")
        self.assertEqual(self.s.changes(), ["stop kmsconvt@tty5.service", "start kmsconvt@tty5.service"])

    def test_the_shell_runs_and_its_paths_reach_the_caller(self):
        typed = os.path.join(self.s.tmp, "typed")
        write(typed, "ls -l\n", 0o644)
        self.s.locale("LANG=de_DE.UTF-8\n")
        self.s.runtime_dir_path = self.s.runtime_dir()
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-shell", f"VTIUM_BIN={self.s.bin}",
                            f"VTIUM_CONFIG_BIN={self.s.helper}", "VT=4"],
                           env=self.s.env(BIN_MODE="session", BIN_STDIN=typed, LOGIN_SHELL=self.s.login_shell,
                                          ACTIVE_UNITS="getty@tty4.service"),
                           capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = os.path.join(self.s.dir, "vt-") + r"[0-9T]+-tty4-\w{6}"
        self.assertRegex(p.stdout, r"health: 0 'panicked at' lines in vtium's log\n"
                                   f"vtium log: {log}\\.log\ntranscript: {log}\\.transcript\n")
        transcript = p.stdout.split("transcript: ")[1].split("\n")[0]
        self.assertEqual(stat.S_IMODE(os.stat(transcript).st_mode), 0o600)
        text = read(transcript).replace("\r", "")
        self.assertIn(f"login shell: -l as {USER} in {self.s.home}\n", text)
        self.assertIn("read: ls -l\n", text)
        self.assertIn("env: LANG=de_DE.UTF-8\n", text)
        self.assertIn(f"env: XDG_RUNTIME_DIR={self.s.runtime_dir_path}\n", text)
        # vtium's variables cross setpriv's reset of the environment.
        self.assertIn("env: VTIUM_HEAD=console\n", text)
        self.assertIn("env: VTIUM_CONSOLE=1\n", text)
        # The shell reads the xterm-vtium entry the run compiled from the
        # checkout, as the package ships it.
        self.assertIn("terminfo: xterm-vtium\n", text)
        self.assertEqual(self.s.calls_of("sudo")[0], "-v")
        self.assertEqual(self.s.changes(), ["stop getty@tty4.service", "start getty@tty4.service"])


class Make(Case):
    """The Makefile's guards, which act before anything is built."""

    def test_the_builds_find_the_callers_sysroot_cache(self):
        # The tests' builds fetch the amd64 sysroot first. Stage.env moves
        # HOME, so a cache found through HOME would be an empty one under the
        # tests' home, and each build would download every package from the
        # Debian mirror again (#309).
        sysroot = [os.path.join(ROOT, "tools", "arm", "sysroot"), "dir", "amd64"]
        callers = subprocess.run(sysroot, capture_output=True, text=True, check=True).stdout
        tests = subprocess.run(sysroot, env=self.s.env(), capture_output=True, text=True, check=True).stdout
        self.assertEqual(tests, callers)

    def make(self, *args, **env):
        # The review targets build the helper unless one is named; these
        # tests name the stand-in's, as they name the binary.
        if not any(a.startswith(("VTIUM_CONFIG_BIN=", "CONFIG_RELEASE=")) for a in args) and "VTIUM_CONFIG_BIN" not in env:
            args = (*args, f"VTIUM_CONFIG_BIN={self.s.helper}")
        # and the image helper, which vt-accept gives vtium with pictures
        if not any(a.startswith(("VTIUM_IMAGE_BIN=", "IMAGE_RELEASE=")) for a in args) and "VTIUM_IMAGE_BIN" not in env:
            args = (*args, f"VTIUM_IMAGE_BIN={self.s.image}")
        return subprocess.run(["make", "--no-print-directory", "-C", ROOT, *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=BUILD_BOUND, start_new_session=True)

    def test_help_lists_the_targets(self):
        p = self.make("help")
        for target in ("minspec", "test-tools", "vt-accept", "vt-login", "vt-check", "vt-shell"):
            self.assertIn(target, p.stdout)

    def test_the_binary_is_never_taken_from_the_environment(self):
        p = self.make("vt-accept", "DRY_RUN=1", VTIUM_BIN=self.s.bin)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_BIN is set in the environment", p.stderr)
        self.assertEqual(self.s.calls_of("sudo") + self.s.calls_of("loginctl"), [])

    def test_the_default_binary_is_the_minspec_build(self):
        # VTIUM_MINSPEC points the default at the stand-in, which the stand-in
        # cargo "builds".
        # The acceptance pass builds it with pictures unless FEATURES is
        # named on make's command line (ADR-1501 D5).
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_MINSPEC={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), ["rustc --profile minspec -p vtium --bin vtium --target x86_64-unknown-linux-gnu --features images -- -C link-arg=-Wl,-z,pack-relative-relocs"])
        self.assertIn(f"--bin={self.s.bin} ", p.stdout)
        self.assertIn(f"--helper={self.s.helper} ", p.stdout)
        self.assertIn(" --features=images ", p.stdout)
        open(self.s.calls, "w").close()
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_MINSPEC={self.s.bin}", "FEATURES=")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), ["rustc --profile minspec -p vtium --bin vtium --target x86_64-unknown-linux-gnu -- -C link-arg=-Wl,-z,pack-relative-relocs"])
        self.assertIn(" --features= ", p.stdout)
        self.assertNotIn("--image", p.stdout)

    def test_the_default_helper_is_the_release_build(self):
        # CONFIG_RELEASE points the default helper at the stand-in, which the
        # stand-in cargo "builds"; run-vt.sh runs vtium with the copy it makes.
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", f"CONFIG_RELEASE={self.s.helper}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), ["build --release -p vtium-config --target x86_64-unknown-linux-gnu"])
        self.assertIn(f"--helper={self.s.helper} ", p.stdout)
        self.assertIn(f"--font-helper={self.s.standins}/run-vtium-vt/vtium-config ", p.stdout)
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", VTIUM_CONFIG_BIN=self.s.helper)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_CONFIG_BIN is set in the environment", p.stderr)

    def test_another_binary_is_run_as_it_is(self):
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.s.calls_of("cargo"), [])

    def test_a_named_binary_is_recorded_with_the_features_it_was_built_with(self):
        # A binary named with VTIUM_BIN is run as it is: without the line only
        # a build with pictures carries, the pass records no features and
        # vtium gets no image helper, and with it, images and the helper.
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(" --features= ", p.stdout)
        self.assertNotIn("--image", p.stdout)
        self.assertEqual(self.s.calls_of("cargo"), [])
        write(self.s.bin, BIN + "# cannot open the image helper {name}: {e}\n")
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(" --features=images ", p.stdout)
        self.assertIn(f" --image={self.s.image} ", p.stdout)
        self.assertIn(f" --image-helper={self.s.standins}/run-vtium-vt/vtium-image ", p.stdout)
        # FEATURES on make's command line is taken as it is named.
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", "FEATURES=")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(" --features= ", p.stdout)

    def test_vt_check_without_a_guide_fails_before_the_build(self):
        p = self.make("vt-check")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("vt-check: GUIDE=NAME, a guide in tools/vt/checks: blank chords daily restart start token vt wave-a.", p.stderr)
        self.assertNotIn("cargo", p.stdout)

    def test_vt_debug_reaches_every_vt_target(self):
        for target in ("vt-accept", "vt-login", "vt-shell"):
            with self.subTest(target=target):
                p = self.make(target, "DRY_RUN=1", "VT_DEBUG=1", f"VTIUM_BIN={self.s.bin}")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertIn(" --debug=" + self.s.dir, p.stdout)
        p = self.make("vt-check", "GUIDE=start", "DRY_RUN=1", "VT_DEBUG=1", f"VTIUM_BIN={self.s.bin}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(" --debug=" + self.s.dir, p.stdout)
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}")
        self.assertNotIn("--debug", p.stdout)

    def test_a_guide_reaches_only_vt_check(self):
        # One exported in the shell does not turn vt-accept into a guided run.
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", GUIDE="start", GUIDE_STEPS="9")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertNotIn("--guide", p.stdout)
        p = self.make("vt-check", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", "GUIDE=start", "GUIDE_STEPS=2")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"--guide={os.path.join(VT, 'checks', 'start.guide')} --steps=2 5 --", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])

    # A stand-in cargo that records, beside its arguments, the build
    # settings for hwaccel and display-buffers when it is given them.
    CARGO_SETTINGS = "#!/bin/bash\n" + ('printf "%s\\n" "cargo ${VTIUM_HWACCEL+VTIUM_HWACCEL=$VTIUM_HWACCEL }'
                                        '${VTIUM_DISPLAY_BUFFERS+VTIUM_DISPLAY_BUFFERS=$VTIUM_DISPLAY_BUFFERS }'
                                        '$*" >>"$CALLS"\n')

    def test_every_review_target_hands_hwaccel_and_display_buffers_to_its_builds(self):
        # The console, the glyph helper and, with pictures, the image helper
        # each take the two settings, as make install's builds do (ADR-1500
        # D8).
        write(os.path.join(self.s.standins, "cargo"), self.CARGO_SETTINGS)
        builds = {"rustc --profile minspec -p vtium --bin vtium", "build --release -p vtium-config",
                  "build --release -p vtium-image-helper"}
        for target, extra in (("vt-accept", ()), ("vt-login", ()), ("vt-shell", ()), ("vt-check", ("GUIDE=start",))):
            with self.subTest(target=target):
                open(self.s.calls, "w").close()
                p = self.make(target, "DRY_RUN=1", *extra, f"VTIUM_MINSPEC={self.s.bin}",
                              f"CONFIG_RELEASE={self.s.helper}", f"IMAGE_RELEASE={self.s.image}",
                              "FEATURES=images", "HWACCEL=on", "DISPLAY_BUFFERS=2")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                calls = self.s.calls_of("cargo")
                want = builds if target == "vt-accept" else builds - {"build --release -p vtium-image-helper"}
                self.assertEqual({c for c in want if any(f" {c} " in f" {call} " for call in calls)}, want, calls)
                for call in calls:
                    self.assertTrue(call.startswith("VTIUM_HWACCEL=on VTIUM_DISPLAY_BUFFERS=2 "), call)

    def test_a_review_target_given_a_built_console_refuses_the_build_settings(self):
        # The settings are baked into a build, and a binary named with
        # VTIUM_BIN is built already, so naming them would change nothing.
        for setting in ("HWACCEL=on", "DISPLAY_BUFFERS=2"):
            with self.subTest(setting=setting):
                p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", setting)
                self.assertNotEqual(p.returncode, 0, p.stdout)
                self.assertIn("HWACCEL and DISPLAY_BUFFERS set the defaults a build bakes into the console", p.stderr)
                self.assertEqual(self.s.calls_of("cargo") + self.s.calls_of("sudo"), [])
        p = self.make("vt-accept", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", HWACCEL="on")
        self.assertEqual(p.returncode, 0, "one exported in the shell is not refused: " + p.stderr)



class Install(Case):
    maxDiff = None
    """vt-install.sh, make vt-install and make vt-uninstall, with the installed
    paths under the stand-ins' root."""

    UNIT = "etc/systemd/system/vtium-dev@.service"
    FALLBACK = "etc/systemd/system/vtium-dev-getty@.service"
    BIN = "usr/local/bin/vtium-dev"
    HELPER = "usr/local/bin/vtium-dev-config"
    STATE = "var/lib/vtium-dev-install"
    # Every file vt-install writes, by its path under the root.
    FILES = (BIN, HELPER, UNIT, FALLBACK)
    # What it writes beside them for a console with pictures: the image
    # helper and its account's sysusers file.
    IMAGE = "usr/local/bin/vtium-dev-image"
    SYSUSERS = "etc/sysusers.d/vtium-dev-image.conf"
    PICTURES = (IMAGE, SYSUSERS)

    def setUp(self):
        super().setUp()
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl: the unit links are read and written through systemctl --root")
        self.root = os.path.join(self.s.standins, "root")
        self.seed()

    def seed(self):
        """The root as this machine has it: getty@.service with its alias
        autovt@, the C reference's kmsconvt@.service, the template alias
        autovt@.service pointing at kmsconvt@, a getty enabled on VTs 1 to 6,
        and the programs the units run."""
        lib = self.at("usr/lib/systemd/system")
        etc = self.at("etc/systemd/system")
        os.makedirs(lib)
        os.makedirs(os.path.join(etc, "getty.target.wants"))
        write(os.path.join(lib, "getty.target"), "[Unit]\nDescription=Login Prompts\n", 0o644)
        write(os.path.join(lib, "getty@.service"),
              "[Unit]\nDescription=Getty on %I\nConflicts=rescue.service\n\n[Service]\n"
              "ExecStart=-/sbin/agetty --noclear - ${TERM}\nType=idle\n\n"
              "[Install]\nAlias=autovt@.service\nWantedBy=getty.target\nDefaultInstance=tty1\n", 0o644)
        write(os.path.join(lib, "kmsconvt@.service"),
              "[Unit]\nDescription=KMS System Console on %I\nConflicts=getty@%i.service\n"
              "OnFailure=getty@%i.service\n\n[Service]\nExecStart=/usr/bin/kmscon --vt=%I --seats=seat0\n\n"
              "[Install]\nAlias=autovt@.service\nWantedBy=getty.target\nDefaultInstance=tty1\n", 0o644)
        os.symlink("/usr/lib/systemd/system/kmsconvt@.service", os.path.join(etc, "autovt@.service"))
        for n in range(1, 7):
            os.symlink("/usr/lib/systemd/system/getty@.service",
                       os.path.join(etc, "getty.target.wants", f"getty@tty{n}.service"))
        for prog in ("sbin/agetty", "usr/bin/kmscon", "usr/bin/systemctl"):
            os.makedirs(os.path.dirname(self.at(prog)), exist_ok=True)
            write(self.at(prog), "#!/bin/sh\n")

    def links(self):
        """Every symlink under the root's /etc/systemd/system, with its target."""
        etc = self.at("etc/systemd/system")
        out = {}
        for d, _, names in os.walk(etc):
            for n in names:
                path = os.path.join(d, n)
                if os.path.islink(path):
                    out[os.path.relpath(path, etc)] = os.readlink(path)
        return out

    def vt_install(self, *args, **env):
        env = dict(dict(VTIUM_VT_BIN=self.s.bin, VTIUM_VT_CONFIG=self.s.helper, VTIUM_VT_IMAGE=self.s.image), **env)
        return subprocess.run(["bash", VT_INSTALL, *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=WAIT_BOUND, cwd=self.s.tmp)

    def at(self, path):
        return os.path.join(self.root, path)

    def shipped(self, unit_lines=None, fallback_lines=None):
        """The stand-in crates/vtium/install: the shipped units, the main
        one's lines passed through unit_lines and the fallback's through
        fallback_lines."""
        src = os.path.join(ROOT, "crates", "vtium", "install")
        dst = os.path.join(self.s.standins, "install")
        os.mkdir(dst)
        for f in ("vtium@.service", "vtium-getty@.service", "vtium-image.conf"):
            text = read(os.path.join(src, f))
            if f == "vtium@.service" and unit_lines:
                text = "\n".join(unit_lines(text.splitlines())) + "\n"
            if f == "vtium-getty@.service" and fallback_lines:
                text = "\n".join(fallback_lines(text.splitlines())) + "\n"
            write(os.path.join(dst, f), text, 0o644)

    def refused(self, p, why):
        super().refused(p, why)
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])

    def test_install_writes_the_binary_the_units_and_the_manifest_and_swaps_the_getty(self):
        p = self.vt_install("install", "4", ACTIVE_UNITS="getty@tty4.service")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(read(self.at(self.BIN)), read(self.s.bin))
        self.assertTrue(os.access(self.at(self.BIN), os.X_OK))
        self.assertEqual(read(self.at(self.HELPER)), read(self.s.helper))
        self.assertTrue(os.access(self.at(self.HELPER), os.X_OK))
        unit = read(self.at(self.UNIT)).splitlines()
        self.assertIn("ExecStart=/usr/local/bin/vtium-dev --vt=%I --no-switchvt", unit)
        self.assertIn("OnFailure=vtium-dev-getty@%i.service", unit)
        self.assertIn("Conflicts=getty@%i.service", unit)
        # Conflicts= sets no order, so vtium starts only once the getty, whose
        # stop hangs up and resets the VT, has stopped.
        self.assertIn("After=getty@%i.service", unit)
        # It stops the packaged vtium and the C reference's console on its
        # VT before it starts, as it stops the getty (ADR-1500 D4).
        for other in ("vtium@%i.service", "kmsconvt@%i.service"):
            self.assertIn(f"Conflicts={other}", unit)
            self.assertIn(f"After={other}", unit)
        self.assertEqual([l for l in unit if l.startswith("DefaultInstance=")], ["DefaultInstance=name-a-VT/vtium-dev@ttyN"])
        self.assertIn("WantedBy=getty.target", unit)
        directives = [l for l in unit if l and not l.startswith("#")]
        ordering = {f"{k}={u}" for k in ("Conflicts", "After") for u in ("vtium@%i.service", "kmsconvt@%i.service")}
        self.assertFalse([l for l in directives if l not in ordering and ("kmsconvt" in l or "/usr/bin/vtium" in l
                          or ("vtium@" in l and "vtium-dev" not in l))], directives)
        # The alias stays, so logind's autovt@tty4 on a switch to VT 4 is
        # vtium and never the C reference's console beside it.
        self.assertIn("Alias=autovt@.service", unit)
        links = self.links()
        self.assertEqual(links["autovt@tty4.service"], "/etc/systemd/system/vtium-dev@.service")
        self.assertEqual(links["autovt@.service"], "/usr/lib/systemd/system/kmsconvt@.service")
        fallback = read(self.at(self.FALLBACK)).splitlines()
        self.assertIn("ConditionPathExists=!/run/vtium/%i.closed", fallback)
        # The fallback runs the development binary's vtium console fallback,
        # which starts no getty whose start would stop another program.
        self.assertIn("ExecStart=/usr/local/bin/vtium-dev console fallback %i", fallback)
        manifest = read(self.at(self.STATE + "/tty4")).splitlines()
        self.assertEqual([l for l in manifest if not l.startswith("#")],
                         ["vt=4", "getty-enabled=yes", "getty-active=yes"])
        # vtium starts first, and its Conflicts= stops the getty, so the VT is
        # never left with neither; the getty is disabled after.
        self.assertEqual(self.s.changes(), ["daemon-reload", "enable vtium-dev@tty4.service",
                                            "restart vtium-dev@tty4.service", "disable getty@tty4.service"])
        self.assertIn("-v", self.s.calls_of("sudo"))
        self.assertIn("vtium-dev runs on VT 4", p.stdout)
        self.assertIn("make vt-uninstall VT=4", p.stdout)

    def sha(self, path):
        with open(self.at(path), "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()

    def test_install_records_a_checksum_for_each_file_it_writes(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in self.FILES))
        # A new build is installed over the one the record names, and the
        # record follows it.
        write(self.s.bin, read(self.s.bin) + "# a newer build\n")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"{self.sha(self.BIN)}  /{self.BIN}", read(self.at(self.STATE + "/files")).splitlines())

    def test_a_reinstall_leaves_only_the_new_builds_checksums(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        old_build = read(self.at(self.BIN))
        old_sha = self.sha(self.BIN)
        write(self.s.bin, read(self.s.bin) + "# a newer build\n")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in self.FILES))
        self.assertNotIn(f"{old_sha}  /{self.BIN}", record)
        # The old build put back is not the file vt-install wrote, so neither
        # an install on another VT nor uninstall touches it.
        write(self.at(self.BIN), old_build)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "5"), "/usr/local/bin/vtium-dev changed since vt-install wrote it")
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("kept /usr/local/bin/vtium-dev: it changed since vt-install wrote it", p.stdout)
        self.assertEqual(read(self.at(self.BIN)), old_build)

    def temps(self):
        """Every file under the root at one of vt-install's temporary names."""
        return [f for f in self.left() if f.endswith(".vt-install-new")]

    def test_a_reinstall_renames_each_new_file_into_place(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.temps(), [])
        # The running vtium holds its binary open, as these do each file.
        # A rename leaves the open file as it was, while a copy into the
        # same file would change what the running vtium reads.
        paths = (*self.FILES, self.STATE + "/files")
        held = {f: open(self.at(f), "rb") for f in paths}
        try:
            old = {f: held[f].read() for f in paths}
            write(self.s.bin, read(self.s.bin) + "# a newer build\n")
            p = self.vt_install("install", "4")
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
            self.assertEqual(read(self.at(self.BIN)), read(self.s.bin))
            for f in paths:
                with self.subTest(f=f):
                    held[f].seek(0)
                    self.assertEqual(held[f].read(), old[f])
                    self.assertNotEqual(os.stat(self.at(f)).st_ino, os.fstat(held[f].fileno()).st_ino)
        finally:
            for h in held.values():
                h.close()
        self.assertEqual(self.temps(), [])

    def test_each_copy_is_on_the_disk_before_it_is_renamed_into_place(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        calls = self.s.calls_of("sudo")
        renames = [i for i, c in enumerate(calls) if c.startswith("mv -fT -- ")]
        # The manifest, the record twice, the helper, the binary, the units.
        self.assertEqual(len(renames), 7, calls)
        for i in renames:
            temp = calls[i].split()[3]
            self.assertEqual(calls[i - 1], f"sync -- {temp}")
            self.assertTrue(calls[i - 2].startswith("install -D ") and calls[i - 2].endswith(f" {temp}"), calls[i - 2])
        # The manifest's rename is on the disk before any other copy starts.
        first = renames[0]
        self.assertEqual(calls[first].split()[4], self.at(self.STATE + "/tty4"))
        self.assertEqual(calls[first + 1], f"sync -- {self.at(self.STATE)}")

    def test_a_checksum_counts_only_for_the_path_it_was_recorded_for(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # vtium-dev@.service now holds what vtium-dev-getty@.service holds, whose
        # checksum the record has, but under the fallback's path.
        write(self.at(self.UNIT), read(self.at(self.FALLBACK)), 0o644)
        changed = read(self.at(self.UNIT))
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "5"), "/etc/systemd/system/vtium-dev@.service changed since vt-install wrote it")
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("kept /etc/systemd/system/vtium-dev@.service: it changed since vt-install wrote it", p.stdout)
        self.assertEqual(read(self.at(self.UNIT)), changed)

    def test_a_symlink_at_a_files_path_is_not_the_file_install_wrote(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # A symlink to a file that holds the same bytes as the binary
        # install wrote: its checksum matches, but install wrote no symlink.
        elsewhere = os.path.join(self.s.tmp, "vtium-elsewhere")
        shutil.copy(self.at(self.BIN), elsewhere)
        os.unlink(self.at(self.BIN))
        os.symlink(elsewhere, self.at(self.BIN))
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "5"), "/usr/local/bin/vtium-dev changed since vt-install wrote it")
        self.assertEqual(os.readlink(self.at(self.BIN)), elsewhere)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("kept /usr/local/bin/vtium-dev: it changed since vt-install wrote it", p.stdout)
        self.assertEqual(os.readlink(self.at(self.BIN)), elsewhere)

    def test_a_file_changed_since_install_wrote_it_is_neither_replaced_nor_removed(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        with open(self.at(self.UNIT), "a") as f:
            f.write("# a line the maintainer added\n")
        changed = read(self.at(self.UNIT))
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "5"), "/etc/systemd/system/vtium-dev@.service changed since vt-install wrote it")
        self.assertEqual(read(self.at(self.UNIT)), changed)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("kept /etc/systemd/system/vtium-dev@.service: it changed since vt-install wrote it", p.stdout)
        self.assertEqual(read(self.at(self.UNIT)), changed)
        for f in (self.BIN, self.HELPER, self.FALLBACK, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    # A stand-in sudo whose install fails the $FAIL_NTH copy (the first when
    # it is unset) to $FAIL_COPY as GNU install does on a full disk: it writes
    # half the file at the path it was given, then fails. The copy may go to
    # $FAIL_COPY itself or to the temporary name vt-install writes it under,
    # .NAME.vt-install-new in the same directory. $FAIL_COUNT counts the
    # copies to $FAIL_COPY.
    HALF_WRITING_SUDO = "#!/bin/bash\n" + RECORD + r"""
[ "$1" != -v ] || exit 0
export VTIUM_STANDIN_ROOT=1
if [ -n "${FAIL_COPY-}" ] && [ "$1" = install ]; then
	dst=${*: -1}
	if [ "$dst" = "$FAIL_COPY" ] || [ "$dst" = "${FAIL_COPY%/*}/.${FAIL_COPY##*/}.vt-install-new" ]; then
		n=$(( $(cat "$FAIL_COUNT" 2>/dev/null || echo 0) + 1 ))
		echo "$n" >"$FAIL_COUNT"
		if [ "$n" = "${FAIL_NTH:-1}" ]; then
			src=${*: -2:1}
			mkdir -p "${dst%/*}"
			rm -f "$dst"
			head -c $(( $(stat -c %s "$src") / 2 )) "$src" >"$dst"
			echo "install: error writing '$dst': No space left on device" >&2
			exit 1
		fi
	fi
fi
exec "$@"
"""

    def reseed(self):
        """The root as seed() makes it, the stand-in binary as Stage wrote it,
        and no calls recorded."""
        shutil.rmtree(self.root)
        self.seed()
        write(self.s.bin, BIN)
        open(self.s.calls, "w").close()

    def left(self):
        """What vt-install writes that is under the root: its files, its
        state directory, and any file at one of its temporary names."""
        out = [f for f in (*self.FILES, *self.PICTURES, self.STATE) if os.path.lexists(self.at(f))]
        for d, _, names in os.walk(self.root):
            out += [os.path.relpath(os.path.join(d, n), self.root) for n in names if n.endswith(".vt-install-new")]
        return out

    def fail_copy(self, vt, target, nth=1):
        """Runs install on VT vt with the copy to target failing halfway."""
        count = os.path.join(self.s.tmp, "fail-count")
        if os.path.exists(count):
            os.unlink(count)
        p = self.vt_install("install", vt, FAIL_COPY=self.at(target), FAIL_NTH=str(nth), FAIL_COUNT=count)
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        # The stand-in failed it, and no step before it did.
        self.assertIn("No space left on device", p.stderr)
        return p

    def test_a_copy_that_fails_halfway_can_be_undone_or_tried_again(self):
        write(os.path.join(self.s.standins, "sudo"), self.HALF_WRITING_SUDO)
        record = self.STATE + "/files"
        # Each copy, by its target and which copy to that target it is: the
        # record is written before the other copies and again after them.
        copies = ((record, 1), (self.HELPER, 1), (self.BIN, 1), (self.UNIT, 1), (self.FALLBACK, 1), (record, 2))
        for (target, nth), case, way in itertools.product(
                copies, ("first install", "reinstall", "a second VT"), ("uninstall", "install again")):
            with self.subTest(target=target, nth=nth, case=case, way=way):
                self.reseed()
                seeded = self.links()
                if case != "first install":
                    p = self.vt_install("install", "3" if case == "a second VT" else "4")
                    self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    write(self.s.bin, read(self.s.bin) + "# a newer build\n")
                self.fail_copy("4", target, nth)
                if way == "uninstall":
                    # Uninstall undoes the failed run whole, and keeps no
                    # half-written file as changed.
                    p = self.vt_install("uninstall", "4")
                    self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    self.assertNotIn("changed since", p.stdout)
                    if case != "a second VT":
                        self.assertEqual(self.links(), seeded)
                        self.assertEqual(self.left(), [])
                # A run after it installs: after uninstall, or straight
                # after the failed run, as the failure message says.
                p = self.vt_install("install", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(read(self.at(self.BIN)), read(self.s.bin))
                for vt in ("4", "3") if case == "a second VT" else ("4",):
                    p = self.vt_install("uninstall", vt)
                    self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    self.assertNotIn("changed since", p.stdout)
                self.assertEqual(self.links(), seeded)
                self.assertEqual(self.left(), [])

    def test_uninstall_clears_the_temporary_files_a_run_cut_short_left(self):
        # A run killed in the middle of its copies leaves a file at each
        # temporary name it was writing.
        for vt in ("3", "4"):
            p = self.vt_install("install", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        temps = []
        for f in (*self.FILES, self.STATE + "/files", self.STATE + "/tty4"):
            d, name = os.path.split(f)
            temps.append(os.path.join(d, f".{name}.vt-install-new"))
            write(self.at(temps[-1]), "half a file", 0o600)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual([t for t in temps if os.path.lexists(self.at(t))], [])
        for f in (*self.FILES, self.STATE + "/tty3"):
            self.assertTrue(os.path.exists(self.at(f)), f)
        p = self.vt_install("uninstall", "3")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])

    def test_uninstall_clears_what_a_killed_first_install_left(self):
        # A first install killed while it copied its manifest leaves the
        # state directory with the manifest's temporary file and no manifest.
        stray = self.STATE + "/.tty4.vt-install-new"
        os.makedirs(self.at(self.STATE))
        write(self.at(stray), "# Written by make vt-install\nvt=4\n", 0o600)
        p = self.vt_install("uninstall", "--dry-run", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"+ sudo rm -f -- {self.at(stray)}", p.stdout)
        self.assertEqual(self.left(), [self.STATE, stray])
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("removed the temporary files that a run cut short left", p.stdout)
        self.assertEqual(self.left(), [])
        # With VT 3 installed, the stray goes and VT 3's install stays.
        p = self.vt_install("install", "3")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        write(self.at(stray), "vt=4\n", 0o600)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.temps(), [])
        for f in (*self.FILES, self.STATE + "/tty3"):
            self.assertTrue(os.path.exists(self.at(f)), f)
        # Uninstall of another VT clears it too.
        write(self.at(stray), "vt=4\n", 0o600)
        p = self.vt_install("uninstall", "3")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])
        # With nothing left behind, there is still nothing to undo.
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("uninstall", "4"), "make vt-install did not install vtium on VT 4, so there is nothing to undo")

    def test_a_run_refuses_while_another_holds_the_lock(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        before = self.left()
        fd = os.open(self.at(self.STATE), os.O_RDONLY | os.O_DIRECTORY)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            # The lock is taken before sudo is asked for anything.
            for args in (("install", "5"), ("install", "4"), ("uninstall", "4"), ("install", "--dry-run", "5")):
                with self.subTest(args=args):
                    open(self.s.calls, "w").close()
                    self.refused(self.vt_install(*args), "another make vt-install or make vt-uninstall is running")
                    self.assertEqual(self.left(), before)
        finally:
            os.close(fd)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])

    def test_a_first_install_refuses_when_another_run_got_there_first(self):
        # A first install finds no state directory, reads what is installed
        # without the lock, and takes the lock once it has made the directory.
        # Here another install, on VT 5, runs while the first asks sudo.
        hook = os.path.join(self.s.tmp, "meanwhile")
        write(hook, f"#!/bin/bash\nexec bash {VT_INSTALL!r} install 5\n")
        write(os.path.join(self.s.standins, "sudo"), "#!/bin/bash\n" + RECORD + """
if [ "$1" = -v ]; then
	[ -z "${MEANWHILE-}" ] || [ -e "$MEANWHILE.done" ] || { : >"$MEANWHILE.done"; "$MEANWHILE" >&2 || exit 1; }
	exit 0
fi
export VTIUM_STANDIN_ROOT=1
exec "$@"
""")
        p = self.vt_install("install", "4", MEANWHILE=hook)
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("another make vt-install or make vt-uninstall ran while this one started; run this again", p.stderr)
        self.assertIn("vtium-dev runs on VT 5", p.stderr)
        # The first install changed nothing, and VT 5's install is whole.
        self.assertFalse(os.path.lexists(self.at(self.STATE + "/tty4")))
        self.assertNotIn("autovt@tty4.service", self.links())
        self.assertEqual([c for c in self.s.changes() if "tty4" in c], [])
        self.assertEqual(self.temps(), [])
        p = self.vt_install("uninstall", "5")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])

    def test_a_run_refuses_a_lock_on_a_state_directory_that_is_gone(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # Between this run's open of the directory and its lock, the last
        # uninstall removed it and a first install made a new one.
        state = self.at(self.STATE)
        write(os.path.join(self.s.standins, "flock"), "#!/bin/bash\n"
              f'mv {state!r} {state + ".gone"!r} && mkdir {state!r} && exec {shutil.which("flock")!r} "$@"\n')
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("uninstall", "4"),
                     "another make vt-install or make vt-uninstall removed /var/lib/vtium-dev-install while this one started")

    def test_a_failed_step_names_the_way_back(self):
        for verb in ("daemon-reload", "enable", "restart", "disable"):
            with self.subTest(verb=verb):
                p = self.vt_install("install", "4", FAIL_VERB=verb)
                self.assertNotEqual(p.returncode, 0)
                self.assertIn("make vt-uninstall VT=4 undoes what this run did", p.stderr)
                self.assertNotIn("vtium-dev runs on VT 4", p.stdout)
                p = self.vt_install("uninstall", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # A refusal before anything changed names no way back.
        p = self.vt_install("install", "1")
        self.assertNotIn("make vt-uninstall", p.stderr)

    def test_the_way_back_fits_what_the_failed_run_changed(self):
        write(os.path.join(self.s.standins, "sudo"), self.HALF_WRITING_SUDO)
        # Before the manifest is written, nothing was installed: a manifest
        # written halfway leaves no manifest, and no part of one.
        p = self.fail_copy("4", self.STATE + "/tty4")
        self.assertIn("vt-install: a step failed before anything was installed", p.stderr)
        self.assertNotIn("make vt-uninstall", p.stderr)
        self.assertEqual(self.left(), [self.STATE])
        # Uninstall removes the empty state directory such a run leaves, and
        # still says there was nothing to undo.
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("so there is nothing to undo; vt-uninstall removed the empty /var/lib/vtium-dev-install", p.stdout)
        self.assertEqual(self.left(), [])
        # On a reinstall, uninstall removes the earlier install too.
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.vt_install("install", "4", FAIL_VERB="restart")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("make vt-uninstall VT=4 removes the whole install on VT 4, the earlier one too", p.stderr)

    def test_systemd_analyze_verifies_the_generated_units(self):
        if not REAL_ANALYZE:
            self.skipTest("no systemd-analyze")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # systemd-analyze verify builds a start job for each unit. Every
        # service gets Requires=sysinit.target from its default dependencies,
        # and the root holds only the units seed() writes, which leave out
        # sysinit.target, so systemd 255 fails the job with "Unit
        # sysinit.target not found", where systemd 262 passes. This stand-in
        # sysinit.target lets the job resolve on both. The other units named
        # through After=, Before= and Conflicts=, by the generated units or by
        # the default dependencies (basic.target, shutdown.target), need no
        # stand-in, because systemd fails no job when a unit named only in
        # those lines is missing.
        write(self.at("usr/lib/systemd/system/sysinit.target"),
              "[Unit]\nDescription=Stand-in for systemd's own sysinit.target\n", 0o644)
        for unit in ("vtium-dev@tty4.service", "vtium-dev-getty@tty4.service"):
            with self.subTest(unit=unit):
                v = subprocess.run([REAL_ANALYZE, "verify", "--man=no", f"--root={self.root}",
                                    os.path.join(self.root, "etc/systemd/system", unit)],
                                   capture_output=True, text=True, timeout=WAIT_BOUND)
                self.assertEqual((v.returncode, v.stdout + v.stderr), (0, ""))

    def test_the_units_keep_every_shipped_line_but_the_ones_it_renames(self):
        # The Restart= lines a later shipped unit holds come through as they are.
        def restart(lines):
            i = lines.index("TTYVTDisallocate=yes")
            return lines[:i + 1] + ["Restart=on-failure", "RestartSec=2s"] + lines[i + 1:]
        self.shipped(restart)
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        src = read(os.path.join(self.s.standins, "install", "vtium@.service")).splitlines()
        want = generated(src, "vtium-dev", "/usr/local/bin/vtium-dev", ("vtium@%i.service", "kmsconvt@%i.service"))
        unit = read(self.at(self.UNIT)).splitlines()
        # The generated unit is its header, then the shipped unit so changed.
        self.assertTrue(unit[0].startswith("# Generated by make vt-install from crates/vtium/install/vtium@.service"))
        self.assertEqual(unit[-len(want):], want)
        self.assertTrue(all(l.startswith("#") or l == "" for l in unit[:-len(want)]))
        self.assertIn("Restart=on-failure", unit)
        self.assertIn("RestartSec=2s", unit)
        fallback_src = read(os.path.join(self.s.standins, "install", "vtium-getty@.service")).splitlines()
        fallback = read(self.at(self.FALLBACK)).splitlines()
        want = [l.replace("ExecStart=/usr/bin/vtium ", "ExecStart=/usr/local/bin/vtium-dev ")
                for l in body(fallback_src)]
        self.assertEqual(fallback[-len(want):], want)

    def test_a_shipped_unit_of_another_shape_is_refused(self):
        for change, why in (
                (lambda l: "ExecStart=/usr/sbin/vtium --vt=%I" if l.startswith("ExecStart=") else l,
                 "no one ExecStart=/usr/bin/vtium line to change"),
                (lambda l: "OnFailure=getty@%i.service" if l.startswith("OnFailure=") else l,
                 "no OnFailure=vtium-getty@%i.service line to change"),
                (lambda l: "Alias=autovt@.service kmsconvt@.service" if l.startswith("Alias=") else l,
                 "still names kmsconvt")):
            with self.subTest(why=why):
                shutil.rmtree(os.path.join(self.s.standins, "install"), ignore_errors=True)
                self.shipped(lambda lines: [change(l) for l in lines])
                self.refused(self.vt_install("install", "4"), why)
                for f in (*self.FILES, self.STATE):
                    self.assertFalse(os.path.exists(self.at(f)), f)
        # A fallback that no longer runs vtium console fallback would run the
        # packaged binary, or start a getty itself.
        shutil.rmtree(os.path.join(self.s.standins, "install"), ignore_errors=True)
        self.shipped(fallback_lines=lambda lines: [
            "ExecStart=systemctl --no-block start getty@%i.service" if l.startswith("ExecStart=") else l
            for l in lines])
        self.refused(self.vt_install("install", "4"),
                     "no one ExecStart=/usr/bin/vtium console fallback %i line to change")
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    def test_a_vt_that_cannot_be_taken_is_refused(self):
        for args, env, why in ((["1"], {}, "VT 1 is refused"),
                               (["13"], {}, "VT must be a number from 2 to 12"),
                               (["x"], {}, "VT must be a number from 2 to 12"),
                               ([], {}, "VT=N names the VT"),
                               (["7"], {}, "VT 7 is on screen now"),
                               (["4"], {"SEQ": "4"}, "logind session c4_active is on VT 4"),
                               (["4"], {"SEQ": "F"}, "loginctl could not list the sessions"),
                               (["4"], {"ACTIVE_UNITS": "kmsconvt@tty4.service"}, "kmsconvt@tty4 is running on VT 4")):
            with self.subTest(why=why):
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", *args, **env), why)
        # A per-VT autovt@ link that vt-install did not make is someone else's.
        open(self.s.calls, "w").close()
        link = self.at("etc/systemd/system/autovt@tty4.service")
        os.symlink("/usr/lib/systemd/system/getty@.service", link)
        self.refused(self.vt_install("install", "4"), "/etc/systemd/system/autovt@tty4.service is there already")
        os.unlink(link)
        # A binary that vt-install did not write is someone else's.
        open(self.s.calls, "w").close()
        os.makedirs(os.path.dirname(self.at(self.BIN)))
        write(self.at(self.BIN), "#!/bin/sh\n")
        self.refused(self.vt_install("install", "4"), "/usr/local/bin/vtium-dev is there already")
        self.assertEqual(read(self.at(self.BIN)), "#!/bin/sh\n")

    def test_a_binary_another_user_could_have_written_is_refused(self):
        # It runs as root once installed, so only root or the user may own it,
        # and only they may write it.
        for mode in (0o775, 0o757, 0o777):
            with self.subTest(mode=oct(mode)):
                os.chmod(self.s.bin, mode)
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", "4"), "is writable by group or others")
        os.chmod(self.s.bin, 0o755)
        # A stand-in stat says another uid owns the binary, since only root
        # could make that so.
        write(os.path.join(self.s.standins, "stat"), "#!/bin/bash\n"
              f'if [ "${{*: -1}}" = {self.s.bin!r} ]; then echo "4242 755"; exit; fi\n'
              f'exec {shutil.which("stat")!r} "$@"\n')
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4"), "belongs to uid 4242, neither root nor you")
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    def test_a_dry_run_shows_the_units_and_the_commands_and_runs_none(self):
        p = self.vt_install("install", "--dry-run", "4", ACTIVE_UNITS="getty@tty4.service")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("--- /etc/systemd/system/vtium-dev@.service", p.stdout)
        self.assertIn("ExecStart=/usr/local/bin/vtium-dev --vt=%I --no-switchvt", p.stdout)
        self.assertIn("--- /var/lib/vtium-dev-install/tty4", p.stdout)
        self.assertIn("getty-active=yes", p.stdout)
        commands = [l for l in p.stdout.splitlines() if l.startswith("+ sudo ")]
        self.assertEqual([c.split()[-2:] for c in commands[-4:]],
                         [[f"{self.s.standins}/systemctl", "daemon-reload"], ["enable", "vtium-dev@tty4.service"],
                          ["restart", "vtium-dev@tty4.service"], ["disable", "getty@tty4.service"]])
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    def test_uninstall_puts_the_getty_back_as_it_was_and_removes_what_install_wrote(self):
        wants = self.at("etc/systemd/system/getty.target.wants/getty@tty4.service")
        for enabled, active, back in ((True, "getty@tty4.service",
                                       ["add-wants getty.target getty@tty4.service", "start getty@tty4.service"]),
                                      (False, "getty@tty4.service", ["start getty@tty4.service"]),
                                      (False, "", [])):
            with self.subTest(enabled=enabled, active=active):
                if os.path.lexists(wants):
                    os.unlink(wants)
                if enabled:
                    os.symlink("/usr/lib/systemd/system/getty@.service", wants)
                before = self.links()
                p = self.vt_install("install", "4", ACTIVE_UNITS=active)
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                open(self.s.calls, "w").close()
                # The getty is off now, as install left it.
                p = self.vt_install("uninstall", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(self.s.changes(), ["stop vtium-dev@tty4.service", "disable vtium-dev@tty4.service",
                                                    "reset-failed vtium-dev@tty4.service vtium-dev-getty@tty4.service",
                                                    *back, "daemon-reload"])
                for f in (*self.FILES, self.STATE):
                    self.assertFalse(os.path.exists(self.at(f)), f)
                # Every unit link is as it was before install: vtium's per-VT
                # alias gone, the getty wanted again where it was, and no
                # autovt@tty4 of the getty's own.
                self.assertEqual(self.links(), before)
                # What install did not write stays.
                self.assertTrue(os.path.isfile(self.at("usr/lib/systemd/system/getty@.service")))
                self.assertTrue(os.path.isfile(self.at("usr/bin/kmscon")))

    def test_uninstall_keeps_the_files_another_vt_still_holds(self):
        for vt in ("4", "5"):
            p = self.vt_install("install", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("vt-install holds tty5 as well", p.stdout)
        for f in (*self.FILES, self.STATE + "/tty5"):
            self.assertTrue(os.path.exists(self.at(f)), f)
        self.assertFalse(os.path.exists(self.at(self.STATE + "/tty4")))
        p = self.vt_install("uninstall", "5")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    # The helper's path before the tool was renamed vtium-config (ADR-1201
    # D12). An install made then holds it, and its record names it.
    OLD_HELPER = "usr/local/bin/vtium-dev-vitrify"

    def install_under_the_old_name(self, vt="4"):
        """VT vt as an install made before the rename left it: the helper at
        OLD_HELPER, and the record naming it there."""
        p = self.vt_install("install", vt)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        os.rename(self.at(self.HELPER), self.at(self.OLD_HELPER))
        record = self.at(self.STATE + "/files")
        write(record, read(record).replace(f"/{self.HELPER}\n", f"/{self.OLD_HELPER}\n"), 0o644)
        open(self.s.calls, "w").close()

    def test_uninstall_removes_the_helper_an_install_under_the_old_name_wrote(self):
        self.install_under_the_old_name()
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for f in (*self.FILES, self.OLD_HELPER, self.STATE):
            self.assertFalse(os.path.lexists(self.at(f)), f)
        self.assertIn("VT 4 is back as it was before make vt-install", p.stdout)

    def test_a_reinstall_removes_the_helper_an_install_under_the_old_name_wrote(self):
        self.install_under_the_old_name()
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(read(self.at(self.HELPER)), read(self.s.helper))
        self.assertFalse(os.path.lexists(self.at(self.OLD_HELPER)))
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in self.FILES))
        self.assertEqual(self.temps(), [])

    def test_a_dry_run_over_an_install_under_the_old_name_shows_the_removal_and_makes_none(self):
        self.install_under_the_old_name()
        p = self.vt_install("install", "--dry-run", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"+ sudo rm -f -- {self.root}/{self.OLD_HELPER}", p.stdout)
        self.assertTrue(os.path.isfile(self.at(self.OLD_HELPER)))
        self.assertEqual(self.s.changes(), [])

    def test_an_old_name_helper_changed_since_or_not_vt_installs_stays(self):
        for recorded in (True, False):
            with self.subTest(recorded=recorded):
                self.install_under_the_old_name()
                if recorded:
                    # Edited since vt-install wrote it.
                    write(self.at(self.OLD_HELPER), "#!/bin/sh\n# someone's edit\n", 0o755)
                else:
                    # vt-install's record does not name it: someone else's.
                    record = self.at(self.STATE + "/files")
                    with open(record) as f:
                        kept_lines = [l for l in f if self.OLD_HELPER not in l]
                    write(record, "".join(kept_lines), 0o644)
                kept = read(self.at(self.OLD_HELPER))
                p = self.vt_install("install", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(read(self.at(self.OLD_HELPER)), kept)
                self.assertNotIn(self.OLD_HELPER, read(self.at(self.STATE + "/files")))
                p = self.vt_install("uninstall", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(read(self.at(self.OLD_HELPER)), kept)
                self.assertFalse(os.path.lexists(self.at(self.STATE)))
                os.unlink(self.at(self.OLD_HELPER))

    def test_uninstall_names_a_template_alias_to_vtium_dev(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # Someone ran systemctl enable vtium-dev@.service with no VT named on
        # a machine with no template alias of its own.
        template = self.at("etc/systemd/system/autovt@.service")
        os.unlink(template)
        os.symlink("/etc/systemd/system/vtium-dev@.service", template)
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("/etc/systemd/system/autovt@.service points at vtium-dev@.service", p.stdout)
        self.assertEqual(os.readlink(template), "/etc/systemd/system/vtium-dev@.service")
        # The C reference's template alias is named by nobody.
        os.unlink(template)
        os.symlink("/usr/lib/systemd/system/kmsconvt@.service", template)
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.vt_install("uninstall", "4")
        self.assertNotIn("autovt@.service points at", p.stdout)

    def test_a_reinstall_refuses_a_per_vt_alias_someone_else_made(self):
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # Someone points autovt@tty4 elsewhere after the install.
        link = self.at("etc/systemd/system/autovt@tty4.service")
        os.unlink(link)
        os.symlink("/usr/lib/systemd/system/getty@.service", link)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4"),
                     "/etc/systemd/system/autovt@tty4.service is there already and vt-install did not make it")
        self.assertEqual(os.readlink(link), "/usr/lib/systemd/system/getty@.service")

    def test_a_second_install_keeps_the_first_record_of_the_getty(self):
        p = self.vt_install("install", "4", ACTIVE_UNITS="getty@tty4.service")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        write(self.s.bin, read(self.s.bin) + "# a newer build\n")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("VT 4 holds vtium-dev already", p.stdout)
        self.assertEqual(read(self.at(self.BIN)), read(self.s.bin))
        manifest = read(self.at(self.STATE + "/tty4")).splitlines()
        self.assertIn("getty-active=yes", manifest)

    def test_uninstall_needs_the_manifest_and_a_vt_that_can_be_taken(self):
        self.refused(self.vt_install("uninstall", "4"), "no /var/lib/vtium-dev-install/tty4")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for env, why in (({"SEQ": "4"}, "logind session c4_active is on VT 4; log out there first"),):
            open(self.s.calls, "w").close()
            self.refused(self.vt_install("uninstall", "4", **env), why)
        write(self.at(self.STATE + "/tty4"), "vt=4\ngetty-enabled=maybe\ngetty-active=no\n", 0o644)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("uninstall", "4"), "has no getty-enabled=yes or getty-enabled=no line")
        write(self.at(self.STATE + "/tty4"), "vt=4\ngetty-enabled=no\ngetty-active=no\n", 0o644)
        open(self.s.calls, "w").close()
        p = self.vt_install("uninstall", "--dry-run", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("+ sudo " + self.s.standins + "/systemctl disable vtium-dev@tty4.service", p.stdout)
        self.assertEqual(self.s.calls_of("sudo"), [])
        self.assertEqual(self.s.changes(), [])
        self.assertTrue(os.path.exists(self.at(self.BIN)))

    def as_root(self):
        """A stand-in id that says uid 0 for -u, as under sudo, and a stand-in
        stat that says the binary is root's, as a run as root would make it.
        The stand-ins' directory is open to its owner alone, as vt-install
        asks of it under root."""
        write(os.path.join(self.s.standins, "id"), "#!/bin/bash\n"
              f'[ "$1" != -u ] || {{ echo 0; exit; }}\nexec {shutil.which("id")!r} "$@"\n')
        write(os.path.join(self.s.standins, "stat"), "#!/bin/bash\n"
              f'case "${{*: -1}}" in {self.s.bin!r} | {self.s.helper!r} | {self.s.image!r}) echo "0 755"; exit ;; esac\n'
              f'exec {shutil.which("stat")!r} "$@"\n')
        os.chmod(self.s.standins, 0o755)
        # sudo names the user who ran it.
        return {"SUDO_UID": str(os.getuid())}

    def test_root_is_refused_before_the_build(self):
        sudo_uid = self.as_root()
        # On the machine itself, with no stand-ins named, make and the script
        # refuse root before they do anything.
        real = self.s.env()
        del real["VTIUM_VT_STANDINS"]
        for target in ("vt-install", "vt-uninstall"):
            with self.subTest(target=target):
                open(self.s.calls, "w").close()
                p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, target, "VT=4", "DRY_RUN=1"],
                                   env=real, capture_output=True, text=True, timeout=BUILD_BOUND)
                self.assertNotEqual(p.returncode, 0)
                self.assertIn(f"{target}: run it as yourself, not as root", p.stderr)
                self.assertEqual(self.s.calls_of("cargo"), [])
                self.assertEqual(self.s.calls_of("sudo"), [])
        p = subprocess.run(["bash", VT_INSTALL, "install", "--dry-run", "4"],
                           env=dict(real, VTIUM_VT_BIN=self.s.bin, VTIUM_VT_CONFIG=self.s.helper),
                           capture_output=True, text=True, timeout=WAIT_BOUND, cwd=self.s.tmp)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("run it as yourself, not as root", p.stderr)
        self.assertEqual(self.s.calls_of("sudo"), [])
        # With the stand-ins, which write only under their own root, a test
        # run as root goes through.
        p = self.vt_install("install", "--dry-run", "4", **sudo_uid)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                            f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}"], env=self.s.env(**sudo_uid),
                           capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_make_as_root_under_the_stand_ins_builds_nothing(self):
        sudo_uid = self.as_root()
        # make -n prints each recipe it would run and runs none, so a build
        # that make does not refuse shows in its output.
        p = subprocess.run(["make", "-n", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1"],
                           env=self.s.env(**sudo_uid), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("vt-install: as root under the tests' stand-ins, name the binary and the helper with VTIUM_BIN=PATH VTIUM_CONFIG_BIN=PATH", p.stderr)
        self.assertNotIn("cargo", p.stdout)
        self.assertNotIn("sysroot", p.stdout)
        # Naming one of the two still leaves the other to build.
        for named in (f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}"):
            with self.subTest(named=named):
                p = subprocess.run(["make", "-n", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                                    named], env=self.s.env(**sudo_uid), capture_output=True, text=True, timeout=BUILD_BOUND)
                self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertIn("name the binary and the helper", p.stderr)
                self.assertNotIn("cargo", p.stdout)
        # vt-uninstall builds nothing, so it reaches the script.
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-uninstall", "VT=4", "DRY_RUN=1"],
                           env=self.s.env(**sudo_uid), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("make vt-install did not install vtium on VT 4", p.stderr)

    def test_root_runs_only_stand_ins_root_or_the_sudo_user_wrote(self):
        sudo_uid = self.as_root()
        uid = os.getuid()
        sudo = os.path.join(self.s.standins, "sudo")
        for why, env, standins_mode, sudo_mode in (
                (f"{self.s.standins} belongs to uid {uid}, neither root nor the user who ran sudo", {}, 0o755, 0o755),
                (f"{self.s.standins} belongs to uid {uid}, neither root nor the user who ran sudo",
                 {"SUDO_UID": str(uid + 1)}, 0o755, 0o755),
                (f"{self.s.standins} is writable by group or others (mode 775)", sudo_uid, 0o775, 0o755),
                (f"{sudo} is writable by group or others (mode 777)", sudo_uid, 0o755, 0o777)):
            with self.subTest(why=why, env=env):
                os.chmod(self.s.standins, standins_mode)
                os.chmod(sudo, sudo_mode)
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", "4", **env), why)
                self.refused(self.vt_install("uninstall", "4", **env), why)
        os.chmod(self.s.standins, 0o755)
        os.chmod(sudo, 0o755)
        p = self.vt_install("install", "4", **sudo_uid)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_root_checks_every_file_in_the_stand_ins_directory(self):
        # The stand-ins lead PATH, so as root any file there runs as root
        # when the script calls a command of its name: id and stat, which the
        # check itself runs, and sha256sum, which install runs after it.
        sudo_uid = self.as_root()
        sha = os.path.join(self.s.standins, "sha256sum")
        write(sha, f'#!/bin/bash\nexec {shutil.which("sha256sum")!r} "$@"\n')
        # A symlink there is checked by the file it names.
        elsewhere = os.path.join(self.s.tmp, "awk")
        write(elsewhere, f'#!/bin/bash\nexec {shutil.which("awk")!r} "$@"\n')
        awk = os.path.join(self.s.standins, "awk")
        os.symlink(elsewhere, awk)
        for name, path in (("id", None), ("stat", None), ("sha256sum", sha), ("awk", elsewhere)):
            path = path or os.path.join(self.s.standins, name)
            with self.subTest(name=name):
                os.chmod(path, 0o777)
                for action in ("install", "uninstall"):
                    open(self.s.calls, "w").close()
                    shown = awk if name == "awk" else path
                    self.refused(self.vt_install(action, "4", **sudo_uid),
                                 f"{shown} is writable by group or others (mode 777)")
                os.chmod(path, 0o755)
        # A symlink that names no file has no owner to read, so it is refused.
        gone = os.path.join(self.s.standins, "gone")
        os.symlink(os.path.join(self.s.tmp, "nothing-here"), gone)
        for action in ("install", "uninstall"):
            open(self.s.calls, "w").close()
            self.refused(self.vt_install(action, "4", **sudo_uid), f"cannot read the owner of {gone}")
        os.unlink(gone)
        p = self.vt_install("install", "4", **sudo_uid)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_make_runs_it_with_the_binary_named_and_no_build(self):
        make = lambda *a, **e: subprocess.run(["make", "--no-print-directory", "-C", ROOT, *a],
                                              env=self.s.env(**e), capture_output=True, text=True, timeout=BUILD_BOUND)
        p = make("vt-install", "VT=4", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("restart vtium-dev@tty4.service", p.stdout)
        self.assertEqual(self.s.calls_of("cargo"), [])
        self.assertEqual(self.s.calls_of("sudo"), [])
        p = make("vt-uninstall", "DRY_RUN=1")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VT=N names the VT", p.stderr)
        p = make("vt-install", "VT=4", "DRY_RUN=yes", f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("DRY_RUN=yes: 1 or nothing", p.stderr)
        p = make("vt-install", "VT=4", "DRY_RUN=1", VTIUM_BIN=self.s.bin)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_BIN is set in the environment", p.stderr)
        p = make("vt-install", "VT=4", "DRY_RUN=1", f"VTIUM_BIN={self.s.bin}", VTIUM_CONFIG_BIN=self.s.helper)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_CONFIG_BIN is set in the environment", p.stderr)
        self.assertEqual(self.s.calls_of("cargo") + self.s.calls_of("sudo"), [])

    # A stand-in cargo that records, beside its arguments, the helper path
    # the build bakes in.
    CARGO_ENV = "#!/bin/bash\n" + 'printf "%s\\n" "cargo VTIUM_FONT_HELPER=${VTIUM_FONT_HELPER-unset} $*" >>"$CALLS"\n'

    def test_make_builds_the_development_console_and_its_helper_by_default(self):
        write(os.path.join(self.s.standins, "cargo"), self.CARGO_ENV)
        # VTIUM_DEV and CONFIG_RELEASE point the defaults at the stand-ins,
        # which the stand-in cargo "builds".
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                            f"VTIUM_DEV={self.s.bin}", f"CONFIG_RELEASE={self.s.helper}"],
                           env=self.s.env(), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        builds = self.s.calls_of("cargo")
        # The console starts its helper at the installed helper's path, and
        # builds in a target directory of its own, so the minspec build that
        # the review runs use is never replaced.
        self.assertIn("VTIUM_FONT_HELPER=/usr/local/bin/vtium-dev-config rustc --profile minspec -p vtium --bin vtium "
                      f"--target x86_64-unknown-linux-gnu --target-dir {TARGET}/vt-dev -- -C link-arg=-Wl,-z,pack-relative-relocs",
                      builds)
        self.assertIn("VTIUM_FONT_HELPER=unset build --release -p vtium-config --target x86_64-unknown-linux-gnu", builds)
        self.assertEqual(len(builds), 2, builds)
        self.assertIn("+ sudo " + self.s.standins + "/systemctl restart vtium-dev@tty4.service", p.stdout)

    def test_a_binary_that_starts_another_helper_is_refused(self):
        # The minspec build starts /usr/bin/vtium-config, the packaged
        # helper.
        write(self.s.bin, BIN.replace("/usr/local/bin/vtium-dev-config", "/usr/bin/vtium-config"))
        self.refused(self.vt_install("install", "4"),
                     "does not look for its glyph helper at /usr/local/bin/vtium-dev-config")
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)

    def test_a_helper_another_user_could_have_written_is_refused(self):
        os.chmod(self.s.helper, 0o775)
        self.refused(self.vt_install("install", "4"),
                     f"{self.s.helper} is writable by group or others (mode 775); vtium, as root, would start it")
        os.chmod(self.s.helper, 0o755)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4", VTIUM_VT_CONFIG=""), "VTIUM_VT_CONFIG names glyph helper")
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4", VTIUM_VT_CONFIG=self.s.helper + "-missing"), "no glyph helper at")

    def test_a_vt_the_packaged_vtium_holds_is_refused(self):
        why = "VT 4 refused: the packaged vtium@tty4 holds it; sudo systemctl disable --now vtium@tty4 frees it"
        self.refused(self.vt_install("install", "4", ACTIVE_UNITS="vtium@tty4.service"), why)
        # Enabled, it starts there at the VT's next start.
        lib = self.at("usr/lib/systemd/system")
        write(os.path.join(lib, "vtium@.service"),
              "[Service]\nExecStart=/usr/bin/vtium --vt=%I\n\n[Install]\nAlias=autovt@.service\nWantedBy=getty.target\n",
              0o644)
        os.symlink("/usr/lib/systemd/system/vtium@.service",
                   self.at("etc/systemd/system/getty.target.wants/vtium@tty4.service"))
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4"), why)
        for f in (*self.FILES, self.STATE):
            self.assertFalse(os.path.exists(self.at(f)), f)
        # Another VT is the development install's to take, beside it.
        p = self.vt_install("install", "5", ACTIVE_UNITS="vtium@tty4.service")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def pictures(self):
        """The stand-in binary becomes a console with pictures, as make
        vt-install FEATURES=images builds it: it names the image helper's
        path beside the glyph helper's."""
        write(self.s.bin, BIN + "# and its image helper at /usr/local/bin/vtium-dev-image.\n")

    def shipped_sysusers(self):
        return read(os.path.join(ROOT, "crates", "vtium", "install", "vtium-image.conf"))

    def test_a_console_with_pictures_installs_its_image_helper_and_the_account(self):
        self.pictures()
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(read(self.at(self.IMAGE)), read(self.s.image))
        self.assertTrue(os.access(self.at(self.IMAGE), os.X_OK))
        # The account's sysusers file is the shipped one, under an opening
        # comment of its own, so the account is the one the package creates
        # (ADR-1101 D2).
        sysusers = read(self.at(self.SYSUSERS))
        self.assertTrue(sysusers.endswith(self.shipped_sysusers()), sysusers)
        self.assertTrue(all(l.startswith("#") for l in sysusers[:-len(self.shipped_sysusers())].splitlines()))
        self.assertIn('u vtium-image - "vtium image helper" / -', sysusers.splitlines())
        self.assertEqual(stat.S_IMODE(os.stat(self.at(self.SYSUSERS)).st_mode), 0o644)
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in (*self.FILES, *self.PICTURES)))
        # systemd-sysusers creates the account from that file alone, as
        # root, before vtium starts.
        self.assertEqual(self.s.calls_of("systemd-sysusers"), [self.at(self.SYSUSERS)])
        sudo = self.s.calls_of("sudo")
        sysusers_run = sudo.index(f"{self.s.standins}/systemd-sysusers {self.at(self.SYSUSERS)}")
        self.assertLess(sysusers_run, sudo.index(f"{self.s.standins}/systemctl restart vtium-dev@tty4.service"))
        # The helpers are in place before the binary that starts them.
        renames = [c.split()[4] for c in sudo if c.startswith("mv -fT -- ")]
        self.assertLess(renames.index(self.at(self.IMAGE)), renames.index(self.at(self.BIN)))
        # A reinstall replaces both as it replaces the other files.
        write(self.s.image, read(self.s.image) + "# a newer build\n")
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(read(self.at(self.IMAGE)), read(self.s.image))
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in (*self.FILES, *self.PICTURES)))

    def test_a_sysusers_file_systemd_sysusers_cannot_read_fails_the_install(self):
        # systemd-sysusers refuses a line that is no u, g, m or r line, as it
        # would in a shipped vtium-image.conf that went wrong; install then
        # stops before vtium restarts and names the way back.
        self.pictures()
        self.shipped()
        conf = os.path.join(self.s.standins, "install", "vtium-image.conf")
        write(conf, read(conf).replace("u vtium-image", "user vtium-image"), 0o644)
        p = self.vt_install("install", "4")
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("unknown line: user vtium-image", p.stderr)
        self.assertIn("make vt-uninstall VT=4 undoes what this run did", p.stderr)
        self.assertNotIn("restart vtium-dev@tty4.service", self.s.changes())

    def test_a_console_without_pictures_installs_no_image_helper(self):
        # The minspec build names the packaged /usr/bin/vtium-image, as every
        # build names the path it was built with through the image-helper
        # key, and never starts it, so no image helper is needed, and the one
        # make vt-install would name is never read.
        write(self.s.bin, BIN + "# and names /usr/bin/vtium-image, which it never starts.\n")
        p = self.vt_install("install", "4", VTIUM_VT_IMAGE=self.s.image + "-missing")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for f in self.PICTURES:
            self.assertFalse(os.path.lexists(self.at(f)), f)
        self.assertEqual(self.s.calls_of("systemd-sysusers"), [])
        self.assertNotIn("vtium-dev-image", read(self.at(self.STATE + "/files")))

    def test_a_console_with_pictures_that_starts_another_image_helper_is_refused(self):
        # A build with images and no VTIUM_IMAGE starts /usr/bin/vtium-image,
        # the packaged helper; it carries the line only a build with images
        # carries.
        write(self.s.bin, BIN + "# and /usr/bin/vtium-image, and logs: cannot open the image helper {name}: {e}\n")
        self.refused(self.vt_install("install", "4"),
                     "is built with images but does not look for its image helper at /usr/local/bin/vtium-dev-image")
        self.assertEqual(self.left(), [])

    def test_the_line_that_marks_a_build_with_images_is_in_code_built_only_with_images(self):
        # vt-install tells a build with images by a line only vtium-platform's
        # image helper start logs, in a module built only with images; a
        # reworded message would let a build with images that starts another
        # image helper through, and the line anywhere else would make a build
        # without images look like one with them.
        marker = next(l.split("=", 1)[1].strip("'") for l in read(VT_INSTALL).splitlines()
                      if l.startswith("images_marker="))
        start = os.path.join("crates", "vtium-platform", "src", "image_helper", "start.rs")
        self.assertIn(f'"{marker}{{name}}: {{e}}"', read(os.path.join(ROOT, start)))
        lib = read(os.path.join(ROOT, "crates", "vtium-platform", "src", "lib.rs"))
        self.assertIn('#[cfg(feature = "images")]\npub mod image_helper;', lib)
        found = []
        for d, _, names in os.walk(os.path.join(ROOT, "crates")):
            found += [os.path.relpath(os.path.join(d, n), ROOT) for n in names
                      if n.endswith(".rs") and marker in read(os.path.join(d, n))]
        self.assertEqual(found, [start])
        # make vt-dev-console checks the built console for the same line,
        # read from vt-install.sh.
        p = subprocess.run(["make", "-n", "--no-print-directory", "-C", ROOT, "vt-dev-console", "FEATURES=images"],
                           env=self.s.env(), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"grep -qaF '{marker}' ", p.stdout)

    def test_a_console_with_pictures_needs_an_image_helper_only_root_or_you_could_have_written(self):
        self.pictures()
        for env, why in (({"VTIUM_VT_IMAGE": ""}, "VTIUM_VT_IMAGE names image helper; make vt-install sets it"),
                         ({"VTIUM_VT_IMAGE": self.s.image + "-missing"},
                          f"no image helper at {self.s.image}-missing: make vt-install FEATURES=images builds it")):
            with self.subTest(why=why):
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", "4", **env), why)
        os.chmod(self.s.image, 0o775)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4"),
                     f"{self.s.image} is writable by group or others (mode 775); vtium, as root, would start it")
        self.assertEqual(self.left(), [])

    def test_a_console_with_pictures_refuses_an_image_helper_or_account_file_not_vt_installs(self):
        self.pictures()
        for f in self.PICTURES:
            with self.subTest(f=f):
                os.makedirs(os.path.dirname(self.at(f)), exist_ok=True)
                write(self.at(f), "someone else's\n", 0o644)
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", "4"), f"/{f} is there already and vt-install did not write it")
                self.assertEqual(read(self.at(f)), "someone else's\n")
                os.unlink(self.at(f))

    def test_a_dry_run_with_pictures_shows_the_account_file_and_the_commands_and_runs_none(self):
        self.pictures()
        p = self.vt_install("install", "--dry-run", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("--- /etc/sysusers.d/vtium-dev-image.conf", p.stdout)
        self.assertIn('u vtium-image - "vtium image helper" / -', p.stdout)
        self.assertIn(f"  /{self.IMAGE}\n", p.stdout)
        self.assertIn(f"+ sudo mv -fT -- {self.root}/usr/local/bin/.vtium-dev-image.vt-install-new {self.at(self.IMAGE)}",
                      p.stdout)
        self.assertIn(f"+ sudo {self.s.standins}/systemd-sysusers {self.at(self.SYSUSERS)}", p.stdout)
        self.assertEqual(self.s.calls_of("sudo") + self.s.calls_of("systemd-sysusers"), [])
        self.assertEqual(self.left(), [])

    def test_uninstall_removes_the_image_helper_and_the_account_file_and_leaves_the_account(self):
        self.pictures()
        for vt in ("4", "5"):
            p = self.vt_install("install", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        open(self.s.calls, "w").close()
        p = self.vt_install("uninstall", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        # VT 5 still runs the console, which starts the image helper.
        self.assertIn(f"/{self.IMAGE} /{self.SYSUSERS}: vt-install holds tty5 as well", p.stdout)
        for f in (*self.FILES, *self.PICTURES):
            self.assertTrue(os.path.exists(self.at(f)), f)
        p = self.vt_install("uninstall", "5")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])
        # systemd-sysusers never removes an account, and uninstall asks it
        # for nothing.
        self.assertEqual(self.s.calls_of("systemd-sysusers"), [])

    def test_uninstall_names_as_kept_only_the_image_files_vt_install_wrote(self):
        # A sysusers file of the same name that vt-install never wrote is
        # someone else's: uninstall neither names it among the files it keeps
        # for another VT nor removes it.
        for vt in ("4", "5"):
            p = self.vt_install("install", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        os.makedirs(os.path.dirname(self.at(self.SYSUSERS)))
        write(self.at(self.SYSUSERS), "u someone - - / -\n", 0o644)
        for vt in ("4", "5"):
            p = self.vt_install("uninstall", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
            self.assertNotIn(self.SYSUSERS, p.stdout)
        self.assertEqual(read(self.at(self.SYSUSERS)), "u someone - - / -\n")

    def test_a_reinstall_without_pictures_removes_the_image_helper_an_install_with_them_wrote(self):
        # Every VT runs the one binary, so the console without pictures,
        # installed on VT 5, leaves VT 4 with no use for the image helper.
        self.pictures()
        p = self.vt_install("install", "4")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        write(self.s.bin, BIN)
        p = self.vt_install("install", "--dry-run", "5")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"+ sudo rm -f -- {self.at(self.IMAGE)}", p.stdout)
        self.assertIn(f"+ sudo rm -f -- {self.at(self.SYSUSERS)}", p.stdout)
        self.assertTrue(os.path.exists(self.at(self.IMAGE)))
        p = self.vt_install("install", "5")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for f in self.PICTURES:
            self.assertFalse(os.path.lexists(self.at(f)), f)
        record = read(self.at(self.STATE + "/files")).splitlines()
        self.assertEqual(sorted(record), sorted(f"{self.sha(f)}  /{f}" for f in self.FILES))
        self.assertEqual(self.temps(), [])
        for vt in ("4", "5"):
            p = self.vt_install("uninstall", vt)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.left(), [])

    def test_an_image_helper_or_account_file_changed_since_stays(self):
        for changed in self.PICTURES:
            with self.subTest(changed=changed):
                self.reseed()
                self.pictures()
                p = self.vt_install("install", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                with open(self.at(changed), "a") as f:
                    f.write("# the maintainer's edit\n")
                edited = read(self.at(changed))
                # A reinstall with pictures refuses it, as it does any file
                # changed since install wrote it.
                open(self.s.calls, "w").close()
                self.refused(self.vt_install("install", "4"), f"/{changed} changed since vt-install wrote it")
                # One without pictures keeps and names it, removes the
                # other, and the record forgets it.
                write(self.s.bin, BIN)
                p = self.vt_install("install", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertIn(f"kept /{changed}: it changed since vt-install wrote it", p.stdout)
                self.assertEqual(read(self.at(changed)), edited)
                other, = [f for f in self.PICTURES if f != changed]
                self.assertFalse(os.path.lexists(self.at(other)))
                self.assertNotIn(changed, read(self.at(self.STATE + "/files")))
                # It is someone else's now: uninstall leaves it.
                p = self.vt_install("uninstall", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(self.left(), [changed])

    def test_a_copy_with_pictures_that_fails_halfway_can_be_undone_or_tried_again(self):
        write(os.path.join(self.s.standins, "sudo"), self.HALF_WRITING_SUDO)
        for target, case, way in itertools.product(
                self.PICTURES, ("first install", "reinstall", "over one without pictures"), ("uninstall", "install again")):
            with self.subTest(target=target, case=case, way=way):
                self.reseed()
                seeded = self.links()
                if case != "first install":
                    if case == "reinstall":
                        self.pictures()
                    p = self.vt_install("install", "4")
                    self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    write(self.s.image, read(self.s.image) + "# a newer build\n")
                self.pictures()
                self.fail_copy("4", target)
                if way == "uninstall":
                    p = self.vt_install("uninstall", "4")
                    self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                    self.assertNotIn("changed since", p.stdout)
                    self.assertEqual(self.links(), seeded)
                    self.assertEqual(self.left(), [])
                p = self.vt_install("install", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(read(self.at(self.IMAGE)), read(self.s.image))
                p = self.vt_install("uninstall", "4")
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertNotIn("changed since", p.stdout)
                self.assertEqual(self.links(), seeded)
                self.assertEqual(self.left(), [])

    # A stand-in cargo that records, beside its arguments, both helper paths
    # the build bakes in. A rustc of -p vtium writes the console at
    # $CARGO_OUT as option_env! bakes it: the stand-in vtium from $CARGO_SRC,
    # the image helper path VTIUM_IMAGE names, else /usr/bin/vtium-image, and
    # with images among its features the line only a build with images
    # carries. $CARGO_STALE builds as if VTIUM_IMAGE never reached the build.
    CARGO_BUILDS = "#!/bin/bash\n" + ('printf "%s\\n" "cargo VTIUM_FONT_HELPER=${VTIUM_FONT_HELPER-unset} '
                                      'VTIUM_IMAGE=${VTIUM_IMAGE-unset} '
                                      '${VTIUM_HWACCEL+VTIUM_HWACCEL=$VTIUM_HWACCEL }'
                                      '${VTIUM_DISPLAY_BUFFERS+VTIUM_DISPLAY_BUFFERS=$VTIUM_DISPLAY_BUFFERS }'
                                      '$*" >>"$CALLS"\n') + """
if [ "$1" = rustc ] && [[ " $* " == *" -p vtium "* ]]; then
	image=${VTIUM_IMAGE:-/usr/bin/vtium-image}
	[ -z "${CARGO_STALE-}" ] || image=/usr/bin/vtium-image
	{
		cat "$CARGO_SRC"
		printf '# starts its image helper at %s\\n' "$image"
		[[ " $* " != *" --features "*images* ]] || printf '# cannot open the image helper {name}: {e}\\n'
	} >"$CARGO_OUT"
	chmod 755 "$CARGO_OUT"
fi
"""

    def make_vt_install(self, *args, **env):
        """make vt-install VT=4 DRY_RUN=1 with the stand-in cargo building
        the console at the stand-in binary's path, VTIUM_DEV, and the
        helpers' defaults at the stand-in helpers, which it leaves as they
        are."""
        write(os.path.join(self.s.standins, "cargo"), self.CARGO_BUILDS)
        src = os.path.join(self.s.tmp, "vtium.src")
        write(src, BIN, 0o644)
        os.unlink(self.s.bin)
        return subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1", *args,
                               f"VTIUM_DEV={self.s.bin}", f"CONFIG_RELEASE={self.s.helper}",
                               f"IMAGE_RELEASE={self.s.image}"],
                              env=self.s.env(CARGO_SRC=src, CARGO_OUT=self.s.bin, **env),
                              capture_output=True, text=True, timeout=BUILD_BOUND)

    def test_make_builds_a_console_with_pictures_and_its_image_helper(self):
        p = self.make_vt_install("FEATURES=images")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        builds = self.s.calls_of("cargo")
        # The console starts its image helper at the development install's
        # own path, never the packaged /usr/bin/vtium-image.
        self.assertIn("VTIUM_FONT_HELPER=/usr/local/bin/vtium-dev-config VTIUM_IMAGE=/usr/local/bin/vtium-dev-image "
                      f"rustc --profile minspec -p vtium --bin vtium --target x86_64-unknown-linux-gnu --target-dir {TARGET}/vt-dev "
                      "--features images -- -C link-arg=-Wl,-z,pack-relative-relocs", builds)
        self.assertIn("VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset build --release -p vtium-config --target x86_64-unknown-linux-gnu",
                      builds)
        self.assertIn("VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset build --release -p vtium-image-helper "
                      "--target x86_64-unknown-linux-gnu", builds)
        self.assertEqual(len(builds), 3, builds)
        # vt-install reads the pictures from what was built.
        self.assertEqual(read(self.s.bin).splitlines()[-2:], ["# starts its image helper at /usr/local/bin/vtium-dev-image",
                                                             "# cannot open the image helper {name}: {e}"])
        self.assertIn(f"+ sudo install -D -m 0755 -- {self.s.image} {self.root}/usr/local/bin/.vtium-dev-image.vt-install-new",
                      p.stdout)
        self.assertIn(f"+ sudo {self.s.standins}/systemd-sysusers {self.at(self.SYSUSERS)}", p.stdout)

    def test_make_hands_hwaccel_and_display_buffers_to_every_build(self):
        # The development console, its glyph helper and its image helper each
        # take the two settings, as make install's builds do (ADR-1500 D8).
        p = self.make_vt_install("FEATURES=images gpu", "HWACCEL=on", "DISPLAY_BUFFERS=2")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        builds = self.s.calls_of("cargo")
        self.assertEqual(len(builds), 3, builds)
        for build in builds:
            self.assertIn(" VTIUM_HWACCEL=on VTIUM_DISPLAY_BUFFERS=2 ", build)
        self.assertIn("--target-dir " + TARGET + "/vt-dev --features images gpu ", " ".join(builds))
        # A console named with VTIUM_BIN is built already, so the settings
        # are refused before anything is built.
        open(self.s.calls, "w").close()
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                            f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}", "HWACCEL=on"],
                           env=self.s.env(), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertNotEqual(p.returncode, 0, p.stdout)
        self.assertIn("vt-install: HWACCEL and DISPLAY_BUFFERS set the defaults a build bakes into the console", p.stderr)
        self.assertEqual(self.s.calls_of("cargo") + self.s.calls_of("sudo"), [])

    def test_make_builds_a_console_without_pictures_and_no_image_helper(self):
        # A console built without images names the packaged image helper,
        # which it never starts, and make vt-install builds no image helper
        # and installs none.
        p = self.make_vt_install()
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        builds = self.s.calls_of("cargo")
        self.assertEqual(len(builds), 2, builds)
        self.assertTrue(all("VTIUM_IMAGE=unset" in b and "vtium-image-helper" not in b for b in builds), builds)
        self.assertIn("/usr/bin/vtium-image", read(self.s.bin))
        self.assertNotIn("vtium-dev-image", p.stdout)

    def test_make_refuses_a_console_with_images_that_does_not_name_the_development_image_helper(self):
        # A build that never took VTIUM_IMAGE, as a stale one would, names
        # the packaged image helper.
        p = self.make_vt_install("FEATURES=images", CARGO_STALE="1")
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"{self.s.bin} does not start its image helper at /usr/local/bin/vtium-dev-image", p.stderr)
        self.assertEqual(self.s.calls_of("sudo"), [])

    def test_make_names_the_image_helper_on_its_command_line_only(self):
        self.pictures()
        make = lambda *a, **e: subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                                               f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}", *a],
                                              env=self.s.env(**e), capture_output=True, text=True, timeout=BUILD_BOUND)
        p = make(f"VTIUM_IMAGE_BIN={self.s.image}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"+ sudo install -D -m 0755 -- {self.s.image} ", p.stdout)
        p = make(VTIUM_IMAGE_BIN=self.s.image)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("VTIUM_IMAGE_BIN is set in the environment", p.stderr)
        self.assertEqual(self.s.calls_of("cargo") + self.s.calls_of("sudo"), [])

    def test_make_as_root_with_images_needs_the_image_helper_named(self):
        sudo_uid = self.as_root()
        named = (f"VTIUM_BIN={self.s.bin}", f"VTIUM_CONFIG_BIN={self.s.helper}")
        p = subprocess.run(["make", "-n", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                            "FEATURES=images", *named], env=self.s.env(**sudo_uid), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("with images in FEATURES the image helper with VTIUM_IMAGE_BIN=PATH", p.stderr)
        self.assertNotIn("cargo", p.stdout)
        self.pictures()
        p = subprocess.run(["make", "--no-print-directory", "-C", ROOT, "vt-install", "VT=4", "DRY_RUN=1",
                            "FEATURES=images", *named, f"VTIUM_IMAGE_BIN={self.s.image}"],
                           env=self.s.env(**sudo_uid), capture_output=True, text=True, timeout=BUILD_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"+ sudo {self.s.standins}/systemd-sysusers {self.at(self.SYSUSERS)}", p.stdout)
        # As root, the systemd-sysusers it runs is a stand-in only root or
        # the user who ran sudo wrote.
        sysusers = os.path.join(self.s.standins, "systemd-sysusers")
        os.chmod(sysusers, 0o777)
        open(self.s.calls, "w").close()
        self.refused(self.vt_install("install", "4", **sudo_uid), f"{sysusers} is writable by group or others (mode 777)")
        os.unlink(sysusers)
        self.refused(self.vt_install("install", "4", **sudo_uid), f"cannot read the owner of {sysusers}")

class Package(Case):
    """make install, which a package's build runs, and units.sh, which makes
    the units for it and for make vt-install."""
    maxDiff = None

    # Every file make install stages, by its path under DESTDIR with
    # PREFIX=/usr, with no FEATURES and the default FRITS.
    STAGED = sorted([
        "etc/vtium/fonts/desktop.vtfrit",
        "etc/vtium/fonts/minspec-16.vtfrit",
        "etc/vtium/fonts/minspec-8.vtfrit",
        "etc/vtium/vtium.conf.example",
        "usr/bin/vtium",
        "usr/bin/vtium-config",
        "usr/lib/systemd/system/vtium@.service",
        "usr/lib/systemd/system/vtium-getty@.service",
        "usr/share/doc/vtium/examples/71-vtium-local.rules",
        "usr/share/doc/vtium/examples/vtium.hwdb",
        "usr/share/licenses/vtium/COPYING",
        "usr/share/licenses/vtium/LICENSE",
        "usr/share/licenses/vtium/OFL-1.1.txt",
        "usr/share/licenses/vtium/rgb-license.txt",
    ])

    # What the ordinary-machine build, FEATURES='images logind gpu', stages
    # beside STAGED: the image helper, its account file and the picture
    # gallery tool (ADR-1101 D1, ADR-1500 D8).
    WITH_IMAGES = ["usr/bin/vtium-gallery", "usr/bin/vtium-image", "usr/lib/sysusers.d/vtium-image.conf"]

    # Where each staged file that vtium's tree holds as it is comes from.
    SOURCES = {
        "etc/vtium/fonts/desktop.vtfrit": "tools/specimens/frits/desktop.vtfrit",
        "etc/vtium/fonts/minspec-16.vtfrit": "crates/vtium-config/frits/minspec-16.vtfrit",
        "etc/vtium/fonts/minspec-8.vtfrit": "crates/vtium-config/frits/minspec-8.vtfrit",
        "usr/bin/vtium-gallery": "tools/gallery/gallery",
        "usr/share/licenses/vtium/COPYING": "COPYING",
        "usr/share/licenses/vtium/LICENSE": "LICENSE",
        "usr/share/licenses/vtium/OFL-1.1.txt": "crates/vtium-font/data/OFL-1.1.txt",
        "usr/share/licenses/vtium/rgb-license.txt": "crates/vtium-term/gen-color-names/src/rgb-license.txt",
    }

    # A stand-in cargo that records the helper paths each build bakes in,
    # and the build settings for hwaccel and display-buffers when it is
    # given them.
    CARGO = "#!/bin/bash\n" + ('printf "%s\\n" "cargo VTIUM_FONT_HELPER=${VTIUM_FONT_HELPER-unset} '
                               'VTIUM_IMAGE=${VTIUM_IMAGE-unset} '
                               '${VTIUM_HWACCEL+VTIUM_HWACCEL=$VTIUM_HWACCEL }'
                               '${VTIUM_DISPLAY_BUFFERS+VTIUM_DISPLAY_BUFFERS=$VTIUM_DISPLAY_BUFFERS }'
                               '$*" >>"$CALLS"\n')

    def setUp(self):
        super().setUp()
        write(os.path.join(self.s.standins, "cargo"), self.CARGO)
        self.build = os.path.join(self.s.tmp, "build")
        self.dest = os.path.join(self.s.tmp, "stage")

    def built(self, helper, image=None):
        """The stand-in builds in INSTALL_TARGET_DIR: a console that starts
        its glyph helper at helper, and the helper; with image, a console
        that also starts its image helper there, and the image helper."""
        files = [("minspec/vtium", f"#!/bin/sh\n# starts {helper}\n" + (f"# and {image}\n" if image else "")),
                 ("release/vtium-config", "#!/bin/sh\n# the helper\n")]
        if image:
            files.append(("release/vtium-image", "#!/bin/sh\n# the image helper\n"))
        for path, text in files:
            os.makedirs(os.path.dirname(os.path.join(self.build, path)), exist_ok=True)
            write(os.path.join(self.build, path), text)

    def make(self, *args, **env):
        return subprocess.run(["make", "--no-print-directory", "-C", ROOT, *args], env=self.s.env(**env),
                              capture_output=True, text=True, timeout=BUILD_BOUND, start_new_session=True)

    def staged(self):
        out = []
        for d, _, names in os.walk(self.dest):
            out += [os.path.relpath(os.path.join(d, n), self.dest) for n in names]
        return sorted(out)

    def test_install_stages_what_the_package_ships_and_nothing_the_c_references_packages_own(self):
        self.built("/usr/bin/vtium-config")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.staged(), self.STAGED)
        # The C reference's packages own /etc/kmscon/kmscon.conf.example, the
        # compiled terminfo entry kmscon and the kmsconvt@ units. vtium's own
        # example sits in /etc/vtium, beside the file vtium reads, whatever
        # PREFIX is.
        for f in self.staged():
            self.assertFalse(f.startswith(("etc/kmscon/", "usr/share/terminfo/")) or "kmsconvt" in f, f)
        for f in ("usr/bin/vtium", "usr/bin/vtium-config"):
            self.assertTrue(os.access(os.path.join(self.dest, f), os.X_OK), f)
        unit = read(os.path.join(self.dest, "usr/lib/systemd/system/vtium@.service")).splitlines()
        self.assertTrue(unit[0].startswith("# Generated by make install from crates/vtium/install/vtium@.service"))
        for line in ("ExecStart=/usr/bin/vtium --vt=%I --no-switchvt", "OnFailure=vtium-getty@%i.service",
                     "Alias=autovt@.service", "WantedBy=getty.target", "Restart=on-failure",
                     "Conflicts=getty@%i.service", "After=getty@%i.service",
                     "Conflicts=kmsconvt@%i.service", "After=kmsconvt@%i.service"):
            self.assertIn(line, unit)
        self.assertEqual([l for l in unit if l.startswith("DefaultInstance=")], ["DefaultInstance=name-a-VT/vtium@ttyN"], unit)
        directives = [l for l in unit if l and not l.startswith("#")]
        self.assertFalse([l for l in directives if ("kmsconvt" in l and l not in ("Conflicts=kmsconvt@%i.service",
                                                                                "After=kmsconvt@%i.service"))
                          or "vtium-dev" in l], directives)
        # The console starts its helper where the package puts it, and both
        # build in a target directory of their own with the host's toolchain.
        builds = self.s.calls_of("cargo")
        self.assertEqual(builds, [
            f"VTIUM_FONT_HELPER=/usr/bin/vtium-config VTIUM_IMAGE=unset rustc --profile minspec -p vtium --bin vtium "
            f"--target-dir {self.build} -- -C link-arg=-Wl,-z,pack-relative-relocs",
            f"VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset build --release -p vtium-config --target-dir {self.build}"])

    def test_install_passes_the_prefix_and_the_package_builds_flags_through(self):
        self.built("/opt/vt/bin/vtium-config", "/opt/vt/bin/vtium-image")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/opt/vt", f"INSTALL_TARGET_DIR={self.build}",
                      "CARGO_BUILD_FLAGS=--frozen", "FEATURES=images")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("opt/vt/lib/systemd/system/vtium@.service", self.staged())
        # vtium reads /etc/vtium whatever PREFIX is, so the example goes there.
        self.assertIn("etc/vtium/vtium.conf.example", self.staged())
        unit = read(os.path.join(self.dest, "opt/vt/lib/systemd/system/vtium@.service")).splitlines()
        self.assertIn("ExecStart=/opt/vt/bin/vtium --vt=%I --no-switchvt", unit)
        # With images the console starts its image helper where the package
        # puts it, and the helper and its account file are staged (ADR-1101
        # D1, D2).
        for f in ("opt/vt/bin/vtium-image", "opt/vt/lib/sysusers.d/vtium-image.conf"):
            self.assertIn(f, self.staged())
        self.assertTrue(os.access(os.path.join(self.dest, "opt/vt/bin/vtium-image"), os.X_OK))
        self.assertEqual(self.s.calls_of("cargo"), [
            f"VTIUM_FONT_HELPER=/opt/vt/bin/vtium-config VTIUM_IMAGE=/opt/vt/bin/vtium-image rustc --frozen "
            f"--profile minspec -p vtium --bin vtium --target-dir {self.build} --features images "
            "-- -C link-arg=-Wl,-z,pack-relative-relocs",
            f"VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset build --frozen --release -p vtium-config -p vtium-image-helper "
            f"--target-dir {self.build}"])

    def test_install_finds_images_in_features_separated_by_commas(self):
        # cargo takes --features images,logind as it takes "images logind",
        # so make install stages the image helper for either form.
        self.built("/usr/bin/vtium-config", "/usr/bin/vtium-image")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}",
                      "FEATURES=images,logind")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for f in ("usr/bin/vtium-image", "usr/lib/sysusers.d/vtium-image.conf"):
            self.assertIn(f, self.staged())
        self.assertIn("-p vtium-image-helper", " ".join(self.s.calls_of("cargo")))

    def test_a_minspec_install_stages_no_image_helper(self):
        # A console built without images starts no image helper, so the
        # package ships neither the helper nor its account file (ADR-1101 D1).
        self.built("/usr/bin/vtium-config")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual([f for f in self.staged() if "vtium-image" in f], [])
        self.assertNotIn("-p vtium-image-helper", " ".join(self.s.calls_of("cargo")))

    def test_the_desktop_install_stages_its_frits_gallery_and_head_template(self):
        # ADR-1500 D8: the vtium package's build, with its features and its
        # two build settings, which reach both builds.
        self.built("/usr/bin/vtium-config", "/usr/bin/vtium-image")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}",
                      "FEATURES=images logind gpu", "HWACCEL=on", "DISPLAY_BUFFERS=3")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.staged(), sorted(self.STAGED + self.WITH_IMAGES))
        for staged, source in self.SOURCES.items():
            self.assertEqual(read(os.path.join(self.dest, staged)), read(os.path.join(ROOT, source)), staged)
        modes = {f: stat.S_IMODE(os.stat(os.path.join(self.dest, f)).st_mode) for f in self.SOURCES}
        self.assertEqual(modes, {f: 0o755 if f.startswith("usr/bin/") else 0o644 for f in self.SOURCES})
        self.assertEqual(self.s.calls_of("cargo"), [
            "VTIUM_FONT_HELPER=/usr/bin/vtium-config VTIUM_IMAGE=/usr/bin/vtium-image VTIUM_HWACCEL=on "
            "VTIUM_DISPLAY_BUFFERS=3 rustc --profile minspec -p vtium --bin vtium "
            f"--target-dir {self.build} --features images logind gpu "
            "-- -C link-arg=-Wl,-z,pack-relative-relocs",
            "VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset VTIUM_HWACCEL=on VTIUM_DISPLAY_BUFFERS=3 build --release "
            f"-p vtium-config -p vtium-image-helper --target-dir {self.build}"])

    def test_a_minspec_install_stages_the_frits_it_names_and_no_gallery(self):
        # ADR-1500 D8: a minspec package ships the two minspec frits alone.
        self.built("/usr/bin/vtium-config")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}",
                      "FRITS=minspec-16 minspec-8", "HWACCEL=off", "DISPLAY_BUFFERS=2")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.staged(), [f for f in self.STAGED if f != "etc/vtium/fonts/desktop.vtfrit"])
        self.assertEqual([c.split(" rustc ")[0].split(" build ")[0] for c in self.s.calls_of("cargo")], [
            "VTIUM_FONT_HELPER=/usr/bin/vtium-config VTIUM_IMAGE=unset VTIUM_HWACCEL=off VTIUM_DISPLAY_BUFFERS=2",
            "VTIUM_FONT_HELPER=unset VTIUM_IMAGE=unset VTIUM_HWACCEL=off VTIUM_DISPLAY_BUFFERS=2"])

    def test_install_refuses_a_frit_vtium_does_not_hold_and_a_setting_out_of_its_range(self):
        self.built("/usr/bin/vtium-config")
        for arg, why in (("FRITS=minspec-16 absent", "no frit absent in"),
                         ("HWACCEL=yes", "HWACCEL is on or off"),
                         ("HWACCEL=on off", "HWACCEL is on or off"),
                         ("DISPLAY_BUFFERS=4", "DISPLAY_BUFFERS is 1, 2 or 3")):
            p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}", arg)
            self.assertNotEqual(p.returncode, 0, arg)
            self.assertIn(why, p.stderr, arg)
        for target in ("install-tree", "install-build"):
            p = self.make(target, f"DESTDIR={self.dest}", f"INSTALL_TARGET_DIR={self.build}", "FRITS=minspec-16 absent")
            self.assertNotEqual(p.returncode, 0, target)
            self.assertIn("no frit absent in", p.stderr, target)
        # Every target that builds checks the settings, the tier 3 runs that
        # build the install tree among them. make -n only prints the
        # recipes, so a target that failed to check would build and run
        # nothing here either.
        for target in ("install-tree", "minspec", "vt-dev-console", "config-release", "image-release",
                       "tier3", "tier3-minimal", "tier3-images", "tier3-nosystemd"):
            for arg, why in (("HWACCEL=yes", "HWACCEL is on or off"),
                             ("DISPLAY_BUFFERS=0", "DISPLAY_BUFFERS is 1, 2 or 3")):
                p = self.make("-n", target, arg)
                self.assertNotEqual(p.returncode, 0, (target, arg))
                self.assertIn(why, p.stderr, (target, arg))
        # each is refused as make reads the Makefile, before any build
        self.assertEqual(self.s.calls_of("cargo"), [])

    def test_install_refuses_a_console_that_starts_another_helper(self):
        self.built("/usr/local/bin/vtium-dev-config")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}")
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("does not start its helper at /usr/bin/vtium-config", p.stderr)
        self.assertEqual(self.staged(), [])

    def test_install_with_images_refuses_a_console_that_starts_another_image_helper(self):
        self.built("/usr/bin/vtium-config", "/usr/local/bin/vtium-image")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}",
                      "FEATURES=images")
        self.assertNotEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("does not start its image helper at /usr/bin/vtium-image", p.stderr)
        self.assertEqual(self.staged(), [])

    def test_install_needs_destdir_named(self):
        # It never installs onto the machine, nor into install-tree's default:
        # an empty DESTDIR, or one that is the root, would write
        # /usr/local/lib/systemd/system/vtium@.service over the packaged unit.
        for target in ("install", "install-terminfo"):
            for args, why in (([], f"{target}: name DESTDIR=DIR"),
                              (["DESTDIR="], f"{target}: DESTDIR is empty"),
                              (["DESTDIR=/"], f"{target}: DESTDIR=/ is this machine's root"),
                              (["DESTDIR=//."], f"{target}: DESTDIR=//. is this machine's root"),
                              (["DESTDIR=/usr/.."], f"{target}: DESTDIR=/usr/.. is this machine's root")):
                with self.subTest(target=target, args=args):
                    p = self.make(target, "PREFIX=/usr", *args)
                    self.assertNotEqual(p.returncode, 0)
                    self.assertIn(why, p.stderr)
                    self.assertEqual(self.s.calls_of("cargo"), [])

    def test_install_names_the_license_for_the_package(self):
        self.built("/usr/bin/vtium-config")
        p = self.make("install", f"DESTDIR={self.dest}", "PREFIX=/usr", f"INSTALL_TARGET_DIR={self.build}",
                      "PKGNAME=vtium-git")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual([f for f in self.staged() if f.startswith("usr/share/licenses/")],
                         ["usr/share/licenses/vtium-git/COPYING", "usr/share/licenses/vtium-git/LICENSE",
                          "usr/share/licenses/vtium-git/OFL-1.1.txt",
                          "usr/share/licenses/vtium-git/rgb-license.txt"])
        self.assertFalse([f for f in self.staged() if f.startswith("usr/share/licenses/vtium/")], self.staged())

    def test_enabling_the_packaged_template_with_no_vt_links_nothing(self):
        # Enabling the template with no VT named must link nothing on every
        # machine (ADR-1500 D3). With DefaultInstance=tty1 it would enable VT
        # 1. With no DefaultInstance=, systemctl links Alias=autovt@.service
        # before it refuses the template, so on a machine with no
        # /etc/systemd/system/autovt@.service yet, logind would start vtium
        # on every VT it spawns. Each root: none, the getty's link, which
        # systemd's preset makes, and the C reference's.
        if not REAL_SYSTEMCTL:
            self.skipTest("no systemctl")
        for holder in (None, "getty@.service", "kmsconvt@.service"):
            with self.subTest(autovt=holder):
                root = os.path.join(self.s.tmp, f"root-{holder}")
                unit_dir = os.path.join(root, "usr/lib/systemd/system")
                _, p = self.units("--name", "vtium", "--exec", "/usr/bin/vtium", "--by", "a test",
                                  "--conflicts", "kmsconvt@%i.service", out=unit_dir)
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                write(os.path.join(unit_dir, "getty.target"), "[Unit]\nDescription=Login Prompts\n", 0o644)
                etc = os.path.join(root, "etc/systemd/system")
                os.makedirs(etc)
                before = []
                if holder:
                    os.symlink(f"/usr/lib/systemd/system/{holder}", os.path.join(etc, "autovt@.service"))
                    before = ["autovt@.service"]
                e = subprocess.run([REAL_SYSTEMCTL, f"--root={root}", "enable", "vtium@.service"],
                                   capture_output=True, text=True, timeout=WAIT_BOUND)
                self.assertNotEqual(e.returncode, 0, e.stdout + e.stderr)
                links = sorted(os.path.relpath(os.path.join(d, n), etc) for d, _, ns in os.walk(etc) for n in ns)
                self.assertEqual(links, before)
                # One VT named is enabled as before: its wants and its own
                # autovt link, which points at vtium@.
                e = subprocess.run([REAL_SYSTEMCTL, f"--root={root}", "enable", "vtium@tty3.service"],
                                   capture_output=True, text=True, timeout=WAIT_BOUND)
                self.assertEqual(e.returncode, 0, e.stdout + e.stderr)
                links = sorted(os.path.relpath(os.path.join(d, n), etc) for d, _, ns in os.walk(etc) for n in ns)
                self.assertEqual(links, sorted(before + ["autovt@tty3.service", "getty.target.wants/vtium@tty3.service"]))
                self.assertEqual(os.readlink(os.path.join(etc, "autovt@tty3.service")), "/usr/lib/systemd/system/vtium@.service")

    # A small entry under vtium's terminfo name, as the test's stand-in for
    # crates/vtium/install/xterm-vtium.ti.
    TERMINFO = "xterm-vtium|a stand-in for vtium's entry,\n\tam, cols#80, lines#24, bel=^G, clear=\\E[H\\E[2J,\n"

    def test_install_terminfo_stages_xterm_vtium_alone(self):
        if not shutil.which("tic"):
            self.skipTest("no tic")
        src = os.path.join(self.s.tmp, "xterm-vtium.ti")
        write(src, self.TERMINFO, 0o644)
        p = self.make("install-terminfo", f"DESTDIR={self.dest}", "PREFIX=/usr", f"TERMINFO_SRC={src}")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.staged(), ["usr/share/licenses/vtium-terminfo/COPYING",
                                         "usr/share/licenses/vtium-terminfo/LICENSE",
                                         "usr/share/terminfo/x/xterm-vtium"])
        for name in ("COPYING", "LICENSE"):
            self.assertEqual(read(os.path.join(self.dest, "usr/share/licenses/vtium-terminfo", name)),
                             read(os.path.join(ROOT, name)), name)
        p = self.make("install-terminfo", f"DESTDIR={self.dest}-git", "PREFIX=/usr", f"TERMINFO_SRC={src}",
                      "PKGNAME=vtium-terminfo-git")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for name in ("COPYING", "LICENSE"):
            self.assertTrue(os.path.isfile(os.path.join(self.dest + "-git", "usr/share/licenses/vtium-terminfo-git", name)), name)
        # Without the entry's source it stops, and stages nothing.
        shutil.rmtree(self.dest)
        p = self.make("install-terminfo", f"DESTDIR={self.dest}", "PREFIX=/usr", f"TERMINFO_SRC={src}.missing")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("install-terminfo: no", p.stderr)
        self.assertEqual(self.staged(), [])

    def units(self, *args, out=None):
        out = out or os.path.join(self.s.tmp, "units")
        return out, subprocess.run(["bash", UNITS_SH, *args, out], capture_output=True, text=True, timeout=WAIT_BOUND,
                                   env=self.s.env())

    def test_units_makes_the_packaged_and_the_development_units_from_one_source(self):
        src = read(os.path.join(ROOT, "crates/vtium/install/vtium@.service")).splitlines()
        for name, exec_path, conflicts in (("vtium", "/usr/bin/vtium", ("kmsconvt@%i.service",)),
                                           ("vtium-dev", "/usr/local/bin/vtium-dev", ("vtium@%i.service", "kmsconvt@%i.service")),
                                           ("vtium", "/usr/bin/vtium", ())):
            with self.subTest(name=name, conflicts=conflicts):
                out, p = self.units("--name", name, "--exec", exec_path, "--by", "a test",
                                    *[a for c in conflicts for a in ("--conflicts", c)],
                                    out=os.path.join(self.s.tmp, f"units-{name}-{len(conflicts)}"))
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertEqual(sorted(os.listdir(out)), [f"{name}-getty@.service", f"{name}@.service"])
                unit = read(os.path.join(out, f"{name}@.service")).splitlines()
                want = generated(src, name, exec_path, conflicts)
                self.assertEqual(unit[-len(want):], want)
                self.assertTrue(all(l.startswith("#") or l == "" for l in unit[:-len(want)]))
                self.assertIn(f"#   ExecStart runs {exec_path}", unit)
                fallback_src = read(os.path.join(ROOT, "crates/vtium/install/vtium-getty@.service")).splitlines()
                fallback = read(os.path.join(out, f"{name}-getty@.service")).splitlines()
                # The fallback runs EXEC's vtium console fallback.
                want = [l.replace("ExecStart=/usr/bin/vtium ", f"ExecStart={exec_path} ") for l in body(fallback_src)]
                self.assertEqual(fallback[-len(want):], want)
                self.assertIn(f"ExecStart={exec_path} console fallback %i", fallback)
                self.assertIn(f"#   ExecStart runs {exec_path} console fallback", fallback)

    def test_the_units_comments_name_their_own_units(self):
        # The shipped units' opening comments describe the source; the
        # generated units carry their own. A comment may name kmsconvt only
        # where the header says what the unit was made from, or a
        # Conflicts= unit.
        for name, exec_path, conflicts in (("vtium", "/usr/bin/vtium", ("kmsconvt@%i.service",)),
                                           ("vtium-dev", "/usr/local/bin/vtium-dev",
                                            ("vtium@%i.service", "kmsconvt@%i.service"))):
            with self.subTest(name=name):
                out, p = self.units("--name", name, "--exec", exec_path, "--by", "a test",
                                    *[a for c in conflicts for a in ("--conflicts", c)],
                                    out=os.path.join(self.s.tmp, f"comments-{name}"))
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                for f in (f"{name}@.service", f"{name}-getty@.service"):
                    lines = read(os.path.join(out, f)).splitlines()
                    stray = [l for l in lines if "kmsconvt" in l and not l.startswith("# Generated by ")
                             and "kmsconvt@%i.service" not in l]
                    self.assertEqual(stray, [], f)
                unit = read(os.path.join(out, f"{name}@.service"))
                comment = unit.split("[Unit]")[0]
                # The packaged unit names the vtium console verbs, which
                # manage vtium@ alone; the development unit names the
                # systemctl enable that make vt-install runs.
                verbs = ["#   vtium console enable 3", "#   vtium console enable --all",
                         "#   vtium console disable 3", "#   vtium console disable --all"]
                if name == "vtium":
                    for v in verbs:
                        self.assertIn(v + "\n", comment)
                    self.assertNotIn("systemctl enable", comment)
                else:
                    self.assertIn(f"#   systemctl enable {name}@tty3.service\n", comment)
                    self.assertNotIn("vtium console", comment)
                self.assertIn(f"# Enabling {name}@.service with no VT named fails and links nothing", comment)
                self.assertNotIn("by hand", comment)
                self.assertIn(f"{name}-getty@", comment)

    def test_units_refuses_a_name_or_a_path_it_cannot_put_in_a_unit(self):
        for args, why in ((["--name", "Vtium", "--exec", "/usr/bin/vtium"], "is not a unit name"),
                          (["--name", "vtium|x", "--exec", "/usr/bin/vtium"], "is not a unit name"),
                          (["--name", "vtium", "--exec", "usr/bin/vtium"], "is not an absolute path"),
                          (["--name", "vtium", "--exec", "/usr/bin/vt ium"], "is not an absolute path"),
                          (["--name", "vtium", "--exec", "/usr/bin/vtium|&"], "is not an absolute path"),
                          (["--name", "vtium", "--exec", "/usr/bin/vtium", "--conflicts", "getty@tty1.service"],
                           "is not an instance unit on the same VT"),
                          (["--name", "vtium", "--exec", "/usr/bin/vtium", "--conflicts", "kmsconvt@%i.service;x"],
                           "is not an instance unit on the same VT"),
                          (["--name", "vtium", "--exec", "/usr/bin/vtium", "--conflicts", "vtium@%i.service"],
                           "names the unit itself")):
            with self.subTest(args=args):
                out, p = self.units(*args, "--by", "a test")
                self.assertNotEqual(p.returncode, 0)
                self.assertIn(why, p.stderr)
                self.assertFalse(os.path.exists(out))

    def test_systemd_analyze_verifies_the_packaged_units(self):
        if not REAL_ANALYZE:
            self.skipTest("no systemd-analyze")
        root = os.path.join(self.s.tmp, "root")
        unit_dir = os.path.join(root, "usr/lib/systemd/system")
        _, p = self.units("--name", "vtium", "--exec", "/usr/bin/vtium", "--by", "a test", out=unit_dir)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        for prog in ("usr/bin/vtium", "usr/bin/systemctl"):
            os.makedirs(os.path.dirname(os.path.join(root, prog)), exist_ok=True)
            write(os.path.join(root, prog), "#!/bin/sh\n")
        # The stand-in sysinit.target that Install's analyze test explains:
        # systemd 255 fails the start job without one.
        write(os.path.join(unit_dir, "sysinit.target"),
              "[Unit]\nDescription=Stand-in for systemd's own sysinit.target\n", 0o644)
        for unit in ("vtium@tty4.service", "vtium-getty@tty4.service"):
            with self.subTest(unit=unit):
                v = subprocess.run([REAL_ANALYZE, "verify", "--man=no", f"--root={root}", os.path.join(unit_dir, unit)],
                                   capture_output=True, text=True, timeout=WAIT_BOUND)
                self.assertEqual((v.returncode, v.stdout + v.stderr), (0, ""))


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

    def wait_for(self, text, timeout=WAIT_BOUND):
        end = time.monotonic() + timeout
        while text not in self.out:
            if time.monotonic() > end or not self.pump(end - time.monotonic()):
                raise AssertionError(f"no {text!r} in:\n{self.out[-2000:].decode(errors='replace')}")
        self.out = self.out[self.out.index(text) + len(text):]

    def send(self, data):
        os.write(self.fd, data)

    def page(self):
        """Waits for accept.sh's cursor position query and answers it with a
        row the footer fits below."""
        self.wait_for(b"\x1b[6n")
        self.send(b"\x1b[14;1R")

    def finish(self, timeout=WAIT_BOUND):
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
                              capture_output=True, text=True, timeout=WAIT_BOUND)

    def test_every_guide_in_the_tree_passes_its_check(self):
        checks = os.path.join(VT, "checks")
        guides = sorted(f for f in os.listdir(checks) if f.endswith(".guide"))
        self.assertIn("start.guide", guides)
        for g in guides:
            with self.subTest(guide=g):
                p = subprocess.run(["bash", GUIDE_SH, "--check", os.path.join(checks, g)],
                                   capture_output=True, text=True, timeout=WAIT_BOUND)
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

    def test_a_cleanup_line_is_checked(self):
        ok = "title: T\nstep: S\ndo: d\nexpect: e\n"
        for text, why in ((ok + "cleanup: true\n", "cleanup: goes before the first step"),
                          ("title: T\ncleanup: true\ncleanup: true\n" + ok[9:], "one cleanup: line"),
                          ("title: T\ncleanup:\n" + ok[9:], "cleanup: needs the command")):
            with self.subTest(why=why):
                self.refused(self.check(text), why)
        p = self.check("title: T\ncleanup: rm -f x\n" + ok[9:])
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(p.stdout, "T: 1 steps\n   1  (no probe)   S\n  cleanup when the checks end: rm -f x\n")

    def test_the_cleanup_runs_however_the_checks_end(self):
        # A guide that changes the machine for some steps puts it back in its
        # cleanup: command, which runs once whether the checks end at q, at
        # the last step, when the terminal closes or on a signal.
        marks = os.path.join(self.s.tmp, "cleaned")
        guide = os.path.join(self.s.tmp, "t.guide")
        write(guide, f"title: Cleanup guide\ncleanup: echo cleaned >>{marks}\n"
                     "step: One\ndo: look\nexpect: e\nstep: Two\ndo: look\nexpect: e\n", 0o644)
        log = os.path.join(self.s.dir, "vt-check.log")

        def start():
            g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env())
            g.wait_for(b"Press Enter to begin.")
            g.send(b"\r")
            g.wait_for(b"Step 1 of 2: One")
            g.wait_for(b"Press Enter when done")
            return g

        def answer(g):
            g.send(b"\r")
            g.wait_for(b"[y]es [n]o [s]kip: ")
            g.send(b"y")
            g.wait_for(b"Note (Enter for none): ")
            g.send(b"\r")

        def last(g):
            answer(g)
            g.wait_for(b"Step 2 of 2: Two")
            g.wait_for(b"Press Enter when done")
            answer(g)
            g.wait_for(b"Press Enter to end the run.")
            g.send(b"\r")

        for how, end, status in (("q", lambda g: g.send(b"q\r"), 0),
                                 ("the last step", last, 0),
                                 ("end of input", lambda g: g.send(b"\x04"), 0),
                                 ("SIGTERM", lambda g: os.kill(g.pid, signal.SIGTERM), -signal.SIGTERM),
                                 ("SIGHUP", lambda g: os.kill(g.pid, signal.SIGHUP), -signal.SIGHUP)):
            with self.subTest(how=how):
                if os.path.exists(marks):
                    os.unlink(marks)
                g = start()
                end(g)
                code = g.finish()
                if status:
                    self.assertIn(code, (status, 128 - status))
                else:
                    self.assertEqual(code, 0)
                self.assertEqual(read(marks), "cleaned\n")
        self.assertIn(f"  cleanup: echo cleaned >>{marks}", read(log))
        # A guide with no cleanup: line runs nothing at its end.
        write(guide, "title: Plain guide\nstep: One\ndo: look\nexpect: e\n", 0o644)
        os.unlink(marks)
        g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env())
        g.wait_for(b"Press Enter to begin.")
        g.send(b"\r")
        g.wait_for(b"Press Enter when done")
        g.send(b"q\r")
        self.assertEqual(g.finish(), 0)
        self.assertNotIn(b"Cleanup:", g.seen)
        self.assertFalse(os.path.exists(marks))

    def test_restart_guide_gives_its_closed_policy_to_one_unit_and_takes_it_back(self):
        # restart.guide never changes /etc/vtium, which every vtium reads: its
        # closed vtium.conf goes into a directory of root's under /run, and a
        # drop-in under /run/systemd/system gives vtium-dev@ttyN alone that
        # directory as --configdir. Step 8 and the cleanup remove both.
        checks = os.path.join(VT, "checks")
        text = read(os.path.join(checks, "restart.guide"))
        dropin = read(os.path.join(checks, "restart-closed.service.conf"))
        confdir = "/run/vtium-restart-guide"
        unitdir = "/run/systemd/system/vtium-dev@tty{vt}.service.d"
        # The drop-in's ExecStart is make vt-install's, with the directory.
        out = os.path.join(self.s.tmp, "units")
        p = subprocess.run(["bash", UNITS_SH, "--name", "vtium-dev", "--exec", "/usr/local/bin/vtium-dev",
                            "--by", "a test", out], capture_output=True, text=True, timeout=WAIT_BOUND,
                           env=self.s.env())
        self.assertEqual(p.returncode, 0, p.stderr)
        unit = [l for l in read(os.path.join(out, "vtium-dev@.service")).splitlines() if l.startswith("ExecStart=")]
        self.assertEqual(len(unit), 1, unit)
        lines = [l for l in dropin.splitlines() if l and not l.startswith("#")]
        self.assertEqual(lines, ["[Service]", "ExecStart=", f"{unit[0]} --configdir {confdir}"])
        steps = {}
        for line in text.splitlines():
            if line.startswith("step: "):
                title = line[6:]
            elif line.startswith("run: "):
                steps[title] = line[5:]
            elif line.startswith("cleanup: "):
                steps["cleanup"] = line[9:]
        install = steps["vtium reads a closed policy"]
        self.assertIn(f"install -m 0644 -o root -g root tools/vt/checks/restart-closed.conf {confdir}/vtium.conf",
                      install)
        self.assertIn("install -m 0644 -o root -g root tools/vt/checks/restart-closed.service.conf "
                      f"{unitdir}/restart-guide.conf", install)
        self.assertLess(install.index("check-config"), install.index("sudo"))
        self.assertIn("daemon-reload", install)
        for name in ("Recovery after the start limit", "cleanup"):
            with self.subTest(name=name):
                run = steps[name]
                self.assertIn(f"d={unitdir}; ", run)
                self.assertIn('sudo rm -f "$d/restart-guide.conf"', run)
                self.assertIn(f"sudo rm -rf {confdir}", run)
                self.assertIn("daemon-reload", run)
        # Nothing the guide runs writes under /etc/vtium.
        for name, run in steps.items():
            with self.subTest(name=name):
                self.assertNotRegex(run, r"(install|cp|mv|rm|rmdir|tee)\b[^;&|]*/etc/vtium")

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
        self.assertEqual(sorted(os.listdir(self.s.tmp)), sorted(["standins", "vtium", "vtium-config", "vtium-image", "vt", "open", "home", "login-shell", "caller-home"]))
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
        # The command ends when the test creates this file, after it has typed its keys.
        release = os.path.join(self.s.tmp, "release")
        ran = ("printf 'ran %s on %s\\n' $((6 * 7)) \"$(tty)\"; "
               f"while [ ! -e {release} ]; do sleep 0.05; done")
        write(guide, "title: Run guide\n"
                     f"step: Ran\nrun: {ran}\ndo: keep still\nexpect: ran 42\n"
                     "step: Failed\nrun: echo VT {vt}; exit 3\nexpect: VT 6\n"
                     "step: Stopped\nrun: echo started; sleep infinity; echo not reached\nexpect: started\n"
                     "step: Quit\nrun: echo begun; sleep infinity\nexpect: begun\n", 0o644)
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
        # and answers nothing. The test looks for the keys only in what the
        # guide wrote after they were typed, because the command shown above
        # names the temporary directory, whose random name can hold them.
        typed = len(g.seen)
        g.send(b"yz")
        open(release, "w").close()
        g.wait_for(b"[y]es [n]o [s]kip: ")
        self.assertNotIn(b"yz", g.seen[typed:])
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
            end = time.monotonic() + WAIT_BOUND
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
                                     "-- step 3 of 4: Stopped", "  run: echo started; sleep infinity; echo not reached",
                                     "  command exited with status 130", "  answer: no  note: cut short",
                                     "-- step 4 of 4: Quit", "  run: echo begun; sleep infinity",
                                     "  command exited with status 131", "  answer: no  note: cut short"])
        self.assertRegex(log[16], r"^=== done ")


    def test_a_console_after_the_first_runs_a_shell_in_place_of_the_guide(self):
        # Ctrl+Super+Enter starts the head's command, the guide, again in the
        # new console (ADR-1401 D5); one run asks each step once.
        guide = os.path.join(self.s.tmp, "t.guide")
        write(guide, "title: Console guide\nstep: Look\ndo: look\nexpect: it is there\n", 0o644)
        g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env(VTIUM_HEAD="desk", VTIUM_CONSOLE="2"))
        g.wait_for(b"This is console 2 of the head desk. The guide runs in console 1.")
        g.wait_for(b"console-2$ ")
        g.send(b"echo shell $((6 * 7))\r")
        g.wait_for(b"shell 42")
        g.wait_for(b"console-2$ ")
        g.send(b"exit\r")
        self.assertEqual(g.finish(), 0)
        self.assertNotIn(b"Console guide", g.seen)
        self.assertFalse(os.path.exists(os.path.join(self.s.dir, "vt-check.log")))
        # The first console, and a value that is no console id, run the guide.
        for console in ("1", "x2", ""):
            with self.subTest(console=console):
                g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env(VTIUM_HEAD="desk", VTIUM_CONSOLE=console))
                g.wait_for(b"Console guide")
                g.wait_for(b"Press Enter to begin.")
                g.send(b"q\r")
                g.wait_for(b"Step 1 of 1: Look")
                g.send(b"q\r")
                self.assertEqual(g.finish(), 0)
                self.assertNotIn(b"This is console", g.seen)

    def test_the_closing_lines_fit_a_run_in_vtium_and_one_by_hand(self):
        # In vtium (VTIUM_CONSOLE set) the run ends with vtium; by hand in a
        # desktop terminal, as restart.guide runs, it ends in that terminal.
        guide = os.path.join(self.s.tmp, "t.guide")
        write(guide, "title: Closing guide\nstep: Look\ndo: look\nexpect: it is there\n", 0o644)
        log = os.path.join(self.s.dir, "vt-check.log")
        for env, start, end, absent in (
                ({"VTIUM_CONSOLE": "1"}, b"After the last step vtium exits and VT 6 gets its console back",
                 b"vtium exits and VT 6 gets its console back. Ctrl+Alt+F7 is your desktop.", b"in this terminal"),
                ({}, b"After the last step the checks end here, in this terminal.",
                 b"The answers are in " + log.encode() + b".", b"vtium exits")):
            with self.subTest(env=env):
                g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env(**env))
                g.wait_for(start)
                g.wait_for(b"Press Enter to begin.")
                g.send(b"\r")
                g.wait_for(b"Press Enter when done")
                g.send(b"\r")
                g.wait_for(b"[y]es [n]o [s]kip: ")
                g.send(b"y")
                g.wait_for(b"Note (Enter for none): ")
                g.send(b"\r")
                g.wait_for(b"Press Enter to end the run.")
                g.wait_for(end)
                g.send(b"\r")
                self.assertEqual(g.finish(), 0)
                self.assertNotIn(absent, g.seen)

    def test_the_steps_get_the_systems_locale_when_no_lang_is_set(self):
        conf = os.path.join(self.s.tmp, "locale.conf")
        write(conf, '# a comment\nLANG="de_DE.UTF-8"\nLC_TIME=en_GB.UTF-8\nLC_ALL=$(touch pwned)\n'
                    'LC_PAPER=de DE\nTZ=UTC\n', 0o644)
        guide = os.path.join(self.s.tmp, "t.guide")
        write(guide, "title: Locale guide\nstep: Locale\n"
                     "run: echo \"got LANG=$LANG LC_TIME=$LC_TIME LC_ALL=${LC_ALL-unset} "
                     "LC_PAPER=${LC_PAPER-unset} TZ=${TZ-unset}.\"\nexpect: e\n", 0o644)
        for env, want in (({}, b"got LANG=de_DE.UTF-8 LC_TIME=en_GB.UTF-8 LC_ALL=unset LC_PAPER=unset TZ=unset."),
                          ({"LANG": "C.UTF-8"}, b"got LANG=C.UTF-8 LC_TIME= LC_ALL=unset LC_PAPER=unset TZ=unset.")):
            with self.subTest(env=env):
                g = Pty(["/bin/bash", GUIDE_SH, guide], self.guide_env(GUIDE_LOCALE_CONF=conf, **env))
                g.wait_for(b"Press Enter to begin.")
                g.send(b"\r")
                g.wait_for(b"Press Enter to run it (")
                g.send(b"\r")
                g.wait_for(b"vtium-check$ echo")
                g.wait_for(want)
                g.wait_for(b"[y]es [n]o [s]kip: ")
                g.send(b"y")
                g.wait_for(b"Note (Enter for none): ")
                g.send(b"\r")
                g.wait_for(b"Press Enter to end the run.")
                g.send(b"\r")
                self.assertEqual(g.finish(), 0)
        self.assertFalse(os.path.exists("pwned"))
        self.assertFalse(os.path.exists(os.path.join(self.s.tmp, "pwned")))

if __name__ == "__main__":
    unittest.main()
