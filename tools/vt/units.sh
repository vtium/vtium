#!/bin/bash
# Generates vtium's VT unit and its getty fallback from the shipped units,
# crates/vtium/install/vtium@.service and vtium-getty@.service, under a
# name of the caller's. It is the one place the units are made: make install,
# which the package build runs, and make install-tree, the tree tier 3
# installs, make vtium@.service and vtium-getty@.service, and make vt-install
# makes vtium-dev@.service and vtium-dev-getty@.service (docs/vt-review.md,
# "Installing vtium on a VT").
#
#   units.sh --name NAME --exec PATH --by WHO [--conflicts UNIT]... [--note LINE]
#            [--seat] [--source DIR] OUTDIR
#
# It writes OUTDIR/NAME@.service, which is vtium@.service with these
# changes (ADR-1500 D3, D4): for a NAME other than vtium, its name and
# OnFailure= naming NAME-getty@.service; ExecStart= running PATH in place of
# /usr/bin/vtium; for each --conflicts
# UNIT, a Conflicts=UNIT line and an After=UNIT line just above OnFailure=,
# below the shipped unit's own Conflicts= and After= for the getty, so that
# starting the unit on a VT stops UNIT there first, as it stops the getty;
# and DefaultInstance=name-a-VT/NAME@ttyN, which is no VT. UNIT is an
# instance unit on the same VT, such as kmsconvt@%i.service.
#
# The DefaultInstance= is there so that systemctl enable NAME@.service with
# no VT named fails before it links anything, on every machine. systemctl
# ignores DefaultInstance= when it enables an instance, so enable NAME@ttyN
# works as before; for the template alone it refuses a value that names no
# instance, and stops before Alias=autovt@.service is linked. With no
# DefaultInstance= at all, systemctl links autovt@.service -> NAME@.service
# first and only then refuses the template, and logind then starts the unit
# on every VT it spawns; with the shipped DefaultInstance=tty1 it enables VT
# 1 (measured with systemd 262; systemd 254's install.c does the same). The
# cost: systemctl list-unit-files shows the template as "bad", and
# systemctl is-enabled NAME@.service fails, while each NAME@ttyN reads as
# enabled or disabled as usual. It writes
# OUTDIR/NAME-getty@.service, which is vtium-getty@.service under NAME, its
# ExecStart= running PATH console fallback in place of /usr/bin/vtium. Each
# unit's opening comment, which describes the shipped source, gives way to
# one that describes the unit under its own name. Every other line is
# the shipped unit's, so Restart= and Alias=autovt@.service come through as
# the shipped unit has them. Each unit opens with a comment that names WHO made
# it, the commit it was made from and each change, and then LINE when --note
# gives one. With --seat it also writes OUTDIR/NAME-seat@.service, the unit
# of seat mode (ADR-1400 D3, ADR-1500 D3): NAME@.service as generated above,
# under its own name and opening comment, with Conflicts= and After=
# NAME@%i.service, so a VT runs one mode at a time, and with the [Service]
# lines of vtium-libseat.conf, User=root, PAMName=vtium and KillMode=mixed,
# in place of KillMode=process. Its OnFailure= names NAME-getty@, the
# fallback both modes share. --source reads the shipped units from DIR in
# place of crates/vtium/install, for the tests. It fails, and writes nothing,
# when the shipped unit no longer has the lines it changes.
set -euo pipefail

usage() {
	sed -n '10,11s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'units: %s\n' "$*" >&2
	exit 1
}

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
checkout=$(readlink -m "$here/../..")
name=
exec_path=
by=
note=
conflicts=()
seat=
src=$checkout/crates/vtium/install
out=
while [ $# -gt 0 ]; do
	case $1 in
	--name) [ $# -ge 2 ] || usage; name=$2; shift ;;
	--exec) [ $# -ge 2 ] || usage; exec_path=$2; shift ;;
	--by) [ $# -ge 2 ] || usage; by=$2; shift ;;
	--note) [ $# -ge 2 ] || usage; note=$2; shift ;;
	--conflicts) [ $# -ge 2 ] || usage; conflicts+=("$2"); shift ;;
	--seat) seat=1 ;;
	--source) [ $# -ge 2 ] || usage; src=$2; shift ;;
	-h | --help) usage ;;
	-*) usage ;;
	*) [ -z "$out" ] || usage; out=$1 ;;
	esac
	shift
