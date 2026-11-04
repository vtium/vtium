#!/bin/bash
# vtium draws a shell session on a 1920x1080 display in Iosevka Term:
# prompts in powerline segments with an icon from Symbols Nerd Font Mono, a
# colored directory listing, a git history with its branch graph, a test
# run, and greetings in several scripts with emoji. The scene writes the
# session byte for byte, and the last prompt is the shell's own, with its
# cursor.
# alt: A console screen showing a shell session in a smooth monospaced
# alt: font: prompts in blue, gray and green segments, a colored listing of
# alt: the vtium source tree, a git log with a branch graph and yellow
# alt: commit hashes, the crates of vtium, a passing test run, and three
# alt: lines of greetings and thanks in English, Chinese, Japanese, Korean,
# alt: Thai, Greek, Russian, Vietnamese, French and German with emoji.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
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
prompt 'cargo test -p vtium-term --lib'
printf '   %sCompiling%s vtium-term v0.1.0-alpha.2 (/home/tester/src/vtium/crates/vtium-term)\n' "$green" "$o"
printf '    %sFinished%s `test` profile [unoptimized + debuginfo] target(s) in 12.84s\n' "$green" "$o"
printf '     %sRunning%s unittests src/lib.rs (target/debug/deps/vtium_term-6b1f0c2d9e4a7f13)\n' "$green" "$o"
printf '\n'
printf 'test result: %sok%s. 94 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s\n' "$green" "$o"
prompt 'cat greetings.txt'
printf 'Hello 👋   你好   こんにちは   안녕하세요   สวัสดี   Γεια σου   Привет   Xin chào   Bonjour   Hallo\n'
printf 'Thank you 🙏   谢谢   ありがとう   감사합니다   ขอบคุณ   Ευχαριστώ   Спасибо   Cảm ơn   Merci   Danke\n'
printf 'Good night 🌙   晚安   おやすみ   잘 자요   ราตรีสวัสดิ์   Καληνύχτα   Спокойной ночи   Bonne nuit   Gute Nacht\n'
segments
