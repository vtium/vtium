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
