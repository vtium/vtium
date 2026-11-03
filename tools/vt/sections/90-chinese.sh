#!/bin/bash
# look-for: Each Chinese character whole across its two cells, all the same
# look-for: size in one even line; every decoration reaches across both cells
# look-for: of each character.
# wrong-looks-like: A character cut, squeezed into one cell or overlapping its
# wrong-looks-like: neighbour, boxes, or a decoration under half a character.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page chinese
