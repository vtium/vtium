#!/bin/bash
# Runs vtium on one VT in place of the VT's console unit (kmsconvt@, getty@ or
# autovt@ttyN), with the demo, a login prompt, a guide or the user's shell as
# its only session.
# When the session ends, this script is interrupted or vtium fails, the units
# that were running are started again. See docs/vt-review.md.
#
#   sudo tools/vt/run-vt.sh --bin=PATH [--dry-run] [--label=TEXT] [--login |
#        --guide=FILE [--steps=LIST] | --shell [--transcript=FILE]] [N] [-- vtium-option...]
#
# --bin names the vtium binary, which runs as root: a regular file owned by
# root or the invoking user, writable by nobody else. N is the VT, 2-12,
# default 3. Options after -- go to vtium, for one machine's display and font
# settings (--mode=..., --font-size=...). Only display, font, palette, text and
# input options are accepted, each as one word: --name=value, or --name /
# --no-name for a switch. --label names the build on the demo's title page.
# --login runs vtium's login prompt (/bin/login, through PAM) in place of the
# demo, so the session is a logind session. The first logout ends the run; so
# do Ctrl-C, a vtium crash and vtium ending on its own, the last two as
# failures.
# --guide=FILE runs the guided checks of FILE (checks/guide.sh) in place of
# the demo, as the user, with no login; --steps="4 8 9" (or 4,8,9) runs only
# those steps. The answers go to /tmp/vtium-vt/vt-check.log.
# --shell runs the user's login shell, the one the password database names and
# /etc/shells lists, as the user, with no login, in the user's home directory,
# with the system's locale and, when they are the user's, the user's runtime
# directory and bus. Exiting it ends the run. --transcript=FILE records the session into FILE
# with script(1), which runs as the user; the user must be able to write FILE's
# directory, and FILE must not be a symlink.
# --dry-run asks what it would ask (the VT on screen, the sessions, the active
# units) and prints every command that would change anything without running
# it; it needs no root.
#
# For the tests, VTIUM_VT_STANDINS names a directory holding stand-ins for
# systemctl, loginctl, setpriv and getent, a file "active" in place of
# /sys/class/tty/tty0/active, "shells" and "locale.conf" in place of
# /etc/shells and /etc/locale.conf, and "run-user" in place of /run/user.
# Each is run from there by its full path, so no system command is reached,
# and the run goes through without root. As root the variable is refused.
set -euo pipefail
# sudo's own PATH, whatever the caller's, before any command runs: what runs
# as root is the system's. The tests' stand-ins are named by full path.
PATH=/usr/sbin:/usr/bin:/sbin:/bin

usage() {
	sed -n '8,9s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'run-vt: %s\n' "$*" >&2
	exit 1
}

dry=
vt=
bin=
label=
extra=()
login=
guide=
steps=
shell=
transcript=
while [ $# -gt 0 ]; do
	case $1 in
	--dry-run) dry=1 ;;
	--bin=*) bin=${1#*=} ;;
	--label=*) label=${1#*=} ;;
	--login) login=1 ;;
	--guide=*) guide=${1#*=}; [ -n "$guide" ] || die "--guide needs the guide's path: --guide=tools/vt/checks/NAME.guide" ;;
	--steps=*) steps=${1#*=} ;;
	--shell) shell=1 ;;
	--transcript=*) transcript=${1#*=}; [ -n "$transcript" ] || die "--transcript needs the file's path" ;;
	-h | --help) usage ;;
	--) shift; extra=("$@"); break ;;
	[0-9]*) [ -z "$vt" ] || usage; vt=$1 ;;
	*) usage ;;
	esac
	shift
