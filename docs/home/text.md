# Text in any script

Text in your language looks right on the console: accented letters, emoji, Chinese characters, Devanagari syllables, and Arabic written right to left, all in straight columns. Any script that Unicode encodes works, as long as one of your fonts can draw it.

Underneath, vtium splits text into [extended grapheme clusters](https://www.unicode.org/reports/tr29/), the characters as a reader sees them, and gives each cluster one or two character cells. Chinese, Japanese, and Korean characters take two cells each, and vtium renders combining marks, emoji, and color emoji. A font helper shapes each cluster with HarfRust, which lets scripts such as Devanagari and Tamil join their syllables. vtium lays out right-to-left text, such as Arabic and Hebrew, by the [Unicode bidirectional algorithm](https://www.unicode.org/reports/tr9/), and joins Arabic letters.

## Fonts

You choose the fonts. A font set lists them in order, and each font fills in the characters that the ones before it lack. vtium reads bitmap fonts, including the kernel's own console fonts, and outline fonts with their bold and italic faces. Two fonts are built in, so vtium starts even without a font file. Box drawing, block elements, and Braille patterns join from cell to cell in any font.

Programming ligatures are not supported yet, and [the ligatures issue](https://github.com/vtium/vtium/issues/23) tracks them.

[Choose a font](../fonts.md) walks you through setting one up, and [Fonts for other scripts](../fonts-for-scripts.md) suggests fonts for scripts beyond Latin. Every font format and text feature is listed under [Fonts](../features.md#fonts) and [The terminal](../features.md#the-terminal).
