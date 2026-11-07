#!/bin/bash
# look-for: Hebrew letters whole, one a cell; most letters are square blocks
# look-for: of one height; ל rises above them and ן drops below: both are
# look-for: right. Each line starts at the left edge and reads right to
# look-for: left: each word's first letter is its rightmost, and the words
# look-for: follow it leftward.
# wrong-looks-like: Boxes, a letter cut or overlapping its neighbour, a
# wrong-looks-like: decoration broken between letters, or a word whose
# wrong-looks-like: letters run left to right in typed order.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page hebrew
