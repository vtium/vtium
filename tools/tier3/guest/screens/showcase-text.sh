#!/bin/bash
# vtium draws outline fonts smoothly at any size: Iosevka Term at 30 points
# on a 1920x1080 display, prose, a code listing in 24-bit color, every
# underline style a program can ask for, and the symbols of mathematics and
# typography.
# alt: A console screen in a smooth monospaced font: a paragraph about
# alt: vtium, a short Rust function with colored keywords, strings and
# alt: comments beside line numbers, words underlined in single, double,
# alt: curly, dotted and dashed lines in several colors, and rows of arrows,
# alt: mathematical symbols and typographic marks.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
kw=$(fg 198 120 221) fn=$(fg 97 175 239) st=$(fg 152 195 121) cm=$(fg 120 128 140)
ty=$(fg 229 192 123) nu=$(fg 209 154 102) ln=$(fg 90 96 110) d=$(fg 150 150 150) o=$(off)
printf '\n  %sOutline text%s\n\n' "$(fg 255 196 87)" "$o"
printf '  vtium is a terminal for the Linux console. It draws straight to the display through KMS,\n'
printf '  with no desktop underneath, and it draws outline fonts with the care a desktop terminal\n'
printf '  takes: every glyph antialiased, hinted to the pixel grid and placed in its cell.\n\n'
n=0
code() { n=$((n + 1)); printf '  %s%3d%s  %s\n' "$ln" "$n" "$o" "$1"; }
code "${cm}// The cells a run of text takes on the screen, a wide character counting twice.${o}"
code "${kw}pub fn${o} ${fn}width${o}(text: &${ty}str${o}) -> ${ty}usize${o} {"
code "    text.${fn}chars${o}()"
code "        .${fn}map${o}(|c| ${kw}if${o} ${fn}is_wide${o}(c) { ${nu}2${o} } ${kw}else${o} { ${nu}1${o} })"
code "        .${fn}sum${o}()"
code "}"
code ""
code "${kw}assert_eq!${o}(${fn}width${o}(${st}\"vtium\"${o}), ${nu}5${o});"
code "${kw}assert_eq!${o}(${fn}width${o}(${st}\"端末\"${o}), ${nu}4${o});"
printf '\n'
u() { printf '%s[%sm%s%s[0m' "$esc" "$1" "$2" "$esc"; }
printf '  %sUnderlines%s  %s   %s   %s   %s   %s   %s   %s   %s\n' "$d" "$o" \
	"$(u 4 single)" "$(u 4:2 double)" "$(u 4:3 curly)" "$(u 4:4 dotted)" "$(u 4:5 dashed)" \
	"$(u '4:3;58:2::255:85:85' spelling)" "$(u '4:3;58:2::90:160:255' grammar)" \
	"$(u '4;58:5:214' colored)"
printf '  %sStyles%s      %s   %s   %s   %s\n\n' "$d" "$o" "$(u 9 'struck through')" \
	"$(u 2 'dim')" "$(u 7 ' reversed ')" "$(u '38;2;255;120;90;48;2;40;44;52' ' any 24-bit color ')"
printf '  %sArrows%s      ← ↑ → ↓ ↔ ↕ ⇐ ⇒ ⇔ ↩ ↪ ⟵ ⟶ ↻\n' "$d" "$o"
printf '  %sMathematics%s ∀x ∈ ℝ: x² ≥ 0   ∑ᵢ aᵢ   ∫₀¹ f(t) dt   √2 ≈ 1.414   π ≠ 3   ∞   ∂/∂x   ∅ ⊂ ∪ ∩   ¬p ∧ q ∨ r\n' "$d" "$o"
# shellcheck disable=SC1112
printf '  %sTypography%s  “quotes” ‘marks’ «guillemets» — dashes – … • † ‡ § ¶ © ® ™ € £ ¥ ¢ ° ‰ № ½ ¾\n' "$d" "$o"
