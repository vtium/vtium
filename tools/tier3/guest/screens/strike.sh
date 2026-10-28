#!/bin/bash
# look-for: A line through the middle of each struck run, in the text's
# look-for: colour and across CJK; it stops at 29 and before the trailing
# look-for: spaces.
# wrong-looks-like: No strike anywhere (stock kmscon has no SGR 9), a
# wrong-looks-like: strike off the middle or in another colour, or one
# wrong-looks-like: running on into the spaces.
# Strikethrough, SGR 9 (set) and 29 (reset).
row() {
	printf '  %-16s \033[%sm%s\033[0m\n' "$1" "$2" "$3"
}
row '9' '9' 'struck through'
row '9;31' '9;31' 'red text, red strike'
row '1;9' '1;9' 'bold and struck'
row '9;4' '9;4' 'struck and underlined'
row '9;4:3;58:5:196' '9;4:3;58:5:196' 'struck, red curly underline'
row '9;7' '9;7' 'struck in reverse video'
row '9' '9' '中文 and 日本語 struck whole'
row '9 then 29' '9' "struck here$(printf '\033[29m') and plain here"
