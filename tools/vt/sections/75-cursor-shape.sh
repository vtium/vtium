#!/bin/bash
# look-for: The cursor after the prompt at the bottom is a steady underline,
# look-for: one cell wide; it does not blink.
# wrong-looks-like: A block or a bar, an underline that blinks, or no cursor.
# The cursor's shape, DECSCUSR (CSI n SP q): 4 is a steady underline. vtium
# keeps the shape until a program sets another; accept.sh sets the default
# (CSI 0 SP q) again before the next page.
printf '  DECSCUSR 4 sets the cursor to a steady underline.\n\n'
printf '  Look at the cursor after the prompt at the bottom of the screen.\n'
printf '  vtium draws a blinking block until a program asks for another shape;\n'
printf '  the next page has the block again.\n'
printf '\033[4 q'
