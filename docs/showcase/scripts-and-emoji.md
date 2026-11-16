<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Scripts and emoji

These screens are part of [vtium's showcase](README.md), which says how they are made.

![A console screen with three rows of syllables on colored tiles, in Tamil, Hindi and Kannada, the tiles one or two cells wide; two rows of Kannada and Tamil conjuncts, each drawn as one joined shape across tiles of alternating shades; and a sentence in Tamil and one in Kannada, each beside the name of its language.](syllables.png)

vtium draws the syllables of South Asian scripts whole. The upper rows show how many character cells a syllable takes: each sits on a colored tile as wide as its cells, and a syllable with a spacing vowel sign, such as Tamil கொ or Hindi का, takes two, as the shell's line editor counts it. In the lower rows each Kannada and Tamil conjunct is several clusters, each on a shade of its own, as Unicode cuts it. vtium keeps those cells and shapes the syllable once, so the joined form stretches across them: ra under ka, the reph over ka, and five consonants stacked in one syllable. Under them, the first sentence of the Universal Declaration of Human Rights in Tamil and in Kannada.

![A console screen with lines of text mixing words and color emoji, a family, a flag, a raised fist, scales and a dove each under a single, struck, red curly, over or double line, then rows of faces, animals, food and objects, people and families drawn as single pictures, national flags, number keycaps, and hands in five skin tones.](emoji.png)

vtium draws color emoji from Noto Color Emoji beside the text, two cells wide, with the sequences that join several emoji into one picture: people and families joined by zero-width joiners, flags, keycaps and skin tones. An underline, a strike or an overline reaches across the whole emoji.

![A console screen listing the first sentence of the Universal Declaration of Human Rights under the English sentence, each beside the name of its language: Chinese, Japanese and Korean; Thai and Vietnamese; Hindi and Bengali; Arabic and Hebrew, whose lines run from the right edge, the language's name at the right; a Hebrew line from the right edge holding the Latin name vtium 0.1, and an English sentence quoting Hebrew words and the number 1; Amharic, Armenian and Georgian; then French, German, Polish, Greek and Russian.](scripts.png)

vtium draws the world's scripts. Each line is the first sentence of the Universal Declaration of Human Rights, beside the name of its language. Chinese, Japanese and Korean are two cells wide. Thai stacks its vowel and tone marks above and below its letters. Hindi and Bengali join their consonants and place each vowel sign before, after, above or below its letter. Arabic and Hebrew run right to left: each of those two lines starts at the right edge, and the Arabic letters join into words. Two lines mix the directions in one paragraph: a Hebrew line that carries the Latin name vtium and a version number, which read left to right inside it, and an English sentence that quotes Hebrew with a number, which read right to left inside it. Amharic is written in the Ethiopic script, and Armenian, Georgian, Latin, Greek and Cyrillic carry their own letters and accents.

## How it behaves

![Four numbered panels, each a row of sixteen cells. 1: after SCP 2 the cursor waits in the rightmost cell. 2: the Hebrew letter shin stands in the rightmost cell and the cursor is to its left. 3: the word shalom fills the four rightmost cells, read from the right, and the cursor is to its left. 4: vtium 0.1 stands to the left of shalom, reading left to right, and the cursor is near the left edge.](../figures/showcase-rtl-paragraphs.svg)

In a build with right-to-left support, a program starts a right-to-left paragraph with SCP 2 and then writes its text in the order it is read. vtium draws the paragraph from the right edge. Each letter lands to the left of the one before, and the cursor moves leftward with them. A run of Latin letters and digits inside the paragraph still reads left to right.

