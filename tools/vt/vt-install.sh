#!/bin/bash
# Installs vtium as the console of one VT of this machine, so it stays there
# across reboots, and takes it off again (docs/vt-review.md, "Installing
# vtium on a VT"). make vt-install VT=N and make vt-uninstall VT=N run it.
#
#   vt-install.sh install|uninstall [--dry-run] N
#
# install copies the binary VTIUM_VT_BIN to /usr/local/bin/vtium, writes the
# unit vtium@.service and its getty fallback vtium-getty@.service into
# /etc/systemd/system, generated from crates/vtium/install/kmsconvt@.service
# and kmsconvt-getty@.service, records what it did in a manifest,
# /var/lib/vtium-vt-install/ttyN, then stops and disables getty@ttyN and
# enables and starts vtium@ttyN. uninstall reads the manifest and undoes all
# of it: vtium@ttyN stopped and disabled, getty@ttyN enabled and started
# again where it was before, and the files removed once no other VT's
# manifest needs them. The names kmsconvt@ and kmsconvt-getty@ are left
# alone: the C reference's package owns them on this machine. Its template
# alias /etc/systemd/system/autovt@.service is left alone too; enabling
# vtium@ttyN adds the per-VT alias autovt@ttyN.service -> vtium@.service, so
# that logind, which starts autovt@ttyN on a VT nothing holds, starts vtium
# there and never the C reference's console beside it. Neither command
# touches /etc/kmscon/kmscon.conf, which vtium reads.
#
# Each command that changes anything runs through sudo and is printed first.
# --dry-run (make DRY_RUN=1) asks what it would ask and prints the units and
# every command, and runs none of them.
#
# For the tests, VTIUM_VT_STANDINS names a directory of stand-ins: sudo,
# systemctl and loginctl run from there, "active" stands for the VT on
# screen, "root" is the directory the installed paths are under, and an
# "install" directory there, when present, stands for crates/vtium/install.
set -euo pipefail

usage() {
	sed -n '7,7s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'vt-install: %s\n' "$*" >&2
	exit 1
}

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
checkout=$(readlink -m "$here/../..")
# shellcheck source=tools/vt/common.sh
. "$here/common.sh"

action=
dry=
vt=
while [ $# -gt 0 ]; do
	case $1 in
	install | uninstall) [ -z "$action" ] || usage; action=$1 ;;
	--dry-run) dry=1 ;;
	-h | --help) usage ;;
	*) [ -z "$vt" ] || usage; vt=$1 ;;
	esac
	shift
done
[ -n "$action" ] || usage
[ -n "$vt" ] || die "VT=N names the VT, 2 to 12"
[[ $vt =~ ^[1-9][0-9]*$ ]] || die "VT must be a number from 2 to 12, not '$vt'"
[ "$vt" != 1 ] || die "VT 1 is refused: it holds the boot's messages and its first login, and a VT that comes back wrong there leaves no console to fix it from"
[ "$vt" -ge 2 ] && [ "$vt" -le 12 ] || die "VT must be a number from 2 to 12, not '$vt'"

# It asks sudo for each step that needs root, and prints each one first.
[ "$(id -u)" -ne 0 ] || die "run it as yourself, not as root or under sudo; it asks sudo for each step that needs root"

standins=${VTIUM_VT_STANDINS:-}
src=$checkout/crates/vtium/install
if [ -n "$standins" ]; then
	[ "$(id -u)" -ne 0 ] || die "VTIUM_VT_STANDINS is for the tests, which run without root; unset it"
	sudo=$standins/sudo
	systemctl=$standins/systemctl
	loginctl=$standins/loginctl
	active_file=$standins/active
	root=$standins/root
	[ ! -d "$standins/install" ] || src=$standins/install
	# The tests run without root, so the files they install stay theirs.
	own=()
else
	sudo=sudo
	systemctl=systemctl
	loginctl=loginctl
	active_file=/sys/class/tty/tty0/active
	root=
	own=(-o root -g root)
fi
bin_path=/usr/local/bin/vtium
unit_dir=/etc/systemd/system
state_dir=$vt_install_state
unit=vtium@tty$vt.service
fallback=vtium-getty@tty$vt.service
getty=getty@tty$vt.service
manifest=$state_dir/tty$vt
# The checksum of each file install wrote, one "SHA256  PATH" line each, as
# sha256sum prints them. The files are every VT's, so the record is too.
# While install copies, a file can hold its checksum from before or after
# the copy, so the record holds both until the copies are done.
record=$state_dir/files
# What install writes, by its path on the machine; uninstall removes these.
files=("$bin_path" "$unit_dir/vtium@.service" "$unit_dir/vtium-getty@.service")
# The per-VT alias that enabling vtium@ttyN makes, and where it points.
alias_link=$unit_dir/autovt@tty$vt.service
alias_target=$unit_dir/vtium@.service

