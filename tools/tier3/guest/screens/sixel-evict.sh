#!/bin/bash
# kit: sixel
# look-for: The last of N pictures under its caption. Logo+= once turns the
# look-for: screen a quarter and the picture shows turned. Do not scroll while
# look-for: turned. Logo+- once turns it back; then Shift+PageUp to the top:
# look-for: picture 1's caption over blank cells, picture 2 on drawn. With
# look-for: --hwaccel, picture 1 drawn too: the GPU's turned copy has a budget
# look-for: of its own and evicts nothing (ADR-103's Memory section).
# wrong-looks-like: After Logo+=, the last picture missing, its caption over
# wrong-looks-like: blank cells (left out for the budget); in scrollback, a
# wrong-looks-like: picture after the first blank, or, without --hwaccel, the
# wrong-looks-like: first still drawn. Without --hwaccel, scrolling (wheel,
# wrong-looks-like: Shift+Up, Shift+PageUp) while turned or upside down blanks
# wrong-looks-like: more pictures by design (ADR-103 Q1): press r to redraw the
# wrong-looks-like: page and repeat the keys exactly.
# screen-text: none, sixel pictures do not print as rows
# The image budget under pressure (ADR-103's Q1 note): a turned or zoomed
# screen draws a scaled copy of each picture on it, and the copy for a
# picture on screen may evict pictures in scrollback when that lets it fit.
# N pictures (12, or more on a short screen), each a band the screen's width
# in one color, fill image-storage-limit=auto as the terminal's size gives
# it, four frames in ARGB of its cells from 16 MiB to 192 MiB, to within a
# picture's rounding, so a scaled copy fits only once the oldest picture,
# picture 1, has gone. Each sits under its caption; the last starts below a
# gap taller than the screen is wide in rows, so that turning the screen,
# which pulls lines back from scrollback onto the taller screen, leaves the
# others wholly in scrollback. With a set image-storage-limit the pictures
# do not fill it, and nothing is evicted. Scrolled back while turned, each
# picture that comes into view needs its own copy and evicts the oldest
# again, so the page asks to turn the screen back before scrolling. With
# hwaccel on, gltex's copies are textures in a budget of their own, which
# evict no picture, so every picture stays. With
# hwaccel on, gltex's copies are textures in a budget of their own, which
# evict no picture, so every picture stays.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/sixel-common.inc"
sixel_offered
cell_size
term_size

# the auto limit for a display the size of the terminal's cells
W=$((cols * cw))
H=$((rows * ch))
limit=$((W * H * 16))
((limit < 16 << 20)) && limit=$((16 << 20))
((limit > 192 << 20)) && limit=$((192 << 20))

# N pictures W wide, each the limit's share less its record's 512 bytes, at
# most the screen less eight rows tall
n=12
max=$(((rows - 8) * ch))
((n < limit / (4 * W * max + 512) + 1)) && n=$((limit / (4 * W * max + 512) + 1))
h=$(((limit / n - 512) / (4 * W)))
hrows=$(((h + ch - 1) / ch))
# lines from the second-last picture to the last: more than a turned screen has rows
gap=$((W / ch + 2))

colors=(100,0,0 0,100,0 0,0,100 100,100,0 0,100,100 100,0,100
	100,50,0 50,0,100 0,100,50 100,0,50 50,100,0 0,50,100)
for ((k = 1; k <= n; k++)); do
	((k == n)) && printf '%*s' "$gap" '' | tr ' ' '\n'
	printf 'picture %d of %d, %dx%d pixels\n' "$k" "$n" "$W" "$h"
	swatch "$W" "$h" "${colors[(k - 1) % 12]}"
	printf '\n'
	((k < n)) && printf '\n'
done
printf 'Logo+= once turns the screen: the last picture stays drawn, turned.\n'
printf 'Do not scroll while turned. Logo+- once turns it back. Shift+PageUp\n'
printf 'to the top: picture 1 blank under its caption, picture 2 on drawn.\n'
