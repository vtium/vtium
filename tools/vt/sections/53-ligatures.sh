#!/bin/bash
# look-for: In the code, ->, =>, !=, ===, <=, >=, &&, || and /* draw as
# look-for: the font's joined shapes, each across the cells of its
# look-for: characters, and the columns line up with the ruler. The spaced
# look-for: and two-colored operators, www, 0x and the 15 equals signs draw
# look-for: each character on its own.
# wrong-looks-like: Every operator drawn as separate characters, a joined
# wrong-looks-like: shape that pushes the text after it out of line with
# wrong-looks-like: the ruler, a two-colored operator drawn joined, or a
# wrong-looks-like: piece of a shape missing or doubled.
# frits: ligatures-jetbrains
# Programming ligatures (ADR-1000's text.ligatures): with font-ligatures on
# in vtium.conf, vtium draws a run of 2 to 14 ASCII symbols on one row as
# the font shapes it, across the cells of its characters. The terminal
# keeps each character in its own cell, so the ruler under each line of
# code still counts one column for each character. A run whose characters
# differ in color draws apart, and so do letters and digits, which never
# join. The page writes plain ASCII, so it does not depend on the locale.
#
# References:
# - ADR-1308 D8: which cells join, the option, and when a ligature draws
#   apart.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
red=$'\033[31m' green=$'\033[32m' reset=$'\033[0m'
ruler='  0123456789012345678901234567890123456789'

printf '  Operators in a line of code, each followed by the ruler:\n\n'
printf '  if (a != b && c >= d) || e <= f { go(); }\n'
printf '%s\n' "$ruler"
printf '  let f = |x| x -> y; match v { _ => 0 }\n'
printf '%s\n' "$ruler"
printf '  a === b /* note */ x == y\n'
printf '%s\n\n' "$ruler"
printf '  Drawn apart, each character on its own:\n\n'
printf '  spaced:      - >  ! =  = = =\n'
printf '  two colors:  %s-%s>%s  %s!%s=%s\n' "$red" "$green" "$reset" "$red" "$green" "$reset"
printf '  letters:     www 0x 0xff\n'
printf '  15 of them:  ===============\n\n'
frit_line
