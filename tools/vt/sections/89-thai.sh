#!/bin/bash
# look-for: Thai letters with their vowel and tone marks above and below,
# look-for: stacked where two meet, none touching the row above; the words run
# look-for: without spaces, as Thai is written.
# wrong-looks-like: Boxes, dotted circles, a mark in a cell of its own beside
# wrong-looks-like: its letter, or a mark cut at the row's edge.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page thai
