# Text in any script

vtium splits text into grapheme clusters, the characters as a reader sees them. Each cluster takes one or two character cells, so columns line up. vtium draws combining marks, emoji, and color emoji. Chinese, Japanese, and Korean characters take two cells each. A font helper shapes each cluster, so scripts such as Devanagari and Tamil join their syllables. vtium lays out right-to-left text, such as Arabic and Hebrew, by the Unicode bidirectional algorithm. vtium also joins Arabic letters.

## Fonts

You choose the fonts. A font set lists fonts in order, and each later font fills in the characters that the earlier fonts lack. vtium reads bitmap fonts, such as the kernel's own console fonts, and outline fonts with their bold and italic faces. Two fonts are built into vtium, so vtium starts even without a font file. Box drawing, block elements, and Braille patterns join cell to cell in any font.

Programming ligatures are still to come. See [the ligatures issue](https://github.com/vtium/vtium/issues/23).

## More about text and fonts

- [Choose a font](../fonts.md) walks you through setting up a font.
- [Fonts for other scripts](../fonts-for-scripts.md) lists fonts that cover scripts beyond Latin.
- [Fonts](../features.md#fonts) and [The terminal](../features.md#the-terminal), in What vtium does, list each font format and text feature.
