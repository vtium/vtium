#!/bin/bash
# look-for: small's card says Pointer theme /usr/share/vtium/pointers.svg, and
# look-for: big's says vtium's own pictures, from vtium.conf. small's list
# look-for: offers the theme vtium.conf sets and its own file; picking the
# look-for: first puts vtium's own pictures on small's card.
# wrong-looks-like: Both looks showing the same theme, a list without the
# wrong-looks-like: theme vtium.conf sets, or a pick the card does not show.
# Each look picks its own pointer theme: a [look NAME] section may set
# pointer-theme, and a look that sets none uses the one vtium.conf sets at
# its top level. The page runs vtium-config on a vtium.conf of its own
# (common.inc's looks_tui), where small sets vtium's own pointer file and
# big sets nothing.
#
# References:
# - ADR-1309 D1: the pointer-theme key, which a look may set.
# - ADR-1201 D4: the look card's Pointer theme line and its list.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
looks_tui \
	'Each look has a pointer theme of its own.' \
	'  1. Press 3 for the Looks tab, Enter on the look big, and read its' \
	'     Pointer theme line. Press Esc.' \
	'  2. Down to the look small, press Enter, and read its Pointer theme line.' \
	'  3. Down to that line and press Enter for its list. Pick the theme' \
	'     vtium.conf sets, and check the line. Then quit with q, and n if it' \
	'     asks.'
