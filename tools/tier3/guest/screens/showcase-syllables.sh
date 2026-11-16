#!/bin/bash
# category: scripts and emoji
# hero: yes
# vtium draws the syllables of South Asian scripts whole. The upper rows
# show how many character cells each syllable takes, on a colored tile as
# wide as its cells. A syllable with a vowel sign that takes space of its
# own, such as Tamil கொ or Hindi का, takes two cells, as the shell's line
# editor counts it. Unicode splits text into clusters, the units a terminal
# normally gives one cell each. In the lower rows, each Kannada and Tamil
# conjunct, consonants joined into one shape, spans several clusters, each
# on a shade of its own. vtium keeps those cells and shapes the syllable
# once, so the joined form stretches across them. The examples include ra
# written under ka, ra written as a mark after ka, and five consonants
# stacked in one syllable. Under them is the first sentence of the Universal
# Declaration of Human Rights, in Tamil and in Kannada.
# alt: A console screen with three rows of syllables on colored tiles, in
# alt: Tamil, Hindi and Kannada, the tiles one or two cells wide; two rows
# alt: of Kannada and Tamil conjuncts, each drawn as one joined shape across
# alt: tiles of alternating shades; and a sentence in Tamil and one in
# alt: Kannada, each beside the name of its language.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
gold=$(fg 255 196 87) gray=$(fg 150 150 150) o=$(off)
# The tiles' backgrounds: two hues that alternate from one syllable to the
# next, each in a darker and a lighter shade that alternate from one cluster
# to the next inside a syllable. The text keeps one color, since a change
# of color inside a syllable would draw its clusters apart.
hues=("35 55 95" "55 85 140" "70 60 30" "110 95 45")

# tiles SYLLABLE...: each syllable on a tile of its own, the tiles a space
# apart; a syllable written with | between its clusters puts each cluster
# on its own shade of the syllable's hue
tiles() {
	local n=0 syllable cluster shade
	for syllable in "$@"; do
		shade=0
		IFS='|' read -ra clusters <<<"$syllable"
		for cluster in "${clusters[@]}"; do
			# shellcheck disable=SC2086
			printf '%s%s' "$(bg ${hues[$((n % 2 * 2 + shade))]})" "$cluster"
			shade=$((1 - shade))
		done
		printf '%s ' "$o"
		n=$((n + 1))
	done
	printf '\n'
}
label() { printf '  %s%-10s%s' "$gray" "$1" "$o"; }

printf '\n  %sSyllables%s\n\n' "$gold" "$o"
printf '  %sThe cells each syllable takes%s\n' "$gray" "$o"
label Tamil; tiles க கொ கோ கை கு கௌ
label Hindi; tiles क का कि की को क्ष
label Kannada; tiles ಕ ಕಾ ಕಿ ಕೊ ಕೌ
printf '\n'
printf '  %sOne syllable across several clusters%s\n' "$gray" "$o"
label Kannada; tiles 'ಕ್|ರ' 'ರ್|ಕ' 'ಸ್|ತ್|ರೀ' 'ಕ|ನ್|ನ|ಡ' 'ಕಾ|ರ್|ತ್|ಸ್|ನ್|ಯ'
label Tamil; tiles 'க்|ஷ' 'ஸ்|ரீ'
printf '\n'
label Tamil
printf 'மனிதப் பிறவியினர் சகலரும் சுதந்திரமாகவே பிறக்கின்றனர்; அவர்கள் மதிப்பிலும், உரிமைகளிலும் சமமானவர்கள்.\n'
label Kannada
printf 'ಎಲ್ಲಾ ಮಾನವರೂ ಸ್ವತಂತ್ರರಾಗಿಯೇ ಜನಿಸಿದ್ದಾರೆ. ಹಾಗೂ ಘನತೆ ಮತ್ತು ಹಕ್ಕುಗಳಲ್ಲಿ ಸಮಾನರಾಗಿದ್ದಾರೆ.\n'
