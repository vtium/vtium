#!/bin/bash
# look-for: In bitmap-outline every character has hard edges, each pixel fully
# look-for: on or off: the symbols, Hebrew, Devanagari and CJK match Terminus's
# look-for: Latin letters. In bitmap-outline-smooth those characters have soft
# look-for: grey edges. Emoji stay in color in both.
# wrong-looks-like: Soft grey edges in bitmap-outline, hard edges on the
# wrong-looks-like: outline characters in bitmap-outline-smooth, a stroke
# wrong-looks-like: broken or missing, or an emoji in one color.
# frits: bitmap-outline bitmap-outline-smooth
# Outline glyphs that fill the gaps of a bitmap font. Both font sets start
# with Terminus at 8x16 and fill the characters it lacks from outline fonts.
# In bitmap-outline those outline glyphs draw 1-bit and hinted to the pixel
# grid, so the page reads as one bitmap font. bitmap-outline-smooth adds
# render=smooth under each outline source, and its outline glyphs draw
# antialiased. Box drawing and blocks are vtium's own sprites in both.
#
# References:
# - ADR-1200 D1, the render= row: the 1-bit default behind a bitmap first
#   source, the threshold, and render=smooth.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"

printf '  Latin, from Terminus:    Hamburgefonstiv 0123456789 AEOU aeou\n'
printf '  Symbols, DejaVu Sans:    ★ ☆ ♠ ♣ ♥  ∀ ∃ ∑ ≠ ∂ ∫  ⇒ ⇔  ✓ ✗  ♪ ♫  ℃\n'
printf '  Hebrew, Noto Sans:       שלום עולם\n'
printf '  Devanagari, Noto Sans:   नमस्ते दुनिया\n'
printf '  CJK, Noto Sans CJK:      中文 日本語 한국어\n'
printf '  Bold:                    %s[1mHamburg ★ ∀ ∑ 中文%s[0m\n' "$esc" "$esc"
printf '  Reverse:                 %s[7mHamburg ★ ∀ ∑ 中文%s[0m\n' "$esc" "$esc"
printf '  Emoji, in color:         😀 🚀 🌈\n'
printf '  Box drawing, by vtium:   ┌──┬──┐ ▀▄█ ░▒▓\n'
printf '                           └──┴──┘ ▄▀█ ░▒▓\n\n'
frit_line
