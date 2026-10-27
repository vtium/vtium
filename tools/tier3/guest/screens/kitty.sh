#!/bin/bash
# kit: kitty
# look-for: Three pictures in a row, each 20 cells wide under a row of blue,
# look-for: plain and green cells. Left, a gradient, its right half dimmed: the
# look-for: blue and green cells solid over it. Middle, an orange-pink disc
# look-for: over the backgrounds, the words over it. Right, yellow and blue
# look-for: stripes hiding the words. Then the gradient again, 4 by 2, with
# look-for: <cursor just past it on its lower row.
# wrong-looks-like: No pictures (stock kmscon has no kitty graphics; the page
# wrong-looks-like: says so), the middle picture missing (the image helper did
# wrong-looks-like: not decode), a picture over the blue or green cells at the
# wrong-looks-like: left, the words hidden in the middle, words over the stripes,
# wrong-looks-like: or <cursor anywhere but right of the small gradient.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# The kitty graphics protocol (APC G ... ST, ADR-103 step 4c) through the
# pty, as a program sends it, one picture in each composition band and each
# of the three inline media: the left one 32x16 straight RGBA (f=32) made
# here and decoded by libtsm itself, in the band below cell backgrounds
# (z=-1073741825), its right half at half alpha; the middle one a PNG
# (f=100) of a disc, clear outside it, which kmscon's image helper decodes,
# in the band below text (z=-1); the right one RGB stripes (f=24) sent zlib
# compressed (o=z), which the helper inflates, above text (z=0). Each is
# scaled into 20x5 cells (c and r), so the page looks the same at any font
# size, and placed at the cursor without moving it (C=1). Every command
# carries q=2, so the terminal sends no reply into the session. Then a=p
# places the first image again, 4x2 cells, without C=1: the cursor moves to
# the cell after the picture on its last row, where <cursor is printed.
# Inline data only: before login, and in the VT demo's session, which is in
# no logind session of its own, kmscon reads no file or shared memory for a
# program (docs/image-helper.md), and inline PNG and zlib decode as the
# pre-login user. The page asks first, with the spec's query of one pixel
# followed by DA1, and says so when the terminal does not answer it.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/kitty-common.inc"
kitty_offered
top_row

# 32x16 RGBA, red across, green down, blue 64; the right half at alpha 128
rgba=
for ((y = 0; y < 16; y++)); do
	for ((x = 0; x < 32; x++)); do
		printf -v px '\\x%02x\\x%02x\\x40\\x%02x' $((x * 255 / 31)) $((y * 255 / 15)) \
			$((x < 16 ? 255 : 128))
		rgba+=$px
	done
done
rgba=$(printf '%b' "$rgba" | base64 -w0)
png='iVBORw0KGgoAAAANSUhEUgAAACAAAAAQCAYAAAB3AH1ZAAAA6klEQVR42u2VsWoCQRRF74EpNlZx
K7ES/YS1EVvtUkn0E8y/xTZd7NPET0iwCqnUKqS7qTIbiZER3LXJhYEZmPfuGRjeRYnyRFYmxXX1
Y3/gzK1I6RuOmg6ajs1PlB/kbyiGW5IB3OnGwnPJq/IhdF/5E8CtwqpYfitMe8UvAF+PKjePXpuR
yZdEAGfT2swjxMfUNBYEa167eYTw3EFq6pL6BwjoDuv+Iv8AZgRJ4nOGs8daIWiM2ZsD7Ma49VwL
BHmfg5OQ9z7uvFQKQbvH0SxgXV7wYHMWGIo8PYz2Cp/KQk98Egw3JMXxF5FnQiUjfUXCAAAAAElF
TkSuQmCC'
png=${png//$'\n'/}
stripes='eNr7dULjFwxpRN2Bo1+j4qPio+Kj4qPiw1ocAKPzSC4='

printf 'kitty graphics: inline RGBA, PNG and zlib, one in each band\n'
row=$((top + 2))
words="${esc}[44m blue ${esc}[0m plain ${esc}[42m green ${esc}[0m"
for col in 1 28 55; do
	printf '%s[%d;%dH%s' "$esc" $((row + 1)) "$col" "$words"
	printf '%s[%d;%dH%s' "$esc" $((row + 3)) "$col" "$words"
done
printf '%s[%d;1H' "$esc" "$row"
kitty_send "a=T,i=1,q=2,C=1,f=32,s=32,v=16,c=20,r=5,z=-1073741825" "$rgba"
printf '%s[%d;28H' "$esc" "$row"
kitty_send "a=T,i=2,q=2,C=1,f=100,c=20,r=5,z=-1" "$png"
printf '%s[%d;55H' "$esc" "$row"
kitty_send "a=T,i=3,q=2,C=1,f=24,s=32,v=16,o=z,c=20,r=5,z=0" "$stripes"
printf '%s[%d;1H%-27s%-27s%s' "$esc" $((row + 5)) "RGBA, below backgrounds" \
	"PNG (helper), below text" "zlib f=24, above text"
printf '%s[%d;1Ha=p of image 1, 4x2, moves the cursor: ' "$esc" $((row + 7))
printf '%s_Ga=p,i=1,q=2,c=4,r=2,z=-1073741825;%s\\' "$esc" "$esc"
printf '<cursor\n\n'
