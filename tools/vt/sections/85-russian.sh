#!/bin/bash
# look-for: Cyrillic letters whole, one a cell, of even height in one line;
# look-for: underlines and strikes run unbroken across each word.
# wrong-looks-like: Boxes, letters of uneven height or in another font, or a
# wrong-looks-like: decoration broken inside a word.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page russian
