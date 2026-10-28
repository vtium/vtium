#!/bin/bash
# look-for: In each group the colon row matches the semicolon row swatch
# look-for: for swatch; both halves of the last line are bold green on blue.
# wrong-looks-like: A colon row with default text and empty backgrounds, or
# wrong-looks-like: a plain colon half of the last line: stock kmscon drops
# wrong-looks-like: colon sequences.
# Colour selection with sub-parameters (CSI 38:5:n m, CSI 38:2::r:g:b m) beside
# the semicolon forms. Upstream libtsm discards a whole SGR sequence holding a
# colon, so there the colon rows show no colour at all.
idx='196 208 226 46 51 21 129 201'
rgb='255:40:40 255:150:0 230:220:0 40:200:80 0:200:220 40:80:255 150:60:220 240:60:180'

# One row of swatches: SGR <prefix><colour>, with the colour's parts joined by
# <sep>.
swatches() {
	local label=$1 prefix=$2 set=$3 sep=$4 text=$5 c
	printf '  %-14s' "$label"
	for c in $set; do
		printf '\033[%sm%s\033[0m ' "$prefix${c//:/$sep}" "$text"
	done
	printf '\n'
}
printf '  256 colours, text\n'
swatches '38;5;n' '38;5;' "$idx" ';' '████'
swatches '38:5:n' '38:5:' "$idx" ':' '████'
printf '\n  256 colours, background\n'
swatches '48;5;n' '48;5;' "$idx" ';' '    '
swatches '48:5:n' '48:5:' "$idx" ':' '    '
printf '\n  24-bit, text\n'
swatches '38;2;r;g;b' '38;2;' "$rgb" ';' '████'
swatches '38:2::r:g:b' '38:2::' "$rgb" ':' '████'
printf '\n  24-bit, background\n'
swatches '48;2;r;g;b' '48;2;' "$rgb" ';' '    '
swatches '48:2::r:g:b' '48:2::' "$rgb" ':' '    '
printf '\n  mixed in one sequence: \033[1;38:5:46;48;5;19m bold green on blue \033[0m'
printf ' \033[1;38;5;46;48;5;19m bold green on blue \033[0m\n'
