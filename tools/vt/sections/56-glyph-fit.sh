#!/bin/bash
# look-for: Default fit: each Powerline separator runs the whole row, added
# look-for: rows included, so its segment joins it; box lines join from row
# look-for: to row; icons, letters and marks keep the text's size. Natural:
# look-for: separators and box lines keep the text's height, with gaps.
# wrong-looks-like: A separator shorter than its colored segment, a gap in
# wrong-looks-like: the box lines under the default fit, an icon off the
# wrong-looks-like: row's middle, or a mark away from its letter.
# frits: glyph-fit-default glyph-fit-natural
# How each kind of glyph uses a row made taller by line spacing, and how a
# merged symbol font fits the main font's cell. Both font sets merge Monoid
# Nerd Font Mono behind DejaVu Sans Mono and add 30 per cent of line
# spacing. glyph-fit-default leaves every fit= at its default, and
# glyph-fit-natural sets fit=natural for the merged symbols and for box
# drawing. The text is written as UTF-8 bytes so the output does not depend
# on the locale.
#
# References:
# - ADR-1302 D13: line spacing and the fit= of each kind of glyph.
# - ADR-1200 D17: a later source's glyphs in the first source's cell.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
reset=$'\033[0m'
right=$'\xee\x82\xb0' # U+E0B0, the Powerline right-pointing triangle
thin=$'\xee\x82\xb1'  # U+E0B1, the Powerline thin separator
left=$'\xee\x82\xb2'  # U+E0B2, the Powerline left-pointing triangle
home=$'\xef\x80\x95'  # U+F015, Font Awesome's house through Nerd Fonts
folder=$'\xef\x81\xbb' # U+F07B, Font Awesome's folder through Nerd Fonts
marks=$'e\xcc\x81 a\xcc\x88 o\xcc\x82 q\xcc\xa3 A\xcc\x8a n\xcc\x83' # letters with combining marks

printf '  A prompt of Powerline segments, from the merged symbol font:\n'
printf '  %s[30;44m vt %s[34;42m%s%s[30m src %s[32;49m%s%s %s %s[37m%s%s[30;47m ok %s\n\n' \
	"$esc" "$esc" "$right" "$esc" "$esc" "$right" "$reset" "$thin" "$esc" "$left" "$esc" "$reset"
printf '  Icons beside letters: %s home  %s src  gjpqy\n' "$home" "$folder"
printf '  Combining marks:      %s\n\n' "$marks"
printf '  Box drawing and blocks, drawn by vtium:\n'
printf '  ┌──┬──┐ ▀▄█ ││\n'
printf '  │  │  │ ███ ││\n'
printf '  └──┴──┘ ▄▀█ ││\n\n'
frit_line
