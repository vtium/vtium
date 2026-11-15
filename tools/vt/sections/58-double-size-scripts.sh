#!/bin/bash
# look-for: Each sample at normal size, twice as wide, and twice as tall
# look-for: over two rows. Emoji keep their color, marks sit on their
# look-for: letters, letters join as at normal size, and an outline font set
# look-for: stays smooth. Box lines stay thin when wide and join up.
# wrong-looks-like: Square steps of doubled pixels in an outline font set, a
# wrong-looks-like: mark off its letter, joined letters drawn apart, box
# wrong-looks-like: lines thick on the wide rows, or a gap in a box line or
# wrong-looks-like: in a Powerline arrow.
# frits: acceptance-outline acceptance-bitmap glyph-fit-default
# Lines of double size for the characters that are not plain text. ESC # 6
# (DECDWL) makes a line double width, and ESC # 3 and ESC # 4 (DECDHL) make
# a pair of lines the top and the bottom half of one double-height line.
# Each sample is written on a line of the normal size, a double-width line
# and a double-height pair. In an outline font set, the font helper draws
# color emoji, marks and joined letters at the larger size. vtium draws box
# lines and blocks itself, again in the doubled cell. A bitmap font's
# glyphs keep their doubled pixels. The text is written as UTF-8 bytes so
# the output does not depend on the locale.
#
# References:
# - ADR-1308 D1: a double-size line draws each character at twice the
#   width, and at twice the height across a pair.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
dwl="${esc}#6" top="${esc}#3" bottom="${esc}#4"
reset="${esc}[0m"
vs16=$'\xef\xb8\x8f'                       # U+FE0F VARIATION SELECTOR-16
emoji="😀 🔥 ❤$vs16 👍🏽"
marks=$'e\xcc\x81 a\xcc\x88 o\xcc\x82 q\xcc\xa3 n\xcc\x83 E\xcc\xa3\xcc\x82' # letters with combining marks
arabic='سلام مَرْحَبًا'
hindi='नमस्ते क्षत्रिय'
graphics="${esc}(0lqwqk tqnqu mqvqj${esc}(B"  # DEC special graphics: box corners and lines
blocks='▀▄█░▓ ╔═╗╭─╮'
right=$'\xee\x82\xb0' # U+E0B0, the Powerline right-pointing triangle
thin=$'\xee\x82\xb1'  # U+E0B1, the Powerline thin separator
prompt="${esc}[30;44m vt ${esc}[34;42m${right}${esc}[30m src ${esc}[32;49m${right}${reset} ${thin}"

# Each sample on a line of the normal size, a double-width line and a
# double-height pair: color emoji with a VS16 heart and a skin tone, then
# letters with marks; Arabic with the lam-alef ligature and vowel marks, then
# Hindi; DEC special graphics, blocks and box drawing, which vtium draws
# itself; and Powerline separators. The page holds 17 rows, so the samples
# carry no titles.
for text in "$emoji  $marks" "$arabic  $hindi" "$graphics $blocks" "$prompt"; do
	printf '%s%s\n' "$text" "$reset"
	printf '%s%s%s\n' "$dwl" "$text" "$reset"
	printf '%s%s%s\n' "$top" "$text" "$reset"
	printf '%s%s%s\n' "$bottom" "$text" "$reset"
done
frit_line
