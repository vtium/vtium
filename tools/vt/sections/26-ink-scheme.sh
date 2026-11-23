#!/bin/bash
# look-for: Black text on a white panel. Red, yellow and magenta, normal and
# look-for: bright, all print the same red; green, blue, cyan and gray print
# look-for: black. White text shows only on the red and black backgrounds.
# wrong-looks-like: Any color other than black, red and white on the panel,
# wrong-looks-like: or text other than white that disappears into the white.
# Ink, the color scheme vtium ships for black, red and white displays such
# as three-color e-paper panels. The page sets the 16 colors from the
# shipped file with OSC 4, and paints a panel in color 7, the scheme's
# paper, with text in color 0, the scheme's ink. The walk puts the colors
# back after the page with OSC 104.
#
# References:
# - ADR-1205 D10: the color schemes vtium ships.
here=$(dirname "$(readlink -f "$0")")
palette=$here/../../../crates/vtium/install/palettes/ink.palette
[ -r "$palette" ] || { printf 'no color scheme at %s\n' "$palette"; exit 1; }
while IFS='=' read -r _ n c; do
	printf '\033]4;%s;%s\033\\' "${n// /}" "${c// /}"
done < <(grep '^palette *= *[0-9]*=#' "$palette")
paper=$'\033[0;30;47m'
line() { printf '%s  %s\033[K\033[0m\n' "$paper" "$1"; }
names=(black red green yellow blue magenta cyan white)
fg='' bfg='' bg=''
for i in 0 1 2 3 4 5 6 7; do
	fg+=$(printf '\033[3%dm%-8s' "$i" "${names[i]}")
	bfg+=$(printf '\033[9%dm%-8s' "$i" "${names[i]}")
	bg+=$(printf '\033[4%d;97m %-6s \033[30;47m' "$i" "${names[i]}")
done
line 'The color scheme Ink on its white paper:'
line ''
line "${fg}${paper}"
line "${bfg}${paper}"
line ''
line "${bg}"
line ''
line "$(printf '\033[1;31merror:\033[22;30m the build failed in \033[34mmain.rs\033[30m line 12')"
line "$(printf '\033[31m- a deleted line\033[30m')"
line "$(printf '\033[32m+ an added line\033[30m')"
line "$(printf '\033[7m reverse video on the paper \033[27m')"
