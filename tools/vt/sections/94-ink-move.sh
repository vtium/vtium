#!/bin/bash
# look-for: After every step: no leftover accent, picture sliver or half letter
# look-for: where something stood; ink under the sixels, over the kitty
# look-for: pictures below text, hidden by the one above; accents whole beside
# look-for: them. Bitmap: whole letters, nothing past a cell.
# wrong-looks-like: Pixel poop: an accent, a sliver of picture or a half-erased
# wrong-looks-like: letter left behind by a move, an accent cut where a picture
# wrong-looks-like: moved off, or ink on the wrong side of a picture.
# frits: acceptance-outline acceptance-bitmap
# Glyph ink next to pictures, then moved (ADR-1306): kitty pictures below the
# backgrounds (z=-1073741825), below the text (z=-1) and above it (z=1), and
# two sixels, each with accented capitals, italics, Arabic marks and
# descenders in the cells on and around it. In a walk, each key moves the
# pictures and the text, 6 times: a kitty placement is placed again at its new
# cell, a sixel is drawn at its new cell, and the old text and the old sixel
# cells are overwritten with spaces. The page reads those keys itself from
# the terminal, which accept.sh leaves to it until the page ends; with no
# terminal or in a check, only the first step is drawn. The text is written
# as UTF-8 bytes so the output does not depend on the locale.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
pictures_shown || cannot_show "vtium was built without pictures: its DA1 answer has no 4."
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
cell_size
((cw > 0)) || { cw=10 ch=20; }
reset=$'\033[0m'
caps=$'\xc3\x81\xc3\x89\xc3\x8e\xc3\x95\xc3\x9c \xe1\xba\xaa\xe1\xba\xb6\xe1\xbb\x82' # ÁÉÎÕÜ ẪẶỂ
ital=$'\033[3mH I E W m A O f\033[0m'
arab=$'\xd9\x84\xd9\x91\xd9\x8e \xd8\xa8\xd9\x90 \xd8\xa7\xd9\x92 \xd9\x84\xd9\x91\xd9\x8e' # lam, beh, alef, lam with marks
desc='gjpqy gjpqy gjpqy'
steps=6
dxs=(0 1 1 2 2 3 3)
dys=(0 0 1 1 0 0 1)

# pixels as a \xNN list, then base64
pixels() { printf '%b' "$1" | base64 -w0; }
grad='' disc='' bars=''
for ((y = 0; y < 16; y++)); do
	for ((x = 0; x < 32; x++)); do
		printf -v px '\\x%02x\\x%02x\\x40\\x%02x' $((x * 255 / 31)) $((y * 255 / 15)) $((x < 16 ? 255 : 160))
		grad+=$px
		if (((x / 4) % 2 == 0)); then bars+='\xf0\xd2\x1e'; else bars+='\x28\x50\xe6'; fi
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

at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
six() { printf "\\x$(printf '%02x' $((63 + $1)))"; }
# a W by H sixel in R G B percent, T pixels thick and clear inside when T
# is not 0, else solid
sixel() {
	local w=$1 h=$2 t=$3 y j all mid
	printf '%sP0;1;0q"1;1;%d;%d#1;2;%d;%d;%d' "$esc" "$w" "$h" "$4" "$5" "$6"
	for ((y = 0; y < h; y += 6)); do
		all=0 mid=0
		for ((j = 0; j < 6 && y + j < h; j++)); do
			all=$((all | 1 << j))
			((t > 0 && y + j >= t && y + j < h - t)) || mid=$((mid | 1 << j))
		done
		if ((t > 0)); then
			printf '#1!%d%s!%d%s!%d%s-' "$t" "$(six "$all")" $((w - 2 * t)) "$(six "$mid")" "$t" "$(six "$all")"
		else
			printf '#1!%d%s-' "$w" "$(six "$all")"
		fi
	done
	printf '%s\\' "$esc"
}
blank() { printf '%*s' "$1" ''; }