done
vt=${vt:-3}

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
checkout=$(readlink -m "$here/../..")
standins=${VTIUM_VT_STANDINS:-}
if [ -n "$standins" ]; then
	[ "$(id -u)" -ne 0 ] || die "VTIUM_VT_STANDINS is for the tests, which run without root; unset it"
	for c in systemctl loginctl setpriv getent; do
		[ -x "$standins/$c" ] || die "VTIUM_VT_STANDINS has no stand-in $c"
	done
	systemctl=$standins/systemctl
	loginctl=$standins/loginctl
	setpriv=$standins/setpriv
	getent=$standins/getent
	active_file=$standins/active
	shells_file=$standins/shells
	locale_file=$standins/locale.conf
	run_user=$standins/run-user
else
	systemctl=systemctl
	loginctl=loginctl
	# vtium starts its session with execve, without a PATH search.
	setpriv=$(command -v setpriv) || die "setpriv (util-linux) is missing; it runs the session as you"
	getent=getent
	active_file=/sys/class/tty/tty0/active
	shells_file=/etc/shells
	locale_file=/etc/locale.conf
	run_user=/run/user
fi
# The units that may hold the VT: the stock kmscon, a getty, and autovt@,
# which logind starts on a switch to the VT and which is one of the other two.
units=("kmsconvt@tty$vt.service" "getty@tty$vt.service" "autovt@tty$vt.service")

# show prints a command as it would be typed; act shows it, then runs it unless
# this is a dry run.
show() {
	printf '+'
	printf ' %q' "$@"
	printf '\n'
}
act() {
	show "$@"
	[ -n "$dry" ] || "$@"
}

[[ $vt =~ ^[1-9][0-9]*$ ]] && [ "$vt" -ge 2 ] && [ "$vt" -le 12 ] ||
	die "VT must be a number from 2 to 12, not '$vt'"
# The allowlist is written for the command line #21 gives vtium, the C
# reference's; vtium parses no arguments yet. The checks below are made for
# VT $vt with the demo as the session, so only options that cannot change that
# are let through: an allowlist of exact names, since the C reference's
# getopt_long also took a unique prefix (--lo for --login). An
# option with a value carries it in the same word: a separate word would be
# taken as the value, or, as a stray word, moved to the end of argv, into the
# login command that runs as root. Left out on purpose: the VT, seat and
# session options, --login, --configdir, --oneshot, the grabs but grab-paste
# (one reboots), and options naming files (asciicast, issue-path, xkb-keymap,
# xkb-compose-file).
valued=" font-engine font-name font-size mode multi-monitor gpus term sb-size
	palette palette-black palette-red palette-green palette-yellow palette-blue
	palette-magenta palette-cyan palette-light-grey palette-dark-grey
	palette-light-red palette-light-green palette-light-yellow palette-light-blue
	palette-light-magenta palette-light-cyan palette-white palette-foreground
	palette-background underline-color strike-color grapheme-width xkb-model
	xkb-layout xkb-variant xkb-options xkb-repeat-delay xkb-repeat-rate
	dpms-timeout render-timer image-storage-limit sync-timeout blink-text
	grab-paste pointer-hide-timeout pointer-char "
switches=" drm hwaccel use-original-mode blink bell soft-cursor mouse
	natural-scrolling force-decoration-colors backspace-delete verbose debug silent
	blink-decorations program-blank pointer-hide-while-typing "
