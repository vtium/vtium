#!/bin/bash
# kit: kitty
# look-for: Two kitty pictures drawn through Unicode placeholder cells. At row
# look-for: 3, column 35, red, green, blue and yellow quarters in a white
# look-for: frame, 8 cells by 3 rows, and nothing at column 5, where its cells
# look-for: were written before they moved. At row 9, column 5, cyan and
# look-for: magenta quarters in a white frame, 4 cells by 2 rows.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), a picture left at column 5 or missing at
# wrong-looks-like: column 35, a row of the picture repeated or out of order,
# wrong-looks-like: a glyph or a colored box where a picture should be, or no
# wrong-looks-like: picture at row 9.
# screen-text: none, placeholder cells and cursor moves do not print as rows
# Kitty's Unicode placeholders (ADR-1100 D13), written from the kitty spec as
# a program such as icat --unicode-placeholder sends them: a=p with U=1 makes
# a virtual placement, which has a size in cells and no position, and the
# program then writes the character U+10EEEE into the cells the picture should
# cover, as text. The cell's foreground color names the image, its underline
# color the placement, and its combining diacritics the picture's row, its
# column and the top byte of the image id; a cell that leaves them out takes
# them from the placeholder to its left.
#
# The first picture is the quarters (image 60, named by the palette color
# 60), placed virtually at 8 by 3 cells. Its first row gives the row and
# column on its first cell alone, its second row gives the row on every
# cell, and its third gives both on every cell. The cells are written at
# column 5 and then moved 30 columns right by inserting blanks before them
# (ICH), as a program such as tmux moves a pane's text when it redraws it:
# the picture follows its cells.
#
# The second is the child under image id 16909060, 0x01020304, too large for
# a palette color: the direct color 2;3;4 gives its lower three bytes and a
# third diacritic its top byte, 1. Its virtual placement has the placement
# id 7, which its cells name with the underline color 7 (SGR 58).
#
# Both virtual placements ask for their replies, which the screen reports
# on the event port, the second's naming its placement (i=16909060,p=7;OK);
# the transmissions carry q=2.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-placeholder: kitty pictures drawn through Unicode placeholder cells\n'
kitty_send "a=t,i=60,q=2,f=32,s=64,v=48" "$(pic64 quad.rgba)"
ask_event "a=p,U=1,i=60,c=8,r=3"
kitty_send "a=t,i=16909060,q=2,f=32,s=32,v=32" "$(pic64 child.rgba)"
ask_event "a=p,U=1,i=16909060,p=7,c=4,r=2"

printf '%s[38;5;60m' "$esc"
at 3 5
placeholder 0 0
for c in 1 2 3 4 5 6 7; do placeholder; done
at 4 5
for c in 0 1 2 3 4 5 6 7; do placeholder 1; done
at 5 5
for c in 0 1 2 3 4 5 6 7; do placeholder 2 "$c"; done
printf '%s[m' "$esc"
for r in 3 4 5; do
	at "$r" 1
	printf '%s[30@' "$esc"
done

printf '%s[38;2;2;3;4;58;5;7m' "$esc"
for r in 0 1; do
	at $((9 + r)) 5
	for c in 0 1 2 3; do placeholder "$r" "$c" 1; done
done
printf '%s[m' "$esc"

at 7 1
printf 'Rows 3 to 5: image 60, its cells written at column 5, then moved to 35.\n'
at 12 1
printf 'Rows 9 and 10: image 16909060, placement 7, its top byte a diacritic.\n'
