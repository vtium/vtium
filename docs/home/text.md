# Text in any script

The console shows text in many scripts, kept in straight columns: accented letters, emoji, Chinese characters, Devanagari syllables, and Arabic written right to left. Two things decide what you see. Unicode has to encode the script, and one of your fonts has to draw it. Fitting every script into a grid of character cells has limits, and some typographic features are not built yet: programming ligatures are planned in [the ligatures issue](https://github.com/vtium/vtium/issues/23).

Underneath, vtium splits text into [extended grapheme clusters](https://www.unicode.org/reports/tr29/), the characters as a reader sees them, and gives each cluster one or two character cells. Chinese, Japanese, and Korean characters take two cells each. Combining marks, emoji, and color emoji all render. A font helper shapes each cluster with HarfRust, which lets scripts such as Devanagari and Tamil join their syllables. Right-to-left text, such as Arabic and Hebrew, is laid out by the [Unicode bidirectional algorithm](https://www.unicode.org/reports/tr9/), and Arabic letters join.

## Fonts

You choose the fonts. A font set lists them in order, and each font fills in the characters that the ones before it lack. Bitmap fonts work, including the kernel's own console fonts, and so do outline fonts with their bold and italic faces. Two fonts are built in, which is why vtium starts even without a font file. Box drawing, block elements, and Braille patterns join from cell to cell in any font. Every supported font format is listed under [Fonts](../features.md#fonts), and the text features under [The terminal](../features.md#the-terminal).

To get started, follow [Choose a font](../fonts.md). If you write in a script beyond Latin, [Fonts for other scripts](../fonts-for-scripts.md) suggests fonts that cover it.
