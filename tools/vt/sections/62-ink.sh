#!/bin/bash
# look-for: Outline: accents whole in the row above, under its letters; where
# look-for: descenders meet accents, each cell's own glyph on top; italics
# look-for: lean into the colored cell; Arabic marks whole; the cursor's cell
# look-for: shows neighbours' ink. Bitmap: whole letters, one baseline.
# wrong-looks-like: An accent cut flat at the top of its cell, ink over a
# wrong-looks-like: neighbour's letter, a seam where a lean meets the next
# wrong-looks-like: cell, a mark missing, or letters off one baseline.
# frits: acceptance-outline acceptance-bitmap
# Glyph ink past its cell (ADR-1306): an outline glyph may draw one row up
# and down and one cell sideways. Each case sits between empty or colored
# cells so that ink past a cell shows. The text is written as UTF-8 bytes so
# the output does not depend on the locale. On a terminal, the last line
# parks the cursor on a capital and waits for a key.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
reset=$'\033[0m'
caps=$'\xc3\x81\xc3\x89\xc3\x8e\xc3\x95\xc3\x9c \xe1\xba\xaa\xe1\xba\xb6\xe1\xbb\x82' # ÁÉÎÕÜ ẪẶỂ
lam=$'\xd9\x84\xd9\x91\xd9\x8e'                                                      # lam, shadda, fatha
beh=$'\xd8\xa8\xd9\x90'                                                              # beh, kasra
alef=$'\xd8\xa7\xd9\x92'                                                             # alef, sukun

# A letter in the pen PEN, then one yellow cell after it.
lean() {
	local pen=$1 l out=
	for l in H I E W m A O f; do
		out+="${esc}[${pen}m$l$reset${esc}[43m $reset"
	done
	printf '%s' "$out"
}

printf '  Ink past a cell shows over the colored cells and rows around it.\n'
printf '  xxxxxxxxxxxxxx\n'
printf '  %s  accents under a plain row\n' "$caps"
printf '%s[44;97m  xxxxxxxxxxxxxx%s\n' "$esc" "$reset"
printf '%s[41;97m  %s%s  on red, under blue\n' "$esc" "$caps" "$reset"
printf '%s[42;30m  gjpqygjpqygjpqy%s  descenders above\n' "$esc" "$reset"
printf '%s[45;97m  %s%s  accents below\n\n' "$esc" "$caps" "$reset"
printf '  %s  italic\n' "$(lean 3)"
printf '  %s  bold italic\n\n' "$(lean 1\;3)"
printf '%s[44m                    %s\n' "$esc" "$reset"
printf '%s[46;30m  %s %s %s  %s  Arabic marks\n' "$esc" "$lam" "$beh" "$alef" "$reset"
printf '%s[41m                    %s\n' "$esc" "$reset"
printf '%s[44;97m  xxxxxxxxxxxxxx%s  the cursor parks on Î below\n' "$esc" "$reset"
printf '  %s' "$caps"
printf '\r%s[5G' "$esc"
# Wait for a key with the cursor parked, but only on a terminal in a real
# walk; the walk's footer moves the cursor.
if [ -t 0 ] && [ -z "${ACCEPT_NOPROMPT:-}" ] && [ -z "${ACCEPT_NOPAGE:-}" ]; then
	IFS= read -rsn1 </dev/tty
fi
printf '\n'
