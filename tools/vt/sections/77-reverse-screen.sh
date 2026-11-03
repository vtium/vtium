#!/bin/bash
# look-for: The whole screen inverted, header and questions too: dark text on
# look-for: a light background. Each colored word is dark letters on its
# look-for: color; "reversed" is light letters on dark.
# wrong-looks-like: Only some rows inverted, a dark background left at the
# wrong-looks-like: edges, or a colored word that lost its color.
# Reverse video for the whole screen, DECSCNM (DEC private mode 5). demo.sh
# turns it off again (CSI ? 5 l) before the next page.
printf '\033[?5h'
printf '  DECSCNM, mode 5, shows the whole screen in reverse video.\n\n'
printf '  The background is light and this text is dark, to the edges of the\n'
printf '  screen, the header above and the questions below included.\n\n'
printf '  A colored word turns into its color behind dark letters: \033[31mred\033[0m, \033[32mgreen\033[0m.\n'
printf '  Yellow on blue turns into blue on yellow: \033[33;44myellow on blue\033[0m.\n'
printf '  A word in reverse video (SGR 7) turns back to light on dark: \033[7mreversed\033[0m.\n'