p=$((top + 2)) s=$((top + 9))
kcols=(2 28 54) # the three kitty pictures, 20 cells wide and 4 tall
pdx=0 pdy=0
# One step: the old text and sixel cells are overwritten, the three
# kitty pictures are placed at the new cells, the text and the sixels drawn.
draw_step() {
	local k=$1 dx=${dxs[$1]} dy=${dys[$1]} c i
	printf '%s[0m' "$esc"
	at "$top" 1
	if ((k == steps)); then
		printf '  Step %d of %d: done, the page stops moving.%s[K' "$k" "$steps" "$esc"
	else
		printf '  Step %d of %d: press a key to move the pictures and the text.%s[K' "$k" "$steps" "$esc"
	fi
	# erase what the last step drew: its text rows and its sixel cells
	for c in "${kcols[@]}"; do
		for i in -1 1 2 3 4; do at $((p + pdy + i)) $((c + pdx)); blank 22; done
	done
	for i in -1 0 1 2 3; do at $((s + pdy + i)) $((2 + pdx)); blank 70; done
	# the kitty pictures: below the backgrounds, below the text, above it
	at $((p + dy)) $((kcols[0] + dx))
	kitty_send "a=p,i=1,p=1,q=2,C=1,c=20,r=4,z=-1073741825"
	at $((p + dy)) $((kcols[1] + dx))
	kitty_send "a=p,i=2,p=1,q=2,C=1,c=20,r=4,z=-1"
	at $((p + dy)) $((kcols[2] + dx))
	kitty_send "a=p,i=3,p=1,q=2,C=1,c=20,r=4,z=1"
	for c in "${kcols[@]}"; do
		at $((p + dy - 1)) $((c + dx + 1)); printf '%s' "$desc"
		at $((p + dy + 1)) $((c + dx + 3)); printf '%s[44;97m%s%s' "$esc" "$caps" "$reset"
		at $((p + dy + 2)) $((c + dx + 3)); printf '%s' "$ital"
		# The Arabic starts 1 cell right of the other rows and reads right to
		# left, so each of its letters stands under a space of the italic row
		# and beh stands over the space in the capitals row: its marks reach
		# into empty cells.
		at $((p + dy + 3)) $((c + dx + 4)); printf '%s' "$arab"
		at $((p + dy + 4)) $((c + dx + 3)); printf '%s' "$caps"
	done
	# two sixels, a frame with a clear middle and a solid bar, with text
	# above, in the frame's middle and below
	at $((s + dy - 1)) $((3 + dx)); printf '%s   %s' "$desc" "$desc"
	at $((s + dy + 1)) $((4 + dx)); printf ' %s %s' "$caps" "$desc"
	at $((s + dy + 3)) $((3 + dx)); printf '%s   %s' "$caps" "$caps"
	at $((s + dy)) $((2 + dx))
	sixel $((30 * cw)) $((3 * ch)) $((ch / 3)) 100 60 0
	at $((s + dy)) $((41 + dx))
	sixel $((12 * cw)) $((3 * ch)) 0 0 60 100
	pdx=$dx pdy=$dy
}

# each picture sent once, then placed again at each step
kitty_send "a=t,i=1,q=2,f=32,s=32,v=16" "$(pixels "$grad")"
kitty_send "a=t,i=2,q=2,f=32,s=32,v=32" "$(pixels "$disc")"
kitty_send "a=t,i=3,q=2,f=24,s=32,v=16" "$(pixels "$bars")"
printf '%s[?25l' "$esc"
printf '%s[%d;1H' "$esc" "$top"
for ((k = 0; k <= steps; k++)); do
	draw_step "$k"
	((k == steps)) && break
	if [ -t 0 ] && [ -z "${ACCEPT_NOPROMPT:-}${ACCEPT_NOPAGE:-}" ]; then
		IFS= read -rsn1 </dev/tty || break
		while IFS= read -rsn1 -t 0.02 _ </dev/tty; do :; done
	else
		break
	fi
done
at $((s + 5)) 1
printf '\n'
