#!/bin/bash
# look-for: Each link takes a dashed underline only while the pointer is on
# look-for: it; both parts of "one link" underline together; Ctrl shows the
# look-for: address on the bottom row.
# wrong-looks-like: Links underlined with the pointer elsewhere, plain text
# wrong-looks-like: underlined, one part of "one link" alone, or no address.
# Hyperlinks, OSC 8. A link draws nothing until the pointer is over it; then
# each of its visible cells takes a dashed underline. While Ctrl is held over
# a link, its address shows in inverted colors from the first column of the
# bottom row, or the top row when the pointer is on the bottom one. Two runs
# of text opened with the same id and address are one link.
link() { printf '\033]8;%s;%s\033\\%s\033]8;;\033\\' "$1" "$2" "$3"; }
printf '  Move the mouse pointer over each word below, then hold Ctrl on it.\n\n'
printf '  a link: %s, plain text after it\n' "$(link '' https://example.com/vtium/first 'first link')"
printf '  a second one: %s\n' "$(link '' https://example.com/vtium/second 'second link')"
printf '  %s, some plain words, %s\n' \
	"$(link id=one https://example.com/vtium/one 'one link')" \
	"$(link id=one https://example.com/vtium/one 'in two parts')"
printf '  plain text that is no link at all\n\n'
printf '  Ctrl over "first link" shows https://example.com/vtium/first.\n'
