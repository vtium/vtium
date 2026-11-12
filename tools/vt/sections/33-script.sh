#!/bin/bash
# look-for: Each raised or lowered glyph is about two thirds of normal size,
# look-for: its top on the row's top or its bottom on the row's bottom; tails
# look-for: and accents stay in their row; a small wide glyph is centred on its
# look-for: two cells; under "under" the line lies below the raised letters.
# wrong-looks-like: A small glyph at full size, cut off, or off its row's top
# wrong-looks-like: or bottom; a line through the wrong part of a small glyph.
# frits: acceptance-outline acceptance-bitmap script-terminus
# Superscript and subscript, SGR 73 and 74, ended by 75 (ADR-1000's
# sgr.script, ADR-1202): vtium draws the glyph at two thirds of its size,
# against the top of the text band for 73 and its bottom for 74. The page is
# walked in three font sets: an outline set, scaled smoothly; a bitmap set
# with no smaller size, whose glyphs vtium scales with every pixel on or
# off; and script-terminus, whose script-source= line names Terminus 12, a
# smaller size of the same font, drawn whole. The text is written as UTF-8
# bytes so the output does not depend on the locale.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
accents=$'\xc3\x89\xc3\x85'   # ÉÅ
wide=$'\xe4\xb8\xad\xe6\x96\x87' # 中文
printf '  x%s[73m2%s[75m + y%s[73m10%s[75m = z     H%s[74m2%s[75mO     C%s[74m6%s[75mH%s[74m12%s[0m\n' \
	"$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc"
printf '  normal %s[73msuperscript%s[75m %s[74msubscript%s[75m normal\n' "$esc" "$esc" "$esc" "$esc"
printf '  %s[73mgjpqy %s%s[75m gjpqy %s %s[74mgjpqy %s%s[0m  tails and accents\n' \
	"$esc" "$accents" "$esc" "$accents" "$esc" "$accents" "$esc"
printf '  %s[73m%s%s[75m%s%s[74m%s%s[0m  wide characters\n' "$esc" "$wide" "$esc" "$wide" "$esc" "$wide" "$esc"
printf '  %s[1;73mbold%s[0m %s[3;74mitalic%s[0m %s[31;73mred%s[0m %s[7;74mreversed%s[0m\n' \
	"$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc"
printf '  %s[4;73munder%s[0m %s[9;74mstruck%s[0m %s[53;73mover%s[0m  %s[4mx%s[73m2%s[75m+y%s[74m2%s[75m=z%s[0m\n' \
	"$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc" "$esc"
echo
frit_line
