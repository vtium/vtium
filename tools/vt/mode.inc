# Sourced by run-vt.sh, accept.sh and the pages' common.inc; not run on its
# own. These functions read the lines of a vtium.conf that choose each
# display's mode, so that a review run gives vtium the modes the machine's own
# vtium.conf sets, and the acceptance pass records the mode each display runs
# at.
# shellcheck shell=bash

# vt_mode_lines FILE prints the lines of FILE's top level, the lines above its
# first section heading, that choose a display's mode, one a line in the
# file's order. vtium's key table has two such keys: mode, which names a
# resolution, and the switch use-original-mode, which keeps the mode the
# console showed. Each line is printed as vtium reads it, with the blanks
# around the key and the value dropped: mode=VALUE, use-original-mode or
# no-use-original-mode. A # starts a comment at the start of a line or after
# a blank, as in vtium. The function checks no value; vt_mode_value does.
#
# References:
# - ADR-1403 D3: the top level's key syntax and its comment rule.
vt_mode_lines() {
	awk '/^[ \t]*\[/ { exit }
		{ sub(/(^|[ \t])#.*/, "") }
		match($0, /^[ \t]*mode[ \t]*=/) {
			v = substr($0, RLENGTH + 1)
			gsub(/^[ \t]+|[ \t\r]+$/, "", v)
			print "mode=" v
			next
		}
		{ gsub(/^[ \t]+|[ \t\r]+$/, "") }
		$0 == "use-original-mode" || $0 == "no-use-original-mode" { print }' "$1"
}

# vt_mode_value LINE succeeds when LINE, one that vt_mode_lines prints, is a
# switch line or a mode line whose value has the form vtium reads:
# [GPU[CONNECTOR]]WxH[@RATE], where GPU is a card's udev ID_PATH. Each name
# holds only letters, digits and the characters : . _ + and -, so a line the
# run carries into a file of root's is never more than a mode.
vt_mode_value() {
	local name='[A-Za-z0-9:._+-]{1,63}'
	local re="^mode=(($name)?\\[$name\\])?[0-9]{1,5}x[0-9]{1,5}(@[0-9]{1,5})?\$"
	[[ $1 == use-original-mode || $1 == no-use-original-mode || $1 =~ $re ]]
}

# vt_mode_pick GPU CONNECTOR LINE... prints what vtium does with the display
# on CONNECTOR of the card whose ID_PATH is GPU, given the display-mode LINEs
# vtium is given: the mode line that matches the display, or
# use-original-mode, or nothing when the display runs at its preferred mode.
# vtium takes the first line that names both the card and the connector, then
# the first that names the connector alone, then the first that names
# neither. use-original-mode counts, by its last line, only when no mode line
# is given.
vt_mode_pick() {
	local gpu=$1 connector=$2 l spec both='' alone='' any='' original=''
	shift 2
	for l in "$@"; do
		case $l in
		use-original-mode) original=1; continue ;;
		no-use-original-mode) original=''; continue ;;
		esac
		spec=${l#mode=}
		if [[ $spec == *\[*\]* ]]; then
			local g=${spec%%\[*} c=${spec#*\[}
			c=${c%%\]*}
			[ "$c" = "$connector" ] || continue
			if [ -n "$g" ]; then
				[ "$g" != "$gpu" ] || [ -n "$both" ] || both=$l
			else
				[ -n "$alone" ] || alone=$l
			fi
		else
			[ -n "$any" ] || any=$l
		fi
	done
	if [ -n "$both$alone$any" ]; then
		printf '%s\n' "${both:-${alone:-$any}}"
		return
	fi
	# A mode line for another display still turns use-original-mode off.
	for l in "$@"; do
		[[ $l != mode=* ]] || return 0
	done
	[ -z "$original" ] || echo use-original-mode
}
