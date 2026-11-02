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

# vt_installed ROOT N: whether make vt-install holds VT N, which its manifest
# under ROOT (empty for the machine itself) says.
vt_installed() {
	[ -f "$1$vt_install_state/tty$2" ]
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

# vt_held SYSTEMCTL ROOT N: why an installed vtium holds VT N, the
# development install or the packaged one, or nothing when neither does. A
# review run never borrows such a VT: it is the maintainer's everyday
# console, which a review run would stop, and a start limit or a failed
# restore would leave without one.
vt_held() {
	if vt_installed "$2" "$3" || vt_unit_held "$1" "vtium-dev@tty$3.service"; then
		echo "make vt-install installed vtium-dev@tty$3 there; make vt-uninstall VT=$3 frees it"
	else
		vt_packaged "$1" "$3"
	fi
}
