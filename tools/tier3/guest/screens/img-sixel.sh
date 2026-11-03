#!/bin/bash
# kit: sixel
# look-for: Under the caption, at row 3 and column 5, a picture 8 cells wide
# look-for: and 3 rows tall in a white frame: red top left, green top right,
# look-for: blue bottom left, yellow bottom right. To its right, an orange
# look-for: ring around a blue disc, 6 cells by 3, drawn over three rows of
# look-for: dots, which show around the disc and between it and its corners.
# look-for: Under them, the row and column the cursor came to rest at after
# look-for: the first picture.
# wrong-looks-like: No picture (the screen says sixel is not offered), a
# wrong-looks-like: picture a cell off its place or cut short, the quarters in
# wrong-looks-like: another order, or a black box around the disc that hides
# wrong-looks-like: the dots.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# A sixel picture (DCS q ... ST) sent through the pty as a program sends it,
# made by tools/tier3/t3_pictures.py: 64x48 pixels, opaque, which is 8 cells
# by 3 rows at the variant's 8x16 cells. The cursor's place after it, which
# DSR 6 reports, is printed below the pictures, since a character written on
# a cell the picture covers would cut that cell out of it. Then a disc of
# 48x48 pixels whose corners are transparent (P2=1), drawn over text already
# on the screen.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
sixel_offered
pictures_there
top_row

printf 'img-sixel: a sixel picture, 64x48 pixels, at row 3, column 5\n'
for r in 3 4 5; do
	at $r 40
	printf '. . . . . . . . . . . .'
done
at 3 5
cat "$pics/quad.six"
cursor=$(ask "${esc}[6n" R)
if [[ ${cursor##*[} =~ ^([0-9]+)\;([0-9]+)$ ]]; then
	row=$((BASH_REMATCH[1] - top + 1)) col=${BASH_REMATCH[2]}
else
	row='?' col='?'
fi
at 3 44
cat "$pics/disc.six"
at 8 1
printf 'After the first picture the cursor was at row %s, column %s.\n' "$row" "$col"
printf 'The disc is drawn over the dots, which show where it is transparent.\n'
