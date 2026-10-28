#!/bin/bash
# look-for: Each emoji in color, whole and centred in its two cells, bars in
# look-for: line with |ab|; family, flag, keycap and tone one picture each.
# wrong-looks-like: A grey or cut corner of a large emoji (stock kmscon), a
# wrong-looks-like: cluster in pieces, a plain 1 for the keycap, bars out of line.
# Emoji and multi-code-point clusters: a ZWJ sequence, a flag (two regional
# indicators), a keycap, a skin-tone modifier and VS16, which turns a text
# heart into an emoji one, while VS15 keeps it text. Each sits between bars so
# its width shows; then emoji in a line of text, underlined and reversed. The
# invisible joiners and selectors are written as UTF-8 bytes so the output
# does not depend on the locale.
zwj=$'\xe2\x80\x8d'    # U+200D ZERO WIDTH JOINER
vs15=$'\xef\xb8\x8e'   # U+FE0E VARIATION SELECTOR-15
vs16=$'\xef\xb8\x8f'   # U+FE0F VARIATION SELECTOR-16
keycap=$'\xe2\x83\xa3' # U+20E3 COMBINING ENCLOSING KEYCAP
family="👨$zwj👩$zwj👧"

row() {
	printf '  %-32s |%s|\n' "$1" "$2"
}
row 'single emoji U+1F600' '😀'
row 'family: man ZWJ woman ZWJ girl' "$family"
row 'flag: regional indicators N O' '🇳🇴'
row 'keycap: 1 U+FE0F U+20E3' "1$vs16$keycap"
row 'thumbs up + skin tone U+1F3FD' '👍🏽'
row 'heart U+2764 + VS16' "❤$vs16"
row 'reference, two cells' 'ab'
echo
printf '  the text heart U+2764 alone is one cell:  |%s|  |a|\n' '❤'
printf '  and with VS15, still one cell:            |%s|  |a|\n' "❤$vs15"
printf '  in a line:  |%s|\n' "a😀b${family}c🇳🇴d"
printf '  reference:  |%s|\n' 'axxbxxcxxd'
printf '  underlined and reversed:  |\033[4m%s\033[0m|  |\033[7m%s\033[0m|\n' '😀' '😀'
