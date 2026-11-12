# vtium draws any font a frit names, bitmap or outline. These four bands are
# four captures of the same screen, each in one font and starting with its
# name and kind: Iosevka Term and JetBrains Mono are outline fonts, which
# vtium scales to the cell and antialiases; Terminus is a PSF console font,
# and GNU Unifont, which vtium carries, is drawn at twice its size, both as
# bitmaps of whole pixels. Each band's styles line shows italic, which vtium
# slants itself where a font has no italic face, in whole pixels in the two
# bitmap fonts; strike, a curly underline, raised and lowered text at two
# thirds of the size, and dim. [Choose a font](../fonts.md) shows how
# to choose your own.
# alt: Four horizontal bands on a black background, each naming its font,
# alt: Iosevka Term, Terminus, GNU Unifont and JetBrains Mono, then showing
# alt: in that font the same pangrams, digits and symbols, accented, Greek
# alt: and Cyrillic letters, a line of styles (italic, struck, curly
# alt: underlined, E = mc² and H₂O in raised and lowered text, and dim), and a
# alt: line of box drawing and block elements.

# shellcheck shell=bash
# Shared by the fonts scene's four screens (showcase-fonts-*.sh), which
# source it after showcase.inc: each switches the console to its frit with
# OSC 50, then draws the band, the same lines in every font, and reports the
# band's height in rows and its cell size on the event port, where
# t3_showcase.py reads them to crop the capture to the band.

# The rows the band takes: a blank row above and below its seven lines.
band_rows=9

# Draws the band for frit $1, whose name and kind the band's first line
# gives as $2 and $3.
band() {
	local cell
	switch_font "$1"
	# shellcheck disable=SC2154 # showcase.inc, sourced first, sets esc
	cell=$(ask "${esc}[16t" t)
	printf '\n'
	printf '  %s%s%s  %s%s%s\n' "$(fg 255 196 87)" "$2" "$(off)" "$(fg 150 150 150)" "$3" "$(off)"
	printf '  The quick brown fox jumps over the lazy dog.  Sphinx of black quartz, judge my vow.\n'
	printf '  0123456789  0O 1lI  {}[]()<>  != == => -> <= >=  && || #$%%@^*~  ½ × ÷ ≈ ≠ ± °C\n'
	printf '  Français, Español, Čeština, Ísland · Ελληνικά · Русский язык · Українська\n'
	printf '  %s[3mitalic%s[0m  %s[9mstruck%s[0m  %s[4:3mcurly%s[0m  E = mc%s[73m2%s[0m  H%s[74m2%s[0mO  %s[2mdim%s[0m\n' \
		"$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc"
	printf '  ┌──────┬──────┐  ╔══════╗  ░░▒▒▓▓██  ▁▂▃▄▅▆▇█  ◆ ● ■\n'
	printf '  └──────┴──────┘  ╚══════╝\n'
	printf '\n'
	if [[ $cell =~ \[6\;([0-9]+)\;([0-9]+)$ ]] && [ -n "${T3_EVENTS:-}" ]; then
		echo "T3-BAND $1 rows=$band_rows cell=${BASH_REMATCH[2]}x${BASH_REMATCH[1]}" >"$T3_EVENTS"
	fi
}
