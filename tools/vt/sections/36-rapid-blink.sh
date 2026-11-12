#!/bin/bash
# look-for: Each "rapid:" run blinks faster than the "blink:" run, about two
# look-for: and a half times a second, in the style the "Blink:" line says;
# look-for: after 25 the text is steady, and steady text stays still.
# wrong-looks-like: A rapid run blinking at the slow rate or not at all, one
# wrong-looks-like: that stays gone, or steady text that flickers.
# Rapid blink, SGR 6, beside blinking text, SGR 5. vtium shows SGR 6 for
# 200 ms and hides it for 200 ms, 150 blinks a minute, the slowest rate
# ECMA-48 gives SGR 6; a cell with both blinks at SGR 6's rate. SGR 25 ends
# both. The last lines name the blink style vtium draws it in (blink.inc).
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
row() {
	printf '  %-12s steady  \033[%sm%s\033[0m  steady\n' "$1" "$2" "$3"
}
row '5' '5' 'blink: the slow rate'
row '6' '6' 'rapid: the fast rate'
row '6;31' '6;31' 'rapid: red text'
row '6;33;44' '6;33;44' 'rapid: yellow on blue'
row '6;4' '6;4' 'rapid: underlined'
row '5;6' '5;6' 'rapid: both 5 and 6'
printf '  %-12s steady  \033[6mrapid: here\033[25m, steady from here on\033[0m\n' '6, then 25'
printf '  %-12s steady  %s  steady\n' 'no SGR' 'blink: none, a reference'
blink_line
