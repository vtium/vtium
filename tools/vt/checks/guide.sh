#!/bin/bash
# Guided VT checks: runs a guide's steps one screen each and records the
# answers. run-vt.sh --guide=FILE runs it as vtium's only session, so the
# operator switches to the VT and answers; nobody logs in. See
# docs/vt-review.md.
#
#   guide.sh [--check] GUIDE-FILE
#
# Each step says what to do and what to expect, runs its probe until q (or
# its shell until exit), then asks yes, no or skip and a note. The answers, and
# every line a probe prints, are appended to vt-check.log in /tmp/vtium-vt
# (VTIUM_VT_DIR) under a header line for the run.
# GUIDE_STEPS="4 8 9" (or 4,8,9) runs only those steps. In the guide's text,
# {vt} is GUIDE_VT and {desktop} GUIDE_DESKTOP, the VT of the run and the
# desktop's, as run-vt.sh sets them.
#
# --check reads the guide, checks it and the step list, prints the steps and
# exits, without a terminal.
#
# A guide is lines of "key: text"; blank lines and lines starting with # are
# skipped. "title:" once and "intro:" lines come first, then each step:
# "step: TITLE", an optional "probe: MODE" (shell, or one of probe.py --list),
# and "do:" and "expect:" lines, each shown as typed. A step without a probe
# asks once the operator presses Enter.
set -euo pipefail

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
probe=$here/probe.py
# shellcheck source=tools/vt/common.sh
. "$here/../common.sh"

die() {
	printf 'guide: %s\n' "$*" >&2
	exit 1
}

check=
[ "${1:-}" != --check ] || { check=1; shift; }
[ $# -eq 1 ] || { sed -n '7,7s/^# \{0,1\}//p' "$0" >&2; exit 2; }
guide=$1
[ -f "$guide" ] && [ -r "$guide" ] || die "cannot read the guide $guide"

vt=${GUIDE_VT:-N}
desktop=${GUIDE_DESKTOP:-N}
title=
intro=()
titles=()
probes=()
dos=()
expects=()
# shell is guide.sh's own: an interactive shell, for a step that types at a
# prompt. The others are probe.py's.
mapfile -t modes < <(echo shell; python3 "$probe" --list)

# Reads the guide into the arrays above, one entry per step, failing on the
# first line it cannot take, with its number.
n=0
while IFS= read -r line || [ -n "$line" ]; do
	n=$((n + 1))
	[[ $line =~ ^[[:space:]]*(#|$) ]] && continue
	[[ $line =~ ^([a-z]+):[[:space:]]?(.*)$ ]] || die "$guide:$n: not a 'key: text' line"
	key=${BASH_REMATCH[1]}
	text=${BASH_REMATCH[2]}
	text=${text//\{vt\}/$vt}
	text=${text//\{desktop\}/$desktop}
	# "Logo" was misread at the C reference's first guided pass; a guide names
	# the key as the operator's keyboard does.
	[[ ! ${text,,} =~ (^|[^[:alnum:]])logo([^[:alnum:]]|$) ]] ||
		die "$guide:$n: name the Super (Windows) key, not Logo"
	s=$((${#titles[@]} - 1))
	case $key in
	title)
		[ -z "$title" ] && [ "$s" -lt 0 ] || die "$guide:$n: one title, before the first step"
		title=$text
		;;
	intro)
		[ "$s" -lt 0 ] || die "$guide:$n: intro lines go before the first step"
		intro+=("$text")
		;;
	step)
		[ -n "$text" ] || die "$guide:$n: a step needs a title"
		titles+=("$text") probes+=("") dos+=("") expects+=("")
		;;
	probe | do | expect)
		[ "$s" -ge 0 ] || die "$guide:$n: $key: before the first step"
		case $key in
		probe)
			[ -z "${probes[s]}" ] || die "$guide:$n: one probe per step"
			[[ " ${modes[*]} " == *" $text "* ]] ||
				die "$guide:$n: probe mode '$text' is not one of: ${modes[*]}"
			probes[s]=$text
			;;
		do) dos[s]+=$text$'\n' ;;
		expect) expects[s]+=$text$'\n' ;;
		esac
		;;
	*) die "$guide:$n: unknown key '$key'" ;;
	esac
done <"$guide"
[ -n "$title" ] || die "$guide: no title: line"
total=${#titles[@]}
[ "$total" -gt 0 ] || die "$guide: no steps"
for ((s = 0; s < total; s++)); do
	[ -n "${dos[s]}" ] && [ -n "${expects[s]}" ] ||
		die "$guide: step $((s + 1)) (${titles[s]}) needs do: and expect: lines"
done

only=()
steps=${GUIDE_STEPS:-}
read -ra only <<<"${steps//,/ }"
for k in "${only[@]}"; do
	[[ $k =~ ^[1-9][0-9]*$ ]] && [ "$k" -le "$total" ] ||
		die "GUIDE_STEPS: '$k' is not a step of this guide (1 to $total)"
done

if [ -n "$check" ]; then
	printf '%s: %s steps\n' "$title" "$total"
	for ((s = 0; s < total; s++)); do
		printf '  %2d  %-12s %s\n' "$((s + 1))" "${probes[s]:-(no probe)}" "${titles[s]}"
	done
	exit 0
fi

log=$vt_dir/vt-check.log
# Stops the run before anything is asked, saying why on the screen.
refuse() {
	printf '\nguide: %s, so no answer would be kept.\nPress Enter to end.\n' "$1"
	read -r _ || true
	exit 1
}
vt_own_dir "$vt_dir" || refuse "$vt_dir is a symlink, or not a directory of yours"
touch -- "$log" 2>/dev/null || refuse "cannot write $log"
export PROBE_LOG=$log

note() { printf '%s\n' "$*" >>"$log"; }

# Ends the checks at step $k, the log's last line saying so.
ended() {
	note "$1"
	note "=== ended at step $k, $(date -Is)"
	exit 0
}

clear_screen() {
	printf '\033[0m\033[H\033[2J\033[3J'
}

# Prints a step's lines under a label, the label on the first one only.
lines() {
	local label=$1 l
	while IFS= read -r l; do
		printf '%-8s%s\n' "$label" "$l"
		label=
	done <<<"${2%$'\n'}"
}

ask() {
	local a n
	while :; do
		printf '\nLooked right? [y]es [n]o [s]kip: '
		# Only a closed terminal fails here: read -n takes Ctrl-D as a key.
		read -r -n 1 a || { echo; ended "  ended here (input closed)"; }
		echo
		case $a in
		y | Y) a=yes; break ;;
		n | N) a=no; break ;;
		s | S) a=skip; break ;;
		esac
	done
	printf 'Note (Enter for none): '
	read -r n || n=
	note "  answer: $a${n:+  note: $n}"
}

