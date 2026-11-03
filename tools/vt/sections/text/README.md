# The language pages' text

`udhr-article1.tsv` holds the text the playtest's language pages show (ADR-1501 D7): for each language, the first sentence of Article 1 of the Universal Declaration of Human Rights, a few words of it that each style line shows, those words in English, and a line of Unicode's superscript and subscript characters. Lines starting with `#` are comments, and the fields are separated by tabs.

Each sentence is copied from the UDHR in XML project, github.com/eric-muller/udhr at commit 588b3f4b2d0467aff54842a4b926551b69d5a66a, file `data/udhr/udhr_<code>.xml`, the first `para` of the first `article`, up to and including its first full stop, in Unicode's NFC form. The codes are `eng`, `vie`, `ell_monotonic`, `rus`, `heb`, `arb`, `hin`, `tha`, `cmn_hans`, `jpn` and `kor`. The English meaning every page gives is the English file's sentence: "All human beings are born free and equal in dignity and rights."

Three entries differ from the files:

- Thai is written without full stops, so its sentence is the text up to the first space. The file writes one word as `เกียรติศักด[เกียรติศักดิ์]`, its own correction in brackets, and the sentence here takes the corrected word, `เกียรติศักดิ์`.
- `vietnamese-marks` is the Vietnamese sentence in Unicode's NFD form, where every accent and tone mark is a combining character after its letter, so the page draws the same words through vtium's handling of combining marks.
- `emoji` is vtium's own: the English sentence with emoji between its words, and five emoji for the style lines.
