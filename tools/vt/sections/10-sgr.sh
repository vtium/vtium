#!/bin/bash
# The SGR attributes and palette colors of the minspec slice (#12).
# look-for: Sixteen color names, each in its own color, then each on its own
# look-for: background; bold, dim, italic, underlined, blinking, inverse and
# look-for: struck words, each drawn as it names itself; the last line plain.
# wrong-looks-like: A name in the wrong color or the default one, a word with
# wrong-looks-like: no attribute or another's, or any style running past its
# wrong-looks-like: word into the plain text after it.
names=(black red green yellow blue magenta cyan white)
for base in 30 90; do
	for i in "${!names[@]}"; do
		printf '\033[%dm%-9s\033[0m' $((base + i)) "${names[i]}"
	done
	printf '\n'
done
echo
for base in 40 100; do
	for i in "${!names[@]}"; do
		printf '\033[%dm%-8s\033[0m ' $((base + i)) "${names[i]}"
	done
	printf '\n'
done
echo
printf 'plain \033[1mbold\033[22m plain \033[2mdim\033[22m plain \033[3mitalic\033[23m plain\n'
printf 'plain \033[4munderlined\033[24m plain \033[5mblinking\033[25m plain\n'
printf 'plain \033[7minverse\033[27m plain \033[9mstruck\033[29m plain\n'
printf '\033[1;4;31mbold underlined red\033[0m then plain after SGR 0\n'
