#!/bin/bash
# kit: kitty
# look-for: The screen has scrolled up 3 rows, so the caption and the top two
# look-for: bands of each picture are in the scrollback. At the top left, the
# look-for: lower four bands of two pictures of six bands, yellow, green, blue
# look-for: and violet, one band to a row: a sixel at column 3 and a kitty
# look-for: picture at column 20, each 8 cells wide, with no white frame on
# look-for: top. Beside them, the labels row 4 to row 7, each on its band.
# look-for: Near the bottom, the line that scrolled the screen.
# wrong-looks-like: No pictures (the screen says a protocol is not offered),
# wrong-looks-like: the whole pictures at the top (they did not scroll with
# wrong-looks-like: their lines), the red or orange band on screen, a band out
# wrong-looks-like: of line with its label, or a picture gone.
# screen-text: none, the screen scrolls
# Pictures scrolled partly off the top of the screen (ADR-1100 D8): a
# placement moves with its lines, and lines entering the scrollback carry it,
# so the part above the top row goes with them and the rest stays on the
# screen. Two pictures of six bands, one band to a row at 8x16 cells, made by
# tools/tier3/t3_pictures.py: a sixel and a kitty picture (raw RGBA pixels,
# f=32). The screen draws them at row 2, then prints three newlines on its
# bottom row. Shift+PageUp then shows the whole pictures in the scrollback
# (img-scrollback), and Shift+PageDown returns to this screen
# (img-scrollforward).
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
sixel_offered
kitty_offered
pictures_there
term_size
top_row

printf 'img-scroll: two pictures of six bands, scrolled up by three rows\n'
at 2 3
cat "$pics/bands.six"
at 2 20
kitty_send "a=T,i=50,q=2,C=1,f=32,s=64,v=96" "$(pic64 bands.rgba)"
for r in 2 3 4 5 6 7; do
	at $r 32
	printf 'row %d' $r
done
printf '%s[%d;1H' "$esc" "$rows"
printf 'This line, on the bottom row, is followed by three newlines.\n\n\n'
