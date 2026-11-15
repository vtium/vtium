#!/bin/bash
# category: input and pointer
# hero: yes
# A program changes the mouse pointer's shape with OSC 22, and vtium draws
# the shape itself: here a program shows a list of links and sets the
# pointer to the hand that marks something to click. vtium has a picture of
# its own for each of ten shapes: the arrow, an I-beam over text, the hand,
# a crosshair, an hourglass, the arrow with a small hourglass, a circle with
# a slash, four arrows for a move, and double arrows for resizing sideways
# and up and down.
# alt: A console screen in a smooth monospaced font: a heading, a list of
# alt: four underlined web addresses, and a line that names the ten pointer
# alt: shapes vtium draws. The mouse pointer, a white hand with a raised
# alt: finger and a dark outline, points at the second address.

# The capture shows the pointer only when the showcase variant moves the
# mouse onto the second address and draws the pointer in software: QEMU's
# screendump leaves the display's cursor plane out.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
d=$(fg 150 150 150) h=$(fg 255 196 87) o=$(off)
link() { printf '    %s]8;;%s%s\\%s%s]8;;%s\\%s\n' "$esc" "$1" "$esc" "$(fg 97 175 239)" "$1" "$esc" "$o"; }
printf '\n  %sLinks a program prints%s\n\n' "$h" "$o"
link https://vtium.github.io/vtium/
link https://sw.kovidgoyal.net/kitty/pointer-shapes/
link https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
link https://www.kernel.org/doc/html/latest/gpu/drm-kms.html
printf '\n  %sThe pointer over a link is the hand, which the program sets with OSC 22.%s\n' "$d" "$o"
printf '  %sShapes vtium draws: default, text, pointer, crosshair, wait, progress,%s\n' "$d" "$o"
printf '  %snot-allowed, move, ew-resize and ns-resize.%s\n' "$d" "$o"
printf '%s]22;pointer%s%s' "$esc" "$esc" "\\"
