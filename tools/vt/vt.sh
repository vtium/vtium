#!/bin/bash
# The operator's review of vtium on a real VT in one blocking command, as make
# vt-accept, vt-login, vt-check and vt-shell run it (docs/vt-review.md):
# picks the development VT, or a free VT where there is none, runs run-vt.sh
# there through sudo, and when the run ends prints a health check of vtium's
# log and the run's answers.
#
#   vt.sh [N] [-- VTIUM-OPTION...]
#
# make passes its variables in the environment, so none is ever recipe text:
#   VTIUM_VT_BIN      the vtium binary, which run-vt.sh runs as root
#   VTIUM_VT_HELPER   the glyph helper built with it, which run-vt.sh copies
#                     into /run/vtium-vt/ and names with --font-helper
#   VTIUM_VT_IMAGE    with a binary that has pictures, the image helper built
#                     with it, which run-vt.sh copies beside the glyph helper
#                     and names with --image-helper
#   VTIUM_VT_NR       the VT (VT=); without it, the development VT, the first
#                     VT whose manifest make vt-install wrote and whose
#                     vtium-dev@ttyN is enabled, which the run borrows and
#                     gives back; without one, the first free VT of 5, 6, 4,
#                     3 and 2. A VT can be borrowed when it is not on screen,
#                     no logind session is on it, the packaged vtium does not
#                     hold it and it is not logind's reserved VT; run-vt.sh
#                     checks again before it stops anything.
#   VTIUM_VT_ARGS     vtium options (DEMO_ARGS=), split into words by xargs,
#                     which honours quotes and backslashes and expands nothing
#   VTIUM_VT_SESSION  accept (the default), the acceptance pass; login, guide or shell;
#                     shell's session is recorded in a transcript beside
#                     vtium's log
#   VTIUM_VT_FEATURES the features the binary was built with (FEATURES=),
#                     which the acceptance pass's record names
#   VTIUM_VT_GUIDE    for guide: a name in tools/vt/checks (NAME.guide) or a
#                     path, checked here with guide.sh --check before sudo
#   VTIUM_VT_STEPS    for guide: only these steps (GUIDE_STEPS="4 8 9")
#   VTIUM_VT_DRY_RUN  1: pick the VT and print the run-vt.sh command, and run
#                     run-vt.sh --dry-run without sudo
#   VTIUM_VT_DEBUG    1: vtium logs its debug messages too, and the kernel's
#                     DRM state is copied into a directory beside the log
#                     each time a display comes or goes and at the end
#                     (run-vt.sh --debug)
#
# For the tests, VTIUM_VT_STANDINS names a directory of stand-ins: sudo,
# systemctl, loginctl, systemd-analyze, the file "active" in place of
# /sys/class/tty/tty0/active, and "root" in place of / for the manifests,
# come from there, and run-vt.sh takes its own (see run-vt.sh). VTIUM_VT_DIR
# is honoured only then, since a run's session writes to the default. The
# acceptance pass's passes go to ACCEPT_DIR, else $XDG_STATE_HOME/vtium/acceptance,
# which run-vt.sh hands the session, so the session and this script read the
# same directory.
set -euo pipefail

usage() {
	sed -n '7,7s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'vt: %s\n' "$*" >&2
	exit 1
}

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
standins=${VTIUM_VT_STANDINS:-}
[ -n "$standins" ] || unset VTIUM_VT_DIR
# shellcheck source=tools/vt/common.sh
. "$here/common.sh"

