#!/bin/bash
# look-for: Hebrew letters whole, one a cell; most letters are square blocks
# look-for: of one height; ל rises above them and ן drops below: both are
# look-for: right. vtium does not order right-to-left text yet, so the words
# look-for: run left to right in typed order: expected today.
# wrong-looks-like: Boxes, a letter cut or overlapping its neighbour, or a
# wrong-looks-like: decoration broken between letters.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page hebrew
