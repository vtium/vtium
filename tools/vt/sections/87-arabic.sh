#!/bin/bash
# look-for: Arabic letters whole, each with its dots, joined into words:
# look-for: each letter takes the form for the start, middle or end of its
# look-for: word and meets its neighbours, and lam then alef draws as one
# look-for: combined letter. Each line starts at the left edge and reads
# look-for: right to left.
# wrong-looks-like: Boxes, letters standing apart in the forms they take
# wrong-looks-like: alone, a word whose letters run left to right in typed
# wrong-looks-like: order, dots missing or under the wrong letter, the small
# wrong-looks-like: vowel stroke away from its letter, or a letter cut off.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page arabic
