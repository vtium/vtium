#!/bin/bash
# look-for: The whole screen inverted, header and questions too: a light
# look-for: background with dark text. The colored words keep their colors;
# look-for: "reversed" shows dark on light the other way round.
# wrong-looks-like: Only some rows inverted, a dark background left at the
# wrong-looks-like: edges, or colored words that lost their color.
# Reverse video for the whole screen, DECSCNM (DEC private mode 5). demo.sh
# turns it off again (CSI ? 5 l) before the next page.
printf '\033[?5h'
printf '  DECSCNM, mode 5, shows the whole screen in reverse video.\n\n'
printf '  The background is light and this text is dark, to the edges of the\n'
printf '  screen, the header above and the questions below included.\n\n'
printf '  Colors stay: \033[31mred\033[0m, \033[32mgreen\033[0m, \033[33;44myellow on blue\033[0m.\n'
printf '  A cell in reverse video (SGR 7) turns back: \033[7mreversed\033[0m.\n'
