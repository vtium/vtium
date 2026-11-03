#!/bin/bash
# look-for: Arabic letters whole, one a cell, each with its dots. vtium does
# look-for: not join Arabic letters or order right-to-left text yet, so the
# look-for: letters stand apart, left to right: expected today.
# wrong-looks-like: Boxes, dots missing or under the wrong letter, the small
# wrong-looks-like: vowel stroke away from its letter, or a letter cut off.
# frits: acceptance-outline acceptance-bitmap
# A language page (ADR-1501 D7): the first sentence of Article 1 of the
# Universal Declaration of Human Rights in this language, then a few of its
# words in every style, drawn once in each of the frits above, which the
# walk switches to before the page.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
language_page arabic
