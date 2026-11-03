#!/bin/bash
# look-for: Each sixel bar the same color as the text cells beside it and as
# look-for: tall as two rows; an orange frame around a row of words, its
# look-for: middle clear, so every word inside it shows whole.
# wrong-looks-like: A bar of another color or size, a black box over the
# wrong-looks-like: words where the frame should be clear, or the page's
# wrong-looks-like: "cannot show" line in a build with pictures.
# Sixel pictures (DCS q), sized from the cell size the terminal reports
# (XTWINOPS 16) so each covers whole cells: three opaque bars in red, green
# and blue, each beside text cells of the same color (SGR 48;2), then a
# frame with a transparent middle (P2=1) over a row of words. vtium draws a
# sixel above the text, so the words show where the frame has no pixels.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
pictures_shown || cannot_show "vtium was built without pictures: its DA1 answer has no 4."
top_row
cell_size
note=
if ((cw == 0)); then
	cw=10 ch=20
	note='XTWINOPS 16 gave no cell size, so the pictures are made for 10x20 cells.'
fi
# the sixel data character for the six bits B
six() { printf "\\x$(printf '%02x' $((63 + $1)))"; }
# a W by H picture of one color, R G B in percent
solid() {
	local w=$1 h=$2 y
	printf '%sP0;1;0q"1;1;%d;%d#1;2;%d;%d;%d' "$esc" "$w" "$h" "$3" "$4" "$5"
	for ((y = 0; y + 6 <= h; y += 6)); do printf '#1!%d~-' "$w"; done
	((h % 6)) && printf '#1!%d%s' "$w" "$(six $(((1 << (h % 6)) - 1)))"
	printf '%s\\' "$esc"
}
# a W by H frame T pixels thick, R G B in percent, transparent inside
frame() {
	local w=$1 h=$2 t=$3 y j all mid
	printf '%sP0;1;0q"1;1;%d;%d#1;2;%d;%d;%d' "$esc" "$w" "$h" "$4" "$5" "$6"
	for ((y = 0; y < h; y += 6)); do
		all=0 mid=0
		for ((j = 0; j < 6 && y + j < h; j++)); do
			all=$((all | 1 << j))
			((y + j < t || y + j >= h - t)) && mid=$((mid | 1 << j))
		done
		printf '#1!%d%s!%d%s!%d%s-' "$t" "$(six "$all")" $((w - 2 * t)) "$(six "$mid")" "$t" "$(six "$all")"
	done
	printf '%s\\' "$esc"
}
at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
printf '  Sixel bars, each beside text cells of the same color:'
r=$((top + 2)) col=3
for c in '100 0 0 255 0 0 red' '0 100 0 0 255 0 green' '0 0 100 0 0 255 blue'; do
	read -r pr pg pb tr tg tb name <<<"$c"
	at "$r" "$col"
	solid $((6 * cw)) $((2 * ch)) "$pr" "$pg" "$pb"
	for y in 0 1; do
		at $((r + y)) $((col + 7))
		printf '\033[48;2;%d;%d;%dm      \033[0m' "$tr" "$tg" "$tb"
	done
	at $((r + 2)) "$col"
	printf 'sixel  text  %s' "$name"
	col=$((col + 22))
done
at $((r + 4)) 3
printf 'A sixel frame with a clear middle, over a row of words:'
at $((r + 7)) 3
printf ' WORDS INSIDE THE FRAME SHOW THROUGH ITS CLEAR MIDDLE '
at $((r + 6)) 3
frame $((54 * cw)) $((3 * ch)) $((ch / 3)) 100 60 0
at $((r + 10)) 1
[ -z "$note" ] || printf '  %s\n' "$note"
printf '  cell %dx%d pixels\n' "$cw" "$ch"
