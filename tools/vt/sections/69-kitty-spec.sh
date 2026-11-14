#!/bin/bash
# look-for: Three bars of eight color blocks in a row. All three show the
# look-for: same colors: orange, yellow, teal, purple, pink and gray, then
# look-for: the background and the text color. Below them, two replies that
# look-for: each end at the closing quote, and one red square.
# wrong-looks-like: The first bar still in red, green, yellow, blue, magenta
# wrong-looks-like: and cyan, a bar missing while the page says it was sent,
# wrong-looks-like: a \n before a closing quote, EINVAL in the second reply, or
# wrong-looks-like: a green square.
# The fixes that make vtium's kitty graphics follow the kitty graphics
# protocol's text (ADR-1100 D4, D14, D15).
#
# The bars: each is one kitty picture in the palette format f=8, eight
# pixels that are indices into the terminal's colors: 1 to 6, then 248, the
# default background, and 249, the default foreground. The first bar is sent
# before OSC 4 changes colors 1 to 6, and the spec says such a picture "is
# drawn using the current colors of the terminal and is automatically
# redrawn when they change", so it takes the new colors. The second bar is
# sent after the change. The third is the same indices compressed with zlib
# (o=z), which the image helper inflates and returns unconverted; it is sent
# only when the helper decodes.
#
# The replies: vtium writes each reply of the terminal to the page, with
# printf %q, so a line feed shows as $'\n'. A composition whose destination
# frame does not exist ends with no line feed, since the spec says a reply
# holds "only printable characters and spaces". A placement that names
# itself as its parent is answered ECYCLE, as the spec answers a cycle.
#
# The square: a red picture with the id 40, then a green picture with no
# id, at the same cell and z. A picture with no id has the id 0, and the
# lower id draws below, so the red one stays on top.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
kitty_shown || cannot_show "vtium was built without pictures: it does not answer a kitty query."
top_row
bar='AQIDBAUG+Pk='
printf '  kitty f=8 pictures in the terminal'"'"'s colors (indices 1 to 6, 248, 249):\n'
row=$((top + 2))
printf '%s[%d;3H' "$esc" "$row"
kitty_send "a=T,i=41,q=2,C=1,f=8,s=8,v=1,c=24,r=1" "$bar"
printf '%s[%d;30Hsent before the colors change' "$esc" "$row"
printf '\033]4;1;rgb:ff/80/00\033\\\033]4;2;rgb:f0/e0/40\033\\\033]4;3;rgb:20/a0/a0\033\\'
printf '\033]4;4;rgb:80/40/c0\033\\\033]4;5;rgb:ff/90/c0\033\\\033]4;6;rgb:80/80/80\033\\'
printf '%s[%d;3H' "$esc" $((row + 1))
kitty_send "a=T,i=42,q=2,C=1,f=8,s=8,v=1,c=24,r=1" "$bar"
printf '%s[%d;30Hsent after the change' "$esc" $((row + 1))
printf '%s[%d;3H' "$esc" $((row + 2))
if helper_decodes; then
	kitty_send "a=T,i=43,q=2,C=1,f=8,s=8,v=1,o=z,c=24,r=1" "eJxjZGJmYWX78RMAA1MCBw=="
	printf '%s[%d;30Hsent with zlib, o=z' "$esc" $((row + 2))
else
	printf '%s[%d;30Hnot sent: the image helper does not decode here' "$esc" $((row + 2))
fi
printf '%s[%d;1H' "$esc" $((row + 4))
kitty_send "a=T,i=44,p=1,q=2,C=1,f=24,s=1,v=1" "/wAA"
printf '  A composition onto frame 5, which does not exist:\n'
r=$(ask "${esc}_Ga=c,i=44,r=1,c=5${esc}\\" '\')
printf '    %q\n' "${r%"$esc"}"
printf '  A placement that names itself as its parent:\n'
r=$(ask "${esc}_Ga=p,i=44,p=1,P=44,Q=1${esc}\\" '\')
printf '    %q\n' "${r%"$esc"}"
printf '  A red picture with i=40, then a green one with no id, on one cell: '
kitty_send "a=T,i=40,q=2,C=1,f=24,s=1,v=1,c=2,r=1" "/wAA"
kitty_send "a=T,q=2,C=1,f=24,s=1,v=1,c=2,r=1" "AP8A"
printf '\n\n'
printf '\033]104\033\\'
