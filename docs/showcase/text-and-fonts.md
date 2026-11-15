<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Text and fonts

These screens are part of [vtium's showcase](README.md), which says how they are made.

![A console screen in a smooth monospaced font: a gold title twice as tall and twice as wide reading Lines of double size, a gray normal line under it, a light blue heading twice as wide, words twice as wide in single, double, curly, dotted and dashed underlines, struck and overlined, a blue badge reading READY twice as tall beside the words in 24-bit color, Greek and Russian words twice as wide, and two rulers of digits, one twice as wide, both ending at the right edge.](double-size.png)

vtium draws lines of double size, as the DEC terminals did: a line twice as tall and twice as wide for a title, and a line twice as wide for a heading. The font helper renders each character of Iosevka Term at twice its size from the outline itself, so the large letters stay as smooth as the small ones. A double-width line holds 60 characters, half of a normal line's 120, and its ruler ends at the right edge as the normal ruler under it does. Colors, reverse video and every underline style follow the doubled letters.

![Four horizontal bands on a black background, each naming its font, Iosevka Term, Terminus, GNU Unifont and JetBrains Mono, then showing in that font the same pangrams, digits and symbols, accented, Greek and Cyrillic letters, a line of styles (italic, struck, curly underlined, E = mc² and H₂O in raised and lowered text, and dim), and a line of box drawing and block elements.](fonts.png)

vtium draws any font a frit names, bitmap or outline. These four bands are four captures of the same screen, each in one font and starting with its name and kind: Iosevka Term and JetBrains Mono are outline fonts, which vtium scales to the cell and antialiases; Terminus is a PSF console font, and GNU Unifont, which vtium carries, is drawn at twice its size, both as bitmaps of whole pixels. Each band's styles line shows italic, which vtium slants itself where a font has no italic face, in whole pixels in the two bitmap fonts; strike, a curly underline, raised and lowered text at two thirds of the size, and dim. [Choose a font](../fonts.md) shows how to choose your own.

![A console screen of shell prompts made of colored segments, blue, gray and green, each ending in a solid arrow exactly as tall as the segment, with a house, folder and branch icon inside the segments. Below them, a list of folders and files, each with its own icon, the words café, naïve and São Paulo written with combining marks, and a box of light lines around them that joins from row to row.](glyph-fit.png)

vtium merges a symbol font behind the text font and fits its symbols to the text font's character cell: Symbols Nerd Font Mono's Powerline separators run from the top of each row to its bottom, so the colored segments of a prompt join them, and its icons stand in the middle of their cells beside Iosevka Term's letters. Letters with combining marks and a box of lines that vtium draws itself share the rows.

