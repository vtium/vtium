#!/bin/bash
# look-for: Under the pointer, a link gets a dashed underline, on both parts
# look-for: of "one link" at once. Ctrl shows its address on the bottom row.
# look-for: Ctrl+Shift+O labels the three links a, s and d, inverted and
# look-for: underlined. Typing s copies the second address, shown after
# look-for: "Copied:".
# wrong-looks-like: Links underlined with the pointer elsewhere, plain text
# wrong-looks-like: underlined, one part of "one link" alone, or no address;
# wrong-looks-like: a label on plain text, two links with one label, a letter
# wrong-looks-like: typed into the shell, or a paste that is not the address.
# Hyperlinks, OSC 8. A link draws nothing until the pointer is over it; then
# each of its visible cells takes a dashed underline. While Ctrl is held over
# a link, its address shows in inverted colors from the first column of the
# bottom row, or the top row when the pointer is on the bottom one. Two runs
# of text opened with the same id and address are one link. Ctrl+Shift+O
# starts link hints: each distinct link gets a label of letters, from the
# home row first, and typing a label copies that link's address, so the
# middle button or Shift+Insert pastes it. Escape stops without a copy.
link() { printf '\033]8;%s;%s\033\\%s\033]8;;\033\\' "$1" "$2" "$3"; }
printf '  Move the mouse pointer over each word below, then hold Ctrl on it.\n\n'
printf '  a link: %s, plain text after it\n' "$(link '' https://example.com/vtium/first 'first link')"
printf '  a second one: %s\n' "$(link '' https://example.com/vtium/second 'second link')"
printf '  %s, some plain words, %s\n' \
	"$(link id=one https://example.com/vtium/one 'one link')" \
	"$(link id=one https://example.com/vtium/one 'in two parts')"
printf '  plain text that is no link at all\n\n'
printf '  Ctrl over "first link" shows https://example.com/vtium/first.\n\n'
printf '  Then, without the mouse: press Ctrl+Shift+O. The links show a, s and d.\n'
printf '  Press s: the bottom row shows Copied: https://example.com/vtium/second.\n'
printf '  Shift+Insert at the prompt then pastes that address.\n'
printf '  Press Ctrl+Shift+O again, then Escape: the labels go and nothing is copied.\n'
