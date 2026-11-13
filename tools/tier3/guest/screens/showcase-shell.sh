#!/bin/bash
# vtium draws a shell session on a 1920x1080 display in Iosevka Term, 28
# rows of 128 columns: prompts in powerline segments with an icon from
# Symbols Nerd Font Mono, a colored directory listing, a git history with its
# branch graph, a gallery of six pictures in kitty graphics with their file
# names, greetings in several scripts with emoji, and a line of text styles:
# straight, double, curly, dotted and dashed underlines, a curly underline in
# a color of its own, strike, inverse and dim. vtium's own gallery tool,
# tools/gallery/gallery, draws the gallery: four pictures computed from a few
# lines of arithmetic, and two photographs NASA's astronauts took, Earthrise
# and the Blue Marble. The scene writes the rest of the session byte for
# byte, and the last prompt is the shell's own, with its cursor.
# alt: A console screen showing a shell session in a smooth monospaced font:
# alt: prompts in blue, gray and green segments, a colored listing of the
# alt: vtium source tree, a git log with a branch graph and yellow commit
# alt: hashes, the crates of vtium, then a row of six pictures, each with its
# alt: file name under it: the Earth rising over the Moon, hexagonal tiles in
# alt: teal, gold and coral, a Julia set glowing violet and orange, a rainbow
# alt: plasma, a sun setting behind purple hills, and the whole Earth. Below
# alt: them are three lines of greetings and thanks in English, Chinese,
# alt: Japanese, Korean, Thai, Greek, Russian, Vietnamese, French and German
# alt: with emoji, the first also in Hindi, Arabic and Hebrew, the Arabic and
# alt: Hebrew reading right to left, and a line of words each drawn in the
# alt: style it names: underline, double, curly, dotted, dashed, a red curly
# alt: underline, strike, inverse and dim.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-shell
o=$(off)
blue=$(fg 97 175 239) green=$(fg 152 195 121) yellow=$(fg 229 192 123) red=$(fg 224 108 117)
mag=$(fg 198 120 221) cyan=$(fg 86 182 194) white=$(fg 220 223 228)
# The prompt's segments: the user, the directory after a folder icon
# (U+F07C, Symbols Nerd Font Mono), and the branch after the powerline
# branch symbol (U+E0A0), each segment ending in the powerline arrow
# (U+E0B0).
arrow=$'\xee\x82\xb0' folder=$'\xef\x81\xbc' branch=$'\xee\x82\xa0'
segments() {
	printf '%s%s tester %s%s%s%s %s ~/src/vtium %s%s%s%s %s main %s%s%s ' \
		"$(bg 97 175 239)" "$(fg 30 33 39)" "$(bg 60 66 78)" "$(fg 97 175 239)" "$arrow" \
		"$(fg 220 223 228)" "$folder" "$(bg 152 195 121)" "$(fg 60 66 78)" "$arrow" \
		"$(fg 30 33 39)" "$branch" "$o" "$(fg 152 195 121)" "$arrow$o"
}
prompt() { segments; printf '$ %s\n' "$1"; }
prompt ls
printf '%sCargo.lock  %sCargo.toml%s  COPYING  %scrates%s  %sdocs%s  LICENSE  %sMakefile%s  %spackaging%s  %sREADME.md%s  %stools%s  %s端末メモ.txt%s\n' \
	"$white" "$yellow" "$o" "$blue" "$o" "$blue" "$o" "$yellow" "$o" "$blue" "$o" "$cyan" "$o" "$blue" "$o" "$green" "$o"
prompt 'git log --oneline --graph -5'
h() { printf '%s%s%s' "$yellow" "$1" "$o"; }
printf '%s*%s   %s %s(%sHEAD -> %smain%s, %sorigin/main%s)%s Merge pull request #250 from vtium/stage\n' \
	"$red" "$o" "$(h 1bf3461a)" "$yellow" "$cyan" "$green" "$yellow" "$red" "$yellow" "$o"
