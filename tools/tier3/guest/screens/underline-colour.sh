#!/bin/bash
# look-for: Each underline in the colour its line names while the text
# look-for: keeps its own; after 59 the line turns red with the text; 58
# look-for: alone draws none.
# wrong-looks-like: Underlines in the text's colour or rows with none:
# wrong-looks-like: stock kmscon ignores 58, drops the colon rows, and reads
# wrong-looks-like: 58;2;0;160;255 as dim then reset, so that row is bare.
# Underline colour, SGR 58 (set) and 59 (reset), in the colon and semicolon
# forms. Upstream libtsm takes each number of 58;5;n and 58;2;r;g;b as an SGR
# of its own: the 5 turns on blink (steady where blinking is off, as in tier
# 3), and the 2 and 0 of 58;2;0;160;255 turn on faint and then
# reset everything. It discards the colon rows' sequences entirely.
row() {
	printf '  %-20s \033[%sm%s\033[0m\n' "$1" "$2" "$3"
}
row '4;58:5:196' '4;58:5:196' 'white text, red underline'
row '4;58;5;196' '4;58;5;196' 'white text, red underline'
row '4;58:2::0:160:255' '4;58:2::0:160:255' 'white text, sky-blue underline'
row '4;58;2;0;160;255' '4;58;2;0;160;255' 'white text, sky-blue underline'
row '4:3;58:5:196' '4:3;58:5:196' 'white text, red curly underline'
row '4:3;58:5:226' '4:3;58:5:226' 'white text, yellow curly underline'
row '4:2;32;58:5:201' '4:2;32;58:5:201' 'green text, magenta double underline'
row '4;31;58:5:46 ... 59' '4;31;58:5:46' "red text, green line$(printf '\033[59m') then a red line"
row '58:5:196, no 4' '58:5:196' 'no underline: 58 alone draws nothing'
