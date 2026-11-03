#!/bin/bash
# look-for: Left, four colored squares: red top left, green top right, blue
# look-for: bottom left, yellow bottom right. Middle, a pink disc with the words
# look-for: over it. Right, yellow and blue stripes hiding the words.
# wrong-looks-like: A square in the wrong corner, a missing or broken picture,
# wrong-looks-like: words hidden by the disc, or words over the stripes.
# kitty graphics sent as PNG (f=100) and as zlib-compressed raw pixels (o=z),
# which vtium hands to its image helper to decode: a PNG of four colored
# squares, 64x32, above text; a PNG disc, 32x32, clear outside it, below text
# (z=-1); and 32x16 RGB stripes compressed with zlib, above text (z=0). Each
# is scaled into 20x5 cells and placed at the cursor without moving it
# (C=1); q=2 keeps every reply out of the page. The pictures are written
# here as base64; python3's zlib and struct made them, from the colors the
# guidance names.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
helper_decodes || cannot_show "the image helper did not decode a PNG; vtium runs without one."
top_row
quads='iVBORw0KGgoAAAANSUhEUgAAAEAAAAAgCAIAAAAt/+nTAAAASElEQVR42u3PMQ0AIAwAMJTsRgQiEIsSbm5uJOBhyb4mNdB2IkrFGqWagICAgICAgICAgEA60Oct9XaUEhAQEBAQEBAQEBBI+1bKyJcPtOjvAAAAAElFTkSuQmCC'
disc='iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAA4klEQVR42tWXUQcCURCF939HRESKiIiIiIiIiIiIiIj+0Pa9jbXr7u7dOzN2+V7OzJ1znu7OzbK+fflglofQMR0yPII05iOGdSDeeMyAhLQzn3BIgWbmU5oVqQ8wp1GRsPmCJgOqzZcUDSkHWFEwpBxgTcGQovkG0QEJsEVwQALsEByQAHsEByTAAcEBCXBEcEACnBAckABnBAeKd8EF0ZDyTXilYEg5wI2CIdV/xDtFA8I7wYMmReo3oieNijTbC180K9BuM35zKCHx74MPAzqQ5nX0ZVgEOu/EH8MD9O61/QduB+wfW8UumQAAAABJRU5ErkJggg=='
stripes='eNr7cEnuAwxpBDyDow+j4qPio+Kj4qPiw1ocAFcxPi4='
printf '  kitty graphics decoded by the image helper: PNG and zlib:\n'
row=$((top + 2))
words="${esc}[44m blue ${esc}[0m plain ${esc}[42m green ${esc}[0m"
for col in 28 55; do
	printf '%s[%d;%dH%s' "$esc" $((row + 1)) "$col" "$words"
	printf '%s[%d;%dH%s' "$esc" $((row + 3)) "$col" "$words"
done
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=T,i=7,q=2,C=1,f=100,c=20,r=5" "$quads"
printf '%s[%d;28H' "$esc" "$row"
kitty_send "a=T,i=8,q=2,C=1,f=100,c=20,r=5,z=-1" "$disc"
printf '%s[%d;55H' "$esc" "$row"
kitty_send "a=T,i=9,q=2,C=1,f=24,s=32,v=16,o=z,c=20,r=5,z=0" "$stripes"
printf '%s[%d;1H%-27s%-27s%s\n' "$esc" $((row + 5)) "PNG, four squares" "PNG, below text" "zlib, above text"
