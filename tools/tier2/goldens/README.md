# vtium's goldens

The goldens of vtium's own tier 2 screens, which the C reference's tier 2 has none for. The C reference drew every one: `../reference/draw.sh` at kmscon `b1f7a7c5df563f3378584db7298753848a7a9a13` and libtsm `3ec40dc7a185dea1cd7e1252d4215939af1977fb`, the commit that kmscon's `subprojects/libtsm.wrap` pins, through libtsm, bbulk with the unifont backend and drm2d's blend. Each stays in `../provisional.tsv` until the operator has reviewed it.

| Golden | What it shows |
|---|---|
| `edges.ppm` | 93x67 pixels: a grid of 12 by 4 cells, of which the display holds 11 columns and 4 rows, centred with 2 and 3 pixel columns of border at the left and right and 1 and 2 pixel rows at the top and bottom. `edges` with red `R` and green `G` cells; a blue row; `last456`, `last` black on yellow and `456` underlined and struck; a magenta run in the last row. In the last whole column, the left halves of an underlined `中` and a reversed `語`. |
| `edges-right.ppm` | The same on a display turned right, 67x93: text reads top to bottom. |
| `edges-upside-down.ppm` | The same turned half round. The two cut wide glyphs show their right halves: the C reference's aaronsb/kmscon#44, which vtium departs from. |
| `edges-left.ppm` | The same turned left: text reads bottom to top. The cut wide glyphs show their right halves, as in `edges-upside-down`. |
| `erase-pen.ppm` | 320x96: a red top row scrolled in, then `line one stays as it is`; `line two is ` and red to the end; two red cells and `line three gets two cells inserted`; `line four stays as it is`; `line five is erase` and red to the end. No red cell carries an underline or a strike. |
| `vs16-unifont.ppm` | 256x80: `a`, the heart and `b` four times. The VS16 heart and the underlined VS16 heart take two cells, the text heart centred in them; the plain and VS15 hearts take one, cut to it. |
| `vs16-wcwidth.ppm` | The same by wcwidth: every heart one cell, the VS16 ones included. |
| `wide-deco-unifont.ppm` | 384x160: `日本語` underlined, `漢字` curly, `カタカナ` dotted in red; `한글` double, `中文` dashed, `日本語` struck; `a日b本c` underlined; and `語` underlined and struck at the start of row 6, wrapped from the last column of row 5. |
| `redraw-unifont.ppm` | 384x160: `unchanged line`, `line 2 CHANGED curly` with `curly` under a curly line, `line three stays`, and `new struck` struck through. |
| `ascii-unifont.ppm` | 384x160: the pangram in both cases, digits and punctuation, runs of `i`, `l`, `W` and `m`, and a line of code, each glyph in its own 8x16 cell. |
| `bold-unifont.ppm` | 384x160: `bold` beside `regular`, a bold pangram over two lines, and bold red, green and blue words; bold glyphs widened by a pixel. |
| `palette-unifont.ppm` | 384x160: the 16 background colours as two-cell blocks, `AB` in each of the 16 foreground colours, a 19-step ramp through the 256-colour palette, and a 9-step red-to-green gradient. |
| `inverse-unifont.ppm` | 384x160: `inverse` in reverse between plain words, a whole line reversed, `swapped` green on red after red-on-green text, and `bold inverse`. |
| `cjk-unifont.ppm` | 384x160: Japanese, Chinese and Korean in two-cell glyphs, full-width Latin, `a日b本c語d`, and wide text in bold and in reverse. |
| `box-unifont.ppm` | 384x160: light and double boxes around `cell`, shades and blocks, eighth blocks, rounded, heavy and diagonal pieces, joined from cell to cell. |
| `underline-unifont.ppm` | 384x160: `gjpqy` under single, double, curly, dotted and dashed lines, each in the text colour and in an SGR 58 colour; a yellow curl under red text that SGR 59 returns to red; underlines on blue, in reverse and in bold. |
| `strike-unifont.ppm` | 384x160: struck text alone, in red, on green, with a straight and a curly underline, in bold, in reverse, and six struck spaces. |
| `deco-config-unifont.ppm` | 384x160: an underline in index 196's red and a strike in `#00c0ff`; SGR 58's green curl and yellow dots in their own colours; green underlined and yellow struck text; a double underline and strike in the configured colours. |
| `deco-force-unifont.ppm` | The same with `force-decoration-colors`: SGR 58's curl and dots drawn in the configured red too. |
| `combining-unifont.ppm` | 384x160: `é ã ñ` precomposed; the decomposed forms drawn as their base letters alone, in words, underlined, struck and bold, since unifont draws a cluster's first character. |
| `mosaic-unifont.ppm` | 576x256: every Braille pattern, sextant and octant with their labels, then sextant and octant checkerboards, a Braille grid and wave, sextant and octant bands, and the checkerboards and grid in yellow on blue. |
| `emoji-unifont.ppm` | 384x160: each emoji as its unifont glyph in grey, the family, tone, flag and keycap as their first characters, the hearts, an emoji between letters and under each decoration, the Unicode 15 codepoint, and the grin wrapped from the last column to row 8. |

`edges-upside-down` and `edges-left` differ from what vtium draws at the two cut wide glyphs only; `../departures.tsv` lists those cells under the render crate's departure.
