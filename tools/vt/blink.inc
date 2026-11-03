# Sourced by the acceptance pass (accept.sh) and by its pages that blink text
# (sections/common.inc, ADR-1501): which blink style vtium draws SGR 5 and 6
# in, read as the user from the config file vtium reads.
# shellcheck shell=bash
# shellcheck disable=SC2034 # blink_style and blink_source are read by those that source this

# Sets blink_style to full, partial or off, the style blinking text draws in,
# and blink_source to where that comes from. vtium reads blink-text= and the
# blink switch from vtium.conf in its config directory, the last line of each
# winning, then from its command line, which wins over the file (docs/config.md);
# with the blink switch off, no text blinks, whatever blink-text says. The
# file is ACCEPT_VTIUM_CONF, which a review run sets to the config it gives
# vtium (tools/vt/config/vtium.conf), else /etc/vtium/vtium.conf.
# ACCEPT_BLINK_TEXT and ACCEPT_BLINK, which a review run sets from the vtium
# options it was given, stand for the command line. Without any of them the
# style is vtium's default, partial.
blink_style() {
	local conf=${ACCEPT_VTIUM_CONF:-/etc/vtium/vtium.conf} line key value text='' blink='' text_why='' off_why=''
	blink_style=partial
	blink_source="vtium's default"
	if [ ! -e "$conf" ]; then
		blink_source="vtium's default; $conf does not exist"
	elif [ ! -r "$conf" ] || [ ! -f "$conf" ]; then
		blink_source="vtium's default, assumed: $conf cannot be read as you, so a blink-text= line there is not known"
	else
		while IFS= read -r line || [ -n "$line" ]; do
			line=${line%%#*}
			key=${line%%=*}
			key=${key//[[:space:]]/}
			value=
			[ "$key" = "$line" ] || value=${line#*=}
			value=${value//[[:space:]]/}
			case $key in
			# a value vtium cannot use is ignored, keeping the one before
			blink-text) case $value in full | partial | off) text=$value text_why="blink-text=$value in $conf" ;; esac ;;
			blink) blink=on ;;
			no-blink) blink=off off_why="no-blink in $conf" ;;
			esac
		done <"$conf"
	fi
	case ${ACCEPT_BLINK_TEXT:-} in full | partial | off)
		text=$ACCEPT_BLINK_TEXT text_why="--blink-text=$ACCEPT_BLINK_TEXT on vtium's command line" ;;
	esac
	case ${ACCEPT_BLINK:-} in
	on) blink=on ;;
	off) blink=off off_why="--no-blink on vtium's command line" ;;
	esac
	if [ "$blink" = off ]; then
		blink_style=off blink_source=$off_why
	elif [ -n "$text" ]; then
		blink_style=$text blink_source=$text_why
	fi
}

# What blinking text looks like when it is right in blink_style.
blink_right() {
	case $blink_style in
	full) printf 'blinking text disappears in the off phase, about once a second, and comes back' ;;
	off) printf 'blinking text stays steady and never changes' ;;
	*) printf 'blinking text dims halfway to its background about once a second and never disappears' ;;
	esac
}
