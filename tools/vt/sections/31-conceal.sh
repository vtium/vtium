#!/bin/bash
# look-for: Each pair of brackets holds an empty stretch: no letter, line or
# look-for: mark shows, and a colored background stays. After SGR 28 the text
# look-for: shows again.
# wrong-looks-like: Any letter, underline or strike of a concealed run, a
# wrong-looks-like: concealed background gone, or text after 28 still hidden.
# Concealed text, SGR 8 (set) and 28 (reset): a concealed cell draws its
# background alone and keeps its text, so nothing of the text shows between
# the brackets, but a background color does. The underline and the strike are
# part of the text, so they hide with it.
row() {
	printf '  %-14s [\033[%sm%s\033[0m]\n' "$1" "$2" "$3"
}
printf '  Between each pair of brackets, the text is concealed.\n\n'
row '8' '8' 'hidden text'
row '8;31' '8;31' 'hidden red text'
row '8;43' '8;30;43' 'on a yellow background'
row '8;7' '8;7' 'in reverse video'
row '8;4;9' '8;4;9' 'underlined and struck'
row '8' '8' $'\xe4\xb8\xad\xe6\x96\x87 wide characters'
printf '  %-14s [\033[8mhidden here\033[28m shown here\033[0m]\n' '8, then 28'
printf '  %-14s [%s]  a reference\n' 'no SGR' 'shown text'
