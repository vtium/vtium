#!/bin/bash
# category: text and fonts
# vtium merges a symbol font behind the text font and fits its symbols to
# the text font's character cell: Symbols Nerd Font Mono's Powerline
# separators run from the top of each row to its bottom, so the colored
# segments of a prompt join them, and its icons stand in the middle of
# their cells beside Iosevka Term's letters. Letters with combining marks
# and a box of lines that vtium draws itself share the rows.
# alt: A console screen of shell prompts made of colored segments, blue,
# alt: gray and green, each ending in a solid arrow exactly as tall as the
# alt: segment, with a house, folder and branch icon inside the segments.
# alt: Below them, a list of folders and files, each with its own icon,
# alt: the words café, naïve and São Paulo written with combining marks, and
# alt: a box of light lines around them that joins from row to row.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-shell
o=$(off)
arrow=$'\xee\x82\xb0'  # U+E0B0, the Powerline arrow
branch=$'\xee\x82\xa0' # U+E0A0, the Powerline branch symbol
home=$'\xef\x80\x95'   # U+F015, a house
folder=$'\xef\x81\xbc' # U+F07C, an open folder
file=$'\xef\x85\x9b'   # U+F15B, a file
acute=$'\xcc\x81'      # U+0301
diaeresis=$'\xcc\x88'  # U+0308
tilde=$'\xcc\x83'      # U+0303

# segment R G B NEXT TEXT: white TEXT on the color R G B, then the arrow in
# that color on NEXT, the next segment's background or none
segment() {
	printf '%s%s %s %s%s%s' "$(bg "$1" "$2" "$3")" "$(fg 255 255 255)" "$5" "$(fg "$1" "$2" "$3")" "$4" "$arrow"
}
prompt() {
	segment 40 90 170 "$(bg 90 90 100)" "$home $1"
	segment 90 90 100 "$(bg 40 140 80)" "$folder $2"
	segment 40 140 80 "$o" "$branch $3"
	printf '%s\n' "$o"
}
printf '\n'
printf '  '; prompt user vtium main
printf '  '; prompt user crates glyph-fit
printf '  '; prompt user fonts integration
printf '\n'
# each row's right side placed by column, since a padding count of bytes
# or of characters would both miscount the icons and the marks
line=$(fg 150 150 150)
printf '  %s┌──────────────────────────────────────┐%s\n' "$line" "$o"
row() { printf '  %s│%s  %s%s[42G%s│%s\n' "$line" "$o" "$1" "$esc" "$line" "$o"; }
for entry in "$folder crates/" "$folder tools/" "$file Cargo.toml" "$file README.md"; do
	row "$entry"
done
row "cafe$acute, nai${diaeresis}ve, Sa${tilde}o Paulo"
printf '  %s└──────────────────────────────────────┘%s\n' "$line" "$o"
