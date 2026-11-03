#!/bin/bash
# look-for: Three places over rows of words. UNDER: a color gradient with every
# look-for: word drawn over it. OVER: the same gradient hiding the words inside
# look-for: it. DELETED: the words alone, no picture at all.
# wrong-looks-like: Words hidden on the left, words through the middle
# wrong-looks-like: picture, any part of a picture at the right, or a picture
# wrong-looks-like: over the labels under them.
# Layers by z-index and a delete, through kitty graphics. Three rows of words
# are written first. One raw RGBA picture is then sent once (a=t, i=5) and
# placed three times with a=p, each with its own placement id (p), 20x6
# cells, the cursor kept (C=1): at the left with z=-1, below text, so the
# words draw over it; in the middle with z=1, above text, so it hides them;
# at the right with z=-1 again. Then a=d with d=i, i=5, p=3 deletes the right
# placement only, which leaves the words there on the plain background.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
rgba=
for ((y = 0; y < 24; y++)); do
	for ((x = 0; x < 32; x++)); do
		printf -v px '\\x%02x\\x%02x\\x%02x\\xff' $((x * 8)) $((y * 10)) $((255 - x * 8))
		rgba+=$px
	done
done
printf '  Left: words over the picture. Middle: the picture hides the words.\n'
printf '  Right: words only, no picture; that placement is deleted on purpose.\n'
row=$((top + 3))
for r in 1 3 5; do
	printf '%s[%d;1H' "$esc" $((row + r))
	for _ in 1 2 3; do
		printf ' words over/under %-6s' "row $r"
	done
done
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=t,i=5,f=32,s=32,v=24,q=2" "$(printf '%b' "$rgba" | base64 -w0)"
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=p,i=5,p=1,c=20,r=6,z=-1,C=1,q=2"
printf '%s[%d;27H' "$esc" "$row"
kitty_send "a=p,i=5,p=2,c=20,r=6,z=1,C=1,q=2"
printf '%s[%d;53H' "$esc" "$row"
kitty_send "a=p,i=5,p=3,c=20,r=6,z=-1,C=1,q=2"
kitty_send "a=d,d=i,i=5,p=3,q=2"
printf '%s[%d;1H%-26s%-26s%s\n' "$esc" $((row + 7)) "UNDER (z=-1)" "OVER (z=1)" "DELETED (p=3): no picture"
