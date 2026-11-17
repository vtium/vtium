<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Styles and modes

These screens and figures are part of [vtium's showcase](README.md), which says how they are made.

![A console screen in a smooth monospaced font: a paragraph about vtium, a short Rust function with colored keywords, strings and comments beside line numbers, words underlined in single, double, curly, dotted and dashed lines in several colors, words in italic, struck through, dim, reversed and in 24-bit color, raised and lowered small text such as E = mc² and H₂O with an underlined footnote mark and struck small text, an overlined word, and rows of arrows, mathematical symbols and typographic marks.](text.png)

vtium draws outline fonts smoothly at any size, here Iosevka Term at 30 points on a 1920×1080 display. The screen shows prose, a code listing in 24-bit color, and every underline style a program can ask for. It also shows the other text styles and the symbols of mathematics and typography. Iosevka Term has no italic face, so vtium slants the regular face for italic. Raised and lowered text (SGR 73 and 74) draws at two thirds of the size, and an underline or a strike on it follows the small glyph.

## How it behaves

![Three numbered panels, each with two small screens, the terminal's screen above and the display below. 1: the program sends CSI ? 2026 h, and both show frame A in blue. 2: the program has written the top of frame B, so the terminal's screen is half green and half blue, and the display still shows frame A whole. 3: the program sends CSI ? 2026 l, and both show frame B in green.](../figures/showcase-synchronized-output.svg)

A full-screen program brackets each frame in mode 2026, synchronized output. While the mode is set, vtium keeps showing the last whole frame, however many pieces the next one arrives in. When the program resets the mode, vtium draws the new frame at once, so the display never shows half of one. If a program never resets the mode, vtium shows the screen as it is after sync-timeout, 200 ms by default.

