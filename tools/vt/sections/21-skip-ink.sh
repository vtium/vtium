#!/bin/bash
# look-for: Every underline breaks around the descenders of g, j, p, q and y,
# look-for: about a pixel each side, and runs whole between the letters, the
# look-for: double-width line's gap twice as wide. Strike and overline run
# look-for: unbroken. Walk this page on the CPU path and on the GPU path.
# wrong-looks-like: A line drawn through a descender, a gap with no letter
# wrong-looks-like: above it, a break in a strike or an overline, or a
# wrong-looks-like: style that skips on one path and not on the other.
# frits: acceptance-outline acceptance-bitmap
# Underlines skip the ink of descenders: each style of underline leaves out
# the pixels within one pixel of a letter's ink, so g, j, p, q and y stay
# readable. Strike-through and overline skip nothing. A double-width line
# magnifies the cell, and its gap with it.
#
# References:
# - ADR-1302 D7: the underline's gap around the glyph's ink.
# - ADR-1308 D9: the gap scales with a magnified cell.
row() {
	printf '  %-11s \033[%sm%s\033[0m\n' "$1" "$2" "$3"
}
words='gjpqy: jog, pygmy, quip, yoga, gauge'
printf '  Each underline style under letters with descenders:\n'
row '4:1 single' '4:1' "$words"
row '4:2 double' '4:2' "$words"
row '4:3 curly' '4:3' "$words"
row '4:4 dotted' '4:4' "$words"
row '4:5 dashed' '4:5' "$words"
row '21 double' '21' "$words"
printf '  In color, on color, bold and italic:\n'
row '58 red' '4;58:5:196' "$words"
row 'on blue' '4:2;44' "$words"
row 'bold' '1;4' "$words"
row 'italic' '3;4:3' "$words"
printf '  Lines that skip nothing:\n'
row '9 strike' '9' "$words"
row '53 overline' '53' "$words"
printf '\033#6  \033[4mdouble-width: jog, quip\033[0m\n'
