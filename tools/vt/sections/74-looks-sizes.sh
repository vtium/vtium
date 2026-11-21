#!/bin/bash
# look-for: The look card's size list, and below it a preview of the
# look-for: highlighted size that changes as you move: the console's grid at
# look-for: that size, with its columns and rows. Esc closes the list with the
# look-for: card unchanged, and Enter puts the size on the card's Size line.
# wrong-looks-like: No preview under the list, a preview that stays on one
# wrong-looks-like: size as you move, a list cut off by the preview, or a Size
# wrong-looks-like: line that changes after Esc.
# The Looks tab's size preview: on a look's card, the size list shows below
# it what the highlighted size gives on this console, so you can choose a
# size by what it looks like. The page runs vtium-config on a vtium.conf of
# its own (common.inc's looks_tui).
#
# References:
# - ADR-1201 D4: the Looks tab, the look card and its size list's preview.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
looks_tui \
	'The Looks tab shows a preview below a look card'"'"'s size list.' \
	'  1. Press 3 for the Looks tab. The look big is highlighted.' \
	'  2. Press Enter to open its card, then Down twice to the Size line.' \
	'  3. Press Enter for the size list. Move up and down it, and watch the' \
	'     preview below the list follow the highlighted size.' \
	'  4. Press Esc, then Enter again, pick a size with Enter, and check the' \
	'     card'"'"'s Size line. Then quit with q, and n if it asks.'