done
[ -n "$name" ] && [ -n "$exec_path" ] && [ -n "$by" ] && [ -n "$out" ] || usage
# The name and the path go into the units and into sed's replacement, so each
# holds only characters that mean nothing to either.
[[ $name =~ ^[a-z][a-z0-9-]*$ ]] || die "--name '$name' is not a unit name of lowercase letters, digits and dashes"
[[ $exec_path =~ ^/[A-Za-z0-9/._-]+$ ]] || die "--exec '$exec_path' is not an absolute path of letters, digits and /._-"
for c in "${conflicts[@]}"; do
	[[ $c =~ ^[a-z][a-z0-9-]*@%i\.service$ ]] || die "--conflicts '$c' is not an instance unit on the same VT, NAME@%i.service"
	[ "$c" != "$name@%i.service" ] || die "--conflicts '$c' names the unit itself"
done
sources=(vtium@.service vtium-getty@.service)
[ -z "$seat" ] || sources+=(vtium-libseat.conf)
for f in "${sources[@]}"; do
	[ -f "$src/$f" ] || die "no $src/$f, which the units are generated from"
done

built=$(git -C "$checkout" describe --always --dirty 2>/dev/null) || built=unknown
# header SOURCE CHANGES...: the generated unit's opening comment, a line for
# each change, then the note.
header() {
	printf '# Generated by %s from %s at %s. Changed from it:\n' "$by" "$1" "$built"
	shift
	printf '#   %s\n' "$@" "its opening comment, which says what the unit does under its own name"
	[ -z "$note" ] || printf '# %s\n' "$note"
}
# body FILE: the shipped unit from its first section on, without the opening
# comment that describes the shipped source.
body() { awk 'started || /^\[/ { started = 1; print }' "$1"; }

# unit_body NAME CONFLICTS SERVICE: the shipped vtium@.service from its first
# section on, as the unit NAME@.service: ExecStart= running PATH, OnFailure=
# naming the getty fallback of --name, a Conflicts= and an After= line for
# each unit in CONFLICTS just above OnFailure=, the lines SERVICE in place of
# KillMode= when SERVICE is not empty, and the refused DefaultInstance= in
# place of the shipped one, at the end of [Install], the unit's last
# section. The units and the lines reach awk through the environment.
unit_body() {
	sed -e "s|^ExecStart=/usr/bin/vtium |ExecStart=$exec_path |" \
		-e "s|^OnFailure=vtium-getty@%i\\.service\$|OnFailure=$name-getty@%i.service|" \
		-e '/^DefaultInstance=/d' <(body "$src/vtium@.service") |
		UNITS_CONFLICTS="$2" UNITS_NAME="$1" UNITS_SERVICE="$3" awk '
			/^OnFailure=/ {
				n = split(ENVIRON["UNITS_CONFLICTS"], c, " ")
				for (i = 1; i <= n; i++) print "Conflicts=" c[i] "\nAfter=" c[i]
			}
			/^KillMode=/ && ENVIRON["UNITS_SERVICE"] != "" { print ENVIRON["UNITS_SERVICE"]; next }
			{ print }
			END {
				print "# No VT: systemctl refuses it, so enabling " ENVIRON["UNITS_NAME"] "@.service with no VT"
				print "# named fails and links nothing. Name the VT: " ENVIRON["UNITS_NAME"] "@tty3.service."
				print "DefaultInstance=name-a-VT/" ENVIRON["UNITS_NAME"] "@ttyN"
			}'
}

tmp=$(mktemp -d) || die "cannot create a scratch directory"
trap 'rm -rf -- "$tmp"' EXIT
unit=$name@.service
fallback=$name-getty@.service
# The name, and the fallback the unit names, change only for a name other
# than the shipped one.
renamed=()
renamed_fallback=()
if [ "$name" != vtium ]; then
	renamed=("the name, $name@" "the getty fallback, $name-getty@")
	renamed_fallback=("the name, $name-getty@, which $unit names as its fallback")
fi
changes=()
for c in "${conflicts[@]}"; do
	changes+=("Conflicts= and After= $c")
