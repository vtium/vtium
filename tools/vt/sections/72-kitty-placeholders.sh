#!/bin/bash
# look-for: Three copies of one picture of eight colored squares, red, yellow,
# look-for: green, cyan over blue, magenta, white, grey, each where its
# look-for: placeholder characters are; the third copy has its two rows
# look-for: swapped.
# wrong-looks-like: Boxes or letters where a copy should be, a copy missing,
# wrong-looks-like: squares in another order, or a third copy not swapped.
# kitty Unicode placeholders (ADR-1000's image.kitty-placeholders): the image
# is sent without a placement (a=t), then placed virtually (a=p,U=1) over 4x2
# cells. Text then shows it: each cell is U+10EEEE, with a combining mark for
# its row and one for its column from the protocol's table of diacritics, in
# a foreground color that names the image (38;5;23). The page prints the
# cells three times, the third with its rows in the other order, so each
# copy is the picture its cells name. The marks are written as UTF-8 bytes so
# the output does not depend on the locale.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
# A 32x16 RGB picture of eight 8x8 squares, four to a row, in base64.
colors=('\xe0\x20\x20' '\xf0\xe0\x20' '\x20\xc0\x40' '\x20\xd0\xd0'
	'\x30\x50\xf0' '\xd0\x30\xd0' '\xf0\xf0\xf0' '\x80\x80\x80')
px=''
for ((y = 0; y < 16; y++)); do
	for ((x = 0; x < 32; x++)); do px+=${colors[$((y / 8 * 4 + x / 8))]}; done
done
picture=$(printf '%b' "$px" | base64 -w0)
cell=$'\xf4\x8e\xbb\xae'                                  # U+10EEEE
marks=($'\xcc\x85' $'\xcc\x8d' $'\xcc\x8e' $'\xcc\x90') # U+0305, U+030D, U+030E, U+0310
# The placeholder cells of picture row R.
cells() {
	local c out=''
	for ((c = 0; c < 4; c++)); do out+=$cell${marks[$1]}${marks[$c]}; done
	printf '%s' "$out"
}
printf '  kitty placeholders: the picture where its characters are\n'
kitty_send "a=t,i=23,q=2,f=24,s=32,v=16" "$picture"
kitty_send "a=p,U=1,i=23,q=2,c=4,r=2"
row=$((top + 2))
printf '%s[%d;1H  %-17s%-17s%s' "$esc" "$row" "first copy" "second copy" "rows swapped"
for spot in "3 0 1" "20 0 1" "37 1 0"; do
	read -r col first second <<<"$spot"
	printf '%s[%d;%dH%s[38;5;23m%s%s[0m' "$esc" $((row + 1)) "$col" "$esc" "$(cells "$first")" "$esc"
	printf '%s[%d;%dH%s[38;5;23m%s%s[0m' "$esc" $((row + 2)) "$col" "$esc" "$(cells "$second")" "$esc"
done
printf '%s[%d;1H\n' "$esc" $((row + 3))
