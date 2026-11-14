#!/bin/bash
# kit: kitty
# look-for: Of four pictures placed in a row at row 3, only the first is
# look-for: left, at column 3, with its label above it. The second, deleted
# look-for: by its id with its data kept, shows again at row 9, column 13.
# look-for: Under the labels of the third and fourth, row 3 is blank. Row 13
# look-for: gives the reply to placing the third again after its data was
# look-for: freed, i=33;ENOENT and its message, which wraps onto row 14, and
# look-for: row 15 the reply to placing the second, i=32;OK.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), any picture left on row 3 but the first,
# wrong-looks-like: no picture at row 9, or other replies.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# Kitty's delete command (a=d) on four placements of four images, ids 31 to
# 34, raw RGBA pixels made by tools/tier3/t3_pictures.py. d=i deletes the
# placements of image 32 and keeps its data, so a=p places it again; d=I
# deletes those of image 33 and frees its data, so placing it again is
# refused with ENOENT; d=p deletes the placements that cover a cell, here
# image 34's. The two placements after the deletes ask for their replies (no
# q), which the screen prints and reports on the event port.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-delete: kitty pictures deleted with a=d\n'
quad=$(pic64 quad.rgba)
labels=('kept' 'd=i' 'd=I' 'd=p')
for n in 0 1 2 3; do
	at 2 $((3 + 10 * n))
	printf '%s' "${labels[n]}"
	at 3 $((3 + 10 * n))
	kitty_send "a=T,i=$((31 + n)),q=2,C=1,f=32,s=64,v=48" "$quad"
done
kitty_send "a=d,d=i,i=32,q=2"
kitty_send "a=d,d=I,i=33,q=2"
kitty_send "a=d,d=p,x=$((33 + 4)),y=$((top + 3)),q=2"
at 8 13
printf 'image 32 placed again'
at 9 13
kitty_ask "a=p,i=32,C=1"
second=$reply
reply_event "$reply"
at 13 1
kitty_ask "a=p,i=33,C=1"
reply_event "$reply"
printf 'image 33 placed again: %s\n' "$reply"
printf 'image 32 placed again: %s\n' "$second"
