#!/bin/bash
# vtium draws every color a program can ask for: the 256 colors of xterm's
# palette, the 16 colors of the console's palette, and 24-bit color, here as
# smooth gradients. Below, vtium's named palettes, each drawn as its 16
# colors; vtium.conf's palette key chooses one.
# alt: A console screen of color swatches: the 6 by 6 by 6 color cube and
# alt: the gray ramp of the 256-color palette, the 16 numbered standard
# alt: colors, smooth gradients through every hue and from black to white,
# alt: and nine rows of 16 swatches, one row for each of vtium's named
# alt: palettes.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
idx() { printf '%s[48;5;%dm' "$esc" "$1"; }
at 2 3; printf '%sColor%s' "$h" "$o"
# The 256-color palette: the color cube as six squares of 6 by 6, then the
# gray ramp.
at 4 3; printf '%s256 colors%s' "$d" "$o"
for g in 0 1 2 3 4 5; do
	at $((5 + g)) 3
	for r in 0 1 2 3 4 5; do
		for b in 0 1 2 3 4 5; do
			printf '%s  ' "$(idx $((16 + 36 * r + 6 * g + b)))"
		done
		printf '%s ' "$o"
	done
done
at 11 3
for n in $(seq 232 255); do printf '%s   ' "$(idx "$n")"; done
printf '%s' "$o"
# The 16 colors, with their numbers, beside the cube.
at 4 86; printf '%s16 colors%s' "$d" "$o"
for r in 0 1; do
	at $((5 + r)) 86
	for c in 0 1 2 3 4 5 6 7; do
		n=$((r * 8 + c))
		ink=15
		((n == 7 || n >= 9)) && ink=0
		printf '%s[38;5;%dm%s%3d %s' "$esc" "$ink" "$(idx $n)" "$n" "$o"
	done
done
# 24-bit color: two gradients to a row, the upper half block's foreground
# above its background.
at 13 3; printf '%s24-bit color%s' "$d" "$o"
w=$((cols - 4))
# The full-strength hue at $1 of 0 to 1529, as r g b.
hue() {
	local x=$1 s f
	s=$((x / 255)) f=$((x % 255))
	case $s in
	0) echo 255 $f 0 ;;
	1) echo $((255 - f)) 255 0 ;;
	2) echo 0 255 $f ;;
	3) echo 0 $((255 - f)) 255 ;;
	4) echo $f 0 255 ;;
	*) echo 255 0 $((255 - f)) ;;
	esac
}
full() { hue $(($1 * 1529 / (w - 1))); }
dark() {
	local r g b
	read -r r g b < <(full "$1")
	echo $((r * 45 / 100)) $((g * 45 / 100)) $((b * 45 / 100))
}
gray() { local v=$(($1 * 255 / (w - 1))); echo $v $v $v; }
warm() { local t=$(($1 * 255 / (w - 1))); echo 255 $t $((t / 3)); }
cool() { local t=$(($1 * 255 / (w - 1))); echo $((t / 4)) $((t * 3 / 4)) 255; }
violet() { local t=$(($1 * 255 / (w - 1))); echo $((120 + t / 2)) $((40 + t * 3 / 4)) $((200 - t / 3)); }
ramp() {
	local x r1 g1 b1 r2 g2 b2
	at "$1" 3
	for ((x = 0; x < w; x++)); do
		read -r r1 g1 b1 < <($2 "$x")
		read -r r2 g2 b2 < <($3 "$x")
		printf '%s%s▀' "$(fg "$r1" "$g1" "$b1")" "$(bg "$r2" "$g2" "$b2")"
	done
	printf '%s' "$o"
}
ramp 14 full dark
ramp 15 gray warm
ramp 16 cool violet
# vtium's named palettes, from vtium-term's table (palettes.txt), two to a
# row.
at 18 3; printf '%sThe palettes vtium names, each its 16 colors%s' "$d" "$o"
row=19 col=3
while read -r name colors; do
	at "$row" "$col"
	printf '%-16s' "$name"
	for c in $colors; do printf '%s[48;2;%sm  ' "$esc" "$c"; done
	printf '%s' "$o"
	if ((col == 3)); then col=$((cols / 2 + 2)); else col=3 row=$((row + 1)); fi
done <"$showcase/palettes.txt"
((col == 3)) || row=$((row + 1))
at $((row + 1)) 1
