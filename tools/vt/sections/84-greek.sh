#!/bin/bash
# look-for: Greek letters whole, one a cell, in one even line; each accent
# look-for: sits above its vowel, as on the first letter of the sentence and
# look-for: on the vowels of the styled words.
# wrong-looks-like: Boxes, an accent beside or missing from its vowel, or
# wrong-looks-like: Latin letters standing in for Greek ones.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page greek
