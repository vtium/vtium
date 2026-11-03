#!/bin/bash
# look-for: Three pictures under rows of blue, plain and green cells. Left, a
# look-for: gradient, dimmer on its right half, the blue and green cells solid
# look-for: over it; middle, a pink disc, the words over it; right, stripes
# look-for: hiding the words. Then a small gradient with <cursor after it.
# wrong-looks-like: A picture over the blue or green cells at the left, words
# wrong-looks-like: hidden by the disc, words over the stripes, or <cursor
# wrong-looks-like: anywhere but just right of the small gradient.
# kitty graphics (APC G ... ST), one picture in each layer vtium draws, all
# sent as raw pixels, which vtium decodes itself without the image helper:
# the left one 32x16 RGBA (f=32) below cell backgrounds (z=-1073741825), its
# right half at half alpha; the middle one a 32x32 RGBA disc, clear outside
# it, below text (z=-1); the right one 32x16 RGB stripes (f=24) above text
# (z=0). Each is scaled into 20x5 cells (c and r) and placed at the cursor
# without moving it (C=1); q=2 keeps every reply out of the page. Then a=p
# places the first image again, 4x2 cells, without C=1, which moves the
# cursor to the cell after the picture on its last row, where <cursor is
# printed.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
# The pixels, each a \xNN escape, then base64.
pixels() { printf '%b' "$1" | base64 -w0; }
gradient='' disc='' stripes=''
for ((y = 0; y < 16; y++)); do
	for ((x = 0; x < 32; x++)); do
		printf -v px '\\x%02x\\x%02x\\x40\\x%02x' $((x * 255 / 31)) $((y * 255 / 15)) $((x < 16 ? 255 : 128))
		gradient+=$px
		if (((x / 4) % 2 == 0)); then stripes+='\xf0\xd2\x1e'; else stripes+='\x28\x50\xe6'; fi
	done
done
for ((y = 0; y < 32; y++)); do
	for ((x = 0; x < 32; x++)); do
		dx=$((2 * x - 31)) dy=$((2 * y - 31))
		if ((dx * dx + dy * dy < 900)); then
			printf -v px '\\xff\\x%02x\\x8c\\xff' $((120 + y * 3))
		else
			px='\x00\x00\x00\x00'
		fi
		disc+=$px
	done
done
printf '  kitty graphics, raw pixels, one picture in each layer:\n'
row=$((top + 2))
words="${esc}[44m blue ${esc}[0m plain ${esc}[42m green ${esc}[0m"
for col in 1 28 55; do
	printf '%s[%d;%dH%s' "$esc" $((row + 1)) "$col" "$words"
	printf '%s[%d;%dH%s' "$esc" $((row + 3)) "$col" "$words"
done
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=T,i=1,q=2,C=1,f=32,s=32,v=16,c=20,r=5,z=-1073741825" "$(pixels "$gradient")"
printf '%s[%d;28H' "$esc" "$row"
kitty_send "a=T,i=2,q=2,C=1,f=32,s=32,v=32,c=20,r=5,z=-1" "$(pixels "$disc")"
printf '%s[%d;55H' "$esc" "$row"
kitty_send "a=T,i=3,q=2,C=1,f=24,s=32,v=16,c=20,r=5,z=0" "$(pixels "$stripes")"
printf '%s[%d;1H%-27s%-27s%s' "$esc" $((row + 5)) "below backgrounds" "below text" "above text"
printf '%s[%d;1Ha=p of image 1, 4x2 cells, moves the cursor: ' "$esc" $((row + 7))
kitty_send "a=p,i=1,q=2,c=4,r=2,z=-1073741825"
printf '<cursor\n\n'
