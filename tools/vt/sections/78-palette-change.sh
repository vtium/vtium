#!/bin/bash
# look-for: Every word written in color 1 shows orange, though it was written
# look-for: while color 1 was red; the color 9 words stay bright red.
# wrong-looks-like: The color 1 words still red, only some of them orange,
# wrong-looks-like: or the color 9 words changed too.
# A palette change, OSC 4, after the text was written: vtium resolves a cell's
# palette color when it draws, so the text already on screen takes the new
# color. accept.sh resets the palette (OSC 104) before the next page.
printf '  These words are written in color 1, which starts red:\n'
printf '    \033[31mcolor 1, color 1, color 1, color 1, color 1\033[0m\n'
printf '    \033[30;41m on color 1 \033[0m and \033[4;31munderlined in color 1\033[0m\n'
printf '  These are in color 9, bright red, which nothing changes:\n'
printf '    \033[91mcolor 9, color 9, color 9, color 9, color 9\033[0m\n\n'
printf '  Then OSC 4 sets color 1 to orange, rgb:ff/a5/00.\n'
printf '\033]4;1;rgb:ff/a5/00\033\\'
