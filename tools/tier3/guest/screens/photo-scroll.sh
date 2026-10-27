#!/bin/bash
# kit: sixel
# look-for: Numbered lines filling the screen and the photograph gone above
# look-for: them. Shift+PageUp shows it again, above line 1, with its caption;
# look-for: Shift+PageDown returns to the lines, and the next page leaves
# look-for: nothing of it behind.
# wrong-looks-like: The photograph still on screen over the lines, missing
# wrong-looks-like: or cut when scrolled back, drawn at the wrong row, or a
# wrong-looks-like: piece of it left on the lines after Shift+PageDown.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# ADR-103's acceptance scenario A1, its second half: a photograph placed,
# scrolled into scrollback by text and brought back. It draws the street
# at sunset with chafa (as photos.sh does) six rows tall under a caption,
# counting its rows from the row it starts on (top_row), then prints one
# numbered line per row of the screen, so the picture's lines move into
# scrollback with it. Tier 3 then presses
# Shift+PageUp, which scrolls kmscon's view back a page and shows the picture
# again, and Shift+PageDown, which must give back exactly this screen; the
# screen after it is drawn after clear, which empties the screen and the
# scrollback, and its golden shows that nothing is left.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/acceptance.inc"
sixel_offered
needs chafa chafa
cell_size
term_size
top_row
r=6
photo_cols "$r"
printf 'The photograph, %d by %d cells, then %d lines that scroll it away.\n' "$w" "$r" "$rows"
chafa_photo "$photo_file" 1 $((top + 1)) "$w" "$r"
printf '%s[%d;1H' "$esc" $((top + 1 + r))
for ((i = 1; i <= rows; i++)); do
	printf 'line %d of %d\n' "$i" "$rows"
done