done
{
	header crates/vtium/install/vtium@.service "${renamed[@]}" \
		"ExecStart runs $exec_path" "${changes[@]}" "DefaultInstance=name-a-VT/$name@ttyN, which is no VT"
	cat <<EOF
# vtium on one VT of seat0, in place of its getty. The instance is the VT's
# name, as getty@.service's is.
EOF
	# The packaged vtium's unit is given its VTs by vtium console, which
	# manages vtium@ alone and which the package's scripts run (ADR-1500 D3,
	# D7); the development install's is enabled by make vt-install.
	if [ "$name" = vtium ]; then
		cat <<EOF
# This is the unit of direct mode, in which vtium opens the screen and the
# keyboards itself (ADR-1400 D3). The package's install script gives vtium
# every free VT in direct mode with
#   vtium console enable --direct --all
# which enables vtium at boot on VT 1 when no display manager is enabled,
# and links autovt@.service here, so logind starts vtium on each other free
# VT. Its removal script gives them back with
#   vtium console disable --all
# To give vtium one VT alone, VT 3 here, and the way back:
#   vtium console disable --all
#   vtium console enable --direct 3
#   vtium console disable 3
# vtium console enable with no mode says which mode fits this machine.
EOF
	else
		cat <<EOF
# make vt-install enables it on one VT, naming the VT:
#   systemctl enable $name@tty3.service
# That also links autovt@tty3.service here, so logind starts vtium on that
# VT.
EOF
	fi
	cat <<EOF
# Enabling $name@.service with no VT named fails and links nothing, because
# its DefaultInstance= names no VT.
# It conflicts with getty@ on the same VT. When vtium fails, its OnFailure=
# starts $name-getty@, which starts getty@ on the VT unless the head file's
# policy is closed (ADR-1401 D9), and a Restart= line, where the unit has
# one, starts vtium again first. Without VTs it starts nothing. The unit sets
# no sandboxing directive: the login session vtium starts would inherit it
# (ADR-1400, D3).

EOF
	unit_body "$name" "${conflicts[*]}" ""
} >"$tmp/$unit"
{
	header crates/vtium/install/vtium-getty@.service "${renamed_fallback[@]}" \
		"ExecStart runs $exec_path console fallback"
	cat <<EOF
# The getty fallback of $unit: when vtium on a VT fails, its
# OnFailure= starts this unit, which starts getty@ on that VT unless the
# getty's start would stop another program, such as a display manager whose
# Conflicts= names it (ADR-1500 D7). Under a closed head-file policy a getty's
# login would take any keyboard (ADR-1401 D9), so vtium writes
# /run/vtium/ttyN.closed at start, and this unit's condition skips the getty
# while that file exists. vtium removes the file when it next starts under an
# open policy, and /run is cleared at every boot.

EOF
	sed -e "s|^ExecStart=/usr/bin/vtium |ExecStart=$exec_path |" <(body "$src/vtium-getty@.service")
} >"$tmp/$fallback"

seat_unit=$name-seat@.service
if [ -n "$seat" ]; then
	# The [Service] lines of vtium-libseat.conf, which make vtium's own PAM
	# session, below a comment that says why KillMode= is not process.
	seat_lines=$(awk '/^\[/ { s = ($0 == "[Service]"); next } s && !/^[[:space:]]*(#|$)/' "$src/vtium-libseat.conf")
	[ "$(printf '%s\n' "$seat_lines" | sort)" = "$(printf '%s\n' KillMode=mixed PAMName=vtium User=root)" ] ||
		die "the shipped vtium-libseat.conf's [Service] lines are not User=root, PAMName=vtium and KillMode=mixed"
	seat_service="# Seat mode's lines, from vtium-libseat.conf (ADR-1400 D3). systemd runs
