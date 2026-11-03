#!/bin/bash
# kit: kitty
# look-for: Three pictures of red, green, blue and yellow quarters at row 3,
# look-for: each 8 cells by 3 rows: a sixel at column 3, a kitty picture
# look-for: above text at column 23, and a kitty picture below text at column
# look-for: 43. A row of #, written across row 4 after them, cuts the cells of
# look-for: the sixel it covers, leaving # on black there; it is hidden under
# look-for: the kitty picture above text, and drawn over the kitty picture
# look-for: below text. On row 5, 4 cells erased in each picture's middle cut
# look-for: the sixel's cells to black and leave both kitty pictures whole.
# wrong-looks-like: No pictures (the screen says a protocol is not offered),
# wrong-looks-like: the sixel whole under the # row or the erase, a kitty
# wrong-looks-like: picture cut by text or by the erase, # showing over the
# wrong-looks-like: middle picture, or # hidden under the right one.
# screen-text: none, pictures and cursor moves do not print as rows
# Text written over pictures (ADR-1100 D3, D4). A character written into a
# cell a sixel picture covers cuts the part of the picture under that cell;
# ECH, which erases cells, cuts it the same way. A kitty picture keeps its
# cells whatever is written or erased there, and draws above the text at z
# 0 and below it at z -1. The pictures are tools/tier3/t3_pictures.py's
# quarters, sent as a sixel string and as raw RGBA pixels (f=32).
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
sixel_offered
kitty_offered
pictures_there
top_row

printf 'img-overwrite: text and an erase over a sixel and two kitty pictures\n'
at 3 3
cat "$pics/quad.six"
at 3 23
kitty_send "a=T,i=40,q=2,C=1,f=32,s=64,v=48,z=0" "$(pic64 quad.rgba)"
at 3 43
kitty_send "a=p,i=40,p=2,q=2,C=1,z=-1"
at 4 1
printf '%s' '##################################################################'
for col in 5 25 45; do
	at 5 $col
	printf '%s[4X' "$esc"
done
at 7 1
printf 'sixel: cut by the text   kitty z=0: above text   kitty z=-1: below text\n'
printf 'Row 4 is a row of # written over all three; on row 5, ECH erases 4 cells\n'
printf 'in the middle of each.\n'
