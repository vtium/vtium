#!/bin/bash
# kit: kitty
# look-for: The EXPECTED lines, then three places across over three rows of
# look-for: text. UNDER: the street at sunset with every word drawn over it.
# look-for: OVER: the same picture hiding the words inside it. DELETED: the
# look-for: words alone on black, no picture, deleted on purpose (a=d).
# wrong-looks-like: Words hidden on the left, words through the middle
# wrong-looks-like: picture, any pixel of the picture at the right, or a
# wrong-looks-like: picture over the labels under them.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# Composition by z-index and a delete, through the kitty graphics protocol
# (ADR-103 step 4c). Three rows of text are written first across the page.
# The photograph's PNG is then transmitted once (a=t, i=5) and placed three
# times with a=p, each with its own placement id (p), 20x6 cells, the cursor
# kept (C=1): at the left with z=-1, below text, so the words draw over it;
# in the middle with z=1, above text, so it hides them; and at the right with
# z=-1 again. Then a=d with d=i, i=5, p=3 deletes the right placement only:
# its cells must show the words on black, with no pixel of the picture left.
# The page says what to expect above the pictures and labels each place
# UNDER, OVER or DELETED below them. Every command carries q=2, so the
# terminal sends no reply.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/kitty-common.inc"
kitty_offered
top_row
printf 'EXPECTED: two pictures.\n'
printf 'Left: words drawn over the photo. Middle: the photo hides the words.\n'
printf 'Right: words only, no picture; that placement is deleted on purpose.\n'
row=$((top + 3))
for r in 1 3 5; do
	printf '%s[%d;1H' "$esc" $((row + r))
	for _ in 1 2 3; do
		printf ' words over/under %-6s' "row $r"
	done
done
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=t,i=5,f=100,q=2" "$(base64 -w0 "$photo_png")"
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=p,i=5,p=1,c=20,r=6,z=-1,C=1,q=2"
printf '%s[%d;27H' "$esc" "$row"
kitty_send "a=p,i=5,p=2,c=20,r=6,z=1,C=1,q=2"
printf '%s[%d;53H' "$esc" "$row"
kitty_send "a=p,i=5,p=3,c=20,r=6,z=-1,C=1,q=2"
kitty_send "a=d,d=i,i=5,p=3,q=2"
printf '%s[%d;1H%-26s%-26s%s\n' "$esc" $((row + 7)) "UNDER (z=-1)" \
	"OVER (z=1)" "DELETED (p=3): no picture"
