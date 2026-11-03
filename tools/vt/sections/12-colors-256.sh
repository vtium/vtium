#!/bin/bash
# look-for: Six rows of 36 even swatches stepping smoothly in color, a grey
# look-for: ramp from near black to near white, and a smooth 24-bit ramp from
# look-for: green to red.
# wrong-looks-like: Two neighbouring swatches in one color, a gap or a
# wrong-looks-like: swatch of the default background, or visible steps in the
# wrong-looks-like: 24-bit ramp.
# The 256-color palette, SGR 48;5;n: the 6x6x6 color cube (16 to 231) and the
# grey ramp (232 to 255), then a 24-bit ramp, SGR 48;2;r;g;b, as the C
# reference's sgr screen drew them.
printf '  the 6x6x6 cube, colors 16 to 231\n'
for row in 0 1 2 3 4 5; do
	printf '  '
	for ((c = 16 + row * 36; c < 52 + row * 36; c++)); do
		printf '\033[48;5;%dm  ' "$c"
	done
	printf '\033[0m\n'
done
printf '\n  the grey ramp, colors 232 to 255\n  '
for ((c = 232; c <= 255; c++)); do
	printf '\033[48;5;%dm   ' "$c"
done
printf '\033[0m\n\n  24-bit, SGR 48;2, green to red\n  '
for ((i = 0; i < 72; i++)); do
	printf '\033[48;2;%d;%d;128m ' $((i * 255 / 71)) $((255 - i * 255 / 71))
done
printf '\033[0m\n'
