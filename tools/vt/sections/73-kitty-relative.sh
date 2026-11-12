#!/bin/bash
# look-for: A red square just right of "red ->", and a blue square just right
# look-for: of "blue ->", three rows lower and six columns further right.
# look-for: Nothing at the left of the red square's row, where it first stood.
# wrong-looks-like: The blue square anywhere else, a red square left at the
# wrong-looks-like: left, either square missing, or either over its label.
# kitty relative placements (ADR-1000's image.kitty-relative): a red square is
# placed at column 5 as placement 1 of image 24, and a blue square, image 25,
# is placed relative to it (P=24, Q=1) six columns right and three rows down
# (H=6, V=3). Then the red square's placement is put again at column 30,
# which replaces it, and the blue one follows it. Both are 8x8 RGB pixels
# (f=24) in 4x2 cells, sent as raw pixels, which vtium decodes itself.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
square() {
	local px='' i
	for ((i = 0; i < 64; i++)); do px+=$1; done
	printf '%b' "$px" | base64 -w0
}
printf '  kitty relative placements: blue follows red when red moves\n'
row=$((top + 2))
printf '%s[%d;23Hred ->' "$esc" "$row"
printf '%s[%d;29Hblue ->' "$esc" $((row + 3))
printf '%s[%d;5H' "$esc" "$row"
kitty_send "a=T,i=24,p=1,q=2,C=1,f=24,s=8,v=8,c=4,r=2" "$(square '\xe0\x20\x20')"
kitty_send "a=t,i=25,q=2,f=24,s=8,v=8" "$(square '\x30\x50\xf0')"
kitty_send "a=p,i=25,p=1,q=2,P=24,Q=1,H=6,V=3,c=4,r=2"
printf '%s[%d;30H' "$esc" "$row"
kitty_send "a=p,i=24,p=1,q=2,C=1,c=4,r=2"
printf '%s[%d;1H\n' "$esc" $((row + 5))
