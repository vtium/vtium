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

`edges-upside-down` and `edges-left` differ from what vtium draws at the two cut wide glyphs only; `../departures.tsv` lists those cells under the render crate's departure.
