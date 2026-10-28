#!/bin/bash
# look-for: Twelve frames alternate whole, never part painted; then the top
# look-for: half of frame 13 appears a moment late. The table reads 2 1 2 2.
# wrong-looks-like: Rows painting in one at a time and a table of 0s (stock
# wrong-looks-like: kmscon has no mode 2026), a table of 4s, or 1 in its last row.
# Synchronized output, DEC private mode 2026 (ADR-201): each frame is drawn a
# row at a time, 15 ms apart, between CSI ? 2026 h and l, and kmscon shows it
# only once it is complete. Frame 13 is left open: after sync-timeout (200 ms)
# kmscon shows what it has, and reports the mode reset. The table is what
# DECRQM answered at each step: 1 set, 2 reset, 4 not offered, 0 not known.
# screen-text: none, it reads the terminal's replies and redraws with cursor moves
rows=8

# DECRQM ?2026: the answer's value, or "-" when none comes within 2 seconds.
# Echo is off while it waits, so the reply is not printed.
decrqm() {
	local mode r=
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '\033[?2026$p' >/dev/tty
		IFS= read -rsd y -t 2 r </dev/tty
		stty "$mode" </dev/tty
	fi
	[[ $r == *'[?2026;'?'$' ]] && r=${r: -2:1} || r=-
	printf '%s' "$r"
}

# Rows from..to of frame n, a row each 15 ms, from the cursor's row down.
rows_of() {
	local n=$1 from=$2 to=$3 y glyph colour
	if ((n % 2)); then glyph='▒' colour=33; else glyph='█' colour=34; fi
	for ((y = from; y <= to; y++)); do
		printf '\r\033[%sm frame %2d row %d ' "$colour" "$n" "$y"
		printf "%.0s$glyph" {1..44}
		printf '\033[0m\n'
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
