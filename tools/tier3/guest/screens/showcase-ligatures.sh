#!/bin/bash
# category: text and fonts
# With font-ligatures on, vtium draws a programming font's ligatures: Iosevka
# Term joins operators such as ->, =>, !=, ===, <= and && into one shape
# across the character cells of their characters. Each character keeps its
# own cell, so the columns under the code count one for each character, and
# the cursor, selection and copying still reach every character. A run of
# symbols in two colors draws its characters apart.
# alt: A console screen in a smooth monospaced font with a short program in
# alt: colored text. Its arrows, comparison signs, equality signs and
# alt: logical operators are drawn as single joined symbols, such as a solid
# alt: arrow for ->, a crossed equals sign for != and a long triple bar for
# alt: ===. Under the program, a row of column numbers lines up with the
# alt: code above it, and a last line shows the same operators with each
# alt: character in a different color, drawn as separate characters.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
kw=$(fg 198 120 221) fn=$(fg 97 175 239) st=$(fg 152 195 121) cm=$(fg 120 128 140)
ty=$(fg 229 192 123) nu=$(fg 209 154 102) ln=$(fg 90 96 110) d=$(fg 150 150 150) o=$(off)
red=$(fg 255 110 110) green=$(fg 120 210 120)
printf '\n  %sProgramming ligatures%s\n\n' "$(fg 255 196 87)" "$o"
n=0
code() { n=$((n + 1)); printf '  %s%3d%s  %s\n' "$ln" "$n" "$o" "$1"; }
code "${cm}/* The cells a run takes, a wide character counting twice. */${o}"
code "${kw}fn${o} ${fn}cells${o}(c: ${ty}char${o}) -> ${ty}usize${o} {"
code "    ${kw}match${o} ${fn}width${o}(c) {"
code "        ${ty}Width${o}::Wide => ${nu}2${o},"
code "        w ${kw}if${o} w != ${ty}Width${o}::Zero && c >= ${st}' '${o} => ${nu}1${o},"
code "        _ => ${nu}0${o},"
code "    }"
code "}"
code ""
code "${kw}assert!${o}(${fn}cells${o}(${st}'a'${o}) === ${nu}1${o} || ${fn}cells${o}(${st}'端'${o}) <= ${nu}2${o});"
printf '%s\n' "       ${d}0123456789012345678901234567890123456789012345678901234567890${o}"
printf '\n'
printf '  %sIn two colors, apart%s  %s-%s>  %s!%s=  %s=%s==  %s&%s&%s\n' "$d" "$o" \
	"$red" "$green" "$red" "$green" "$red" "$green" "$red" "$green" "$o"
