#!/bin/bash
# kit: kitty
# look-for: Four pictures of red, green, blue and yellow quarters in a white
# look-for: frame. At row 3, column 5, the picture at its own size, 8 cells by
# look-for: 3 rows; at row 3, column 30, the same, with <cursor just right of
# look-for: its bottom row. At row 8, column 5, its yellow quarter alone, 4
# look-for: cells wide and a row and a half tall, with no frame on its top or
# look-for: left; at row 8, column 30, the whole picture twice its size, 16
# look-for: cells by 6 rows.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), a picture a cell off, cut short or
# wrong-looks-like: stretched, a quarter in the wrong place, or <cursor
# wrong-looks-like: anywhere but right of the second picture.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# The kitty graphics protocol (APC G ... ST) through the pty, as a program
# sends it. a=T transmits a picture and places it at once: raw RGBA pixels
# (f=32) sent inline (direct transmission), in chunks of 4096 base64 bytes
# with m=1 on all but the last, made by tools/tier3/t3_pictures.py. The
# first placement keeps the cursor where it is (C=1); a=p places the same
# image again without C=1, which moves the cursor to the cell after the
# picture on its bottom row. A source rectangle (x, y, w, h) shows part of
# the picture, and c and r scale it into a box of cells. Every command
# carries q=2, so vtium sends no reply into the session.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-kitty: kitty pictures, transmitted directly (a=T, f=32)\n'
at 3 5
kitty_send "a=T,i=10,p=1,q=2,C=1,f=32,s=64,v=48" "$(pic64 quad.rgba)"
at 3 30
kitty_send "a=p,i=10,p=2,q=2"
printf '<cursor'
at 8 5
kitty_send "a=p,i=10,p=3,q=2,C=1,x=32,y=24,w=32,h=24"
at 8 30
kitty_send "a=p,i=10,p=4,q=2,C=1,c=16,r=6"
at 15 1
printf 'Row 3: at its own size, then placed again without C=1.\n'
printf 'Row 8: its bottom right quarter (x=32,y=24,w=32,h=24), then 16x6 cells.\n'
