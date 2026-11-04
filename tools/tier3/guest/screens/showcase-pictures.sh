#!/bin/bash
# vtium shows pictures in the console as desktop terminals do: on the left
# a sixel picture of the Mandelbrot set in 201 colors, on the right a kitty
# graphics picture of a lit sphere in 24-bit color.
# tools/tier3/t3_showcase.py computes both from a few lines of arithmetic.
# alt: A console screen with two pictures side by side under short
# alt: captions: the Mandelbrot set, black, edged in white, gold and sky
# alt: blue on deep blue, and an orange sphere lit from the upper left over
# alt: a background that fades from violet to teal.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
pics=$showcase
if [ ! -f "$pics/mandelbrot.six" ]; then
	printf 'showcase-pictures: no pictures in %s; tools/tier3/t3_showcase.py writes them.\n' "$pics"
	[ -n "${T3_EVENTS:-}" ] && echo T3-NO-PICTURES >"$T3_EVENTS"
	exit 0
fi
read -r sw sh <"$pics/sphere.size"
left=5 right=$((cols / 2 + 6))
at 2 3; printf '%sPictures%s' "$h" "$o"
at 4 "$left"; printf '%sSixel: the Mandelbrot set, 800x600 pixels%s' "$d" "$o"
at 4 "$right"; printf '%sKitty graphics: a lit sphere, 800x600 pixels%s' "$d" "$o"
at 5 "$left"
cat "$pics/mandelbrot.six"
at 5 "$right"
kitty "a=T,f=32,s=$sw,v=$sh,o=z,i=7,q=2,C=1" "$(base64 -w0 "$pics/sphere.rgba.z")"
at 21 3
printf '%sA program draws pictures with sixel, the kitty graphics protocol or iTerm2%ss inline images,%s\n' \
	"$d" "'" "$o"
printf '  %sand they scroll with the text around them.%s\n' "$d" "$o"
