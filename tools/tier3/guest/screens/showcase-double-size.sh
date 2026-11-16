#!/bin/bash
# category: text and fonts
# hero: yes
# vtium draws lines of double size when a program asks for them. A title
# line is twice as tall and twice as wide, and a heading line is twice as
# wide. vtium's font helper renders each character of Iosevka Term at twice
# its size from the outline, so the large letters stay as smooth as the
# small ones. A double-width line holds 60 characters, half of a normal
# line's 120. Its ruler ends at the right edge, as the normal ruler under it
# does. Colors and every underline style follow the doubled letters.
# alt: A console screen in a smooth monospaced font: a gold title twice as
# alt: tall and twice as wide reading Lines of double size, a gray normal
# alt: line under it, a light blue heading twice as wide, words twice as
# alt: wide in single, double, curly, dotted and dashed underlines, struck
# alt: and overlined, a blue badge reading READY twice as tall beside the
# alt: words in 24-bit color, Greek and Russian words twice as wide, and two
# alt: rulers of digits, one twice as wide, both ending at the right edge.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
# ESC # 3 and ESC # 4 make the cursor's line the top and the bottom half of
# a line twice as tall and twice as wide (DECDHL), ESC # 6 makes it twice as
# wide (DECDWL), and ESC # 5 makes it a normal line again (DECSWL).
top=$'\033#3' bottom=$'\033#4' wide=$'\033#6'
gold=$(fg 255 196 87) blue=$(fg 120 180 255) gray=$(fg 150 150 150) o=$(off)
# ruler N: the digits 1 to 0 over and over, N of them
ruler() { local r=''; while [ ${#r} -lt "$1" ]; do r+='1234567890'; done; printf '%s' "${r:0:$1}"; }

printf '\n'
printf '%s  %sLines of double size%s\n' "$top" "$gold" "$o"
printf '%s  %sLines of double size%s\n' "$bottom" "$gold" "$o"
printf '  %sThe font helper renders each doubled character from Iosevka Term'"'"'s outline, at twice its size.%s\n' "$gray" "$o"
printf '\n'
printf '%s  %sDouble width: 60 characters a line%s\n' "$wide" "$blue" "$o"
printf '  A normal line beside it holds all 120 columns of the 1920x1080 display, in Iosevka Term at 30 points.\n'
printf '\n'
printf '%s  %s %s %s %s %s %s %s\n' "$wide" \
	"${esc}[4msingle$o" "${esc}[4:2mdouble$o" "${esc}[4:3;58:2::224:108:117mcurly$o" \
	"${esc}[4:4mdotted$o" "${esc}[4:5mdashed$o" "${esc}[9mstruck$o" "${esc}[53mover$o"
printf '\n'
badge="$(bg 40 90 170)$(fg 255 255 255) READY $o"
words="$(fg 240 140 40)amber$o $(fg 60 190 170)teal$o $(fg 180 120 240)violet$o"
printf '%s  %s %s\n' "$top" "$badge" "$words"
printf '%s  %s %s\n' "$bottom" "$badge" "$words"
printf '\n'
printf '%s  Ελληνικά, Русский, Café, Ångström\n' "$wide"
printf '\n'
printf '%s%s\n' "$wide" "$(ruler $((cols / 2)))"
printf '%s\n' "$(ruler "$cols")"
