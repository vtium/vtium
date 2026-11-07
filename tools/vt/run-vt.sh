#!/bin/bash
# Runs vtium on one VT in place of the VT's console unit (vtium-dev@, getty@,
# kmsconvt@ or autovt@ttyN), with the acceptance pass, a login prompt, a guide or the
# user's shell as its only session.
# When the session ends, this script is interrupted or vtium fails, the units
# that were running are started again, and only those. From the moment the
# units have stopped until they start again, the script holds the VT open, so
# that logind starts no console there when the VT is switched to. See
# docs/vt-review.md.
#
#   sudo tools/vt/run-vt.sh --bin=PATH [--helper=PATH] [--image=PATH] [--dry-run] [--label=TEXT] [--features=TEXT]
#        [--passes=DIR] [--debug=DIR] [--login |
#        --guide=FILE [--steps=LIST] | --shell [--transcript=FILE]] [N] [-- vtium-option...]
#
# --bin names the vtium binary, which runs as root: a regular file owned by
# root or the invoking user, writable by nobody else. N is the VT, 2-12,
# default 3. Options after -- go to vtium, for one machine's display and font
# settings (--mode=..., --font-size=...). Only display, font, palette, text and
# input options are accepted, each as one word: --name=value, or --name /
# --no-name for a switch. --label names the build on the acceptance pass's title
# page and in its pass record, and --features the features it was built with;
# --passes names the directory the acceptance pass keeps its pass records in
# (ADR-1501), which the session, as the user, creates when it is missing.
# --login runs vtium's login prompt (/bin/login, through PAM) in place of the
# acceptance pass, so the session is a logind session. The first logout ends the run; so
# do Ctrl-C, a vtium crash and vtium ending on its own, the last two as
# failures.
# --guide=FILE runs the guided checks of FILE (checks/guide.sh) in place of
# the acceptance pass, as the user, with no login; --steps="4 8 9" (or 4,8,9) runs only
# those steps. The answers go to /tmp/vtium-vt/vt-check.log.
# --shell runs the user's login shell, the one the password database names and
# /etc/shells lists, as the user, with no login, in the user's home directory,
# with the system's locale and, when they are the user's, the user's runtime
# directory and bus. Exiting it ends the run. --transcript=FILE records the session into FILE
# with script(1), which runs as the user; the user must be able to write FILE's
# directory, and FILE must not be a symlink.
# --debug=DIR runs vtium with --debug, so its log has its debug messages, and
# copies the kernel's DRM state, /sys/kernel/debug/dri/<N>/state of every card,
# into DIR: each time vtium logs that a display came or went, as
# NNN-came-<N>.state or NNN-went-<N>.state, each time a display's first flip
# after its modeset is done, as NNN-modeset-<N>.state, and as end-<N>.state once
# vtium has stopped. Each copy is written as the user, who must be able to write DIR; a
# line in vtium's log names it.
# --helper names the glyph helper vtium-config built with the binary, held
# to the binary's rules. The run copies it, owned by root and mode 0755, into
# /run/vtium-vt/, a directory only root can write, and runs vtium with
# --font-helper naming the copy, so a review run starts the helper built with
# it and never the packaged one or the development install's (ADR-1500 D4).
# --image names the image helper vtium-image built with a binary that has
# pictures, held to the same rules and copied beside it, and the run gives
# vtium --image-helper naming the copy, so the acceptance pass's helper pages
# draw their pictures (ADR-1501 D5).
# The run also compiles the checkout's terminfo entry, xterm-vtium, the name
# vtium gives TERM, into /run/vtium-vt/terminfo, and the acceptance pass, the guide and
# the shell get TERMINFO naming it, so they read the entry the package ships,
# as built from this checkout. The run does not pass that variable to
# /bin/login's session, so with --login the run uses the system's
# xterm-vtium entry when one is installed, as the packaged vtium's sessions
# do, and runs vtium with --term=kmscon when none is, saying so.
# The run copies the review's configuration, tools/vt/config/vtium.conf, into
# /run/vtium-vt/config/, owned by root and mode 0644, and runs vtium with
# --configdir naming that directory. vtium reads a vtium.conf only when root
# owns it and no one else can write it or a directory above it (ADR-1403
# D3), which a file in the checkout never is, so the run reads the copy and
# never the system's /etc/vtium/vtium.conf.
# /run/vtium-vt/ is removed when the run ends.
# --dry-run asks what it would ask (the VT on screen, the sessions, the active
# units) and prints every command that would change anything without running
# it; it needs no root.
#
# For the tests, VTIUM_VT_STANDINS names a directory holding stand-ins for
# systemctl, loginctl, setpriv, getent and systemd-analyze, "ttyN" in place
# of the VT's /dev/ttyN, a file "active" in place of
# /sys/class/tty/tty0/active, "shells" and "locale.conf" in place of
# /etc/shells and /etc/locale.conf, "run-user" in place of /run/user, and
# "debugfs-dri" in place of /sys/kernel/debug/dri, "run-vtium-vt" in place of
# /run/vtium-vt, and "terminfo" in place of /usr/share/terminfo.
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
helper=
image=
label=
features=
passes=
extra=()
login=
guide=
steps=
shell=
transcript=
debug=
while [ $# -gt 0 ]; do
	case $1 in
	--dry-run) dry=1 ;;
	--bin=*) bin=${1#*=} ;;
	--helper=*) helper=${1#*=}; [ -n "$helper" ] || die "--helper needs the glyph helper's path" ;;
	--image=*) image=${1#*=}; [ -n "$image" ] || die "--image needs the image helper's path" ;;
	--label=*) label=${1#*=} ;;
	--features=*) features=${1#*=} ;;
	--passes=*) passes=${1#*=} ;;
	--login) login=1 ;;
	--guide=*) guide=${1#*=}; [ -n "$guide" ] || die "--guide needs the guide's path: --guide=tools/vt/checks/NAME.guide" ;;
	--steps=*) steps=${1#*=} ;;
	--shell) shell=1 ;;
	--transcript=*) transcript=${1#*=}; [ -n "$transcript" ] || die "--transcript needs the file's path" ;;
	--debug=*) debug=${1#*=}; [ -n "$debug" ] || die "--debug needs the directory the DRM state goes in" ;;
	-h | --help) usage ;;
	--) shift; extra=("$@"); break ;;
	[0-9]*) [ -z "$vt" ] || usage; vt=$1 ;;
	*) usage ;;
	esac
	shift
