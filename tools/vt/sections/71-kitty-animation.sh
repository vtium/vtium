#!/bin/bash
# look-for: Left, a square that turns red, green and blue, half a second each,
# look-for: for as long as the page shows. Right, a square that goes red,
# look-for: green, blue twice, then stays blue. r draws the page again and
# look-for: starts both again.
# wrong-looks-like: A square that stays one color, skips a color, shows black
# wrong-looks-like: between colors, or a right square that never stops or
# wrong-looks-like: stops on another color.
# kitty animation (ADR-1000's image.kitty-animation), sent as raw pixels,
# which vtium decodes itself without the image helper. Each image is a 16x16
# RGB square (f=24) placed into 10x5 cells at the cursor without moving it
# (C=1): its root frame red, then two frames, green and blue, added with
# a=f and shown for 500 ms each (z=500), the root frame's gap set to 500 ms
# with a=a,r=1. Then a=a,s=3 runs each: v=1 loops for ever, and v=3 plays
# the frames through twice and stops on the last, as kitty's protocol counts
# loops. q=2 keeps every reply out of the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
# A 16x16 square of one color, R G B as \xNN escapes, in base64.
square() {
	local px='' i
	for ((i = 0; i < 256; i++)); do px+=$1; done
	printf '%b' "$px" | base64 -w0
}
red=$(square '\xe0\x20\x20') green=$(square '\x20\xc0\x40') blue=$(square '\x30\x50\xf0')
printf '  kitty animation, raw pixels:\n'
row=$((top + 2))
for spot in "21 3" "22 30"; do
	read -r id col <<<"$spot"
	printf '%s[%d;%dH' "$esc" "$row" "$col"
	kitty_send "a=T,i=$id,q=2,C=1,f=24,s=16,v=16,c=10,r=5" "$red"
	kitty_send "a=f,i=$id,q=2,f=24,s=16,v=16,z=500" "$green"
	kitty_send "a=f,i=$id,q=2,f=24,s=16,v=16,z=500" "$blue"
	kitty_send "a=a,i=$id,q=2,r=1,z=500"
done
kitty_send "a=a,i=21,q=2,s=3,v=1"
kitty_send "a=a,i=22,q=2,s=3,v=3"
printf '%s[%d;1H  %-27s%s\n\n' "$esc" $((row + 6)) "loops for ever" "plays twice, then stays blue"
