#!/bin/bash
# look-for: A line along the top of each overlined run, in the text's color,
# look-for: unbroken across its cells and across wide characters; it stops at
# look-for: 55 and before the trailing spaces.
# wrong-looks-like: No overline, one in the middle or at the bottom of the
# wrong-looks-like: row, a gap between cells, another color, or one running on
# wrong-looks-like: after 55.
# Overline, SGR 53 (set) and 55 (reset), drawn along the cell's top rows in
# the strike's color, which is the text's color unless strike-color names
# another (docs/config.md).
row() {
	printf '  %-16s \033[%sm%s\033[0m\n' "$1" "$2" "$3"
}
row '53' '53' 'overlined text'
row '53;31' '53;31' 'red text, red line'
row '1;53' '1;53' 'bold and overlined'
row '53;4' '53;4' 'over and underlined'
row '53;9' '53;9' 'overlined and struck'
row '53;7' '53;7' 'overlined in reverse video'
row '53;33;44' '53;33;44' 'yellow on blue'
row '53' '53' $'\xe4\xb8\xad\xe6\x96\x87 \xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e overlined whole'
row '53 then 55' '53' "overlined here$(printf '\033[55m') and plain here"
