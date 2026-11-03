#!/bin/bash
# look-for: The copy matches the source box exactly, colors too; the fill is a
# look-for: solid block of #; the last box has a blank hole two rows high in
# look-for: the middle of its x's.
# wrong-looks-like: A copy shifted or missing its color, a fill or hole of the
# wrong-looks-like: wrong size or place, or any of them reaching into the
# wrong-looks-like: labels.
# Rectangular areas: DECCRA copies a rectangle with its attributes, DECFRA
# fills one with a character, and DECERA erases one. Each places by absolute
# row and column, so the page reads the row it starts on first.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
top_row
r=$((top + 2))
at() { printf '%s[%d;%dH' "$esc" "$1" "$2"; }
printf '  DECCRA copies a box, DECFRA fills one, DECERA erases one:\n'
# the source box, columns 3 to 14, rows r to r+3
at "$r" 3; printf '+----------+'
at $((r + 1)) 3; printf '|\033[1;33;44m  source  \033[0m|'
at $((r + 2)) 3; printf '|\033[4;32m  box  12 \033[0m|'
at $((r + 3)) 3; printf '+----------+'
# DECCRA: rows r to r+3, columns 3 to 14, page 1, to row r, column 20, page 1
printf '%s[%d;3;%d;14;1;%d;20;1$v' "$esc" "$r" $((r + 3)) "$r"
# x's at columns 54 to 65, then DECERA erases rows r+1 and r+2, columns 57 to 62
for ((y = r; y <= r + 3; y++)); do at "$y" 54; printf 'xxxxxxxxxxxx'; done
printf '%s[%d;57;%d;62$z' "$esc" $((r + 1)) $((r + 2))
# DECFRA: # (35) into rows r to r+3, columns 37 to 48
printf '%s[35;%d;37;%d;48$x' "$esc" "$r" $((r + 3))
at $((r + 5)) 3
printf '%-17s%-17s%-17s%s\n' 'source' 'DECCRA copy' 'DECFRA fill #' 'DECERA hole'
