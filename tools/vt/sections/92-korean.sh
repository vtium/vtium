#!/bin/bash
# look-for: Each Hangul syllable one square block across two cells, its parts
# look-for: packed inside it, with a space between the three words;
# look-for: decorations reach across each block.
# wrong-looks-like: A syllable split into separate letters, cut, overlapping
# wrong-looks-like: or drawn as a box.
# frits: playtest-outline playtest-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page korean
