#!/bin/bash
# kit: sixel
# look-for: Ramps of red, green, blue and grey from black to full, a hue sweep,
# look-for: and skin tones and pastels, each sixel row above a row of text
# look-for: cells in the same colours: every column matches the one under it.
# look-for: Last, a dark blue ring over yellow-backed text, the text showing in
# look-for: its hole and corners. Each caption says what right looks like.
# wrong-looks-like: No pictures (stock kmscon has no sixel; the page says so), a
# wrong-looks-like: sixel step unlike the text cell under it (kmscon draws one
# wrong-looks-like: path wrong; both shifted alike is the display), banding or
# wrong-looks-like: dither in a ramp, or a box round the ring hiding the text.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# Sixel pictures for comparing colours by eye, each above the same colours
# drawn through the text path (SGR 48;2 on spaces), all made here at run time
# in whole cells from the cell size the terminal reports (XTWINOPS 16). A
# sixel colour is three whole percents, so a ramp runs from 0% to 100% in
# equal whole-percent steps, one cell each: 101 steps of 1% when the screen
# has the columns and the terminal the colour registers (XTSMGRAPHICS), each
# step in a register of its own, fewer steps when not, which the page says.
# The text row under a ramp takes the 8-bit value kmscon makes of each
# percentage (p * 255 / 100, rounded), so the two rows match exactly. A
# sixel step unlike its text cell is a kmscon fault; a pair shifted alike
# is the display. The hue sweep runs once round the colour wheel at full
# saturation. The swatches are five skin tones, light to dark, and five
# pastels. The ring is drawn with transparent pixels (P2=1) over text in
# red, green and blue on yellow, which shows through its hole and corners.
# The last line is the cell size the terminal reports.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/sixel-common.inc"
sixel_offered
cell_size
term_size

# The colour registers the terminal gives a picture, from XTSMGRAPHICS's
# reply ESC [ ? 1 ; 0 ; count S.
IFS=';' read -r _ status regs <<<"$(ask "$esc[?1;1S" S)"
if [[ $status != 0 || ! $regs =~ ^[0-9]+$ ]]; then
	regs=101
	note2='XTSMGRAPHICS gave no register count, so the ramps assume 101 registers.'
fi
# The most steps that divide 100% evenly and fit both the columns and the
# registers.
for n in 101 51 26 21 11 6 5 3 2; do
	((n <= cols - 4 && n <= regs)) && break
done
((n < 101)) && note2+="${note2:+ }This terminal has $cols columns and $regs registers, so each ramp has $n steps, not 101."
step=$((100 / (n - 1)))

# A sixel ramp one row tall, one cell per step: each argument is a step's
# colour (r,g,b percentages), in register 0 upward.
ramp() {
	local s=$esc'Pq"1;1;'$(($# * cw))';'$ch i y c
	i=0
	for c; do
		s+="#$i;2;${c//,/;}"
		((i++))
	done
	for ((y = 0; y < ch; y += 6)); do
		bits $y 0 $((ch - 1))
		for ((i = 0; i < $#; i++)); do
			s+="#$i!$cw${six:m:1}"
		done
		s+='-'
	done
	printf '%s' "$s$esc\\"
}

# The same colours as text cells, $1 cells each.
twins() {
	local c w=$1
	shift
	for c; do
		block "$(eight "$c")" "$w"
	done
}

red=() green=() blue=() grey=() hue=()
for ((i = 0; i < n; i++)); do
	p=$((i * step))
	red+=($p,0,0) green+=(0,$p,0) blue+=(0,0,$p) grey+=($p,$p,$p)
	# once round the wheel at full saturation: six sectors, one component
	# rising or falling in each
	h=$((i * 360 / n)) f=$(((i * 360 % (60 * n)) * 100 / (60 * n)))
	case $((h / 60)) in
	0) hue+=(100,$f,0) ;;
	1) hue+=($((100 - f)),100,0) ;;
	2) hue+=(0,100,$f) ;;
	3) hue+=(0,$((100 - f)),100) ;;
	4) hue+=($f,0,100) ;;
	*) hue+=(100,0,$((100 - f))) ;;
	esac
done
# skin tones light to dark, then pink, peach, mint, sky and lavender
tones=(100,87,77 94,76,62 82,62,47 63,44,31 42,27,18
	100,82,86 100,85,73 74,94,82 73,87,100 85,80,100)

# Down $1 rows and right $2 columns from the saved cursor, the start of the
# first ramp's row.
at() {
	printf '%s8' "$esc"
	(($1)) && printf '%s[%dB' "$esc" "$1"
	(($2)) && printf '%s[%dC' "$esc" "$2"
}

# The text first, then the pictures over the rows left for them.
printf 'Sixel colours beside the same colours drawn as text cells (SGR 48;2).\n'
printf 'Ramps of %d steps of %d%%, one register each; sixel colours are whole percents.\n\n' $n $step
for name in Red Green Blue Grey; do
	case $name in
	Grey) printf 'Grey, black to white: the sixel row and the text row under it read as one band.\n' ;;
	*) printf '%s, black to full: the sixel row and the text row under it read as one band.\n' $name ;;
	esac
	printf '\n  '
	arr=${name,,}[@]
	twins 1 "${!arr}"
	printf '\n'
done
printf 'Hue, round the wheel from red through green and blue to magenta, as one band:\n\n  '
twins 1 "${hue[@]}"
printf '\nSkin tones light to dark, then pastels; each over its text twin, as one block:\n\n\n  '
for c in "${tones[@]}"; do
	twins 6 "$c"
	printf ' '
done
printf '\n  '
for c in "${tones[@]}"; do
	twins 6 "$c"
	printf ' '
done
printf '\nA dark blue ring, transparent in its hole and corners: the text shows through.\n'
for fg in 200,0,0 0,140,0 0,0,220; do
	printf '  %s[38;2;%s;48;2;255;240;160m%s%s[0m\n' "$esc" "${fg//,/;}" \
		'text under the ring shows through its hole and corners ' "$esc"
done
printf '\n'
printf '%s[24A%s7' "$esc" "$esc"
at 0 2
ramp "${red[@]}"
at 3 2
ramp "${green[@]}"
at 6 2
ramp "${blue[@]}"
at 9 2
ramp "${grey[@]}"
at 12 2
ramp "${hue[@]}"
for k in "${!tones[@]}"; do
	at 15 $((2 + 7 * k))
	swatch $((6 * cw)) $((2 * ch)) ${tones[k]}
done
d=$((8 * cw < 3 * ch ? 8 * cw : 3 * ch))
at 20 10
circles $((8 * cw)) $((3 * ch)) 0,0,60 $d none $((d / 2))
at 24 0
[ "$note" ] && printf '%s\n' "$note"
[ "$note2" ] && printf '%s\n' "$note2"
printf 'Cells of %sx%s pixels, as XTWINOPS 16 reports them.\n' "${rw:-?}" "${rh:-?}"