vt=${VTIUM_VT_NR:-}
extra=()
session=()
mode=${VTIUM_VT_SESSION:-accept}
case $mode in
accept | login | guide | shell) ;;
*) die "VTIUM_VT_SESSION=$mode: accept, login, guide or shell" ;;
esac
[ "$mode" = login ] && session=(--login)
[ "$mode" = shell ] && session=(--shell)
guide=
if [ "$mode" = guide ]; then
	guide=${VTIUM_VT_GUIDE:-}
	names=
	for g in "$here"/checks/*.guide; do
		[ ! -e "$g" ] || names+=" $(basename "$g" .guide)"
	done
	[ -n "$guide" ] || die "GUIDE=NAME names the guide; the guides in tools/vt/checks:${names:- none}"
	[[ $guide == */* ]] || guide=$here/checks/$guide.guide
	[ -f "$guide" ] || die "GUIDE=$VTIUM_VT_GUIDE: no guide $guide; the guides in tools/vt/checks:${names:- none}"
	guide=$(readlink -f -- "$guide")
	GUIDE_STEPS=${VTIUM_VT_STEPS:-} "$here/checks/guide.sh" --check "$guide" >/dev/null ||
		die "GUIDE=$VTIUM_VT_GUIDE: the guide or GUIDE_STEPS is refused (above)"
	# The step numbers guide.sh took, one space apart, as run-vt.sh takes
	# them: "4, 5" or a tab between them is typed easily.
	steps=()
	steps_typed=${VTIUM_VT_STEPS:-}
	read -ra steps <<<"${steps_typed//,/ }"
	session=(--guide="$guide")
	[ ${#steps[@]} -eq 0 ] || session+=(--steps="${steps[*]}")
elif [ -n "${VTIUM_VT_GUIDE:-}${VTIUM_VT_STEPS:-}" ]; then
	die "GUIDE and GUIDE_STEPS go with make vt-check"
fi
if [ -n "${VTIUM_VT_ARGS:-}" ]; then
	printf '%s' "$VTIUM_VT_ARGS" | xargs -r printf '%s\0' >/dev/null ||
		die "DEMO_ARGS cannot be split into words: $VTIUM_VT_ARGS"
	mapfile -d '' -t extra < <(printf '%s' "$VTIUM_VT_ARGS" | xargs -r printf '%s\0')
fi
while [ $# -gt 0 ]; do
	case $1 in
	-h | --help) usage ;;
	--) shift; extra+=("$@"); break ;;
	*) [ -z "$vt" ] || usage; vt=$1 ;;
	esac
	shift
done

bin=${VTIUM_VT_BIN:-}
[ -n "$bin" ] || die "VTIUM_VT_BIN names the vtium binary; make vt-accept sets it"
bin=$(readlink -m -- "$bin")
[ -f "$bin" ] && [ -x "$bin" ] || die "no vtium binary at $bin: build it first (make minspec)"
helper=${VTIUM_VT_HELPER:-}
helper_arg=()
if [ -n "$helper" ]; then
	helper=$(readlink -m -- "$helper")
	[ -f "$helper" ] && [ -x "$helper" ] || die "no glyph helper at $helper: build it first (make config-release)"
	helper_arg=(--helper="$helper")
fi
image=${VTIUM_VT_IMAGE:-}
if [ -n "$image" ]; then
	image=$(readlink -m -- "$image")
	[ -f "$image" ] && [ -x "$image" ] || die "no image helper at $image: build it first (make image-release)"
	helper_arg+=(--image="$image")
fi

dry=${VTIUM_VT_DRY_RUN:-}
case $dry in '' | 1) ;; *) die "VTIUM_VT_DRY_RUN=$dry: 1 or nothing" ;; esac
debug=${VTIUM_VT_DEBUG:-}
case $debug in '' | 1) ;; *) die "VTIUM_VT_DEBUG=$debug: 1 or nothing" ;; esac
active_file=/sys/class/tty/tty0/active
sudo=sudo
if [ -n "$standins" ]; then
	active_file=$standins/active
	sudo=$standins/sudo
	[ -x "$sudo" ] || die "VTIUM_VT_STANDINS has no stand-in sudo"
fi
loginctl=${standins:+$standins/}loginctl
systemctl=${standins:+$standins/}systemctl
analyze=${standins:+$standins/}systemd-analyze
passes=${ACCEPT_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/vtium/acceptance}
passes=$(readlink -m -- "$passes")
checklog=$vt_dir/vt-check.log