# The shell step's shell: no rc files, so the prompt is the one the step
# names, whoever runs it.
shell() {
	printf '\n'
	PS1='vtium-check$ ' /bin/bash --noprofile --norc -i
}

note "=== $title, $(date -Is), guide $(readlink -f "$guide"), tty $(tty), steps ${only[*]:-all}"

clear_screen
printf '%s\n\n' "$title"
[ ${#intro[@]} -eq 0 ] || printf '%s\n' "${intro[@]}" ""
cat <<EOF
Each step says what to do and what to expect. Press Enter to start it, do
what the step says, end it as the step says (q stops a probe, exit a shell),
then answer y, n or s, with a note if something looked off. A probe prints
each sequence the terminal sends, with the time it read it. At a step's
first prompt, s skips the step and q ends the checks.

After the last step vtium exits and VT $vt gets its console back;
Ctrl+Alt+F$desktop is your desktop.

Press Enter to begin.
EOF
read -r _ || true

for ((s = 0; s < total; s++)); do
	k=$((s + 1))
	if [ ${#only[@]} -gt 0 ] && [[ " ${only[*]} " != *" $k "* ]]; then
		continue
	fi
	clear_screen
	printf 'Step %s of %s: %s\n\n' "$k" "$total" "${titles[s]}"
	lines "Do:" "${dos[s]}"
	lines "Expect:" "${expects[s]}"
	case ${probes[s]} in
	'') printf '\nPress Enter when done (s then Enter skips this step, q ends the checks).\n' ;;
	shell) printf '\nPress Enter to start the shell (s then Enter skips this step, q ends the checks).\n' ;;
	*) printf '\nPress Enter to start the probe (s then Enter skips this step, q ends the checks).\n' ;;
	esac
	# Ctrl-D (end of file) at this prompt ends the checks, as q does.
	read -r go || go=q
	note "-- step $k of $total: ${titles[s]}"
	case $go in
	s | S)
		note "  answer: skip  (before the probe)"
		continue
		;;
	q | Q) ended "  ended here" ;;
	esac
	case ${probes[s]} in
	'') ;;
	shell) shell || note "  shell exited with status $?" ;;
	*) python3 "$probe" "${probes[s]}" || note "  probe exited with status $?" ;;
	esac
	ask
done

note "=== done $(date -Is)"
clear_screen
cat <<EOF
All steps done; the answers are saved. Press Enter to end the run.
vtium exits and VT $vt gets its console back. Ctrl+Alt+F$desktop is your desktop.
EOF
read -r _ || true
