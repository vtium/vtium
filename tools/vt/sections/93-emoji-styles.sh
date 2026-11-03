#!/bin/bash
# look-for: Each emoji whole across two cells, in color in the outline font
# look-for: and in one color in the bitmap font; the family, the flag and the
# look-for: toned fist are one picture each; decorations reach across them.
# wrong-looks-like: An emoji in pieces, a flag drawn as two letters, an emoji
# wrong-looks-like: cut at its cells, or a decoration that stops short.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page emoji
