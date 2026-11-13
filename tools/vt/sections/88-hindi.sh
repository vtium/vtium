#!/bin/bash
# look-for: Devanagari letters whole, joined consonants one shape where the
# look-for: font has one, vowel signs beside or above their letters; breaks in
# look-for: the top bar at cell edges are expected today. In the bitmap set,
# look-for: each letter and its vowel signs are squeezed sideways into one
# look-for: cell. They look narrow and dense, but no stroke is cut off, and the
# look-for: stem of the vowel sign ा stands at the cell's right edge.
# wrong-looks-like: Boxes, dotted circles, a vowel sign on the wrong side of
# wrong-looks-like: its consonant, a sign in a cell of its own, or a letter
# wrong-looks-like: whose sides are cut off.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page hindi
