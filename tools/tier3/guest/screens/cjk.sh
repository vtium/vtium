# Double-width CJK glyphs mixed with Latin text, drawn through pango's font
# fallback. Correct: each CJK character two cells wide, the bars of the last two
# lines aligned.
printf 'Latin then CJK: 中文測試 end\n'
printf '日本語のテキスト | 한국어 | ASCII\n'
printf '|一|二|三|四|五|\n'
printf '|ab|cd|ef|gh|ij|\n'
