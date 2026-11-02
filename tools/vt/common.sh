# Sourced by the VT review tools (docs/vt-review.md); not run on its own.
# shellcheck shell=bash

# Where the answers and the run logs go. A run clears the session's
# environment, so a session always writes here; VTIUM_VT_DIR moves it for a
# run by hand and for the tests.
# shellcheck disable=SC2034 # read by the scripts that source this file
vt_dir=${VTIUM_VT_DIR:-/tmp/vtium-vt}

# vt_own_dir DIR: creates DIR with mode 0700 when it is missing, and fails when
# it is a symlink, not a directory of this user's, or open to anyone else.
# Another user can make a directory of that name in /tmp first; nothing is
# written into one.
vt_own_dir() {
	[ -e "$1" ] || [ -L "$1" ] || mkdir -m 0700 -- "$1" 2>/dev/null || true
	[ ! -L "$1" ] && [ -d "$1" ] && [ -O "$1" ] && [ "$(stat -c %a -- "$1")" = 700 ]
}

# vt_sessions LOGINCTL N: prints "ID STATE" for each logind session on VT N,
# and fails when LOGINCTL cannot list the sessions. A session whose query
# fails is skipped only when a second list no longer has it, as one that
# ended between the list and the query; one still listed is a session on an
# unknown VT, so that fails too. run-vt.sh and vt-install.sh refuse a VT
# this prints anything for.
vt_sessions() {
	local loginctl=$1 vt=$2 list s props
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

# Where make vt-install keeps its manifests, one a VT it holds
# (tools/vt/vt-install.sh). The development install is vtium-dev@, beside the
# packaged vtium@, which a package manager installs and make vt-install never
# touches.
vt_install_state=/var/lib/vtium-dev-install

# vt_dev_vts SYSTEMCTL ROOT: the development VTs, one number a line, lowest
# first. A development VT has a manifest of make vt-install's under ROOT
# (empty for the machine itself), and its vtium-dev@ttyN is enabled. The
# review runs borrow the first one when no VT is named.
vt_dev_vts() {
	local m n
	for m in "$2$vt_install_state"/tty*; do
		[ -f "$m" ] || continue
		n=${m##*/tty}
		[[ $n =~ ^[1-9][0-9]*$ ]] || continue
		[ "$("$1" is-enabled "vtium-dev@tty$n.service" 2>/dev/null)" != enabled ] || echo "$n"
	done | sort -n
}

# vt_unit_held SYSTEMCTL UNIT: whether UNIT runs or is enabled, so that it
# holds its VT now or at the VT's next start.
vt_unit_held() {
	"$1" is-active --quiet "$2" || [ "$("$1" is-enabled "$2" 2>/dev/null)" = enabled ]
}

# vt_packaged SYSTEMCTL N: why the packaged vtium holds VT N, or nothing.
vt_packaged() {
	! vt_unit_held "$1" "vtium@tty$2.service" ||
		echo "the packaged vtium@tty$2 holds it; sudo systemctl disable --now vtium@tty$2 frees it"
}

# vt_borrowable SYSTEMCTL ANALYZE N: why a review run cannot borrow VT N, or
# nothing when it can. The packaged vtium's VT is the maintainer's everyday
# console: a review run would stop it, and a start limit or a failed restore
# would leave it without one. That covers a VT where logind starts the
# packaged vtium at a switch, which vtium console enable --all gives every VT
# without a link of its own. A review run cannot keep logind's reserved VT
# either: logind starts autovt@ttyN there at every switch to it, even while
# the run holds the VT, and that console takes the VT from the run's vtium.
# The development VT, vtium-dev@ttyN, is borrowed: run-vt.sh stops it and
# starts it again.
vt_borrowable() {
	local why
	why=$(vt_packaged "$1" "$3")
	if [ -z "$why" ] && [ "$("$1" show -p Id --value "autovt@tty$3.service" 2>/dev/null)" = "vtium@tty$3.service" ]; then
		why="logind starts the packaged vtium@tty$3 there when the VT is switched to (autovt@tty$3 is vtium@tty$3); vtium console status shows what holds each VT"
	fi
	if [ -z "$why" ] && [ "$3" = "$(vt_reserved "$2")" ]; then
		why="it is logind's reserved VT (ReserveVT= in logind.conf), where logind starts a console at every switch, even during a run"
	fi
	[ -z "$why" ] || echo "$why"
}

# vt_reserved ANALYZE: logind's reserved VT, the last ReserveVT= among
# logind.conf and its drop-ins, as systemd-analyze cat-config prints them; 0
# is none. logind's default, 6, when none sets it or the files cannot be read.
vt_reserved() {
	local v
	v=$("$1" cat-config systemd/logind.conf 2>/dev/null |
		sed -n 's/^[[:space:]]*ReserveVT[[:space:]]*=[[:space:]]*\([0-9][0-9]*\)[[:space:]]*$/\1/p' | tail -n 1) || v=
	echo "${v:-6}"
}
