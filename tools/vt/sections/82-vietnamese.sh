#!/bin/bash
# look-for: Each letter carries all its marks, one above the other where there
# look-for: are two (the e and a near the end), and the dots below stay under
# look-for: their letters; no mark is cut at the row's top.
# wrong-looks-like: A mark beside its letter, cut off, missing or drawn as a
# wrong-looks-like: box, or a letter in another font.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page vietnamese
