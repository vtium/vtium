#!/bin/bash
# look-for: Each mark sits on its letter in one cell: the marks column
# look-for: matches the one-code-point column, and the bars line up with
# look-for: |x|.
# wrong-looks-like: Bare letters in the marks column and "cafe manana" in
# wrong-looks-like: the words (stock kmscon drops every combining mark), a
# wrong-looks-like: mark beside its letter, or bars out of line.
# Combining marks: a base letter followed by one or two marks forms one cell.
# The left column builds each letter from a base and marks; the right column
# is the precomposed character where Unicode has one. The marks are written as
# UTF-8 bytes so the output does not depend on the locale.
acute=$'\xcc\x81'     # U+0301
grave=$'\xcc\x80'     # U+0300
tilde=$'\xcc\x83'     # U+0303
diaeresis=$'\xcc\x88' # U+0308
ring=$'\xcc\x8a'      # U+030A
cedilla=$'\xcc\xa7'   # U+0327

row() {
	printf '  %-24s |%s|   |%s|\n' "$1" "$2" "$3"
}
printf '  %-24s %-6s %s\n' '' 'marks' 'one code point'
row 'e + U+0301 acute' "e$acute" 'é'
row 'a + U+0303 tilde' "a$tilde" 'ã'
row 'n + U+0303 tilde' "n$tilde" 'ñ'
row 'o + U+0308 diaeresis' "o$diaeresis" 'ö'
row 'A + U+030A ring' "A$ring" 'Å'
row 'c + U+0327 cedilla' "c$cedilla" 'ç'
row 'a + U+0303 + U+0300' "a$tilde$grave" "ã$grave"
row 'reference' 'x' 'x'
echo
printf '  in words:   |%s|  |%s|\n' "cafe$acute man${tilde}ana" 'café mañana'
printf '  reference:  |%s|  |%s|\n' 'xxxx xxxxxx' 'xxxx xxxxxx'
