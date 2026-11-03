#!/bin/bash
# look-for: The English words in every style, each drawn distinct as on the
# look-for: sgr and underline pages; every decoration runs under or through
# look-for: all of the words.
# wrong-looks-like: A style missing or on the wrong words, a decoration with
# wrong-looks-like: gaps, or a line in another font.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page english
