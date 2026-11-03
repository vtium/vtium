#!/bin/bash
# look-for: Each CJK character takes two cells, whole and not overlapping
# look-for: its neighbour; the bars of the last two rows line up.
# wrong-looks-like: Characters cut, squeezed into one cell or overlapping,
# wrong-looks-like: bars of the last two rows out of line, or empty boxes
# wrong-looks-like: for a missing font.
# Double-width CJK characters mixed with Latin text (the C reference's cjk
# screen).
printf 'Latin then CJK: 中文測試 end\n'
printf '日本語のテキスト | 한국어 | ASCII\n'
printf '|一|二|三|四|五|\n'
printf '|ab|cd|ef|gh|ij|\n'
