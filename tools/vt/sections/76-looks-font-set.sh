#!/bin/bash
# look-for: A form with one field for each role, text font first; Enter on a
# look-for: role lists the fonts that can fill it. The new font set then shows
# look-for: in the Looks tab's font sets, and in the font set list of a look's
# look-for: card, where picking it puts it on the card's Font set line.
# wrong-looks-like: A form with no roles, a role's list that is empty, a new
# wrong-looks-like: font set missing from the font sets or from the card's
# wrong-looks-like: list, or a Font set line that does not change.
# The Looks tab's form for a new font set, by role: the text font, box
# drawing, scripts, symbols and icons, emoji and the fallback, then a name.
# A font set made there belongs to the draft, and the Looks tab lists it
# with the installed ones. The page runs vtium-config on a vtium.conf of its
# own (common.inc's looks_tui).
#
# References:
# - ADR-1201 D4: the Looks tab's font sets and the new-font-set form.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
looks_tui \
	'The Looks tab makes a new font set from a font for each role.' \
	'  1. Press 3 for the Looks tab, Down to a font set, then n for the form.' \
	'  2. Press Enter on Text font and pick ter-124n, or any font it lists.' \
	'  3. Down to Name, press Enter, type mine and press Enter. Then Down to' \
	'     the last row and press Enter to add the font set.' \
	'  4. Find mine among the font sets. Up to the look big, press Enter, and' \
	'     Enter on Font set: pick mine. Then quit with q, and n if it asks.'
