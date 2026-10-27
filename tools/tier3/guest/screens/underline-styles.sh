#!/bin/bash
# look-for: Each line underlined in the shape it names, all distinct:
# look-for: single, double, curly, dotted, dashed; 4:0 and 24 end it
# look-for: mid-line.
# wrong-looks-like: Every style drawn as one single line, or a line with
# wrong-looks-like: none of its attributes: on stock kmscon only the plain
# wrong-looks-like: 4 line is underlined.
# Underline styles as sub-parameters of SGR 4 (CSI 4:n m). Upstream libtsm
# discards a whole SGR sequence holding a colon, so there these lines show no
# underline at all, except the plain SGR 4 line.
row() {
	printf '  %-12s \033[%sm%s\033[0m\n' "$1" "$2" "$3"
}
row '4' '4' 'plain 4: a single line'
row '4:1' '4:1' 'single underline'
row '4:2' '4:2' 'double underline'
row '4:3' '4:3' 'curly underline'
row '4:4' '4:4' 'dotted underline'
row '4:5' '4:5' 'dashed underline'
row '4:3 then 4:0' '4:3' "curly here$(printf '\033[4:0m') and none here"
row '4:2 then 24' '4:2' "double here$(printf '\033[24m') and none here"
echo
printf '  styles over colours, bold and wide text:\n'
row '4:3;31' '4:3;31' 'red text, curly underline in red'
row '1;4:2' '1;4:2' 'bold text, double underline'
row '4:4;7' '4:4;7' 'reverse video, dotted underline'
row '4:3' '4:3' '中文 and 日本語 under one curly line'
row '4:5;44' '4:5;44' 'dashed on a blue background'
