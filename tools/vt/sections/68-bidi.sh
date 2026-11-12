#!/bin/bash
# look-for: Hebrew runs read from the right: א, like an X, ends each Hebrew
# look-for: word at its right; 123 still reads 123; brackets face what they
# look-for: hold. The last two lines end at the screen's right edge, and the
# look-for: Arabic letters join into one word.
# wrong-looks-like: א at the left of its word, 321, a bracket facing away, a
# wrong-looks-like: last line at the left edge, or Arabic letters apart.
# Bidirectional text (ADR-1000's text.bidi, ADR-1004): vtium keeps each line
# in the order it was typed and draws each paragraph in the order Unicode's
# bidirectional algorithm gives. The Hebrew is alef, bet, gimel (אבג), typed
# in that order, so alef, the letter typed first, stands at the right end of
# each Hebrew run. Digits keep their own order, brackets are drawn mirrored
# where the algorithm says, and with DEC mode 2501 set a paragraph whose
# first letter is Hebrew or Arabic starts at the right edge. Only a build
# with the bidi feature has any of it; DECRQM answers mode 8 (BDSM) with 0 in
# one without. The page puts mode 2501 back as it found it. The text is
# written as UTF-8 bytes so the output does not depend on the locale.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"
bdsm=$(ask "${esc}[8\$p" y)
[[ $bdsm =~ \[8\;([0-4])\$$ ]] ||
	cannot_show "The terminal does not answer DECRQM for mode 8, BDSM."
[ "${BASH_REMATCH[1]}" != 0 ] ||
	cannot_show "vtium was built without bidirectional text: DECRQM answers mode 8 with 0."
# DEC mode 2501, the paragraph direction detected, as it was before the page.
detect=$(ask "${esc}[?2501\$p" y)
[[ $detect =~ \;1\$$ ]] && put_back=h || put_back=l
heb=$'\xd7\x90\xd7\x91\xd7\x92'   # alef bet gimel
heb2=$'\xd7\x93\xd7\x94\xd7\x95'  # dalet he vav
arabic=$'\xd8\xb3\xd9\x84\xd8\xa7\xd9\x85' # seen lam alef meem
printf '  Hebrew typed in its own order, alef first:\n\n'
printf '  %-28s abc %s xyz\n' 'Hebrew among Latin:' "$heb"
printf '  %-28s abc %s 123 xyz\n' 'a number after it:' "$heb"
printf '  %-28s abc (%s) xyz\n' 'in brackets:' "$heb"
printf '  %-28s %s (%s) %s\n\n' 'in a Hebrew paragraph:' "$heb" "$heb2" "$heb"
printf '  Two paragraphs that start right to left, at the right edge:\n'
printf '%s[?2501h%s 123 %s\r\n' "$esc" "$heb" "$heb2"
printf '%s 2026\r\n' "$arabic"
printf '%s[?2501%s' "$esc" "$put_back"
