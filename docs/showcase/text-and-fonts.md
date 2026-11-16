<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Text and fonts

These screens are part of [vtium's showcase](README.md), which says how they are made.

![A console screen in a smooth monospaced font: a gold title twice as tall and twice as wide reading Lines of double size, a gray normal line under it, a light blue heading twice as wide, words twice as wide in single, double, curly, dotted and dashed underlines, struck and overlined, a blue badge reading READY twice as tall beside the words in 24-bit color, Greek and Russian words twice as wide, and two rulers of digits, one twice as wide, both ending at the right edge.](double-size.png)

vtium draws lines of double size when a program asks for them. A title line is twice as tall and twice as wide, and a heading line is twice as wide. vtium's font helper renders each character of Iosevka Term at twice its size from the outline, so the large letters stay as smooth as the small ones. A double-width line holds 60 characters, half of a normal line's 120. Its ruler ends at the right edge, as the normal ruler under it does. Colors and every underline style follow the doubled letters.

![Four horizontal bands on a black background, each naming its font, Iosevka Term, Terminus, GNU Unifont and JetBrains Mono, then showing in that font the same pangrams, digits and symbols, accented, Greek and Cyrillic letters, a line of styles (italic, struck, curly underlined, E = mc² and H₂O in raised and lowered text, and dim), and a line of box drawing and block elements.](fonts.png)

vtium draws any bitmap or outline font that a font set file (a frit) names. These four bands are four captures of the same screen, each in one font and starting with its name and kind. Iosevka Term and JetBrains Mono are outline fonts, which vtium scales to the cell and antialiases. Terminus is a PSF console font, and vtium draws it as a bitmap of whole pixels. GNU Unifont, which vtium carries, is a bitmap font too, drawn at twice its size. Each band's styles line shows italic, strike, a curly underline, raised and lowered text at two thirds of the size, and dim. Where a font has no italic face, vtium slants the letters itself, in whole pixels in the two bitmap fonts. [Choose a font](../fonts.md) shows how to choose your own.

![A console screen of shell prompts made of colored segments, blue, gray and green, each ending in a solid arrow exactly as tall as the segment, with a house, folder and branch icon inside the segments. Below them, a list of folders and files, each with its own icon, the words café, naïve and São Paulo written with combining marks, and a box of light lines around them that joins from row to row.](glyph-fit.png)

vtium merges a symbol font behind the text font and fits its symbols to the text font's character cell. Symbols Nerd Font Mono's Powerline separators run from the top of each row to its bottom, so the colored segments of a prompt join them. Its icons stand in the middle of their cells beside Iosevka Term's letters. Letters with combining marks and a box of lines that vtium draws itself share the rows.

![A console screen in a smooth monospaced font with a short program in colored text. Its arrows, comparison signs, equality signs and logical operators are drawn as single joined symbols, such as a solid arrow for ->, a crossed equals sign for != and a long triple bar for ===. Under the program, a row of column numbers lines up with the code above it, and a last line shows the same operators with each character in a different color, drawn as separate characters.](ligatures.png)

With font-ligatures on, vtium draws a programming font's ligatures: Iosevka Term joins operators such as ->, =>, !=, ===, <= and && into one shape across the character cells of their characters. Each character keeps its own cell, so the columns under the code count one for each character, and the cursor, selection and copying still reach every character. A run of symbols in two colors draws its characters apart.

