#!/bin/bash
# look-for: Between the two marker lines, lines 7 to 12, in order; the marker
# look-for: lines and everything above them stay where they were.
# wrong-looks-like: Lines 1 to 6 still showing, a marker line or the page's
# wrong-looks-like: header scrolled away, or numbered lines outside the
# wrong-looks-like: markers.
# A scroll region, DECSTBM: six rows between two marker lines are the region,
# twelve numbered lines are printed into it, and only the region scrolls, so
# its last six lines stay and nothing outside it moves. DECSTBM places by
# absolute row, so the page reads the row it starts on first.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
top_row
first=$((top + 2)) last=$((top + 7))
printf '  Twelve lines printed into a scroll region of six rows:\n'
printf '  ---------------- above the region ----------------\n'
printf '%s[%d;%dH' "$esc" $((last + 1)) 1
printf '  ---------------- below the region ----------------'
printf '%s[%d;%dr%s[%d;1H' "$esc" "$first" "$last" "$esc" "$first"
for i in $(seq 12); do
	printf '  line %2d of 12, printed into the region\n' "$i"
done
# DECSTBM with no rows ends the region and homes the cursor.
printf '%s[r%s[%d;1H' "$esc" "$esc" $((last + 3))
printf '  The region ends, and the screen scrolls as a whole again.\n'