# the PAM stack /etc/pam.d/vtium for vtium, and pam_systemd registers a
# logind session of vtium's own, which libseat's logind backend takes the
# seat from. systemd refuses PAMName= beside KillMode=process, so this unit
# sets KillMode=mixed: systemd still signals vtium alone at a stop or a
# restart, and then ends the unit's other processes, such as a login prompt
# nobody has used yet. Each logged-in session runs in a scope of its own.
$seat_lines"
	{
		header "crates/vtium/install/vtium@.service and vtium-libseat.conf" "the name, $name-seat@" \
			"Description=vtium in seat mode on %I" "ExecStart runs $exec_path" "${changes[@]}" \
			"Conflicts= and After= $name@%i.service" \
			"User=root, PAMName=vtium and KillMode=mixed from vtium-libseat.conf in place of KillMode=process" \
			"DefaultInstance=name-a-VT/$name-seat@ttyN, which is no VT"
		cat <<EOF
# vtium in seat mode on one VT of seat0, in place of its getty. The instance
# is the VT's name, as getty@.service's is. In seat mode vtium opens a logind
# session of its own, takes the screen, the keyboards and VT switching from
# logind through libseat, and starts each login in a systemd scope of its
# own, where the login gets a logind session with no seat (ADR-1400 D3). It
# conflicts with $unit on the same VT, the unit of direct mode, so a
# VT runs one mode at a time. To move every VT vtium has to seat mode, and
# back to direct mode:
#   vtium console enable --seat --all
#   vtium console enable --direct --all
# Enabling $seat_unit with no VT named fails and links nothing,
# because its DefaultInstance= names no VT.
# It conflicts with getty@ on the same VT. When vtium fails, its OnFailure=
# starts $name-getty@, which starts getty@ on the VT unless the head file's
# policy is closed (ADR-1401 D9), and its Restart= line starts vtium again
# first. The unit sets no sandboxing directive: the login session vtium
# starts would inherit it (ADR-1400, D3).

EOF
		unit_body "$name-seat" "${conflicts[*]} $name@%i.service" "$seat_service" |
			sed 's/^Description=vtium on %I$/Description=vtium in seat mode on %I/'
	} >"$tmp/$seat_unit"
fi

directives() { grep -v '^[[:space:]]*\(#\|$\)' "$1"; }
[ "$(directives "$tmp/$unit" | grep -c '^ExecStart=')" = 1 ] &&
	directives "$tmp/$unit" | grep -qF "ExecStart=$exec_path " &&
	grep -q '^ExecStart=/usr/bin/vtium ' "$src/vtium@.service" ||
	die "the shipped vtium@.service has no one ExecStart=/usr/bin/vtium line to change"
for c in "${conflicts[@]}"; do
	directives "$tmp/$unit" | grep -qxF "Conflicts=$c" && directives "$tmp/$unit" | grep -qxF "After=$c" ||
		die "the generated $unit has no Conflicts= and After= line for $c"
done
[ "$(directives "$tmp/$unit" | grep '^DefaultInstance=')" = "DefaultInstance=name-a-VT/$name@ttyN" ] &&
	[ "$(grep '^\[' "$tmp/$unit" | tail -n1)" = "[Install]" ] ||
	die "the generated $unit has no one refused DefaultInstance= at the end of its [Install] section"
[ "$(directives "$tmp/$fallback" | grep '^ExecStart=')" = "ExecStart=$exec_path console fallback %i" ] ||
	die "the shipped vtium-getty@.service has no one ExecStart=/usr/bin/vtium console fallback %i line to change"
directives "$tmp/$unit" | grep -qxF "OnFailure=$name-getty@%i.service" ||
	die "the shipped vtium@.service has no OnFailure=vtium-getty@%i.service line to change"
made=("$unit" "$fallback")
if [ -n "$seat" ]; then
	made+=("$seat_unit")
	for d in User=root PAMName=vtium KillMode=mixed "Conflicts=$name@%i.service" "After=$name@%i.service" \
		"Description=vtium in seat mode on %I" \
		"OnFailure=$name-getty@%i.service" "DefaultInstance=name-a-VT/$name-seat@ttyN"; do
		directives "$tmp/$seat_unit" | grep -qxF "$d" || die "the generated $seat_unit has no $d line"
	done
	[ "$(directives "$tmp/$seat_unit" | grep -c '^KillMode=')" = 1 ] ||
		die "the shipped vtium@.service has no one KillMode= line for seat mode's lines to replace"
fi
for f in "${made[@]}"; do
	! directives "$tmp/$f" | grep -v '^\(Conflicts\|After\)=kmsconvt@%i\.service$' | grep -q 'kmsconvt' ||
		die "the generated $f still names kmsconvt: the shipped unit changed shape"
done

mkdir -p -- "$out"
for f in "${made[@]}"; do
	mv -fT -- "$tmp/$f" "$out/$f"
	chmod 0644 -- "$out/$f"
done
