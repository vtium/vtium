#!/bin/bash
# look-for: Kanji, the dense characters, and hiragana, the rounder ones, each
# look-for: whole across two cells and the same size in one even line;
# look-for: decorations reach across both cells.
# wrong-looks-like: A character cut, squeezed into one cell or overlapping,
# wrong-looks-like: boxes, or a decoration under half a character.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page japanese
