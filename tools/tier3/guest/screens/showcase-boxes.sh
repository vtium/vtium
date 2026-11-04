#!/bin/bash
# vtium draws box drawing, block elements, Braille and sextants itself,
# edge to edge in every cell, so lines join and bars stack without gaps
# whatever the font: here a dashboard a script draws.
# alt: A console dashboard of five framed panels with rounded and double
# alt: borders: a bar chart of block elements, a sine wave plotted in
# alt: Braille dots, disk gauges, a table of services with colored status
# alt: marks, and a filled area chart drawn in sextants.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
rep() { local s=; for ((i = 0; i < $2; i++)); do s+=$1; done; printf '%s' "$s"; }
# box ROW COL HEIGHT WIDTH TITLE STYLE: a frame, rounded or double
box() {
	local r=$1 c=$2 hh=$3 ww=$4 t=$5 tl tr bl br hz vt y
	if [ "$6" = double ]; then tl=╔ tr=╗ bl=╚ br=╝ hz=═ vt=║; else tl=╭ tr=╮ bl=╰ br=╯ hz=─ vt=│; fi
	at "$r" "$c"; printf '%s%s%s %s%s%s %s%s%s' "$d" "$tl" "$hz" "$h" "$t" "$d" "$(rep "$hz" $((ww - ${#t} - 5)))" "$tr" "$o"
	for ((y = 1; y < hh - 1; y++)); do at $((r + y)) "$c"; printf '%s%s%s' "$d" "$vt" "$o"; at $((r + y)) $((c + ww - 1)); printf '%s%s%s' "$d" "$vt" "$o"; done
	at $((r + hh - 1)) "$c"; printf '%s%s%s%s%s' "$d" "$bl" "$(rep "$hz" $((ww - 2)))" "$br" "$o"
}
lw=$((cols / 2 - 3)) rw=$((cols - lw - 5)) rc=$((lw + 4))
printf '\n'
box 2 3 9 "$lw" 'Load, the last 48 minutes' round
box 2 "$rc" 9 "$rw" 'A sine wave in Braille' round
box 11 3 8 "$lw" 'Disks' double
box 11 "$rc" 8 "$rw" 'Services' double
# Bars of block elements, eighths high, in a gradient from green to red.
blocks=(' ' ▁ ▂ ▃ ▄ ▅ ▆ ▇ █)
vals=(12 18 25 31 22 17 28 40 47 38 30 26 35 44 52 49 41 36 30 27 33 39 45 50 56 51 46 40 34 29 25 22 27 31 36 42 47 53 58 54 49 43 38 33 29 26 24 28)
n=${#vals[@]}
for ((line = 0; line < 6; line++)); do
	at $((9 - line)) 5
	for ((i = 0; i < n && i < lw - 4; i++)); do
		v=${vals[i]} e=$((v * 48 / 60 - line * 8))
		((e < 0)) && e=0; ((e > 8)) && e=8
		printf '%s%s' "$(fg $((60 + v * 3)) $((220 - v * 2)) 90)" "${blocks[e]}"
	done
done
printf '%s' "$o"
# A sine wave in Braille: each cell is 2 dots wide and 4 high.
bw=$((rw - 4)) bh=6
declare -A dots=()
for ((x = 0; x < bw * 2; x++)); do
	y=$(awk -v x="$x" -v w=$((bw * 2)) -v h=$((bh * 4)) 'BEGIN { printf "%d", (h - 1) / 2 * (1 - sin(x / w * 2 * 3.14159265 * 2)) + 0.5 }')
	dots[$((y / 4)),$((x / 2))]=$((${dots[$((y / 4)),$((x / 2))]:-0} | (1 << ((x % 2) * 4 + y % 4))))
done
# Braille dot numbers: left column bits 0,1,2,6, right column 3,4,5,7.
map=(1 2 4 64 8 16 32 128)
for ((r = 0; r < bh; r++)); do
	at $((3 + r)) $((rc + 2))
	printf '%s' "$(fg 90 200 255)"
	for ((c = 0; c < bw; c++)); do
		m=${dots[$r,$c]:-0} u=0
		for ((b = 0; b < 8; b++)); do ((m >> b & 1)) && u=$((u | map[b])); done
		printf "\\u$(printf '%04x' $((0x2800 + u)))"
	done
done
printf '%s' "$o"
# Gauges of whole and partial blocks.
gauge() { # row label percent r g b
	local full=$((($3 * (lw - 22)) / 100)) part=$(((($3 * (lw - 22) * 8) / 100) % 8)) eighths=(' ' ▏ ▎ ▍ ▌ ▋ ▊ ▉)
	at "$1" 5; printf '%-10s %s%s%s%s%s %3d%%' "$2" "$(fg "$4" "$5" "$6")" "$(rep █ "$full")" "${eighths[part]}" "$(fg 60 60 60)$(rep ░ $((lw - 22 - full - 1)))" "$o" "$3"
}
gauge 13 '/' 64 120 200 120
gauge 14 '/home' 82 230 190 80
gauge 15 '/var' 37 90 170 255
gauge 16 '/boot' 18 200 120 255
# A table of services with status marks.
at 13 $((rc + 2)); printf '%s%-14s %-10s %8s %s%s' "$d" 'Unit' 'State' 'Memory' 'Uptime' "$o"
svc() { at "$1" $((rc + 2)); printf '%s%s%s %-12s %-10s %8s %s' "$(fg "$5" "$6" "$7")" "$2" "$o" "$3" "$4" "$8" "$9"; }
svc 14 ● vtium@tty2 active 120 220 120 '9.8 MiB' '3 days'
svc 15 ● vtium@tty3 active 120 220 120 '9.1 MiB' '3 days'
svc 16 ● backup waiting 230 190 80 - -
svc 17 ● mirror-sync failed 235 90 90 - -
# An area chart in sextants, each cell 2 dots wide and 3 high: the chart's
# height in dots for each column of dots, from one awk run, then each cell's
# six dots as one of the 60 sextant characters from U+1FB00, or a space, a
# full block, or the left or right half block, which Unicode gives the
# sextant patterns of those shapes.
box 19 3 7 $((cols - 4)) 'Memory, drawn in sextants' round
sw=$((cols - 8)) sh=5
read -ra hts < <(awk -v n=$((sw * 2)) -v m=$((sh * 3)) 'BEGIN {
	for (x = 0; x < n; x++)
		printf "%d ", int(m * (0.45 + 0.25 * sin(x / n * 9.4) + 0.15 * sin(x / n * 31 + 1)) + 0.5)
}')
for ((r = 0; r < sh; r++)); do
	at $((20 + r)) 5
	printf '%s' "$(fg 120 170 255)"
	for ((c = 0; c < sw; c++)); do
		p=0
		for ((sy = 0; sy < 3; sy++)); do
			lvl=$(((sh - r) * 3 - sy))
			((hts[2 * c] >= lvl)) && p=$((p | 1 << (2 * sy)))
			((hts[2 * c + 1] >= lvl)) && p=$((p | 2 << (2 * sy)))
		done
		case $p in
		0) printf ' ' ;;
		63) printf '█' ;;
		21) printf '▌' ;;
		42) printf '▐' ;;
		*) printf "\\U$(printf '%08x' $((0x1FB00 + p - 1 - (p > 21) - (p > 42))))" ;;
		esac
	done
done
printf '%s' "$o"
at 26 1
