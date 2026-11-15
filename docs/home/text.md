# Text in any script

vtium splits text into grapheme clusters, the characters as a reader sees them, and gives each cluster one or two character cells so that columns line up. Chinese, Japanese, and Korean characters take two cells each. vtium renders combining marks, emoji, and color emoji. A font helper shapes each cluster, which lets scripts such as Devanagari and Tamil join their syllables. vtium lays out right-to-left text, such as Arabic and Hebrew, by the Unicode bidirectional algorithm, and joins Arabic letters.

## Fonts

You choose the fonts. A font set lists them in order, and each font fills in the characters that the ones before it lack. vtium reads bitmap fonts, including the kernel's own console fonts, and outline fonts with their bold and italic faces. Two fonts are built in, so vtium starts even without a font file. Box drawing, block elements, and Braille patterns join from cell to cell in any font.

Programming ligatures are still to come, and [the ligatures issue](https://github.com/vtium/vtium/issues/23) tracks them.

## More about text and fonts

- [Choose a font](../fonts.md): setting up a font, step by step.
- [Fonts for other scripts](../fonts-for-scripts.md): fonts that cover scripts beyond Latin.
- [Fonts](../features.md#fonts) and [The terminal](../features.md#the-terminal) in What vtium does: every font format and text feature.
