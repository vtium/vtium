#!/bin/bash
# look-for: Each underline in the color its line names while the text
# look-for: keeps its own; after 59 the line turns red with the text; 58
# look-for: alone draws none.
# wrong-looks-like: Underlines in the text's color, rows with no underline,
# wrong-looks-like: or an underline under the last row.
# Underline color, SGR 58 (set) and 59 (reset), in the colon and semicolon
# forms.
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
