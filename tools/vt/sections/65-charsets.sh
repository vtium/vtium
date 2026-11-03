#!/bin/bash
# look-for: In each pair the left side, drawn through a character set or REP,
# look-for: matches the right side, written out in Unicode, character for
# look-for: character.
# wrong-looks-like: Letters such as lqqk where lines belong, a pair that
# wrong-looks-like: differs, or text after a pair still in line-drawing
# wrong-looks-like: characters.
# Character sets and REP: the DEC special graphics set chosen into G0 with
# ESC ( 0 and back with ESC ( B, the same set in G1 shifted in with SO and
# out with SI, and REP (CSI n b), which repeats the last character printed.
# Each is printed beside what it must look like, written out in Unicode.
g0=$'\033(0' ascii=$'\033(B' so=$'\016' si=$'\017'
printf '  DEC special graphics in G0, ESC ( 0, then ESC ( B:\n'
printf '    %s%s%s   ┌───┬───┐\n' "$g0" 'lqqqwqqqk' "$ascii"
printf '    %s%s%s   │   │   │\n' "$g0" 'x   x   x' "$ascii"
printf '    %s%s%s   ├───┼───┤\n' "$g0" 'tqqqnqqqu' "$ascii"
printf '    %s%s%s   └───┴───┘\n' "$g0" 'mqqqvqqqj' "$ascii"
printf '    %s%s%s   ◆▒°±┘┐┌└┼⎺⎻─⎼⎽├┤┴┬│≤≥π≠£·\n' "$g0" '`afgjklmnopqrstuvwxyz{|}~' "$ascii"
printf '    back to ASCII after ESC ( B: lqqk\n'
echo
printf '  The same set in G1 (ESC ) 0), shifted in with SO and out with SI:\n'
printf '\033)0    abc%slqqk%sabc   abc┌──┐abc\n' "$so" "$si"
echo
printf '  REP, CSI n b, repeats the last character printed n more times:\n'
printf '    x, REP 5:   x\033[5b   xxxxxx\n'
printf '    =, REP 20:  =\033[20b|   =====================|\n'
printf '    中, REP 3:  中\033[3b|   中中中中|\n'