declare -A unit_shift=([B]=0 [K]=10 [M]=20 [G]=30)
valued=$(tr -s ' \t\n' ' ' <<<"$valued")
switches=$(tr -s ' \t\n' ' ' <<<"$switches")
for o in "${extra[@]}"; do
	name=${o#--}
	# vtium holds the image budget as root here: auto, or up to 4G.
	if [[ $o == --image-storage-limit=* && ${o#*=} != auto ]]; then
		size=${o#*=}
		if ! [[ $size =~ ^([0-9]{1,10})([KMG]?)$ ]] ||
			(( 10#${BASH_REMATCH[1]} > (4 << 30) >> ${unit_shift[${BASH_REMATCH[2]:-B}]} )); then
			die "--image-storage-limit takes auto or a size up to 4G here, not '$size'"
		fi
	fi
	# A name is lower-case letters and dashes, so no two names of a list,
	# joined by a space, pass as one.
	if [[ $o == --* && $name =~ ^[a-z][a-z-]*= ]]; then
		[[ $valued == *" ${name%%=*} "* ]] && continue
	elif [[ $o == --* && $name =~ ^[a-z][a-z-]*$ ]]; then
		[[ $switches == *" ${name#no-} "* ]] && continue
	fi
	die "vtium option '$o' is refused. Accepted: display, font, palette, text and input options, --render-timer, --image-storage-limit and --sync-timeout, one word each: --name=value (quoted as one word: --font-name=\"Monoid Nerd Font Mono\"), or --name / --no-name for a switch (see docs/vt-review.md)"
done
# The label reaches the session as one word of env's; it names a build, so it
# takes a build's characters only.
[[ $label =~ ^[[:alnum:]\ ._+:/@-]{0,80}$ ]] ||
	die "--label takes up to 80 letters, digits, spaces and ._+:/@-, not '$label'"
[ -z "$login" ] || [ -z "$guide" ] || die "--guide runs the guided checks, with no login"
[ -z "$shell" ] || [ -z "$login$guide" ] || die "--shell runs your shell, with no login and no guide"
[ -z "$transcript" ] || [ -n "$shell" ] || die "--transcript goes with --shell"
if [ -n "$guide" ]; then
	# The guide is read by the session, as the user, from this absolute path.
	guide=$(readlink -m -- "$guide")
	[[ -z $steps || $steps =~ ^[1-9][0-9]*([ ,][1-9][0-9]*)*$ ]] ||
		die "--steps takes step numbers, \"4 8 9\" or 4,8,9, not '$steps'"
elif [ -n "$steps" ]; then
	die "--steps goes with --guide=FILE"
fi
[ -n "$dry$standins" ] || [ "$(id -u)" -eq 0 ] || die "run as root, through sudo"

user=${SUDO_USER:-}
[ -z "$dry$standins" ] || user=${user:-$(id -un)}
[ -n "$user" ] && [ "$user" != root ] ||
	die "run through sudo from your own account; the session runs as that user, never as root"
# The user's entry in the password database: the uid, gid, home and login
# shell, from one lookup.
entry=$("$getent" passwd "$user") || die "no such user: $user"
IFS=: read -r _ _ uid gid _ home login_shell <<<"$entry"
[[ $uid =~ ^[0-9]+$ && $gid =~ ^[0-9]+$ ]] || die "$user's entry in the password database is unreadable"
[ "$uid" -ne 0 ] || die "$user has uid 0; the session runs as your own user, never as root"
if [ -n "$shell" ]; then
	# The shell reaches script(1) inside a command line, so its path takes no
	# character a shell would read.
	[ -n "$login_shell" ] || die "$user has no login shell in the password database"
	[[ $login_shell =~ ^/[[:alnum:]/._+-]+$ ]] ||
		die "$user's login shell '$login_shell' is not an absolute path of letters, digits and /._+-"
	grep -Fqx -- "$login_shell" "$shells_file" ||
		die "$user's login shell $login_shell is not in $shells_file"
	[ -f "$login_shell" ] && [ -x "$login_shell" ] ||
		die "$user's login shell $login_shell is not an executable file"
	[[ $home == /* ]] || die "$user's home directory '$home' is not an absolute path"
	# The shell would start nowhere, and the run would end as if it had exited.
	[ -d "$home" ] && [ "$(stat -L -c %u -- "$home")" = "$uid" ] ||
		die "$user's home directory $home is missing, not a directory, or not theirs"
	# setpriv --reset-env drops the locale, so the shell gets the system's:
	# LANG and LC_* from locale.conf, each a value of locale-name characters.
	locale=()
	if [ -f "$locale_file" ]; then
		assignment='^(LANG|LC_[A-Z]+)="?([[:alnum:]_.@-]+)"?$'
		while IFS= read -r line || [ -n "$line" ]; do
			[[ ! $line =~ $assignment ]] || locale+=("${BASH_REMATCH[1]}=${BASH_REMATCH[2]}")
		done <"$locale_file"
	fi
	# The user's runtime directory and bus, which the desktop session already
	# gives the user: pipewire, systemctl --user and the user's agents. Only
	# a directory of the user's alone, never a symlink; /run/user is root's,
	# so the user cannot swap it afterwards.
	runtime=()
	rt=$run_user/$uid
	if [ -d "$rt" ] && [ ! -L "$rt" ] && [ "$(stat -c '%u %a' -- "$rt")" = "$uid 700" ]; then
		runtime=(XDG_RUNTIME_DIR="$rt")
		[ ! -S "$rt/bus" ] || [ -L "$rt/bus" ] || runtime+=(DBUS_SESSION_BUS_ADDRESS="unix:path=$rt/bus")
	fi
	if [ -n "$transcript" ]; then
		# Absolute, and not resolved: a symlink is refused below.
		[[ $transcript == /* ]] || transcript=$PWD/$transcript
		# vtium starts its session with execve, without a PATH search.
		script=$(command -v script) || die "script (util-linux) is missing; it records the transcript"
	fi
fi

# The binary runs as root, so only its owner or root may have written it.
[ -n "$bin" ] || die "--bin=PATH names the vtium binary (make vt-demo builds target/minspec/vtium)"
bin=$(readlink -m -- "$bin")
[ -f "$bin" ] && [ -x "$bin" ] || die "no vtium binary at $bin: build it first (make minspec)"
read -r owner mode < <(stat -c '%u %a' -- "$bin")
[ "$owner" -eq 0 ] || [ "$owner" -eq "$uid" ] ||
	die "$bin belongs to uid $owner, neither root nor $user; it would run as root"
(( (8#$mode & 8#022) == 0 )) ||
	die "$bin is writable by group or others (mode $mode); it would run as root"

# --inh-caps=-all empties the one capability set the uid change leaves. The
# bounding set and no-new-privs are left alone: sudo works in the shell.
drop=("$setpriv" --reuid="$uid" --regid="$gid" --init-groups --inh-caps=-all --reset-env)
if [ -n "$guide" ] && [ -z "$dry" ]; then
	"${drop[@]}" -- /usr/bin/test -f "$guide" -a -r "$guide" ||
		die "$user cannot read the guide $guide"
fi
if [ -n "$transcript" ] && [ -z "$dry" ]; then
	"${drop[@]}" -- /usr/bin/test -d "${transcript%/*}/" -a -w "${transcript%/*}/" -a ! -h "$transcript" ||
		die "$user cannot write the transcript $transcript"
fi

# One run at a time, whoever starts it: one build on a VT. The lock is held
# until this script ends; it lives in /run, which only root can write, and is
# opened without truncation. vtium and its session do not inherit it.
if [ -z "$dry" ]; then
	lock=/run/vtium-vt.lock
	[ -z "$standins" ] || lock=$standins/vt.lock
	exec 9<>"$lock"
	flock -n 9 || die "another VT review run is in progress (it holds $lock); one build on a VT at a time"
fi

# Prints "ID STATE" for each logind session on VT $vt, and fails when loginctl
# cannot list the sessions. A session whose query fails is skipped only when a
# second list no longer has it, as one that ended between the list and the
# query; one still listed is a session on an unknown VT, so that fails too.
vt_sessions() {
	local list s props
	list=$("$loginctl" list-sessions --no-legend) || return 1
	while read -r s _; do
		[ -n "$s" ] || continue
		if ! props=$("$loginctl" show-session -p VTNr -p State "$s" 2>/dev/null); then
			"$loginctl" list-sessions --no-legend | awk -v s="$s" '$1 == s { listed = 1 } END { exit listed }' ||
				return 1
			continue
		fi
		[ "$(sed -n 's/^VTNr=//p' <<<"$props")" != "$vt" ] ||
			echo "$s $(sed -n 's/^State=//p' <<<"$props")"
	done <<<"$list"
}

# Refuse the VT on screen (the one this command was typed on) and any VT a
# logind session sits on, the graphical one included: stopping the unit there
# would end that session. When the sessions cannot be listed, refuse too. A
# dry run asks the same, since asking changes nothing.
fg=$(cat "$active_file") || die "cannot read $active_file, so the VT on screen is unknown"
fg=${fg#tty}
[[ $fg =~ ^[0-9]+$ ]] || die "$active_file reads '$fg', so the VT on screen is unknown"
[ "$fg" != "$vt" ] || die "VT $vt is on screen now; run this from another VT"
on_vt=$(vt_sessions) ||
	die "loginctl could not list the sessions or read one of them, so whether a session is on VT $vt is unknown"
if [ -n "$on_vt" ]; then
	s=${on_vt%% *}
	die "logind session $s ($("$loginctl" show-session -p Name -p Type --value "$s" | paste -sd' ')) is on VT $vt; pick another VT"
fi

# The VT's units that run now: each is stopped below and started again on
# exit. autovt@ttyN is another name for one of the others, which systemctl
# takes for the same unit.
active=()
for u in "${units[@]}"; do
	if "$systemctl" is-active --quiet "$u"; then
		active+=("$u")
	fi
done

child=
# Whether vtium still runs, asked of this shell's job table rather than of its
# PID: once bash has reaped it, the PID may be another process's. grep reads
# all of jobs' output, so pipefail sees no SIGPIPE.
running() {
	[ -n "$child" ] && jobs -rp | grep -x -- "$child" >/dev/null
}
# shellcheck disable=SC2317,SC2329 # invoked by the EXIT trap
# A second signal, or a closed terminal or pipe, must not cut it short before
# the units are started again, so it ignores them while it runs, and no failed
# write ends it. A failed start exits 1.
restore() {
	set +e
	trap '' INT TERM HUP PIPE
	trap - EXIT
	if running; then
		show kill -TERM "$child" || true
		kill -TERM "$child" 2>/dev/null || true
		for _ in $(seq 50); do
			running || break
			sleep 0.1
		done
		if running; then
			show kill -KILL "$child" || true
			kill -KILL "$child" 2>/dev/null || true
		fi
		wait "$child" 2>/dev/null || true
	fi
	if [ ${#active[@]} -gt 0 ] && ! act "$systemctl" start "${active[@]}"; then
		echo "run-vt: restarting ${active[*]} failed; restore it with: sudo systemctl start ${active[*]}" >&2
		exit 1
	fi
}
trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

[ ${#active[@]} -eq 0 ] || act "$systemctl" stop "${active[@]}"

label=${label:-unlabelled}
ends="the demo ends" last=3
[ -z "$login" ] || ends="it ends" last=4
[ -z "$guide" ] || ends="the checks end"
[ -z "$shell" ] || ends="the shell exits"
if [ ${#active[@]} -gt 0 ]; then
	after="when $ends, ${active[*]} is started again on VT $vt"
	manual="manual restore: sudo systemctl start ${active[*]}"
else
	after="no console unit was running on VT $vt, so nothing is started when $ends"
	manual="nothing to restore afterwards"
fi
if [ -n "$login" ]; then
	what="log in, then, in $checkout:
       DEMO_LABEL='vtium $label' tools/vt/demo.sh
           the demo; its answers go where make vt-demo's go
     until someone logs in, the prompt comes back after login's timeout
  3. log out to end it; Ctrl-C here ends it too, the login with it"
elif [ -n "$guide" ]; then
	what="follow each step of $guide${steps:+ (steps $steps)}
     and answer y, n or s; the run ends after the last step"
elif [ -n "$shell" ]; then
	what="use $login_shell, which runs as $user in $home;
     exit it to end the run${transcript:+; the session is recorded in
     $transcript}"
else
	what="rate each page with 1, 2 or 3 (c adds a comment, b goes back);
     press q in the demo to finish"
fi
users_shell="$user's shell"
cat <<EOF

vtium $label on VT $vt${login:+, with a login prompt}${guide:+, with guided checks}${shell:+, with $users_shell}
  1. switch with Ctrl-Alt-F$vt
  2. $what
  $last. $after;
     switch back to your desktop with Ctrl-Alt-F$fg
$manual

EOF

# The demo, the guide and the shell run as $user, never as root, with the
# environment cleared by setpriv; env then sets what the session reads, each
# value one word of its own, never through a shell. The user's shell starts
# in the home directory as a login shell (-l), as login(1) would start it;
# script(1) runs the command line it is given through the user's $SHELL.
# vtium runs in the background, so it starts with SIGINT and SIGQUIT ignored;
# its pty starts the session with every signal at its default. With
# --login, /bin/login runs as root and PAM decides who logs in, as under the
# stock unit; it runs without --oneshot, so login's own timeout at the prompt
# brings a new prompt rather than the end, and this script ends the run at the
# first logout instead, below. vtium is started in the background so that it
# ignores Ctrl-C (bash does that for background jobs in a script) and this
# script decides how it stops: SIGTERM from restore(). --configdir keeps the
# system's config out of the run; config/ holds the review's own.
session=(/usr/bin/env DEMO_LABEL="vtium $label" /bin/bash "$here/demo.sh")
if [ -n "$guide" ]; then
	session=(/usr/bin/env GUIDE_VT="$vt" GUIDE_DESKTOP="$fg" ${steps:+GUIDE_STEPS="$steps"}
		/bin/bash "$here/checks/guide.sh" "$guide")
elif [ -n "$shell" ]; then
	session=(/usr/bin/env -C "$home" "${locale[@]}" "${runtime[@]}" "$login_shell" -l)
	[ -z "$transcript" ] ||
		session=(/usr/bin/env -C "$home" "${locale[@]}" "${runtime[@]}"
			"$script" -q -f -c "exec $login_shell -l" "$transcript")
fi
once=(--oneshot)
login_argv=("${drop[@]}" -- "${session[@]}")
if [ -n "$login" ]; then
	once=()
	login_argv=(/bin/login -p)
fi
cmd=("$bin" --vt="tty$vt" --no-switchvt --configdir "$here/config" "${once[@]}" "${extra[@]}"
	--login -- "${login_argv[@]}")
show "${cmd[@]}"
[ -z "$dry" ] || exit 0
# A signal in the moment between these two lines finds child still empty, so
# restore() starts the units again without stopping vtium. The window is a few
# microseconds and is left open.
"${cmd[@]}" 9>&- &
child=$!
# With --login, the first logout ends the run, so a login left behind does
# not keep a root dev build on the VT: once a session on VT $vt has been open,
# no open one there (none, or only one closing) means its user logged out.
# vtium gone by then is no logout: the loop leaves, and its status decides.
# A poll that fails tells nothing, and the next one asks again; ten in a row
# get a warning, since the logout cannot end the run while they fail.
if [ -n "$login" ]; then
	seen=
	failed=0
	while running; do
		sleep 1 &
		wait $! || true
		if ! now=$(vt_sessions); then
			failed=$((failed + 1))
			[ "$failed" -ne 10 ] ||
				echo "run-vt: loginctl failed 10 polls in a row; until it answers, a logout does not end the run (Ctrl-C here does)" >&2
			continue
		fi
		failed=0
		if awk 'NF && $2 != "closing" { open = 1 } END { exit !open }' <<<"$now"; then
			seen=1
		elif [ -n "$seen" ]; then
			running || break
			echo "vt-login: session ended"
			exit 0
		fi
	done
fi
status=0
wait "$child" || status=$?
child=
# With --login, vtium ending on its own is never the expected end, status 0
# included: the logout and Ctrl-C, the expected ends, exit above and in the
# INT trap.
if [ "$status" -ne 0 ] || [ -n "$login" ]; then
	echo "run-vt: vtium exited with status $status" >&2
fi
[ -z "$login" ] || [ "$status" -ne 0 ] || status=1
exit "$status"
