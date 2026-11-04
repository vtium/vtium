#!/bin/bash
# vtium draws the world's scripts: Chinese, Japanese and Korean two cells
# wide, Thai with its vowel and tone marks stacked above and below, and
# Latin, Greek and Cyrillic with their accents, each line the first sentence
# of the Universal Declaration of Human Rights. Devanagari joins this screen
# once vtium draws its vowel signs and conjuncts right, and Arabic and
# Hebrew once vtium orders right-to-left text.
# alt: A console screen listing the first sentence of the Universal
# alt: Declaration of Human Rights in Chinese, Japanese, Korean, Thai,
# alt: Vietnamese, French, German, Polish, Greek and Russian, each beside the
# alt: name of its language, under the English sentence.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
line() { printf '  %s%-11s%s %s\n\n' "$d" "$1" "$o" "$2"; }
printf '\n  %sEnglish%s     %sAll human beings are born free and equal in dignity and rights.%s\n\n' \
	"$d" "$o" "$h" "$o"
line Chinese '人人生而自由，在尊严和权利上一律平等。'
line Japanese 'すべての人間は、生まれながらにして自由であり、かつ、尊厳と権利とについて平等である。'
line Korean '모든 인간은 태어날 때부터 자유로우며 그 존엄과 권리에 있어 동등하다.'
line Thai 'มนุษย์ทั้งหลายเกิดมามีอิสระและเสมอภาคกันในเกียรติศักดิ์และสิทธิ'
line Vietnamese 'Tất cả mọi người sinh ra đều được tự do và bình đẳng về nhân phẩm và quyền.'
line French 'Tous les êtres humains naissent libres et égaux en dignité et en droits.'
line German 'Alle Menschen sind frei und gleich an Würde und Rechten geboren.'
line Polish 'Wszyscy ludzie rodzą się wolni i równi pod względem swej godności i swych praw.'
line Greek 'Όλοι οι άνθρωποι γεννιούνται ελεύθεροι και ίσοι στην αξιοπρέπεια και τα δικαιώματα.'
line Russian 'Все люди рождаются свободными и равными в своем достоинстве и правах.'
