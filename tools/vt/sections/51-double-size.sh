#!/bin/bash
# look-for: Rows marked 2x wide draw each letter twice as wide, and the two
# look-for: rows marked 2x tall join into one line of letters twice as tall.
# look-for: Each 40-character ruler and row of E ends at the right edge, as
# look-for: the 80-character one under it does, and the X sits at that edge.
# wrong-looks-like: A double row drawn at normal size or cut at mid-screen,
# wrong-looks-like: a tall line's halves not lining up or one half missing,
# wrong-looks-like: a ruler that wraps, or the X short of the right edge.
# Lines of double size: ESC # 6 (DECDWL) makes the cursor's line double
# width, ESC # 3 and ESC # 4 (DECDHL) make it the top and the bottom half of
# a double-height line, and ESC # 5 (DECSWL) makes it single width again. A
# double line holds 40 characters on an 80-column screen. The last block
# copies the screen vttest draws for line sizes. The walk erases the screen
# before the next page, which returns every line to single width.
dwl=$'\033#6' top=$'\033#3' bottom=$'\033#4' swl=$'\033#5'
ruler40=1234567890123456789012345678901234567890
printf '  Lines of double size, ESC # 3 to 6, each holding 40 characters:\n'
printf '%s2x wide: one line\n' "$dwl"
printf '%s2x tall: top and bottom\n' "$top"
printf '%s2x tall: top and bottom\n' "$bottom"
printf '%s  1x: a single-width line again holds all 80 columns.\n' "$swl"
printf '%s%s\n' "$dwl" "$ruler40"
printf '%s%s\n' "$ruler40" "$ruler40"
# CUF past the end stops at the line's last column, so the X lands there.
printf '%sThe cursor stops at the edge:\033[99CX\n' "$dwl"
printf '\n'
printf '  The line sizes as vttest draws them:\n'
printf '%sDouble-width, double-height\n' "$top"
printf '%sDouble-width, double-height\n' "$bottom"
printf '%sDouble-width, single-height\n' "$dwl"
printf 'Single-width, single-height\n'
printf '%s%s\n' "$dwl" "$(printf 'E%.0s' $(seq 40))"
printf '%s\n' "$(printf 'E%.0s' $(seq 80))"
