#!/bin/bash
# kit: sixel
# look-for: Left: bars red, yellow, green, cyan, blue, magenta, white and grey
# look-for: over a checkerboard, TEXT across the checkerboard as text. Right: a
# look-for: blue disc in an orange ring, the text behind it showing past its edge.
# look-for: Below: seven discs, red, green, blue, cyan, magenta, yellow and
# look-for: orange, each with a square of text cells in its colour to its right
# look-for: and its name under both; then the same pairs as squares on mid-grey.
# look-for: Each disc matches its square. Each picture fills whole cells.
# wrong-looks-like: No pictures (stock kmscon has no sixel; the page says so), a
# wrong-looks-like: disc or square not the colour of its name, a disc unlike its
# wrong-looks-like: square (kmscon draws one path wrong; both off alike is the
# wrong-looks-like: display), bars out of order, a box hiding the text around the
# wrong-looks-like: ring, bars behind TEXT, or a picture running into a label.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# Sixel graphics (DCS q ... ST) drawn through the pty, as a program such as
# img2sixel sends them. The pictures are made here at run time, sized in
# whole cells from the cell size the terminal reports (XTWINOPS 16): the bars
# 8 columns by 2 rows, the ring 4 by 2, each disc 3 by 2 and each swatch 4 by
# 3, so they line up with the text at any font size. Without an answer they
# are made for 12x24 cells, which the page says. The bars and swatches are
# opaque: the letters written over the bars afterwards remove the picture
# under their cells. The ring and the discs have transparent pixels (P2=1);
# the ring is drawn over text already on the screen, which shows outside it.
# The discs and swatches are the primaries, the secondaries and the ring's
# orange, as sixel RGB percentages (#R;2;r;g;b): red 100;0;0, green 0;100;0,
# blue 0;0;100, cyan 0;100;100, magenta 100;0;100, yellow 100;100;0, orange
# 100;50;0; the ring is orange around blue 0;0;100. Beside each, the same
# colour comes through the text path as a background (SGR 48;2;r;g;b) on
# spaces, in 8 bits: 255 for 100%, 128 for 50% (127.5, which kmscon rounds
# up). The two paths share nothing before the display, so a disc unlike its
# square is a kmscon fault, and a pair off alike is the display's doing: a
# colour profile, or dimming of a small bright patch on a black screen. The
# swatches repeat the pairs on mid-grey (128;128;128), so no colour is judged
# against black alone. The tier 3 goldens are compared exactly, so they pin
# each colour. The last line is the cell size the terminal reports.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/sixel-common.inc"
sixel_offered
cell_size

# Eight bars, one cell wide and one row tall, in registers 1 to 8; under
# them a checkerboard of squares a cell wide and half a row tall, register
# 9 on register 0 (black).
bars() {
	local w=$((8 * cw)) h=$((2 * ch)) half=$((ch + ch / 2))
	local s=$esc'Pq"1;1;'$w';'$h
	local rgb=(100,0,0 100,100,0 0,100,0 0,100,100 0,0,100 100,0,100 100,100,100 50,50,50)
	local i y bar ma mb
	for i in 0 1 2 3 4 5 6 7; do
		s+="#$((i + 1));2;${rgb[i]//,/;}"
	done
	s+='#9;2;100;100;100'
	for ((y = 0; y < h; y += 6)); do
		bits $y 0 $((ch - 1))
		bar=$m
		for ((i = 0; bar && i < 8; i++)); do
			s+="#$((i + 1))"
			((i)) && s+="!$((i * cw))?"
			s+="!$cw${six:bar:1}\$"
		done
		bits $y $ch $((half - 1))
		ma=$m
		bits $y $half $((h - 1))
		mb=$m
		if ((ma || mb)); then
			s+='#9'
			for ((i = 0; i < 8; i++)); do
				((i % 2)) && m=$mb || m=$ma
				s+="!$cw${six:m:1}"
			done
			s+='$'
		fi
		s+='-'
	done
	printf '%s' "$s$esc\\"
}

# The colours of the discs and swatches, as sixel percentages and as the
# 8-bit values of their text twins.
names=(red green blue cyan magenta yellow orange)
pct=(100,0,0 0,100,0 0,0,100 0,100,100 100,0,100 100,100,0 100,50,0)
rgb=(255,0,0 0,255,0 0,0,255 0,255,255 255,0,255 255,255,0 255,128,0)
grey=128,128,128

# The text first, then the pictures over it, each placed from the saved
# cursor at the start of the third row. Each colour has a slot 10 columns
# wide from column 3: its disc, then its text square one column further; on
# the grey band its swatch, then its text square touching it.
printf 'Sixel pictures, sent through the pty as a program sends them:\n\n'
for i in 1 2 3 4; do
	printf '%38s%s\n' '' 'text behind the ring shows past its edge'
done
printf '%38s%s\n\n' '' 'The ring is orange: red 100%, green 50%.'
printf 'Each colour as a sixel disc, then as text (SGR 48;2); the two should match:\n'
for r in 1 2; do
	printf '  '
	for k in "${!rgb[@]}"; do
		printf '%4s' ''
		block ${rgb[k]} 3
		printf '%3s' ''
	done
	printf '\n'
done
printf '  '
printf '%-10s' "${names[@]}"
printf '\n\nThe same on mid-grey: sixel left, text right, each 4 cells by 3:\n'
block $grey 72
printf '\n'
for r in 1 2 3; do
	block $grey 2
	for k in "${!rgb[@]}"; do
		block $grey 4
		block ${rgb[k]} 4
		block $grey 2
	done
	printf '\n'
done
printf '%s[30;48;2;%sm  ' "$esc" "${grey//,/;}"
printf '%-10s' "${names[@]}"
printf '%s[0m\n\n' "$esc"
printf '%s[18A%s7  ' "$esc" "$esc"
bars
printf '%s8%s[B%s[6CTEXT' "$esc" "$esc" "$esc"
d=$((4 * cw < 2 * ch ? 4 * cw : 2 * ch))
printf '%s8%s[B%s[40C' "$esc" "$esc" "$esc"
circles $((4 * cw)) $((2 * ch)) 100,50,0 $d 0,0,100 $((d / 2))
d=$((3 * cw < 2 * ch ? 3 * cw : 2 * ch))
for k in "${!pct[@]}"; do
	printf '%s8%s[7B%s[%dC' "$esc" "$esc" "$esc" $((2 + 10 * k))
	circles $((3 * cw)) $((2 * ch)) ${pct[k]} $d
	printf '%s8%s[13B%s[%dC' "$esc" "$esc" "$esc" $((2 + 10 * k))
	swatch $((4 * cw)) $((3 * ch)) ${pct[k]}
done
printf '%s8%s[18B' "$esc" "$esc"
[ "$note" ] && printf '%s\n' "$note"
printf 'Cells of %sx%s pixels, as XTWINOPS 16 reports them.\n' "${rw:-?}" "${rh:-?}"
