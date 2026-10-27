#!/bin/bash
# look-for: A wide character at the right edge fits the last two cells
# look-for: (cases 1 and 4) or moves whole to the next line (2, 3 and 5).
# wrong-looks-like: Half a character at the edge or one lost (stock kmscon
# wrong-looks-like: cuts 2, 3 and 5), or an emoji in grey or cut at its cells.
# Double-width characters at the right margin. The screen's width is read from
# the terminal, so the cases sit at its real edge.
cols=$(stty size </dev/tty 2>/dev/null) && cols=${cols#* } || cols=${COLUMNS:-80}

# A ruler whose last digit is the last column.
ruler() {
	local i
	printf '\033[%dG' $((cols - 9))
	for i in 1 2 3 4 5 6 7 8 9 0; do printf '%s' "$i"; done
	printf '\n'
}
ruler
printf '  1. placed at the last two columns: fits'
printf '\033[%dG中\n' $((cols - 1))
printf '  2. placed at the last column: wraps, last cell empty'
printf '\033[%dG文 <- wrapped whole\n' "$cols"
printf '%*s' $((cols - 1)) '' | tr ' ' '-'
printf '字 <- 3. typed after %d columns: wrapped whole\n' $((cols - 1))
printf '%*s' $((cols - 2)) '' | tr ' ' '-'
printf '字\n'
printf '  4. ^ typed after %d columns: fits\n' $((cols - 2))
printf '  5. emoji at the last column: wraps'
printf '\033[%dG😀 <- wrapped whole\n' "$cols"
ruler
