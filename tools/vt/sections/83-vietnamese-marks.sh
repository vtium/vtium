#!/bin/bash
# look-for: The same words as the Vietnamese page, built from letters and
# look-for: separate combining marks: each mark sits on its letter, the shapes
# look-for: match that page, and no gap opens between letters.
# wrong-looks-like: A mark in a cell of its own beside its letter, marks
# wrong-looks-like: stacked in the wrong order, or letters spread apart.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page vietnamese-marks