printf '%s|%s%s\\%s\n' "$red" "$o" "$green" "$o"
printf '%s|%s %s*%s   %s Merge fix/helper-epipe into integration\n' "$red" "$o" "$green" "$o" "$(h 85989610)"
printf '%s|%s %s|%s%s\\%s\n' "$red" "$o" "$green" "$o" "$yellow" "$o"
printf '%s|%s %s|%s %s*%s %s fix(vtium-config): a helper that fails before a request is written gives its own reason\n' \
	"$red" "$o" "$green" "$o" "$yellow" "$o" "$(h c58d2278)"
printf '%s|%s %s|%s%s/%s\n' "$red" "$o" "$green" "$o" "$yellow" "$o"
printf '%s|%s %s*%s   %s Merge fix/render-node-test into integration\n' "$red" "$o" "$green" "$o" "$(h c98fbb6f)"
printf '%s|%s %s|%s%s\\%s\n' "$red" "$o" "$green" "$o" "$mag" "$o"
printf '%s|%s %s|%s %s*%s %s test(vtium-platform): a card with no render node of its own has none\n' \
	"$red" "$o" "$green" "$o" "$mag" "$o" "$(h e6bde664)"
printf '%s|%s %s|%s%s/%s\n' "$red" "$o" "$green" "$o" "$mag" "$o"
prompt 'ls crates'
printf '%svtium%s         %svtium-font%s    %svtium-image-helper%s  %svtium-platform%s  %svtium-settings%s\n' \
	"$blue" "$o" "$blue" "$o" "$blue" "$o" "$blue" "$o" "$blue" "$o"
printf '%svtium-config%s  %svtium-image%s   %svtium-parse%s         %svtium-render%s    %svtium-term%s\n' \
	"$blue" "$o" "$blue" "$o" "$blue" "$o" "$blue" "$o" "$blue" "$o"
# The gallery: vtium's gallery tool, tools/gallery/gallery, which tier 3
# carries onto the disc as showcase/bin/gallery, shows the six pictures that
# tools/tier3/t3_showcase_gallery.py writes into showcase/pictures, four rows
# tall with each file's name under its picture. It first asks the terminal
# whether it draws kitty graphics and the size of its cell, which vtium
# answers the same way on every run. When it draws nothing, it says why on
# one line, and the scene keeps the gallery's six rows.
prompt 'gallery -s 4 -n 6 ~/pictures'
if ! bash "$showcase/bin/gallery" -s 4 -n 6 "$showcase/pictures" 2>&1; then
	printf '\n\n\n\n'
	[ -n "${T3_EVENTS:-}" ] && echo T3-NO-GALLERY >"$T3_EVENTS"
fi
prompt 'cat greetings.txt styles.txt'
printf 'Hello 👋   你好   こんにちは   안녕하세요   สวัสดี   Γεια σου   Привет   Xin chào   Bonjour   Hallo   नमस्ते   مرحبا   שלום\n'
printf 'Thank you 🙏   谢谢   ありがとう   감사합니다   ขอบคุณ   Ευχαριστώ   Спасибо   Cảm ơn   Merci   Danke\n'
printf 'Good night 🌙   晚安   おやすみ   잘 자요   ราตรีสวัสดิ์   Καληνύχτα   Спокойной ночи   Bonne nuit   Gute Nacht\n'
# styles.txt: each word in the style it names, each style one that vtium
# draws (ADR-1000's sgr rows): SGR 4 and 4:2 to 4:5, the underline styles,
# 58 the underline's own color, 9 strike, 7 inverse and 2 dim.
styled() { printf '%s[%sm%s%s   ' "$esc" "$1" "$2" "$o"; }
styled 4 underline
styled 4:2 double
styled 4:3 curly
styled 4:4 dotted
styled 4:5 dashed
styled '4:3;58:2::224:108:117' 'red curly'
styled 9 strike
styled 7 inverse
printf '%s[2mdim%s\n' "$esc" "$o"
segments