done
vt=${vt:-3}

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=tools/vt/common.sh
. "$here/common.sh"
checkout=$(readlink -m "$here/../..")
standins=${VTIUM_VT_STANDINS:-}
if [ -n "$standins" ]; then
	[ "$(id -u)" -ne 0 ] || die "VTIUM_VT_STANDINS is for the tests, which run without root; unset it"
	for c in systemctl loginctl setpriv getent systemd-analyze; do
		[ -x "$standins/$c" ] || die "VTIUM_VT_STANDINS has no stand-in $c"
	done
	systemctl=$standins/systemctl
	loginctl=$standins/loginctl
	analyze=$standins/systemd-analyze
	tty_dev=$standins/tty$vt
	setpriv=$standins/setpriv
	getent=$standins/getent
	active_file=$standins/active
	shells_file=$standins/shells
	locale_file=$standins/locale.conf
	run_user=$standins/run-user
	dri=$standins/debugfs-dri
	stage=$standins/run-vtium-vt
	system_frits=$standins/vtium-fonts
	system_terminfo=$standins/terminfo
	# The tests run without root, so the staged files stay theirs.
	own=()
else
	systemctl=systemctl
	loginctl=loginctl
	analyze=systemd-analyze
	tty_dev=/dev/tty$vt
	# vtium starts its session with execve, without a PATH search.
	setpriv=$(command -v setpriv) || die "setpriv (util-linux) is missing; it runs the session as you"
	getent=getent
	active_file=/sys/class/tty/tty0/active
	shells_file=/etc/shells
	locale_file=/etc/locale.conf
	run_user=/run/user
	dri=/sys/kernel/debug/dri
	stage=/run/vtium-vt
	system_frits=/etc/vtium/fonts
	system_terminfo=/usr/share/terminfo
	own=(-o root -g root)
