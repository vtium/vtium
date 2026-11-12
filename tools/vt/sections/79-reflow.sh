#!/bin/bash
# look-for: After the font set grows, line A's numbers run from 001 to its
# look-for: last in order, rewrapped to the narrower screen, none missing.
# look-for: Line B, written with reflow off, keeps each row's start only, so
# look-for: numbers are missing at the end of each of its rows.
# wrong-looks-like: A number missing or repeated in line A, line A cut like
# wrong-looks-like: line B, line B rewrapped whole, or the grid unchanged.
# font-switch: acceptance-large
# Reflow (ADR-1000's text.reflow and DEC-2028): the page writes two long
# lines that wrap, then switches the console with OSC 50 to the font set its
# "# font-switch:" line names, whose character cells are twice as wide, so
# the grid loses half its columns while the page shows. Line A is written
# with mode 2028 set, vtium's default, so it rewraps to the new width; line B
# is written with mode 2028 reset, so each of its rows is cut at the new
# width. The page sets mode 2028 again after line B. accept.sh switches the
# console back to its own font before the next page, and records the font
# set's files. The page needs a walk with prompts, which puts the font back,
# and a terminal that switches font sets with OSC 50, as vtium does.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
frit=$(sed -n 's/^# font-switch: //p' "$0")
[ -z "${ACCEPT_NOPROMPT:-}${ACCEPT_NOPAGE:-}" ] ||
	cannot_show "This page switches the font set, which only a walk with prompts puts back."
font=$(ask "$esc]50;?"$'\a' $'\a')
[[ $font == *']50;'* ]] ||
	cannot_show "The terminal does not answer OSC 50's query, so it cannot switch fonts."
# The grid's size as the terminal sets it: rows, then columns.
size() { stty size </dev/tty 2>/dev/null; }
read -r rows0 cols0 <<<"$(size)"
[[ $cols0 =~ ^[1-9][0-9]*$ ]] || cannot_show "The terminal does not give its size."
# Numbers 001, 002 and on, each four columns with its space, enough to fill
# two and a half rows of the grid as it is now.
numbers() {
	local i out=''
	for ((i = 1; i <= cols0 * 5 / 2 / 4; i++)); do printf -v out '%s%03d ' "$out" "$i"; done
	printf '%s' "${out% }"
}
printf '  Line A, reflow on, then line B, reflow off (mode 2028 reset):\n'
printf 'A: %s\r\n' "$(numbers)"
printf '%s[?2028lB: %s\r\n%s[?2028h' "$esc" "$(numbers)" "$esc"
printf '%s]50;%s\a' "$esc" "$frit"
now=$(ask "$esc]50;?"$'\a' $'\a')
[ "${now##*]50;}" = "$frit" ] ||
	cannot_show "vtium did not switch to the font set $frit: OSC 50 named '${now##*]50;}'."
read -r rows1 cols1 <<<"$(size)"
[ "$cols1" != "$cols0" ] ||
	cannot_show "The font set $frit left the grid at $cols0 columns, so nothing rewrapped."
printf '  The grid went from %s by %s to %s by %s characters.\n' "$cols0" "$rows0" "$cols1" "$rows1"
