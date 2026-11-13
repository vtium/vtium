#!/bin/bash
# vtium draws the world's scripts. Each line is the first sentence of the
# Universal Declaration of Human Rights, beside the name of its language.
# Chinese, Japanese and Korean are two cells wide. Thai stacks its vowel and
# tone marks above and below its letters. Hindi and Bengali join their
# consonants and place each vowel sign before, after, above or below its
# letter. Arabic and Hebrew run right to left: each of those two lines starts
# at the right edge, and the Arabic letters join into words. Two lines mix
# the directions in one paragraph: a Hebrew line that carries the Latin name
# vtium and a version number, which read left to right inside it, and an
# English sentence that quotes Hebrew with a number, which read right to
# left inside it.
# Amharic is
# written in the Ethiopic script, and Armenian, Georgian, Latin, Greek and
# Cyrillic carry their own letters and accents.
# alt: A console screen listing the first sentence of the Universal
# alt: Declaration of Human Rights under the English sentence, each beside the
# alt: name of its language: Chinese, Japanese and Korean; Thai and
# alt: Vietnamese; Hindi and Bengali; Arabic and Hebrew, whose lines run from
# alt: the right edge, the language's name at the right; a Hebrew line from
# alt: the right edge holding the Latin name vtium 0.1, and an English
# alt: sentence quoting Hebrew words and the number 1; Amharic,
# alt: Armenian and Georgian; then French, German, Polish, Greek and Russian.

# The sentences are copied from the UDHR in Unicode project,
# github.com/eric-muller/udhr at commit
# 588b3f4b2d0467aff54842a4b926551b69d5a66a: the file
# data/udhr/udhr_<code>.xml, the first para of the first article, up to and
# including its first full stop, in Unicode's NFC form. The project
# transcribes the translations that the UN's Office of the High Commissioner
# for Human Rights (OHCHR) publishes, and its data/udhr/index.xml names the
# OHCHR translation of each file. The codes are eng, cmn_hans, jpn, kor, tha,
# vie, hin, ben, arb, heb, amh, hye, kat, fra, deu_1996, pol, ell_monotonic
# and rus. Thai is written without full stops, so its sentence is the text up to
# the first space, with the word the file corrects in brackets taken in its
# corrected form, as tools/vt/sections/text/README.md describes. Amharic ends
# its sentence with the Ethiopic full stop, and Hindi and Bengali with the
# danda. Tamil is left out: its first sentence runs to more than 200
# characters, longer than a line of this screen.
#
# The two mixed lines are vtium's own, built from the Hebrew file's words.
# The Hebrew one is the Declaration's Hebrew title, the file's own title,
# then "in vtium 0.1", with the preposition ב (in) joined to the Latin name by
# a hyphen. The English one names the Hebrew file's word for an article, סעיף,
# with the digit 1, and quotes the first six words of the Hebrew sentence. The sentences of the
# languages that the acceptance walk's language pages also show are the same
# text as tools/vt/sections/text/udhr-article1.tsv, which ShowcaseTest checks.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
switch_font showcase-iosevka
h=$(fg 255 196 87) d=$(fg 150 150 150) o=$(off)
entry() { printf '  %s%-11s%s %s' "$d" "$1" "$o" "$2"; }
line() { entry "$1" "$2"; echo; }
# A right-to-left line: SCP 2 (CSI 2 SP k), sent while the cursor stands at
# the start of the line's paragraph, makes that paragraph run right to left
# (ADR-1004 D2), so the line starts at the right edge with the language's
# name and its sentence reads leftward from there. SCP 0, sent after the
# text, puts the default back for the paragraphs that follow, and changes
# nothing on this line.
rtl() { printf '%s[2 k' "$esc"; entry "$1" "$2"; printf '%s[0 k\n' "$esc"; }
printf '  %sEnglish%s     %sAll human beings are born free and equal in dignity and rights.%s\n\n' \
	"$d" "$o" "$h" "$o"
line Chinese '人人生而自由，在尊严和权利上一律平等。'
line Japanese 'すべての人間は、生まれながらにして自由であり、かつ、尊厳と権利とについて平等である。'
line Korean '모든 인간은 태어날 때부터 자유로우며 그 존엄과 권리에 있어 동등하다.'
echo
line Thai 'มนุษย์ทั้งหลายเกิดมามีอิสระและเสมอภาคกันในเกียรติศักดิ์และสิทธิ'
line Vietnamese 'Tất cả mọi người sinh ra đều được tự do và bình đẳng về nhân phẩm và quyền.'
echo
line Hindi 'सभी मनुष्यों को गौरव और अधिकारों के मामले में जन्मजात स्वतन्त्रता और समानता प्राप्त है ।'
line Bengali 'সমস্ত মানুষ স্বাধীনভাবে সমান মর্যাদা এবং অধিকার নিয়ে জন্মগ্রহণ করে।'
echo
rtl Arabic 'يولد جميع الناس أحرارًا متساوين في الكرامة والحقوق.'
rtl Hebrew 'כל בני אדם נולדו בני חורין ושווים בערכם ובזכויותיהם.'
rtl Mixed 'הכרזה לכל באי עולם בדבר זכויות האדם, ב-vtium 0.1.'
line Mixed 'In Hebrew, Article 1 is סעיף 1 and begins כל בני אדם נולדו בני חורין.'
echo
line Amharic 'የሰው፡ልጅ፡ሁሉ፡ሲወለድ፡ነጻና፡በክብርና፡በመብትም፡እኩልነት፡ያለው፡ነው።'
line Armenian 'Բոլոր մարդիկ ծնվում են ազատ ու հավասար իրենց արժանապատվությամբ ու իրավունքներով։'
line Georgian 'ყოველი ადამიანი იბადება თავისუფალი და თანასწორი თავისი ღირსებითა და უფლებებით.'
echo
line French 'Tous les êtres humains naissent libres et égaux en dignité et en droits.'
line German 'Alle Menschen sind frei und gleich an Würde und Rechten geboren.'
line Polish 'Wszyscy ludzie rodzą się wolni i równi pod względem swej godności i swych praw.'
line Greek 'Όλοι οι άνθρωποι γεννιούνται ελεύθεροι και ίσοι στην αξιοπρέπεια και τα δικαιώματα.'
line Russian 'Все люди рождаются свободными и равными в своем достоинстве и правах.'