fi
# The units that may hold the VT and that the run borrows it from: vtium as
# make vt-install installs it, a getty, an installed kmscon's console, and
# autovt@, which logind starts on a switch to the VT and which names one of
# them or another console. A VT the packaged vtium@ holds is refused below.
units=("vtium-dev@tty$vt.service" "getty@tty$vt.service" "kmsconvt@tty$vt.service" "autovt@tty$vt.service")

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
# The allowlist is written for the command line #21 gives vtium; vtium
# parses no arguments yet. The checks below are made for
# VT $vt with the acceptance pass as the session, so only options that cannot change that
# are let through: an allowlist of exact names, since a getopt_long
# parser also takes a unique prefix (--lo for --login). An
# option with a value carries it in the same word: a separate word would be
# taken as the value, or, as a stray word, moved to the end of argv, into the
# login command that runs as root. Left out on purpose: the VT, seat and
# session options, --login, --configdir, --oneshot, the key bindings but key-paste
# (one reboots), and options naming files (asciicast, issue-path, xkb-keymap,
# xkb-compose-file). The on-screen display's osd, osd-timeout and focus-mark
# are choices that name no file, and display-buffers, 1, 2 or 3, how many
# buffers each display draws into (ADR-1401 D2).
valued=" font-engine font-name font-size mode multi-monitor gpus term sb-size
	palette palette-black palette-red palette-green palette-yellow palette-blue
	palette-magenta palette-cyan palette-light-grey palette-dark-grey
	palette-light-red palette-light-green palette-light-yellow palette-light-blue
	palette-light-magenta palette-light-cyan palette-white palette-foreground
	palette-background underline-color strike-color grapheme-width xkb-model
	xkb-layout xkb-variant xkb-options xkb-repeat-delay xkb-repeat-rate
	dpms-timeout render-timer image-storage-limit sync-timeout blink-text
	key-paste pointer-hide-timeout pointer-char osd osd-timeout focus-mark
	display-buffers "
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
	# vtium would refuse any other count, once the console units are stopped.
	if [[ $o == --display-buffers=* ]] && ! [[ ${o#*=} =~ ^[123]$ ]]; then
		die "--display-buffers takes 1, 2 or 3 here, not '${o#*=}'"
	fi
	# A name is lower-case letters and dashes, so no two names of a list,
	# joined by a space, pass as one.
	if [[ $o == --* && $name =~ ^[a-z][a-z-]*= ]]; then
		[[ $valued == *" ${name%%=*} "* ]] && continue
	elif [[ $o == --* && $name =~ ^[a-z][a-z-]*$ ]]; then
		[[ $switches == *" ${name#no-} "* ]] && continue
	fi
	die "vtium option '$o' is refused. Accepted: display, font, palette, text, input and on-screen display options, --render-timer, --image-storage-limit and --sync-timeout, one word each: --name=value (quoted as one word: --font-name=\"Monoid Nerd Font Mono\"), or --name / --no-name for a switch (see docs/vt-review.md)"
done
# The label reaches the session as one word of env's; it names a build, so it
# takes a build's characters only.
[[ $label =~ ^[[:alnum:]\ ._+:/@-]{0,80}$ ]] ||
	die "--label takes up to 80 letters, digits, spaces and ._+:/@-, not '$label'"
[[ $features =~ ^[a-z0-9\ ,_-]{0,80}$ ]] ||
	die "--features takes up to 80 lowercase letters, digits, spaces and ,_-, not '$features'"
# The path reaches the session as one word of its own (ACCEPT_DIR=...), never as shell code, so
# a space or any other printable character is safe in it. A control character, a newline above
# all, is refused because it would garble the lines the run prints.
[[ -z $passes || ( $passes == /* && $passes != *[[:cntrl:]]* && ${#passes} -le 4000 ) ]] ||
	die "--passes takes an absolute path without control characters, not '$passes'"
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
# Absolute, and not resolved: a symlink is refused below.
[ -z "$debug" ] || [[ $debug == /* ]] || debug=$PWD/$debug
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
# setpriv --reset-env drops the locale, so every session, the acceptance
# pass, the guide and the shell, gets the system's: LANG and LC_* from
# locale.conf, each a value of locale-name characters. Without it, bash's
# line editing at the pages' prompts takes a byte for a character.
locale=()
if [ -f "$locale_file" ]; then
	assignment='^(LANG|LC_[A-Z]+)="?([[:alnum:]_.@-]+)"?$'
	while IFS= read -r line || [ -n "$line" ]; do
		[[ ! $line =~ $assignment ]] || locale+=("${BASH_REMATCH[1]}=${BASH_REMATCH[2]}")
	done <"$locale_file"
fi
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

# --inh-caps=-all empties the one capability set the uid change leaves. The
# bounding set and no-new-privs are left alone: sudo works in the shell.
drop=("$setpriv" --reuid="$uid" --regid="$gid" --init-groups --inh-caps=-all --reset-env)

# vtium, the glyph helper, the image helper, the frits, the review's
# vtium.conf and the terminfo source that root runs, copies or compiles are
# first read as the user, so the user's own permissions decide what is read:
# a symlink to a file the user may not read, or a second hard link to one,
# fails to open, and no check races a change, because root checks only what
# it holds. The other files of the checkout, such as this script, are not
# read this way. read_dir is a
# directory of root's alone, removed when the run ends. Run without root
# (a dry run, the tests), the reader is the caller itself.
reader=()
read_base=${standins:-/run}
if [ "$(id -u)" -ne 0 ] && [ -z "$standins" ]; then
	read_base=${TMPDIR:-/tmp}
else
	reader=("${drop[@]}" --)
fi
read_dir=$(mktemp -d -p "$read_base" run-vt-read.XXXXXX) || die "cannot create a directory to read the files in"
trap 'rm -rf -- "$read_dir"' EXIT
read_n=0
# fetch FILE reads FILE as the user into a file of root's, $fetched, and sets
# fkind to symlink or file, with fowner, fmode, flinks and ftype from the open
# file. The user's shell reports whether the path is a symlink (the policy is
# that the run uses only a file of its own, so the test is made before the open
# it governs), then opens FILE once, reports the open file's owner, mode, link
# count and type, and copies that open file, so the checks and the copy are of
# one file. fetch dies on a report that is not what the shell writes, because
# root reads the report as numbers; it fails when the user cannot read FILE.
fetch_script='if [ -L "$1" ]; then echo symlink >&3; exit 0; fi
exec 5<"$1" || exit 1
/usr/bin/stat -L -c "file %u %a %h %F" /dev/fd/5 >&3 || exit 1
exec /bin/cat <&5 >&4'
fetched=
fetch() {
	read_n=$((read_n + 1))
	fetched=$read_dir/$read_n
	"${reader[@]}" /bin/sh -c "$fetch_script" sh "$1" 3>"$fetched.meta" 4>"$fetched" || return 1
	fkind='' fowner='' fmode='' flinks='' ftype=''
	read -r fkind fowner fmode flinks ftype <"$fetched.meta"
	[ -n "$fkind" ] || return 1
	[[ $fkind == symlink || ( $fkind == file && $fowner =~ ^[0-9]+$ && $fmode =~ ^[0-7]{3,4}$ && $flinks =~ ^[0-9]+$ ) ]] ||
		die "the report on $1 is not an owner, mode and link count"
}
# copy MODE SOURCE NAME DEST installs SOURCE, the copy root holds, at DEST, and
# shows NAME, the path the file was found at, as its source.
copy() {
	show install -m "$1" "${own[@]}" -- "$3" "$4"
	[ -n "$dry" ] || install -m "$1" "${own[@]}" -- "$2" "$4"
}
# check_file NAME WHY: the type, owner and mode checks, on what was read.
check_file() {
	[[ $ftype == regular* ]] || die "$1 is not a regular file; $2"
	[ "$fowner" -eq 0 ] || [ "$fowner" -eq "$uid" ] ||
		die "$1 belongs to uid $fowner, neither root nor $user; $2"
	(( (8#$fmode & 8#022) == 0 )) ||
		die "$1 is writable by group or others (mode $fmode); $2"
}

# The binary runs as root, so only its owner or root may have written it.
[ -n "$bin" ] || die "--bin=PATH names the vtium binary (make vt-accept builds target/x86_64-unknown-linux-gnu/minspec/vtium)"
bin=$(readlink -m -- "$bin")
[ -f "$bin" ] && [ -x "$bin" ] || die "no vtium binary at $bin: build it first (make minspec)"
fetch "$bin" || die "$user cannot read the vtium binary $bin"
[ "$fkind" = file ] || die "$bin is a symlink; the run uses only the file it checked"
check_file "$bin" "it would run as root"
bin_src=$fetched
# The helper is held to the same rules: vtium, as root, starts it as nobody,
# from the copy this run makes.
if [ -n "$helper" ]; then
	helper=$(readlink -m -- "$helper")
	[ -f "$helper" ] && [ -x "$helper" ] || die "no glyph helper at $helper: build it first (make config-release)"
	fetch "$helper" || die "$user cannot read the glyph helper $helper"
	[ "$fkind" = file ] || die "$helper is a symlink; the run copies only the file it checked"
	check_file "$helper" "vtium, as root, would start it"
	helper_src=$fetched
fi
# So is the image helper, which vtium, as root, starts as the session's user.
if [ -n "$image" ]; then
	image=$(readlink -m -- "$image")
	[ -f "$image" ] && [ -x "$image" ] || die "no image helper at $image: build it first (make image-release)"
	fetch "$image" || die "$user cannot read the image helper $image"
	[ "$fkind" = file ] || die "$image is a symlink; the run copies only the file it checked"
	check_file "$image" "vtium, as root, would start it"
	image_src=$fetched
fi
# The acceptance pass switches the console among its own frits with OSC 50, which
# vtium takes only for a frit in its frits directory (ADR-1200 D5,
# ADR-1501 D6). The run stages the system's frits and the acceptance pass's into a
# directory of root's alone and names it, so the review sees both. The copy
# runs as root and is readable by everyone, so a frit must be a regular file
# of root's or the user's, never a symlink or a file with another hard link,
# which could name a file the user may not read, such as /etc/shadow. OSC 50
# names a frit by its file name, so a system frit with the name of one of the
# acceptance pass's is refused: the walk would judge whichever was copied last.
frit_files=()
frit_srcs=()
declare -A frit_from=()
for f in "$system_frits"/*.vtfrit "$here"/sections/frits/*.vtfrit; do
	[ -e "$f" ] || [ -L "$f" ] || continue
	[ ! -L "$f" ] || die "$f is a symlink; the run copies only a frit that is a file of its own"
	[ -f "$f" ] || die "$f is not a regular file; the run copies only a frit that is a file of its own"
	fetch "$f" || die "$user cannot read the frit $f"
	[ "$fkind" != symlink ] || die "$f is a symlink; the run copies only a frit that is a file of its own"
	[ "$fkind" = file ] && [[ $ftype == regular* ]] ||
		die "$f is not a regular file; the run copies only a frit that is a file of its own"
	[ "$fowner" -eq 0 ] || [ "$fowner" -eq "$uid" ] ||
		die "$f belongs to uid $fowner, neither root nor $user; the run copies it as root"
	[ "$flinks" -eq 1 ] || die "$f has $flinks hard links; the run copies only a frit that is a file of its own"
	(( (8#$fmode & 8#022) == 0 )) ||
		die "$f is writable by group or others (mode $fmode); vtium, as root, would read it"
	[ -z "${frit_from[${f##*/}]:-}" ] ||
		die "$f and ${frit_from[${f##*/}]} are both named ${f##*/}; OSC 50 names a frit by its file name, so rename the system's"
	frit_from[${f##*/}]=$f
	frit_files+=("$f")
	frit_srcs+=("$fetched")
done
# The review's vtium.conf, which root copies into a directory of root's for
# vtium to read, is held to the frits' rules: a regular file of root's or the
# user's with one link, writable by no one else.
config_src=$here/config/vtium.conf
[ ! -L "$config_src" ] || die "$config_src is a symlink; the run copies only the file it checked"
[ -f "$config_src" ] || die "no $config_src, the configuration the review's vtium reads"
fetch "$config_src" || die "$user cannot read $config_src"
[ "$fkind" = file ] || die "$config_src is a symlink; the run copies only the file it checked"
check_file "$config_src" "vtium, as root, would read it"
[ "$flinks" -eq 1 ] || die "$config_src has $flinks hard links; the run copies only a file of its own"
config_copy=$fetched
# vtium names itself TERM=xterm-vtium; the run compiles that entry from the
# checkout, as the package does, for the sessions it starts itself.
terminfo_src=$checkout/crates/vtium/install/xterm-vtium.ti
[ -f "$terminfo_src" ] || die "no $terminfo_src, the terminfo entry the sessions read"
fetch "$terminfo_src" || die "$user cannot read $terminfo_src"
[ "$fkind" = file ] || die "$terminfo_src is a symlink; tic compiles only the file it checked"
terminfo_copy=$fetched
# The run does not pass TERMINFO to /bin/login's session, so a login session
# reads the system's entries: the packaged one when it is installed, and
# kmscon's, which the machine keeps, when it is not.
term_opt=()
[ -z "$login" ] || [ -f "$system_terminfo/x/xterm-vtium" ] || term_opt=(--term=kmscon)

if [ -n "$guide" ] && [ -z "$dry" ]; then
	"${drop[@]}" -- /usr/bin/test -f "$guide" -a -r "$guide" ||
		die "$user cannot read the guide $guide"
fi
if [ -n "$transcript" ] && [ -z "$dry" ]; then
	"${drop[@]}" -- /usr/bin/test -d "${transcript%/*}/" -a -w "${transcript%/*}/" -a ! -h "$transcript" ||
		die "$user cannot write the transcript $transcript"
fi
if [ -n "$debug" ] && [ -z "$dry" ]; then
	"${drop[@]}" -- /usr/bin/test -d "$debug/" -a -w "$debug/" -a ! -h "$debug" ||
		die "$user cannot write the directory $debug, which --debug names for the DRM state"
fi

# capture NAME copies each card's DRM state to $debug/NAME-<card>.state, as the
# user, and names the copies in vtium's log. A card is a directory of $dri with
# a state file; the same card under a second name (a symlink, or its bus id)
# is copied once, under its number where it has one. It never fails: a copy
# that cannot be made is named instead. The copy runs as the user, so it never
# gets the run's hold on the VT, fd 7, which is root's and read-write.
capture() {
	local name=$1 d e real seen=" " numbered=() named=() copied=()
	for d in "$dri"/*/; do
		[ -d "$d" ] || continue
		e=${d%/}
		e=${e##*/}
		if [[ $e =~ ^[0-9]+$ ]]; then numbered+=("$e"); else named+=("$e"); fi
	done
	for e in "${numbered[@]}" "${named[@]}"; do
		[ -f "$dri/$e/state" ] || continue
		real=$(readlink -f -- "$dri/$e") || continue
		[[ $seen != *" $real "* ]] || continue
		seen+="$real "
		if "${drop[@]}" -- /bin/sh -c 'umask 077 && set -C && cat >"$1"' sh "$debug/$name-$e.state" \
			<"$dri/$e/state" 2>/dev/null 7>&-; then
			copied+=("$name-$e.state")
		else
			echo "run-vt: cannot copy $dri/$e/state to $debug/$name-$e.state" || true
		fi
	done
	if [ ${#copied[@]} -gt 0 ]; then
		echo "run-vt: the DRM state, in $debug: ${copied[*]}" || true
	else
		echo "run-vt: no card's DRM state to copy under $dri" || true
	fi
}

# Copies vtium's log, line by line, to this script's output, and the DRM state
# each time a line says a display came or went, or that the card scans a
# display's new mode out, which vtium logs at debug. It runs until vtium and
# everything holding its output end, and ignores the signals that end a run,
# so the lines before them all arrive; a closed output costs it nothing.
watch_log() {
	trap '' INT TERM HUP PIPE
	local line n=0 what
	while IFS= read -r line || [ -n "$line" ]; do
		printf '%s\n' "$line" 2>/dev/null || true
		what=
		[[ ! $line =~ ^display\ [^\ ]+(,\ left\ dark,)?\ is\ gone$ ]] || what=went
		[[ ! $line =~ ^display\ [^\ ]+\ at\ [0-9]+x[0-9]+$ || -n $what ]] || what=came
		[[ ! $line =~ ^a\ display(\ of\ head\ [^\ ]+)?\ came: || -n $what ]] || what=came
		[[ ! $line =~ ^display\ [^\ ]+:\ the\ first\ flip\ after\ its\ modeset\ is\ done$ || -n $what ]] || what=modeset
		[ -n "$what" ] || continue
		n=$((n + 1))
		capture "$(printf '%03d-%s' "$n" "$what")"
	done
}

# One run at a time, whoever starts it: one build on a VT. The lock is held
# until this script ends; it lives in /run, which only root can write, and is
# opened without truncation. vtium and its session do not inherit it.
if [ -z "$dry" ]; then
	lock=/run/vtium-vt.lock
	[ -z "$standins" ] || lock=$standins/vt.lock
	exec 9<>"$lock"
	flock -n 9 || die "another VT review run is in progress (it holds $lock); one build on a VT at a time"
fi

# Refuse the VT on screen (the one this command was typed on) and any VT a
# logind session sits on, the graphical one included: stopping the unit there
# would end that session. When the sessions cannot be listed, refuse too. A
# dry run asks the same, since asking changes nothing.
fg=$(cat "$active_file") || die "cannot read $active_file, so the VT on screen is unknown"
fg=${fg#tty}
[[ $fg =~ ^[0-9]+$ ]] || die "$active_file reads '$fg', so the VT on screen is unknown"
[ "$fg" != "$vt" ] || die "VT $vt is on screen now; run this from another VT"
on_vt=$(vt_sessions "$loginctl" "$vt") ||
	die "loginctl could not list the sessions or read one of them, so whether a session is on VT $vt is unknown"
if [ -n "$on_vt" ]; then
	s=${on_vt%% *}
	die "logind session $s ($("$loginctl" show-session -p Name -p Type --value "$s" | paste -sd' ')) is on VT $vt; log out there, or pick another VT"
fi

# The packaged vtium's VT and logind's reserved VT are never borrowed
# (vt_borrowable in common.sh).
held=$(vt_borrowable "$systemctl" "$analyze" "$vt")
[ -z "$held" ] || die "VT $vt refused: $held; or pick another VT"

# The VT's units that run now: each is stopped below and started again on
# exit, and no other. autovt@ttyN is another name for the unit logind
# started there, vtium-dev@ttyN on the development VT, so that unit is stopped
# and started once, under its own name.
active=()
for u in "${units[@]}"; do
	"$systemctl" is-active --quiet "$u" || continue
	if [[ $u == autovt@* ]] && id=$("$systemctl" show -p Id --value "$u") && [ -n "$id" ]; then
		u=$id
	fi
	[[ " ${active[*]} " == *" $u "* ]] || active+=("$u")
done

child=
watcher=
fifo_dir=
stage_made=
holding=
# Whether vtium still runs, asked of this shell's job table rather than of its
# PID: once bash has reaped it, the PID may be another process's. grep reads
# all of jobs' output, so pipefail sees no SIGPIPE.
running() {
	[ -n "$child" ] && jobs -rp | grep -x -- "$child" >/dev/null
}
# Whether watch_log still runs, asked the same way.
watching() {
	[ -n "$watcher" ] && jobs -rp | grep -x -- "$watcher" >/dev/null
}
# shellcheck disable=SC2317,SC2329 # invoked by the EXIT trap
# A second signal, or a closed terminal or pipe, must not cut it short before
# the units are started again, so it ignores them while it runs, and no failed
# write ends it. A failed start exits 1.
restore() {
	set +e
	trap '' INT TERM HUP PIPE
	trap - EXIT
	rm -rf -- "$read_dir"
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
	# Under --debug, vtium's last lines reach the log before the state at the
	# end is copied, and the console unit starts after it. Something vtium
	# started may still hold its output open: after 5 s watch_log is stopped,
	# so no root process outlives the run and the output closes.
	if [ -n "$watcher" ]; then
		exec 8>&-
		for _ in $(seq 50); do
			watching || break
			sleep 0.1
		done
		if watching; then
			echo "run-vt: something vtium started still holds its log open; its log ends here, and the DRM state at the end is copied now"
			kill -KILL "$watcher" 2>/dev/null || true
		fi
		wait "$watcher" 2>/dev/null || true
		capture end
	fi
	[ -z "$fifo_dir" ] || rm -rf -- "$fifo_dir"
	[ -z "$stage_made" ] || [ -n "$dry" ] || rm -rf -- "$stage"
	# The VT is let go before its units start: a console unit's start hangs
	# the VT up and may hand it back to the kernel (TTYVHangup=,
	# TTYVTDisallocate=), which an open VT would refuse.
	[ -z "$holding" ] || exec 7>&-
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
# logind starts autovt@ttyN when the VT is switched to and nothing holds it
# open (the kernel's VT_GETSTATE), and on the development VT that is
# vtium-dev@ttyN through its own link. vtium holds the VT while it runs; this
# script holds it too, from the moment its units have stopped until they start
# again, so a switch before vtium opens the VT or after it ends starts nothing
# there. Opening a terminal makes it the controlling one only for a session
# leader that has none, which this script, started by sudo from a terminal,
# never is. vtium and the log's watcher do not inherit the hold.
if [ -n "$dry" ]; then
	echo "  (run-vt.sh holds $tty_dev open from here until the units start again, so logind starts no console there)"
else
	# Marked before it opens, so a signal in between still lets go of it
	# before the units start; letting go of a hold that never opened does
	# nothing.
	holding=1
	exec 7<>"$tty_dev" || die "cannot open $tty_dev to hold VT $vt"
fi

label=${label:-unlabelled}
ends="the acceptance pass ends" last=3
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
       ACCEPT_LABEL='vtium $label' tools/vt/accept.sh
           the acceptance pass; its pass is kept where make vt-accept keeps them
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
	what="answer each page with y, n, u or s (c adds a comment, b goes back);
     q quits the acceptance pass, and the next run resumes the pass"
fi
users_shell="$user's shell"
term_note=
[ ${#term_opt[@]} -eq 0 ] ||
	term_note="  The system has no xterm-vtium terminfo entry, which /bin/login's session
  would read, so vtium runs with --term=kmscon; the packaged vtium-terminfo
  holds the entry.
"
cat <<EOF

vtium $label on VT $vt${login:+, with a login prompt}${guide:+, with guided checks}${shell:+, with $users_shell}
  1. switch with Ctrl-Alt-F$vt
  2. $what
  $last. $after;
     switch back to your desktop with Ctrl-Alt-F$fg
$manual
$term_note
EOF

# The acceptance pass, the guide and the shell run as $user, never as root, with the
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
# script decides how it stops: SIGTERM from restore(). --configdir names the
# stage's copy of config/vtium.conf, the review's own configuration, so the
# system's is never read.
terminfo=(TERMINFO="$stage/terminfo")
# The acceptance pass records the build it judges: the VT, the binary and its
# features, beside the label (ADR-1501 D5).
accept=(ACCEPT_LABEL="vtium $label" ACCEPT_VT="$vt" ACCEPT_BINARY="$bin" ACCEPT_FEATURES="${features:-none}")
[ -z "$passes" ] || accept+=(ACCEPT_DIR="$passes")
# The pages that blink text name the style vtium draws it in (blink.inc): from
# the config this run gives vtium, the copy --configdir names, and from the
# blink options given to vtium, the last of each winning as it does in vtium.
accept+=(ACCEPT_VTIUM_CONF="$stage/config/vtium.conf")
for o in "${extra[@]}"; do
	case $o in
	--blink-text=*) accept+=(ACCEPT_BLINK_TEXT="${o#*=}") ;;
	--blink) accept+=(ACCEPT_BLINK=on) ;;
	--no-blink) accept+=(ACCEPT_BLINK=off) ;;
	esac
done
session=(/usr/bin/env "${locale[@]}" "${terminfo[@]}" "${accept[@]}" /bin/bash "$here/accept.sh")
if [ -n "$guide" ]; then
	session=(/usr/bin/env "${locale[@]}" "${terminfo[@]}" GUIDE_VT="$vt" GUIDE_DESKTOP="$fg" ${steps:+GUIDE_STEPS="$steps"}
		/bin/bash "$here/checks/guide.sh" "$guide")
elif [ -n "$shell" ]; then
	session=(/usr/bin/env -C "$home" "${locale[@]}" "${runtime[@]}" "${terminfo[@]}" "$login_shell" -l)
	[ -z "$transcript" ] ||
		session=(/usr/bin/env -C "$home" "${locale[@]}" "${runtime[@]}" "${terminfo[@]}"
			"$script" -q -f -c "exec $login_shell -l" "$transcript")
fi
# setpriv's --reset-env clears the environment vtium gave the session, so
# the head and console it names (ADR-1401 D5) are carried across as words:
# vtium replaces a word that is $VTIUM_HEAD or $VTIUM_CONSOLE with its value,
# and the shell, already the user's, puts each back in the environment. The
# values are arguments, never code, and a head name holds only A-Z a-z 0-9
# . _ + and -.
# shellcheck disable=SC2016 # expanded by the shell after setpriv
carry=(/bin/sh -c 'h=$1 c=$2; shift 2; exec /usr/bin/env VTIUM_HEAD="$h" VTIUM_CONSOLE="$c" "$@"'
	sh '$VTIUM_HEAD' '$VTIUM_CONSOLE')
once=(--oneshot)
helper_opt=()
[ -z "$helper" ] || helper_opt=(--font-helper="$stage/vtium-config")
[ -z "$image" ] || helper_opt+=(--image-helper="$stage/vtium-image")
helper_opt+=(--font-frits="$stage/frits")
debug_opt=()
[ -z "$debug" ] || debug_opt=(--debug)
login_argv=("${drop[@]}" -- "${carry[@]}" "${session[@]}")
if [ -n "$login" ]; then
	once=()
	login_argv=(/bin/login -p)
fi
cmd=("$stage/vtium" --vt="tty$vt" --no-switchvt --configdir "$stage/config" "${once[@]}" "${helper_opt[@]}"
	"${term_opt[@]}" "${debug_opt[@]}" "${extra[@]}" --login -- "${login_argv[@]}")
# The helper's copy, the terminfo entry, the frits and the configuration, in
# a directory of root's alone, made fresh for this run and removed when it
# ends. tic makes the entry's directory and file under the umask, and the
# caller's may be 077, which would leave the entry unreadable to the session
# that runs as the user, so the staging runs under 022.
stage_made=1
(
	umask 022
	act rm -rf -- "$stage"
	act install -d -m 0755 "${own[@]}" -- "$stage" "$stage/terminfo" "$stage/frits" "$stage/config"
	copy 0644 "$config_copy" "$config_src" "$stage/config/vtium.conf"
	for i in "${!frit_files[@]}"; do
		copy 0644 "${frit_srcs[i]}" "${frit_files[i]}" "$stage/frits/${frit_files[i]##*/}"
	done
	copy 0755 "$bin_src" "$bin" "$stage/vtium"
	[ -z "$helper" ] || copy 0755 "$helper_src" "$helper" "$stage/vtium-config"
	[ -z "$image" ] || copy 0755 "$image_src" "$image" "$stage/vtium-image"
	show tic -x -e xterm-vtium -o "$stage/terminfo" "$terminfo_src"
	[ -n "$dry" ] || tic -x -e xterm-vtium -o "$stage/terminfo" "$terminfo_copy"
)
rm -rf -- "$read_dir"
show "${cmd[@]}"
[ -z "$dry" ] || exit 0
# Under --debug, vtium's output goes through watch_log, which closes the lock
# too. It reads a FIFO in a directory of root's alone in /run, whatever TMPDIR
# the caller kept, gone once both ends are open, and runs as a job, so
# restore() asks the job table whether it runs.
if [ -n "$debug" ]; then
	fifo_base=/run
	[ -z "$standins" ] || fifo_base=$standins
	fifo_dir=$(mktemp -d -p "$fifo_base") || die "cannot create a directory for vtium's log"
	mkfifo -m 0600 -- "$fifo_dir/log" || die "cannot create a FIFO for vtium's log"
	{ exec 7>&- 9>&-; watch_log; } <"$fifo_dir/log" &
	watcher=$!
	exec 8>"$fifo_dir/log"
	rm -rf -- "$fifo_dir"
	fifo_dir=
fi
# A signal in the moment between starting vtium and setting child finds child
# still empty, so restore() starts the units again without stopping vtium. The
# window is a few microseconds and is left open.
if [ -n "$watcher" ]; then
	"${cmd[@]}" >&8 2>&1 7>&- 8>&- 9>&- &
else
	"${cmd[@]}" 7>&- 9>&- &
fi
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
		if ! now=$(vt_sessions "$loginctl" "$vt"); then
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
