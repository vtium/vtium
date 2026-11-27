#!/bin/bash
# look-for: Each sample at normal size, double width, and double height.
# look-for: Emoji keep their color, marks sit on their letters, letters join
# look-for: as at normal size, and outline fonts stay smooth. Box lines stay
# look-for: thin when wide and join up. Powerline arrows show only with a
# look-for: Powerline font.
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

# Succeeds when the font set the walk switched to (ACCEPT_FRIT) gives U+E0B0
# to a font of its own, with a cover= line whose range holds U+E0B0, as
# glyph-fit-default does for Monoid Nerd Font. In any other font set, U+E0B0
# falls through to the built-in unifont, whose Private Use Area holds
# unrelated letters, so the Powerline sample could never look right there.
powerline_font() {
	local file line range
	file=$(dirname "$(readlink -f "$0")")/frits/${ACCEPT_FRIT:-}.vtfrit
	[ -n "${ACCEPT_FRIT:-}" ] && [ -f "$file" ] || return 1
	while IFS= read -r line; do
		case $line in cover=U+*)
			range=${line#cover=U+}
			((16#${range%%-*} <= 0xE0B0 && 0xE0B0 <= 16#${range#*-U+})) && return 0
			;;
		esac
	done <"$file"
	return 1
}

# Each sample on a line of the normal size, a double-width line and a
# double-height pair: color emoji with a VS16 heart and a skin tone, then
# letters with marks; Arabic with the lam-alef ligature and vowel marks, then
# Hindi; DEC special graphics, blocks and box drawing, which vtium draws
# itself; and Powerline separators, in a font set with a Powerline font. The
# page holds 17 rows, so the samples carry no titles.
samples=("$emoji  $marks" "$arabic  $hindi" "$graphics $blocks")
powerline_font && samples+=("$prompt")
for text in "${samples[@]}"; do
	printf '%s%s\n' "$text" "$reset"
	printf '%s%s%s\n' "$dwl" "$text" "$reset"
	printf '%s%s%s\n' "$top" "$text" "$reset"
	printf '%s%s%s\n' "$bottom" "$text" "$reset"
done
powerline_font ||
	printf 'This font set has no Powerline font, so the page shows no Powerline sample.\n'
frit_line