# The VT on screen now: the desktop's, since this runs from it.
active=$(cat "$active_file") || die "cannot read $active_file, so the VT on screen is unknown"
active=${active#tty}

# One "VT session (name type)" entry per logind session that sits on a VT. A
# session that ends between the list and the query is gone, so it is skipped.
sessions=()
list=$("$loginctl" list-sessions --no-legend) || die "loginctl list-sessions failed"
while read -r s _; do
	[ -n "$s" ] || continue
	n=$("$loginctl" show-session -p VTNr --value "$s" 2>/dev/null) || continue
	[ -n "$n" ] && [ "$n" != 0 ] || continue
	what=$("$loginctl" show-session -p Name -p Type --value "$s" 2>/dev/null | paste -sd' ') || continue
	sessions+=("$n $s ($what)")
done <<<"$list"

# Prints why VT $1 is refused, or nothing when a run can borrow it.
why_busy() {
	local e
	[ "$1" != "$active" ] || { echo "it is on screen"; return; }
	e=$(vt_borrowable "$systemctl" "$analyze" "$1")
	[ -z "$e" ] || { echo "$e"; return; }
	for e in "${sessions[@]}"; do
		[ "${e%% *}" != "$1" ] || { echo "logind session ${e#* } is on it"; return; }
	done
}

# Without VT=, the development VT, the lowest when make vt-install holds
# more than one. sed reads the whole list, so nothing before it in the pipe
# meets a closed pipe.
dev=
if [ -z "$vt" ]; then
	dev=$(vt_dev_vts "$systemctl" "${standins:+$standins/root}" | sed -n 1p)
	vt=$dev
fi
if [ -n "$vt" ]; then
	[[ $vt =~ ^[1-9][0-9]*$ ]] && [ "$vt" -ge 2 ] && [ "$vt" -le 12 ] ||
		die "VT must be a number from 2 to 12, not '$vt'"
	why=$(why_busy "$vt")
	if [ -n "$dev" ]; then
		hint="name another VT with VT=N"
		[[ $why != "logind session "* ]] || hint="log out there, or $hint"
		[ -z "$why" ] || die "the development VT $vt is refused: $why; $hint"
		echo "vt: VT $vt is the development VT (make vt-install VT=$vt); the run borrows it and gives it back as it found it"
	fi
	[ -z "$why" ] || die "VT $vt refused: $why"
else
	refused=
	for v in 5 6 4 3 2; do
		why=$(why_busy "$v")
		[ -n "$why" ] || { vt=$v; break; }
		refused+=$'\n'"  VT $v: $why"
	done
	[ -n "$vt" ] || die "no free VT among 5, 6, 4, 3 and 2; pick one with VT=N:$refused"
fi

if [[ $active =~ ^[0-9]+$ ]]; then
	back="Ctrl-Alt-F$active returns to this desktop"
else
	back="your desktop's Ctrl-Alt-F key returns to it"
fi

# The build, for the acceptance pass's title page: the checkout's commit, if git knows it.
label=$(git -C "$here" describe --always --dirty 2>/dev/null | tr -cd '[:alnum:] ._+:/@-' | cut -c1-60) || label=
log=$vt_dir/vt-$(date +%Y%m%dT%H%M%S)-tty$vt-XXXXXX.log
transcript=
drm=

# The command, once the log's name, and with it the transcript's, is final.
build_cmd() {
	local record=() state=()
	[ -z "$transcript" ] || record=(--transcript="$transcript")
	[ -z "$drm" ] || state=(--debug="$drm")
	cmd=("$here/run-vt.sh" --bin="$bin" "${helper_arg[@]}" --label="dev build ${label:-unknown}"
		--features="${VTIUM_VT_FEATURES:-}" --passes="$passes" "${state[@]}" "${session[@]}"
		"${record[@]}" "$vt" -- "${extra[@]}")
}

banner() {
	local rule
	rule=$(printf '=%.0s' {1..64})
	printf '\n%s\n  Switch to VT %s now: Ctrl-Alt-F%s    (%s)\n%s\n\n' \
		"$rule" "$vt" "$vt" "$back" "$rule"
}

if [ -n "$dry" ]; then
	[ "$mode" != shell ] || transcript=${log%.log}.transcript
	[ -z "$debug" ] || drm=${log%.log}.drm
	build_cmd
	banner
	show=("$sudo" "${cmd[@]}")
	printf '+'
	printf ' %q' "${show[@]}"
	printf '\n'
	answers=$passes
	[ -z "$guide" ] || answers=$checklog
	if [ "$mode" = shell ]; then
		echo "  (vtium's log would go to $log; the shell's transcript to $transcript)"
	else
		echo "  (vtium's log would go to $log; the answers are read from $answers)"
	fi
	[ -z "$drm" ] || echo "  (the DRM state would be copied into $drm)"
	echo "  run-vt.sh --dry-run, without sudo:"
	"${cmd[0]}" --dry-run "${cmd[@]:1}"
	exit
fi

# The log goes only in a directory of the invoking user's own, under a new
# name mktemp creates (never a file already there).
vt_own_dir "$vt_dir" || die "$vt_dir is a symlink, or not a directory of yours; remove it and run again"
log=$(mktemp --suffix=.log "${log%.log}") || die "cannot create a log in $vt_dir"
# The transcript is named after the log, so its name is new as well. The shell
# writes it as the invoking user.
if [ "$mode" = shell ]; then
	transcript=${log%.log}.transcript
	(umask 077 && set -C && : >"$transcript") || die "cannot create the transcript $transcript"
fi
# So is the directory the DRM state is copied into, as the user.
if [ -n "$debug" ]; then
	drm=${log%.log}.drm
	mkdir -m 0700 -- "$drm" || die "cannot create the directory $drm for the DRM state"
fi
build_cmd
# Ask for the password, if sudo wants one, here, before the operator leaves
# for the VT.
"$sudo" -v || die "sudo failed"

# A pass written from here on belongs to this run.
started=$(date +%s)
checked=0
[ ! -f "$checklog" ] || checked=$(stat -c %s "$checklog")

banner
# Ctrl-C, a closed terminal or SIGTERM reaches run-vt.sh, whose trap restores
# the VT. tee ignores them and keeps reading, so the restore never writes into
# a closed pipe; on Ctrl-C this script stays to print the summary.
trap 'echo "vt: interrupted" >&2' INT
set +e
"$sudo" "${cmd[@]}" 2>&1 | (trap '' INT HUP TERM; exec tee -p -a "$log")
status=${PIPESTATUS[0]}
set -e
trap - INT

# The dev build's pass this run wrote: the record of a "vtium dev build"
# label written last since the start; an acceptance pass in another terminal may
# have written its own meanwhile.
pass=
if [ -d "$passes" ]; then
	newest=0
	for f in "$passes"/*.md; do
		[ -f "$f" ] && grep -q '^- label: vtium dev build ' "$f" || continue
		t=$(stat -c %Y -- "$f")
		[ "$t" -ge "$started" ] && [ "$t" -ge "$newest" ] || continue
		pass=$f newest=$t
	done
fi

# A Rust panic prints this line before the release profile aborts.
panics=$(grep -c 'panicked at' "$log" || true)
echo
if [ "$panics" -eq 0 ]; then
	echo "health: 0 'panicked at' lines in vtium's log"
else
	echo "health: $panics 'panicked at' lines in vtium's log: vtium panicked"
fi
echo "vtium log: $log"
[ -z "$transcript" ] || echo "transcript: $transcript"
[ -z "$drm" ] || echo "DRM state: $drm"
echo

# The guided checks' lines, from the run's header on.
added=
if [ -n "$guide" ]; then
	if [ -f "$checklog" ]; then
		[ "$(stat -c %s "$checklog")" -ge "$checked" ] || checked=0
		added=$(tail -c "+$((checked + 1))" "$checklog")
	fi
	[ -z "$added" ] || printf '%s\n\n' "$added"
	echo "results: $(readlink -m "$checklog")"
fi

# The acceptance pass's record; a login's or a shell's, when an acceptance pass was run in it.
if [ "$mode" = accept ] || { [ "$mode" != guide ] && [ -n "$pass" ]; }; then
	[ -z "$pass" ] || "$here/feedback.sh" --dir "$passes" "$pass"
	echo
	echo "results: ${pass:-$passes}"
fi

# A login ends at its logout, a shell when it exits, and either at Ctrl-C
# (130); each is a normal end.
if { [ "$mode" = login ] || [ "$mode" = shell ]; } && [ "$status" -eq 130 ]; then
	echo "vt-$mode: ended with Ctrl-C"
	status=0
fi
if [ "$status" -ne 0 ]; then
	echo "vt: run-vt.sh exited with status $status: vtium failed or the run was cut short" >&2
	exit "$status"
fi
case $mode in
guide) [ -n "$added" ] || die "no lines from this run in $checklog: the guided checks did not start" ;;
accept) [ -n "$pass" ] || die "no pass from this run in $passes: the acceptance pass did not run, or q was pressed before the pass started" ;;
esac