show() {
	printf '+'
	printf ' %q' "$@"
	printf '\n'
}
# Runs a command as root, printed first; a dry run prints it alone.
as_root() {
	show sudo "$@"
	[ -n "$dry" ] || "$sudo" "$@"
}

# Refuses the VT on screen (the desktop's, since this runs from it) and a VT
# a logind session is on, as run-vt.sh does: stopping its unit would end
# that session.
fg=$(cat "$active_file") || die "cannot read $active_file, so the VT on screen is unknown"
fg=${fg#tty}
[[ $fg =~ ^[0-9]+$ ]] || die "$active_file reads '$fg', so the VT on screen is unknown"
[ "$fg" != "$vt" ] || die "VT $vt is on screen now; run this from another VT"
on_vt=$(vt_sessions "$loginctl" "$vt") ||
	die "loginctl could not list the sessions or read one of them, so whether a session is on VT $vt is unknown"
[ -z "$on_vt" ] || die "logind session ${on_vt%% *} is on VT $vt; log out there first"

# The manifests in place, by VT: each VT that vt-install holds has one.
manifests() {
	local m
	for m in "$root$state_dir"/tty*; do
		[ ! -f "$m" ] || printf '%s\n' "${m##*/}"
	done
}

# The checksums the record holds for file $1, one a line, or nothing.
recorded() {
	[ -f "$root$record" ] || return 0
	awk -v f="$1" '$2 == f && $1 ~ /^[0-9a-f]+$/ && length($1) == 64 { print $1 }' "$root$record"
}

# The checksum of file $1 as it is on the machine now.
checksum() {
	sha256sum <"$root$1" | cut -d' ' -f1
}

# Whether file $1 is one install wrote: a regular file whose checksum the
# record holds.
written() {
	[ -f "$root$1" ] && [ ! -L "$root$1" ] && recorded "$1" | grep -qxF -- "$(checksum "$1")"
}

# The value of KEY in the manifest, which install wrote as root: yes or no.
manifest_value() {
	local v
	v=$(sed -n "s/^$1=//p" "$root$manifest")
	[[ $v == yes || $v == no ]] || die "$manifest has no $1=yes or $1=no line; restore VT $vt by hand (docs/vt-review.md)"
	printf '%s' "$v"
}

if [ "$action" = uninstall ]; then
	[ -f "$root$manifest" ] || die "no $manifest: make vt-install did not install vtium on VT $vt, so there is nothing to undo"
	was_enabled=$(manifest_value getty-enabled)
	was_active=$(manifest_value getty-active)
	others=$(manifests | grep -vx "tty$vt" || true)
	[ -z "$dry" ] || echo "vt-uninstall: the commands, none of them run:"
	# A run cut short before vtium@.service was copied left no unit to stop
	# or disable, and systemctl refuses a unit it cannot find.
	if [ -e "$root$unit_dir/vtium@.service" ]; then
		as_root "$systemctl" stop "$unit"
		as_root "$systemctl" disable "$unit"
	fi
	# disable leaves the per-VT alias autovt@ttyN, so logind would start
	# vtium there again; it goes when it is still the one enable made.
	if [ "$(readlink -- "$root$alias_link" 2>/dev/null)" = "$alias_target" ]; then
		as_root rm -f -- "$root$alias_link"
	fi
	# A unit that failed stays listed as failed until this clears it.
	show sudo "$systemctl" reset-failed "$unit" "$fallback"
	[ -n "$dry" ] || "$sudo" "$systemctl" reset-failed "$unit" "$fallback" 2>/dev/null || true
	# enable would also make the getty's own alias autovt@ttyN.service, which
	# was never there; add-wants makes only the getty.target.wants link that
	# install's disable removed.
	[ "$was_enabled" = no ] || as_root "$systemctl" add-wants getty.target "$getty"
	[ "$was_active" = no ] || as_root "$systemctl" start "$getty"
	as_root rm -f -- "$root$manifest"
	if [ -n "$others" ]; then
		echo "vt-uninstall: kept ${files[*]}: vt-install holds $(paste -sd' ' <<<"$others") as well"
	else
		# Only a file that is still the one install wrote goes; one changed
		# since is someone's edit, and stays.
		for f in "${files[@]}"; do
			[ -e "$root$f" ] || continue
			if written "$f"; then
				as_root rm -f -- "$root$f"
			else
				echo "vt-uninstall: kept $f: it changed since vt-install wrote it"
			fi
		done
		as_root rm -f -- "$root$record"
		as_root rmdir --ignore-fail-on-non-empty -- "$root$state_dir"
	fi
	as_root "$systemctl" daemon-reload
	[ -n "$dry" ] || echo "vt-uninstall: VT $vt is back as it was before make vt-install"
	exit 0
fi

bin=${VTIUM_VT_BIN:-}
[ -n "$bin" ] || die "VTIUM_VT_BIN names the vtium binary; make vt-install sets it"
bin=$(readlink -m -- "$bin")
[ -f "$bin" ] && [ -x "$bin" ] || die "no vtium binary at $bin: build it first (make minspec)"
read -r owner mode < <(stat -c '%u %a' -- "$bin")
[ "$owner" -eq 0 ] || [ "$owner" -eq "$(id -u)" ] ||
	die "$bin belongs to uid $owner, neither root nor you; it would run as root"
(( (8#$mode & 8#022) == 0 )) || die "$bin is writable by group or others (mode $mode); it would run as root"

# The C reference's console on the VT would fight vtium for it.
! "$systemctl" is-active --quiet "kmsconvt@tty$vt.service" ||
	die "kmsconvt@tty$vt is running on VT $vt; stop it first, or pick another VT"

# A file at one of the paths is replaced only when it is the one install
# wrote, by the record's checksum; any other is someone else's, or an edit.
for f in "${files[@]}"; do
	[ -e "$root$f" ] || [ -L "$root$f" ] || continue
	[ -n "$(recorded "$f")" ] ||
		die "$f is there already and vt-install did not write it; it is left alone, so nothing is installed"
	written "$f" || die "$f changed since vt-install wrote it; it is left alone, so nothing is installed"
done
# The per-VT alias is enable's to make; one there already, unless it is the
# one an earlier install on this VT made, belongs to someone else.
if [ -e "$root$alias_link" ] || [ -L "$root$alias_link" ]; then
	[ -f "$root$manifest" ] && [ "$(readlink -- "$root$alias_link")" = "$alias_target" ] ||
		die "$alias_link is there already and vt-install did not make it; it is left alone, so nothing is installed"
fi

tmp=$(mktemp -d) || die "cannot create a scratch directory"
trap 'rm -rf -- "$tmp"' EXIT

# Generates the two units from the shipped ones, changing only the unit's
# name, ExecStart's path and the fallback's name. Every other line, Restart=
# and the alias autovt@ included, is the shipped unit's. Fails when the
# shipped unit no longer has the lines it changes.
built=$(git -C "$checkout" describe --always --dirty 2>/dev/null) || built=unknown
# header SOURCE CHANGES...: the generated unit's opening comment, a line for
# each change.
header() {
	printf '# Generated by make vt-install from %s at %s. Changed from it:\n' "$1" "$built"
	shift
	printf '#   %s\n' "$@"
	printf '# make vt-uninstall removes it.\n'
}
for f in kmsconvt@.service kmsconvt-getty@.service; do
	[ -f "$src/$f" ] || die "no $src/$f, which the units are generated from"
done
{
	header crates/vtium/install/kmsconvt@.service "the name, vtium@" \
		"ExecStart runs /usr/local/bin/vtium" "the getty fallback, vtium-getty@"
	sed -e 's|^ExecStart=/usr/bin/vtium |ExecStart=/usr/local/bin/vtium |' \
		-e 's|^OnFailure=kmsconvt-getty@%i\.service$|OnFailure=vtium-getty@%i.service|' "$src/kmsconvt@.service"
} >"$tmp/vtium@.service"
{
	header crates/vtium/install/kmsconvt-getty@.service "the name, vtium-getty@, which vtium@.service names as its fallback"
	cat "$src/kmsconvt-getty@.service"
} >"$tmp/vtium-getty@.service"
directives() { grep -v '^[[:space:]]*\(#\|$\)' "$1"; }
[ "$(directives "$tmp/vtium@.service" | grep -c '^ExecStart=')" = 1 ] &&
	directives "$tmp/vtium@.service" | grep -q '^ExecStart=/usr/local/bin/vtium ' ||
	die "the shipped kmsconvt@.service has no one ExecStart=/usr/bin/vtium line to change"
directives "$tmp/vtium@.service" | grep -qx 'OnFailure=vtium-getty@%i.service' ||
	die "the shipped kmsconvt@.service has no OnFailure=kmsconvt-getty@%i.service line to change"
for f in vtium@.service vtium-getty@.service; do
	! directives "$tmp/$f" | grep -q 'kmsconvt\|/usr/bin/vtium' ||
		die "the generated $f still names kmsconvt or /usr/bin/vtium: the shipped unit changed shape"
done

# What getty@ttyN was before, so uninstall puts it back: the first install
# on this VT records it, and a reinstall keeps that record.
if [ -f "$root$manifest" ]; then
	echo "vt-install: VT $vt holds vtium already; this installs the build again and restarts it"
	write_manifest=
else
	enabled=$("$systemctl" is-enabled "$getty" 2>/dev/null || true)
	[ "$enabled" = enabled ] && enabled=yes || enabled=no
	"$systemctl" is-active --quiet "$getty" && active=yes || active=no
	cat >"$tmp/manifest" <<EOF
# Written by make vt-install (tools/vt/vt-install.sh) at $built.
# make vt-uninstall VT=$vt reads it to put VT $vt back as it was.
vt=$vt
getty-enabled=$enabled
getty-active=$active
EOF
	write_manifest=1
fi

{
	printf '%s  %s\n' "$(sha256sum <"$bin" | cut -d' ' -f1)" "$bin_path"
	for f in vtium@.service vtium-getty@.service; do
		printf '%s  %s\n' "$(sha256sum <"$tmp/$f" | cut -d' ' -f1)" "$unit_dir/$f"
	done
} >"$tmp/files"

if [ -n "$dry" ]; then
	echo "vt-install: the units it would write:"
	for f in vtium@.service vtium-getty@.service; do
		printf '\n--- %s/%s\n' "$unit_dir" "$f"
		cat "$tmp/$f"
	done
	[ -z "$write_manifest" ] || { printf '\n--- %s\n' "$manifest"; cat "$tmp/manifest"; }
	printf '\n--- %s\n' "$record"
	cat "$tmp/files"
	printf '\nvt-install: the commands, none of them run:\n'
else
	"$sudo" -v || die "sudo failed"
	# From here on a step that fails leaves part of the install in place,
	# which the manifest, written first, lets uninstall undo.
	set -E
	trap 'echo "vt-install: a step failed; make vt-uninstall VT=$vt undoes what this run did" >&2' ERR
fi
# The manifest goes first, so a run cut short can still be undone.
[ -d "$root$state_dir" ] || as_root install -d -m 0755 "${own[@]}" -- "$root$state_dir"
[ -z "$write_manifest" ] || as_root install -m 0644 "${own[@]}" -- "$tmp/manifest" "$root$manifest"
# The record goes before the copies, holding each file's checksum from before
# and after, so a copy that fails leaves every file in a state the record
# knows: uninstall removes it, and a second run replaces it.
{
	cat "$tmp/files"
	for f in "${files[@]}"; do
		! written "$f" || printf '%s  %s\n' "$(checksum "$f")" "$f"
	done
} >"$tmp/files.both"
as_root install -m 0644 "${own[@]}" -- "$tmp/files.both" "$root$record"
as_root install -D -m 0755 "${own[@]}" -- "$bin" "$root$bin_path"
for f in vtium@.service vtium-getty@.service; do
	as_root install -D -m 0644 "${own[@]}" -- "$tmp/$f" "$root$unit_dir/$f"
done
# The copies are done, so the record holds only what is there now.
as_root install -m 0644 "${own[@]}" -- "$tmp/files" "$root$record"
as_root "$systemctl" daemon-reload
# vtium starts first: its Conflicts=getty@%i stops the getty as it starts,
# so the VT always has one of them. The getty is disabled once vtium runs.
as_root "$systemctl" enable "$unit"
as_root "$systemctl" restart "$unit"
as_root "$systemctl" disable "$getty"
[ -n "$dry" ] || cat <<EOF

vt-install: vtium runs on VT $vt, and starts there at every boot.
  Switch to it with Ctrl-Alt-F$vt; Ctrl-Alt-F$fg returns to this desktop.
  make vt-uninstall VT=$vt puts VT $vt back as it was.
EOF
