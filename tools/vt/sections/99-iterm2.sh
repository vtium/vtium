#!/bin/bash
# look-for: Two pictures of four colored squares, red top left, green top
# look-for: right, blue bottom left, yellow bottom right: one 20 cells by 5
# look-for: rows, one 10 by 5 squeezed; the text after each sits beside it.
# wrong-looks-like: A square in the wrong corner, a missing or broken picture,
# wrong-looks-like: or a picture of the wrong size.
# iTerm2 inline pictures, OSC 1337 File=, which vtium hands whole to its image
# helper to decode and places at the cursor, as a sixel is placed: the PNG of
# four colored squares (64x32), once at 20 cells by 5 rows and once at 10 by
# 5 with preserveAspectRatio=0, which stretches it to that box. The page
# places each by row and column, so the cursor's move after a picture does
# not matter.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
helper_decodes || cannot_show "the image helper did not decode a PNG; vtium runs without one."
top_row
quads='iVBORw0KGgoAAAANSUhEUgAAAEAAAAAgCAIAAAAt/+nTAAAASElEQVR42u3PMQ0AIAwAMJTsRgQiEIsSbm5uJOBhyb4mNdB2IkrFGqWagICAgICAgICAgEA60Oct9XaUEhAQEBAQEBAQEBBI+1bKyJcPtOjvAAAAAElFTkSuQmCC'
picture() {
	printf '%s]1337;File=inline=1;width=%s;height=%s;preserveAspectRatio=0:%s\a' "$esc" "$1" "$2" "$quads"
}
printf '  iTerm2 pictures, OSC 1337 File=, decoded by the image helper:\n'
row=$((top + 2))
printf '%s[%d;3H' "$esc" "$row"
picture 20 5
printf '%s[%d;24H<- 20 by 5' "$esc" $((row + 2))
printf '%s[%d;40H' "$esc" "$row"
picture 10 5
printf '%s[%d;51H<- 10 by 5' "$esc" $((row + 2))
printf '%s[%d;1H' "$esc" $((row + 6))
