#!/bin/bash
# The probe row of tier 3's font-sources case (t3_fonts.py): the letter g,
# the Powerline separator U+E0B0, the Han character U+4E2D, U+20000 beyond
# the Basic Multilingual Plane, the emoji U+1F600, the Nerd Fonts icon U+F121
# and the Font Logos icon U+F303, each at the column t3_fonts.COLUMNS names on
# the screen's second row. The case works out from the pixels which font drew
# each one. The rows above and below the probes stay blank, so the prompt
# comes on the fifth row. The characters are written as UTF-8 bytes, so the
# output does not depend on the locale.
printf '\n'
printf '  g  \xee\x82\xb0  \xe4\xb8\xad  \xf0\xa0\x80\x80  \xf0\x9f\x98\x80  \xef\x84\xa1  \xef\x8c\x83\n\n\n'
