#!/bin/bash
# look-for: Twelve frames alternate whole, never part painted; then the top
# look-for: half of frame 13 appears a moment late. The table reads 2 1 2 2.
# wrong-looks-like: Rows painting in one at a time, a table of 0s or 4s, or
# wrong-looks-like: 1 in its last row.
# Synchronized output, DEC private mode 2026: each frame is drawn a row at a
# time, 15 ms apart, between CSI ? 2026 h and l, and vtium shows it only once
# it is complete. Frame 13 is left open: after sync-timeout (200 ms) vtium
# shows what it has, and reports the mode reset. The table is what DECRQM
# answered at each step: 1 set, 2 reset, 4 not offered, 0 not known.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
rows=8

# DECRQM ?2026: the answer's value, or "-" when none comes within a second.
decrqm() {
	local r
	r=$(ask "${esc}[?2026\$p" y)
	[[ $r == *'[?2026;'?'$' ]] && r=${r: -2:1} || r=-
	printf '%s' "$r"
}

# Rows from..to of frame n, a row each 15 ms, from the cursor's row down. The
# cursor moves down with CUD, which never scrolls, so a frame drawn again
# lands on the same rows.
rows_of() {
	local n=$1 from=$2 to=$3 y glyph color
	if ((n % 2)); then glyph='▒' color=33; else glyph='█' color=34; fi
	for ((y = from; y <= to; y++)); do
		printf '\r\033[%sm frame %2d row %d ' "$color" "$n" "$y"
		printf "%.0s$glyph" {1..44}
		printf '\033[0m\r\033[B'
		sleep 0.015
	done
}

for ((y = 0; y < rows; y++)); do printf '\n'; done
for ((n = 1; n <= 12; n++)); do
	printf '\033[%dA\033[?2026h' "$rows"
	rows_of "$n" 1 "$rows"
	printf '\033[?2026l'
done

before=$(decrqm)
printf '\033[?2026h'
inside=$(decrqm)
printf '\033[?2026l'
after=$(decrqm)
# frame 13, left open past sync-timeout
printf '\033[%dA\033[?2026h' "$rows"
rows_of 13 1 $((rows / 2))
printf '\033[%dB' $((rows / 2))
sleep 0.5
expired=$(decrqm)
printf '\033[?2026l'

printf '\n mode 2026 (synchronized output), as DECRQM answered:\n'
printf '   before a frame                     %s\n' "$before"
printf '   inside a frame                     %s\n' "$inside"
printf '   after the frame ended              %s\n' "$after"
printf '   frame 13, left open for 0.5 s      %s\n' "$expired"
