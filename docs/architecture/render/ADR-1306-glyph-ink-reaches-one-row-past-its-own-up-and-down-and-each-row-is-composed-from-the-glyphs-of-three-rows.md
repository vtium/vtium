---
contract: adr/v1
kind: decision
verb: change
capability: renderer
amends:
  - "ADR-1302#D1: which ink reaches past its cell, and how far"
  - "ADR-1302#D3: the baseline, and ink above and below the row"
  - "ADR-1302#D6: how a glyph carries ink past its cell"
  - "ADR-1302#D7: the order the row is drawn in"
  - "ADR-1302#D8: the colors overhanging ink takes"
  - "ADR-1302#D9: damage reaches a neighbour's ink"
  - "ADR-1302#D10: the GPU path"
  - "ADR-1302#D13: added line spacing, the user's choice"
  - "ADR-1300#D5: cells are composed a row at a time"
  - "ADR-1100#D4: the order a frame draws placements in"
basis:
  - operator: aaronsb
    level: guided
    said: "Arabic letters lose their marks at the row's edge as accented capitals lose their accents, and drawing into the neighbouring cells, above and below as well as beside, is the direction to think through as the fix."
    via: "the maintainer in the steering session, 2026-11-06, on the work about accents cut off capital letters, relayed in this record's brief; the wording is this record's agent's"
    paraphrase: true
  - evidence: "Measured by this record's agent on 2026-11-06 through vtium-config's rasteriser at integration 52d7d9ed, `hinting=light`, with each font's baseline as `vtium_config::outline::baseline` gives it, by drawing each glyph once with the baseline moved down a whole cell, to bring the ink above the cell into view, and once with the baseline at the cell's top row, to bring the ink below it into view. DejaVu Sans Mono 2.37 in a 16-pixel cell, baseline row 12: no printable ASCII glyph passes the cell; 25 of the 31 Latin-1 capitals U+00C0 to U+00DE pass its top, by at most 2 rows (À, É, Î, Ő and Ű by 2, Ộ by 1); 45 of the 128 Latin Extended-A letters pass its top, by at most 2 rows; 15 of the 44 letters DejaVu Sans Mono holds in U+1EA0 to U+1EF9 pass its top, by at most 2 rows; none of them passes the bottom. In a 32-pixel cell, baseline row 25: Ô and Û pass the top by 1 row, 9 Latin Extended-A letters by 1, Ằ by 2, and À, É, Î, Ő, Ű and Ộ fit. The probe is a scratch crate in the agent's directory (ink-overflow/probe), uncommitted."
  - evidence: "Measured by this record's agent on 2026-11-06 with HarfRust 0.12.0 shaping each cluster and skrifa 0.44.0 giving each glyph's outline bounds, unhinted, at the size each font is drawn at in 16- and 32-pixel cells (its ascent and descent spanning the cell less half a pixel) and on its baseline as vtium-config gives it. DejaVu Sans Mono's Arabic: alef with hamza and damma (أُ) reaches 6.4 pixels above a 16-pixel row and 12.3 above a 32-pixel row, 0.40 and 0.38 of the cell; lam with shadda and fatha 3.5 and 6.5 pixels, 0.22 and 0.20; jeem with kasratan (جٍ) 2.9 and 6.9 pixels below the row, 0.18 and 0.22. Noto Sans Arabic 2.x Regular (ascent 1.374 em, descent 0.738 em) is drawn at 7.34 pixels to the em in a 16-pixel cell and 14.91 in a 32-pixel cell, 0.55 of DejaVu Sans Mono's 13.32 and 27.06, and then no cluster measured passes the row; drawn at DejaVu Sans Mono's size and on its baseline, lam with shadda and superscript alef reaches 6.8 and 13.1 pixels above the row, 0.42 and 0.41 of the cell, jeem with kasratan 4.9 and 11.1 pixels below it, 0.31 and 0.35, and yeh with kasra 3.8 and 8.9 below, 0.24 and 0.28. The clusters measured: بِ سْ مِ ٱ لَّ لَّٰ هِ أُ جٍ اً إِ يِ كُ لا لاَ ضِّ نَّ بِسْمِ ٱللَّٰهِ."
  - evidence: "The engine-fixes work's measurements of 2026-11-06, relayed in this record's brief: the helper sizes the first outline font so its ascent and descent fill the cell less half a pixel, the C reference's FreeType request; ADR-1302 D3's whole-pixel baseline rises a row where the underscore would pass the cell's bottom; light hinting lifts accents by up to a pixel; about two dozen cell heights from 10 to 48 pixels cut 1 to 3 rows of accent from DejaVu Sans Mono's capitals, and 19 and 38 pixels cut none of À, É, Î, Õ, Ü and Ộ, while 19 cuts Ő and Ű."
  - evidence: "Observed by this record's agent on 2026-11-06 by running each terminal in a private Xvfb display, never on the maintainer's screen, with DejaVu Sans Mono at 9 points and a script that prints `Hxgjpqy ÀÉÎ ŐŰ Ộ Ấ`, a reverse-video copy of it, `ÀÉÎ ÀÉÎ` with the cursor on the second É, and `بِسْمِ ٱللَّٰهِ أُ جٍ`, each between lines of `gjpqy___ gjpqy___ gjpqy___`, some of those on red or blue backgrounds; screenshots taken with ImageMagick's `import` are in the agent's directory (ink-overflow/shots), uncommitted. xterm 411: at its own row height, the font's line height, the accents fit; with `scaleHeight: 0.9`, the accents above the row do not show, and underscores and descenders drawn into the next row show only where that row draws no cell, since each row's backgrounds paint over what the row above drew into it. Alacritty 0.17.0: with `font.offset.y = -3` no glyph is cut, the accents of a row draw over the red background of the row above, and descenders over the blue background of the row below. kitty 0.49.2: at its own row height the accents fit; with `modify_font cell_height 80%` each glyph is cut at its cell, the descenders and the underscore lose their lower part and the accents keep theirs. Konsole 26.08.2, whose line spacing cannot go below the font's line height: the acute of Ấ draws over the red row above it, the Arabic marks above draw into the row above, and the red row below the Arabic paints over its marks below."
  - evidence: "Alacritty 94e7c887 (Apache-2.0 or MIT), read on 2026-11-06 for its pattern: alacritty/src/renderer/text/glsl3.rs:241-258 draws a batch of up to 65,536 cells in two passes, every cell's background and then every cell's glyph, so a glyph's ink lies over the backgrounds of the rows beside it; alacritty/src/display/mod.rs:838 and 878 clear the window and draw every cell each frame; alacritty/src/display/damage.rs:238-250 widens each damaged line's rectangle by a cell on each side and half a cell above and below for the compositor, without changing what it draws."
  - evidence: "WezTerm e68a8ec2 (MIT), read on 2026-11-06 for its pattern: wezterm-gui/src/termwindow/render/screen_line.rs:251-275 puts cell backgrounds on quad layer 0, and lines 509-512 and 659-665 put each glyph on layer 1 at its bearing, its texture's full height, with no cut above or below (line 531 reads `// TODO: clipping`); the glyph is cut across into strips only to color the parts under the cursor or the selection."
  - evidence: "Ghostty 2f967e3a6ab2 (MIT), read on 2026-11-06 for its pattern, adding to ADR-1302's reading of its shaders: src/renderer/generic.zig:1934-1977 draws the frame as a background image and color, the pictures below the cells, every cell's background, the pictures below the text, then every cell's text, so no cell's background is drawn after any glyph."
  - evidence: "foot 152487e (MIT), read on 2026-11-06 for its pattern: render.c:1018-1022 clips each glyph to its row's height, its cells and one more cell to the right; doc/foot.ini.5.scd:124-139 lets `line-height` set the row's height and warns that too small a line height makes underscores disappear."
  - evidence: "foot 152487e (MIT), read on 2026-11-06 for how it structures drawing into neighbouring cells: render.c:705 marks a cell `confined` when its glyph is drawn inside it, and render.c:1014 clears the mark for each cell of a glyph wider than its cells; render.c:3462-3525, before each frame, marks dirty on each side of a dirty cell every cell up to the next confined one, since each row is drawn right to left and a glyph's overflow is drawn over its neighbour; render.c:3554 draws sixels after every cell."
  - evidence: "WezTerm e68a8ec2 (MIT), read on 2026-11-06 for how it layers pictures against glyphs: wezterm-gui/src/termwindow/render/screen_line.rs:490-495 puts a kitty picture with z below 0 on layer 0 with the backgrounds, and lines 685-690 and 719-730 put a picture with z of 0 or more on layer 2, over the glyphs of layer 1; wezterm-gui/src/termwindow/render/pane.rs:426-446 caches each line's quads keyed by its content, the cursor, the selection and its place."
  - evidence: "Ghostty 2f967e3a6ab2 (MIT), read on 2026-11-06: src/renderer/generic.zig:1582-1599 marks rows dirty and rebuilds their cell data, and lines 1934-2006 draw the frame in passes, the pictures below the backgrounds, every cell's background, the pictures below the text, every cell's text, then the pictures above the text."
  - evidence: "Measured by this record's agent on 2026-11-06 with the same probe, on sample text (the first article of the Universal Declaration of Human Rights in English, French, German and Vietnamese, and in Arabic without vowel marks; a shell listing; a line of accented capitals; and the opening of the Quran's first sura, fully vocalized), counting the non-space cells whose glyph has ink outside its cell. DejaVu Sans Mono at 16 pixels: English 0 of 141, the shell listing 0 of 111, French 3 of 173, German 3 of 168, Vietnamese 3 of 148, the capitals 9 of 76; at 32 pixels: Vietnamese 1 of 148 and the others 0; no glyph sends ink sideways. Arabic clusters passing the row by half a pixel or more, unhinted: unvocalized, DejaVu Sans Mono 2 of 92 at 16 and 32 pixels, Noto Sans Arabic at DejaVu Sans Mono's size 7 and 6 of 94; fully vocalized, DejaVu Sans Mono 10 and 15 of 78, Noto Sans Arabic at DejaVu Sans Mono's size 16 of 78 at both."
  - evidence: "Measured by this record's agent on 2026-11-06 with skrifa 0.44.0 and HarfRust 0.12.0, from unhinted outline control points, with each glyph's advance centred in its cells as ADR-1302 D2 places it, in DejaVu Sans Mono's cells of 9x16 and 17x32 (13.32 and 27.06 pixels to the em), every other font drawn at DejaVu Sans Mono's size as ADR-1200 D17 draws fallback fonts; the fonts from the tier 2 and specimen font caches and the system's Noto fonts. The most past a cell's left and right edges, at 32 pixels: DejaVu Sans Mono Book 0 and 0.04 (Bold 0.06 and 0.15), Oblique 0.09 and 0.20, Bold Oblique 0.11 and 0.31, Book slanted by 0.2 0.05 and 0.28, Bold slanted 0.05 and 0.39, of printable ASCII, Latin-1 and Latin Extended-A; DejaVu Sans Mono's Arabic with joining letters stretched to their cells 0.26 and 0.03; Noto Sans Arabic stretched 0 and 0.19, unstretched 0.53 and 0.66, its lam-alef fitting its two cells; Noto Sans Devanagari, each grapheme cluster in one cell, कि 0.32 and 0.34, क्षि 0.28 and 0.30, स्त्र 0.25 and 0.27, र्त्स्न्य 0.76 and 0.78; MesloLGS DZ Nerd Font icons 0.04 and 0.68; Symbols Nerd Font Mono icons 0.39 and 0.34. Noto Sans Arabic at DejaVu Sans Mono's size and baseline: 4 of the 110 letter glyphs, not marks, of the first article of the Universal Declaration of Human Rights pass the row's bottom by half a pixel or more, and none its top, at 16 and at 32 pixels. The probe is the scratch crate's side and vert programs (ink-overflow/probe), uncommitted."
  - evidence: "VTE's documentation for `vte_terminal_set_cell_height_scale`, read on 2026-11-06: valid values go from 1.0, the default, to 2.0, so a VTE row is never shorter than the font's line height. GNOME Terminal and VTE are not installed on the machine, and were not run."
  - upstream: "the C reference's kmscon at b1f7a7c5: the pango engine set the font's em to the requested height, measured a layout of the ASCII letters and punctuation and took the cell's height from its logical extents, the font's line height (src/font/font_pango.c:324 and 337-347), so DejaVu Sans Mono's accented capitals fitted a cell taller than the request; every engine cut each glyph at its box (ADR-1302's basis gives the lines)"
  - precedent: ADR-1302
  - precedent: ADR-1300
  - precedent: ADR-1200
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Design input, not a decision: overflow could be drawn in two passes, every cell's glyph inside its own cell first, then the parts of glyphs that fall outside their cells, kept in a buffer of their own and put on with XOR. A record of which cells hold a glyph with ink outside its cell would let vtium redraw only those cells and their neighbours when a character changes. Other terminals' ways of writing into neighbouring cells are to be investigated."
    via: the maintainer in the steering session, 2026-11-06, thinking aloud while the first draft was written, relayed by the team lead; the wording is this record's agent's, stated cleanly as the maintainer asks for musings
    paraphrase: true
  - operator: aaronsb
    said: "Design input: drawing text on top of a picture, kitty graphics or sixel, raises challenges of its own that the overflow design must answer."
    via: the maintainer in the steering session, 2026-11-06, relayed by the team lead; the wording is this record's agent's, stated cleanly as the maintainer asks for musings
    paraphrase: true
  - operator: aaronsb
    said: "[Reverse the 2026-11-01 answer that no ink reaches up or down?] Yes, allow one row (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
    covers: [accents whole]
  - operator: aaronsb
    said: "[The mark rule: move its edge from the row to the reach?] Move edge, keep rule (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
    covers: [marks where the font puts them]
  - operator: aaronsb
    said: "[The letter under the cursor: show its ink above and below in both blink phases?] Show it, both phases (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "[Four smaller calls (fit=room draws as natural; where two rows' ink meets the row below draws on top; a blinking accented capital shows its accent cut while it blinks; an underline cursor can be crossed by an accent from the row below) and the build going into alpha.6] Accept all (Recommended)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "Design input, not a final answer: since ink now reaches the rows above and below, ink could also reach further to either side, possibly two cells each way; the bound is left open."
    via: the maintainer in the steering session, 2026-11-06, relayed by the team lead; the wording is this record's agent's, stated cleanly as the maintainer asks for musings
    paraphrase: true
  - operator: aaronsb
    said: "[Blend instead of XOR?] Blend (Recommended)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "[Picture cells do not cut ink, following ADR-1100's layers. Accept?] Accept (Recommended)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
observable:
  - "Tier 2's `overflow-accents` screen shows À, É, Î, Ő, Ű and Ộ whole, their accents drawn over the background of the row above, beside a row of `gjpqy___` whose underscores and descenders stay whole in their own row"
  - "Tier 2's `emoji-spaced-12` and `emoji-spaced-30` screens show the first line's `ÀÉÎ` with whole accents"
  - "Typing an accented capital under a line of text, and erasing it, leaves no ink behind in the row above, in every buffer at every age"
  - "Every tier 2 and tier 3 screen drawn only with a bitmap font, or with outline text whose ink stays in its row, draws byte for byte as it does today"
status: accepted
date: 2026-11-06
deciders:
  - aaronsb
related:
  - ADR-1302
  - ADR-1300
  - ADR-1200
  - ADR-1004
  - ADR-1100
---

# ADR-1306: Glyph ink reaches one row past its own, up and down, and each row is composed from the glyphs of three rows

## Summary

- **Decided:** A glyph from an outline font draws the ink that lies above or below its row into the row above or the row below, at most one row each way, where ADR-1302 cuts it at the row's top and bottom. Together with the reach of one cell to each side that ADR-1302 already gives, a glyph's ink lies within the block of cells around its own. Ink past that block, or past the grid's top or bottom edge, is cut. The renderer still composes one row at a time: to draw a pixel, it lays down that pixel's own cell's background first, then the ink of every glyph whose reach covers the pixel, so no row's background ever paints over ink from the row beside it. A cell whose ink changes redraws the neighbours, above and below as well as beside it, that its old or new ink reaches. A combining mark is moved toward the baseline, or scaled with its letter, only when it would pass that one-row reach, so Arabic marks and Vietnamese stacked accents draw where the font puts them, and a precomposed É and an E followed by a combining acute draw alike. Printable ASCII still stays inside its cell (ADR-1302 D3), and bitmap glyphs, color glyphs, emoji and vtium's own line and block drawing never leave their cells, so a screen in the built-in font draws exactly as it does today. Ink that reaches a cell is part of that cell's text layer, so a kitty picture below the text shows it on top, and a picture above the text or a sixel covers it.
- **Trades away:** The maintainer's answer of 2026-11-01 that ink does not reach up or down, and the move-and-scale rule's edge at the row, which the maintainer reversed on 2026-11-06. A changed cell can redraw up to eight neighbours instead of two. Each row's pixels depend on the glyphs of three rows. Each cell keeps a byte of reach instead of two bits. The ink box in the glyph wire format gains a top and a height. Arabic marks and accents can touch the descenders of the line above in a tight cell, where today they are moved, scaled or cut. The maintainer's sketch of putting the overflow on with XOR is replaced by blending, for antialiased ink and for ink over ink.
- **One-way?** No. The rules live in vtium-config's placement and vtium-render's composition, and the glyph chunk format's ink box gains its new fields on both ends in one release. A font set already written to disk reads as ink cut at its row.
- **Probes:**
  - *Confident (accents whole):* in DejaVu Sans Mono at a 16-pixel cell, À, É and Î show their whole accents, which reach two pixel rows up into the line above and draw over that line's background, the way Alacritty, WezTerm and Ghostty draw them, while a screen of plain English text looks and costs exactly as it does today. Right? (Confirmed 2026-11-06; see `considered`.)
  - *Not confident (marks where the font puts them):* an Arabic vowel mark that reaches above or below its line draws where the font puts it, up to a whole line away, instead of being moved toward its letter or shrunk together with its letter to fit the line, as you decided on 2026-11-01 and 2026-11-02. Only a mark that would reach further than a whole line is still moved or shrunk. Is that what you want? (Confirmed 2026-11-06; see `considered`.)
  - *Not confident (two cells sideways):* every glyph measured, italic, bold italic, Arabic, Devanagari and icons, reaches less than one cell past its own side to side, so the record keeps the reach beside a glyph at one cell, as it is today, rather than two. Is one cell enough? (Proposed 2026-11-06; D12.)
- **Inversion:** The decision sits between "every glyph is cut at its row", which ADR-1302 and the C reference draw, and "a glyph draws wherever its font puts it", which Ghostty, WezTerm and Alacritty draw because they draw every background of the frame before any glyph and redraw the whole frame each time. vtium bounds the ink to one row each way, so that the renderer keeps composing one row at a time in its row scratch (ADR-1300 D5), keeps its damage cell by cell, and never reads the display's buffer.

## Context

The accents of capital letters are cut off at the top of the text row in an outline frit (a frit is a font set file: the fonts a console draws text with, and how). In DejaVu Sans Mono at a 16-pixel cell, the accents on À, É and Î lose 2 pixel rows, and the work that measured it found 1 to 3 rows cut at about two dozen cell heights from 10 to 48 pixels (`basis`). The cut comes from two decisions working together. The helper (vtium-config, the program that turns outline fonts into glyphs for the console) sizes the first font so its ascent and descent fill the cell, as the C reference's FreeType engine sized it (ADR-1200 D17). ADR-1302 D3 then cuts every glyph's ink at its row's top and bottom, because ADR-1300 D5 composes each row on its own. An accent on a capital reaches above the font's ascent in many fonts, so the cut removes it. Line spacing does not help: Latin letters are `natural` under ADR-1302 D13 and are cut at the text band.

The same cut reaches every script whose marks stack above or below the letter. ADR-1302 D3 moves Arabic marks toward the baseline, and scales a letter together with its marks where they cannot move in alone; lam with shadda and fatha loses 2 to 4 rows of its stem's height at 16 to 28 pixels that way, and the hamza on a precomposed alef is cut. Vietnamese stacked accents, Thai, and Devanagari's marks above the headline meet the same edge.

How far the ink goes, measured through vtium-config's rasteriser and with HarfRust's shaping (`basis`, 2026-11-06), as a share of the cell's height:

| Font and text | 16-pixel cell | 32-pixel cell |
|---|---|---|
| DejaVu Sans Mono, printable ASCII | fits | fits |
| DejaVu Sans Mono, Latin-1 capitals | 25 of 31 pass the top, by up to 2 rows (0.13) | 2 of 31, by 1 row (0.03) |
| DejaVu Sans Mono, Latin Extended-A | 45 of 128 pass the top, by up to 2 rows | 9 of 128, by 1 row |
| DejaVu Sans Mono, Vietnamese | 15 of 44 pass the top, by up to 2 rows | Ằ, by 2 rows |
| DejaVu Sans Mono, Arabic, most above (أُ) | 0.40 | 0.38 |
| DejaVu Sans Mono, Arabic, most below (جٍ) | 0.18 | 0.22 |
| Noto Sans Arabic at DejaVu Sans Mono's size, most above (لَّٰ) | 0.42 | 0.41 |
| Noto Sans Arabic at DejaVu Sans Mono's size, most below (جٍ) | 0.31 | 0.35 |

No glyph measured passes its row by as much as half a row. Noto Sans Arabic drawn at its own size, where its tall ascent and descent fill the cell, fits its row, but at 0.55 of DejaVu Sans Mono's size; since ADR-1200 D17 draws every fallback outline font at the first font's size, the rows measured at DejaVu Sans Mono's size are the ones that apply behind it.

### How other terminals draw it

| Terminal | Ink past the row | How backgrounds and ink are ordered | What a change redraws | How it was learnt |
|---|---|---|---|---|
| xterm 411 | Drawn into the next row, but each row's backgrounds paint over what the row above drew into it; accents above the row do not show | Row by row | Not read | Run; documentation |
| Alacritty 0.17.0 | Drawn, never cut | Every background of a batch, then every glyph | The whole frame each time | Run; source read (Apache-2.0 or MIT) |
| WezTerm | Drawn, never cut | Backgrounds on layer 0, glyphs on layer 1 | The whole frame | Source read (MIT); not installed |
| Ghostty | Drawn, never cut | Every cell's background, then every cell's text | The whole frame each time | Source read (MIT); not installed |
| foot | Cut at the row; one cell to the right only | Row by row, right to left | The changed cells and their neighbours to the side | Source and documentation read (MIT); not installed |
| kitty 0.49.2 | Cut at the cell | Not learnt | Not learnt | Run; documentation only (GPL) |
| Konsole 26.08.2 | Drawn into the rows beside it, but the next row's backgrounds paint over ink that reaches down into it | Row by row | Not learnt | Run only (GPL) |
| VTE, GNOME Terminal | Its rows are never shorter than the font's line height | Not learnt | Not learnt | Documentation only (LGPL and GPL); not installed |

Every terminal measured makes its row as tall as the font's line height by default, so accents fit there, and lets the user make rows shorter only by a setting (xterm's `scaleHeight`, Alacritty's offset, kitty's `modify_font`, foot's `line-height`). In a short row, the terminals that draw the whole frame each time let ink cross rows and draw every background first, and the ones that draw row by row either cut it or let the next row paint over it. None draws ink across rows while redrawing only the cells that changed, which is what vtium does on a dumb buffer (the plain buffer in memory that the CPU draws into and the display shows).

How each one structures writing into the cells beside a glyph's own (`basis`):

- **foot,** which draws on the CPU into shared memory as vtium does, marks each cell `confined` when its glyph stays in its cell. Before each frame it marks dirty, on each side of every dirty cell, the cells up to the next confined one, then draws each row from right to left, every cell's background and glyph in turn, so a glyph's overflow lands over the neighbour already drawn. A run of overflowing cells redraws together, because each cell's drawing paints over its neighbour's overflow. It cuts every glyph at its row, and draws sixels after every cell.
- **Alacritty** draws every cell each frame, in batches of up to 65,536 cells, each batch in two passes: every background, then every glyph.
- **WezTerm** builds each line's quads into three layers, backgrounds and pictures below the text on layer 0, glyphs on layer 1, and pictures above the text on layer 2. It caches each line's quads keyed by the line's content, cursor and selection, and draws all layers of all lines each frame.
- **Ghostty** rebuilds the cell data of the rows marked dirty and draws the whole frame each time in fixed passes: the background, the pictures below the backgrounds, every cell's background, the pictures below the text, every cell's text, and the pictures above the text.
- **kitty** cuts each glyph at its cell, as running it shows. **Konsole** draws row by row and lets the next row's backgrounds paint over ink, as running it shows. VTE keeps its rows at least as tall as the font's line height, by its documentation.

The terminals that redraw only what changed, foot and the row-by-row ones, cut ink or let it be painted over; the ones that let ink cross freely redraw the whole frame. vtium's design takes foot's per-cell record of which glyphs leave their cell, and the order of Alacritty's, WezTerm's and Ghostty's passes, applied pixel by pixel.

### How often a glyph sends ink out of its cell

Measured through vtium-config's rasteriser for DejaVu Sans Mono, and with HarfRust's shaping for Arabic, on sample text (`basis`, 2026-11-06): the share of non-space cells whose glyph sends ink into another cell.

| Text | 16-pixel cell | 32-pixel cell |
|---|---|---|
| English prose, a shell listing | none | none |
| French, German, Vietnamese prose | 1.7 to 2.0 per cent | none to 0.7 per cent |
| A line of accented capitals | 11.8 per cent | none |
| Unvocalized Arabic, DejaVu Sans Mono | 2 per cent of clusters | 2 per cent |
| Fully vocalized Arabic, DejaVu Sans Mono | 13 per cent | 19 per cent |
| Fully vocalized Arabic, Noto Sans Arabic at DejaVu Sans Mono's size | 21 per cent | 21 per cent |

Upright DejaVu Sans Mono sends no ink sideways in any of these texts, since its cell is as wide as its widest letter; all of it goes up or down.

### The C reference's flow

The C reference cut every glyph at its box of 1 or 2 cells, in each of its font engines, and drew each cell's background, underline, glyph and strike-through in that cell's box. Its pango engine set the font's em to the requested height and took the cell's height from the font's line height, so a 16-pixel request drew in a taller cell, and DejaVu Sans Mono's accented capitals fitted it. Its FreeType engine fitted the font's ascent and descent to the cell, as vtium's helper does, and cut the accents. The C notes give the lines.

## Decision

The figure shows three rows of four cells, drawn large. The top row has a blue background and holds `gy` and an underscore. The middle row holds É, whose acute accent reaches up into the top row, drawn over the blue background, inside a dashed box that marks É's reach: its own cell and one cell more on every side. The bottom row holds an Arabic lam whose fatha, a vowel mark, reaches up into the middle row. Below the rows, the order in which each pixel is drawn.

![Three rows of four cells, drawn large, on a dark background. The top row is blue and holds g, y and an underscore, all inside the row. The middle row holds a light capital E whose acute accent rises into the bottom of the blue top row and is drawn over the blue, inside a dashed box that spans the three columns around the E and the three rows from the top row to the bottom row. The bottom row holds an amber Arabic lam with a fatha above it that rises into the bottom of the middle row. Below, the order each pixel is drawn in: its cell's background, its cell's underline, the ink from the row above, the ink from the row below, the ink from the cells beside it, its cell's own glyph, its cell's strike-through.](figures/ADR-1306-overflow.svg)

The words used below: a glyph's **reach** is the block of cells its ink may draw in (ADR-1302 D1, widened here); its **ink box** is the rectangle of pixels its ink covers, cut to its reach (ADR-1302 D6); the **row scratch** is the buffer in main memory one row of cells is composed in before it is copied to the display (ADR-1300 D5).

### D1: the reach up and down

- **A glyph's reach is its own cells and one cell more on every side:** one row above and one row below, as well as one cell to the left and one to the right, the corners included. A narrow glyph's ink lies in at most 3 by 3 cells and a wide glyph's in at most 4 by 3. Ink inside the reach is drawn; ink outside it is cut.
- **The glyphs that reach up and down are the ones that reach sideways** under ADR-1302 D1: outline glyphs, regular, bold and italic, real faces and synthetic styles alike, and the shaped cluster glyphs the helper sends. These stay inside their own cells, as ADR-1302 D1 lists them: color glyphs and emoji, vtium's own drawn symbols (box drawing, blocks, shades, Braille and Legacy Computing) and the underline and strike-through, the bar cursor, and bitmap glyphs, whose italic shear moves rows sideways only.
- **A fallback font's glyph that would pass its cells is drawn smaller to fit them,** as ADR-1200 D17 decides (the maintainer, 2026-11-06), and does not use the reach. The reach is for ink that the frit's first source and its own style faces draw past their cells on purpose, and for the marks of a shaped cluster, which D2 places, from any source.
- **The grid's top and bottom edges cut ink,** as its left and right edges do (the maintainer, 2026-11-01, on the grid edge), so the border around the grid never shows ink. An accent on the grid's first row is cut at the grid's top.
- **One row is the bound,** and the measured ink needs less than half of it: at most 0.42 of the cell above the row and 0.35 below (Context). The bound is a whole row, rather than a share of one, because what it costs is set by how many rows a pixel looks at, which is three for any bound up to a row, and one rule then names the same one cell in every direction. A script whose stacks pass a whole row, such as Tibetan with several subjoined letters, meets D2's move-and-scale rule at that edge.

### D2: the baseline, the marks, and a letter's own ink

This decision amends ADR-1302 D3.

- **The baseline stays as ADR-1302 D3 sets it:** the font's ascent below the cell's top, rounded up, and raised by whole pixels where the lowest ink of the font's printable ASCII would pass the cell's bottom. So the underscore and the descenders of ASCII still end inside their own cell, and a screen of ASCII text never sends ink into another row.
- **Ink is cut at the reach, not at the row.** ADR-1302 D3's bullet "Ink above the cell's top or below its bottom is cut" becomes: ink more than one row above the cell's top or below its bottom is cut.
- **A combining mark is moved, or scaled with its letter, only where it would pass the reach** (the maintainer, 2026-11-06). ADR-1302 D3's rule stays as it is, with the reach's top and bottom edges in place of the row's: a stack of marks whose ink would pass more than one row above or below moves toward the baseline until it fits, and where it cannot move in alone, the letter and its marks scale together vertically to fit, anchored at the ink's opposite edge. Inside the reach, a mark draws where HarfRust placed it. ADR-1302 D3's placement of a mark the shaper left unplaced runs first, as it does today.
- **A base letter's own ink is cut at the reach** (the maintainer, 2026-11-06), where ADR-1302 D3 cuts it at the row. The hamza of a precomposed alef with hamza draws whole.
- **A precomposed letter and its decomposed sequence draw alike.** Neither É (U+00C9) nor E followed by U+0301 is moved or cut while its ink stays within a row of its own, so the two differ only where the font draws them differently.

### D3: how a glyph carries ink above and below its row

This decision amends ADR-1302 D6.

- **The ink box gains a top and a height:** the rows its ink covers, cut to the reach, always holding the glyph's own cells, given as the box's top row's offset from the cell's top in pixels, which may be negative, and the number of rows. A glyph whose ink stays inside its row has the cell's height as its box's height, as every box has today.
- **The glyph chunk format carries them** (ADR-1200 D2). Its INKX records carry their own record size, so the two fields are added to each record, and a box written without them reads as the cell's full height, as a font set already written to disk holds it. The encoding is the build's to make.
- **The console's check of each answer from the helper** (ADR-1200 D15) refuses a box that passes the reach up or down, as it refuses one that passes it sideways.
- **The chunk bound of 16 KiB** counts the whole box, as ADR-1302 D6 counts it; a glyph whose box would pass it is cut to its own cells, as there. A narrow accented capital in a 16x32 cell carries a box of about 2.3 KiB.
- **The drawn-glyph cache** (ADR-1300 D2) stores each glyph at its box, and its pools' byte ceilings stay as they are (ADR-1300 D4). Only glyphs whose ink passes their row carry a taller box.

### D4: the order a pixel is drawn in, and composition a row at a time

This decision amends ADR-1302 D7 and ADR-1300 D5.

The renderer still composes one row of cells at a time in the row scratch, and copies each pixel line out to the display's buffer, which it never reads. To compose a pixel, it reads from the frame and the drawn-glyph cache the glyphs of the three rows around the pixel's row whose reach covers the pixel. Each pixel of a composed run is drawn in this order:

1. The background of the cell the pixel lies in.
2. That cell's underline.
3. The ink of the glyphs in the row above whose reach covers the pixel, from the cell before the pixel's column to the cell after it in the cell's direction.
4. The ink of the glyphs in the row below whose reach covers the pixel, in the same order.
5. The ink of the glyphs beside the cell in its own row, as ADR-1302 D7's step 3 orders them.
6. That cell's own glyph.
7. That cell's strike-through.

- **No row's background paints over another row's ink,** because the only background a pixel ever takes is its own cell's, drawn first. This is the order Alacritty, WezTerm and Ghostty reach by drawing every background of the frame before any glyph; vtium reaches it pixel by pixel, so it needs neither a second pass over the frame nor a read of the display's buffer.
- **These are the maintainer's two passes, folded into one walk over each pixel** (the maintainer's design input of 2026-11-06, `considered`). The first pass, every cell's glyph inside its own cell, is step 6; the second, the parts of glyphs that fall outside their cells, is steps 3 to 5. The second pass's buffer is the drawn-glyph cache: the parts of a glyph outside its cell are its ink box (D3), which the helper works out once when it draws the glyph and the cache stores with it, so the renderer reads them from there for each cell they land in. A buffer of overflow ink kept for the whole frame would cost 4 MiB at 1366x768 for a color and a coverage per pixel, and would have to be kept current through every change, where the cache already holds each glyph once.
- **The overflow blends; it is never XORed** (the maintainer, 2026-11-06). XOR combines the bits of two colors, which composes neither antialiased ink nor ink over ink. White ink (255) half covering a dark background (31) blends to 143, a grey edge; XOR gives 31 XOR 128 = 159 for the half-covered pixel and 31 XOR 255 = 224 for a fully covered one, so even solid ink comes out tinted by the background. Two glyphs whose ink covers the same pixel XOR back to the background there, so ink vanishes exactly where a descender meets an accent. A color glyph or a picture beneath XORs to unrelated colors (D7). XOR's one strength, removing ink by XORing it again with no redraw, needs to know exactly what each buffer holds, and vtium's buffers hold frames of different ages; vtium composes each damaged cell from nothing instead (ADR-1300 D5), so it never needs to undo ink. On a 1-bit display ADR-1302 D8 already draws ink that lands in another cell as the opposite of that cell's background, the part of XOR that works there, and a pixel two glyphs cover stays drawn rather than cancelling.
- **Where ink from two rows meets** (the maintainer, 2026-11-06), as a descender from the row above and an accent from the row below can in a tight cell, the row below's ink lies over the row above's, as a later line of text lies over an earlier one; ink from the pixel's own row lies over both, and the cell's own glyph on top. Only the color at a pixel where they meet depends on this order: ink of one color combines to the same coverage in either order.
- **Steps 2 to 7 form the pixel's text layer,** premultiplied, which goes over the background in one step, as ADR-1302 D7 has it.
- **A cell that no glyph's ink reaches from another cell draws exactly as today.** Each cell keeps one byte saying which of its eight neighbours its glyph sends ink into (D5), so the renderer finds the cells that receive ink by reading their neighbours' bytes, and every other cell costs that test alone. A screen drawn in bitmap fonts with no italic, or in printable ASCII, has no such cell.
- **The row scratch does not grow:** it still holds one row of cells at the display's width, 87,424 bytes at 1,366 pixels across and 16-pixel rows, at most 1 MiB, and a taller row is still composed in bands of pixel lines. A box from the row above or below is read for the pixel lines of this row it covers.
- **The cost on the minspec build** (vtium's smallest default build) is bounded: one more byte per cell, 8,160 bytes at 1366x768 in 8x16 cells; a test of that byte per drawn cell; and, for a cell that receives ink, at most eight more glyph blends, each the same blend its own glyph takes. The built-in fonts are bitmap fonts, so a console that uses only them pays the byte and the test and draws the same pixels. `crates/vtium-render/tests/figures.rs` measures the frame in both cases and `crates/vtium-render/tests/memory.rs` pins the byte.
- **vtium does not move rows already drawn when the screen scrolls.** The renderer draws every row whose content changed again, and D5 draws again each row beside it whose received ink changed, at a scroll region's top and bottom margins and at the screen's edges as anywhere else. If a later decision moves rows already drawn, by a copy within the display's buffer or on the GPU, it draws again each row whose row above or row below is not the one it was beside before the move: at a scroll region's margins, the region's first and last rows and the rows just outside them, and at the screen's top and bottom, the row beside the one that scrolled away or arrived.

### D5: damage reaches the rows above and below

This decision amends ADR-1302 D9.

- **Each cell remembers its reach in eight directions:** whether the glyph it last drew had ink in each of the eight cells around it, one bit each.
- **A cell whose ink changes damages the neighbours its old reach or its new reach names,** above and below and at the corners as well as beside it. What counts as a change of ink is ADR-1302 D9's list, unchanged: the glyph, its style, its foreground, its own background where it changes dim ink or hides or shows it, whether it blinks, and whether the cursor stands on it.
- **Damage never chains,** as ADR-1302 D9 has it: a neighbour drawn again because of its neighbour has not changed its own ink.
- **The same holds for every source of change:** program output, the cursor, the selection, a glyph arriving from the helper in place of the built-in drawing, and scrolling.
- **The damage rectangles** handed to the display cover the neighbours drawn this way, in the rows above and below too.
- Typing an accented capital whose accent passes the row draws the cell it wrote, the cell the cursor entered, and the cell above the written cell: three cells, where two are drawn today. A keystroke in an italic run of accented capitals draws at most the written cell and its eight neighbours, and the cell the cursor entered with the cell after it, as ADR-1302 D9 counts a leaning letter that the cursor arrives on.
- **What a new character costs.** A glyph's reach is part of its ink box, worked out once when the helper draws it and stored with it (D3). Writing a character reads its eight bits from the stored glyph, compares them with the cell's last reach, and stamps the neighbours either names: a few instructions, and no stamp at all for a glyph whose ink stays in its cell, which every printable ASCII glyph and every bitmap glyph is. On the CPU path the frame then composes the cells stamped, each neighbour with one more glyph blend for this glyph's ink. On the GPU path the same stamps choose the cells drawn again, and the shader samples a neighbour's glyph only where the cell's reach byte names it.
- **How often it happens,** measured (Context): in DejaVu Sans Mono, no cell of English prose or of a shell listing sends ink out of its cell at 16 or 32 pixels; about 2 per cent of the letters of French, German and Vietnamese prose do at 16 pixels, and almost none at 32; 12 per cent of a line of accented capitals at 16 pixels; about 2 per cent of the clusters of unvocalized Arabic, and 13 to 19 per cent of fully vocalized Arabic, up to 21 per cent in Noto Sans Arabic at the same size. A screen of fully vocalized Arabic therefore composes about a fifth of its cells with one or two more glyph blends.

### D6: the cursor, the selection, reverse video and the decorations

This decision amends ADR-1302 D8, whose rules hold for ink above and below the row as they hold for ink beside it:

- **Ink takes the colors of the cell it came from,** its foreground as that cell draws it, dimmed or hidden with that cell, wherever it lands in an unmarked cell. Under reverse video (SGR 7, or DECSCNM for the whole screen), that is the reversed foreground: an accent on a reverse-video capital in a status line draws in the status line's text color over the row above.
- **A mark takes over the ink inside it:** inside a block cursor's cell, or a selected cell, all ink, from the rows above and below included, draws in the mark's text color. A selected cell's own glyph draws the ink that lands outside the selection, above or below it included, in the colors it would have unselected, and the renderer reads the frame without its selection for the rows that ink lands in.
- **The letter under the cursor keeps its ink above and below its row,** in the colors it has without the cursor, in both phases of the cursor's blink, so no neighbour changes with the blink and the letter does not lose its accent while the cursor stands on it (the maintainer, 2026-11-06). Its ink beside its cell stays inside the cell, as the maintainer decided on 2026-11-01. The cursor's blink therefore changes only the cursor's own cell.
- **A blinking letter's ink stays inside its own cell** (the maintainer, 2026-11-01), above and below as well as beside it, so no neighbour ever appears to blink. An accented capital set to blink shows its accent cut at the row while it blinks, wherever its accent passes the row.
- **Ink invisible in its own cell is invisible everywhere** (the maintainer, 2026-11-01), up and down too.
- **Underlines and strike-throughs stay in their own cells;** only glyph ink reaches. An accent from the row below lies over the underline of the row above (D4's steps 2 and 4), as a cell's own descender lies over its underline today. The underline cursor is its cell's underline, so the same accent can cross it; the bar cursor is its cell's glyph and lies over every neighbour's ink.
- **On a 1-bit display** ink that lands in another cell, from any direction, is drawn as the opposite of that cell's background, as ADR-1302 D8 has it.
- **vtium's notices and the focus mark** (ADR-1401) draw over the composed frame and cover whatever ink lies beneath them. Ink from the row below a notice that reaches into it stays hidden while the notice shows, and the frame drawn whole when the notice ends shows it again.

### D7: pictures

This decision adds to ADR-1100 D4, the order a frame draws pictures in, and replaces ADR-1302 D8's bullet on pictures. A picture here is a kitty graphics placement or a sixel, in the layers ADR-1100 D4 gives them.

- **Ink that reaches a cell is part of that cell's text layer,** from whichever neighbour it comes, so it takes the text layer's place in ADR-1100 D4's order:
  - a kitty picture below the backgrounds (z below -1,073,741,824) or below the text (z from -1,073,741,824 to -1) draws under it, so an accent from the row below lies over the picture as a letter in that cell would;
  - a kitty picture at z 0 or more, and every sixel, draws over it, and hides it where the picture covers the cell;
  - the selection's tint, the cursor, the pointer, vtium's notices and the focus mark draw over both, as ADR-1100 D4 has them.
- **A picture's cells do not cut ink from the text around them** (the maintainer, 2026-11-06). A program that puts a picture below the text asks for text to show on it, and an accent reaching in from the row below is text; a picture above the text hides the ink already. A cut would also make the cells a glyph's ink reaches depend on pictures, so a picture appearing or leaving would change what its neighbours' glyphs damage.
- **Ink blends over a picture's pixels** as it blends over a background, with the premultiplied arithmetic ADR-1300 carries forward. It is never XORed: an XOR of a picture's pixel with ink gives a color unrelated to both, as an orange photo pixel (200, 120, 40) XOR white ink gives blue (55, 135, 215).
- **A picture's change repairs only the cells it covers.** A new frame of an animation, and a placement that appears, moves, is cut or is deleted, mark the cells under its old and new box changed (ADR-1100 D8). The renderer composes each of those cells whole: its background, the picture layers, and the ink that reaches it from its eight neighbours, read from their glyphs (D4). A picture changes no pixel outside its cells, and the ink that crosses into its cells is drawn again with them, so no cell outside them needs repair. The cell under a picture's ragged right or bottom edge is one of the cells it covers, and is composed whole too.
- **A cell a picture covers sends ink like any other.** Its glyph, where a picture below the text leaves one there, reaches its neighbours as D1 says.
- Ghostty and WezTerm, which draw kitty pictures and never cut glyphs, order them the same way. Ghostty draws its pictures below the backgrounds and below the text in passes before its text pass, and the pictures above the text after it. WezTerm puts pictures below the text on its background layer and pictures above it on a third layer over the glyphs. foot draws sixels after every cell, over every glyph's ink. kitty cuts each glyph at its cell, so none of its ink reaches into a picture's cell (`basis`).

### D8: line spacing

This decision amends ADR-1302 D13.

- **A `natural` glyph's ink is no longer cut at the text band.** The glyph keeps the font's size and sits on its baseline in the text band, as ADR-1302 D13 has it, and its ink may use the added rows above and below the text band, which are part of its cell, and past them the reach of D1. An accent therefore draws in the added rows before it reaches the row above.
- **`room` draws as `natural`,** as it already does today, since every glyph's marks may now use the whole cell and the reach (the maintainer, 2026-11-06). `fit=room` stays a value a frit may write.
- **`fill` and color glyphs are unchanged:** line and block characters fill the whole cell, and color glyphs and emoji stay inside the text band.
- **D2's move-and-scale rule** applies at the reach's edges inside a taller row too.

### D9: the GPU path

This decision amends ADR-1302 D10. The GPU path draws the CPU path's pixels (ADR-1300 D6): a cell's fragment shader samples its own glyph and the glyphs of the up to eight neighbours whose reach covers it, which the cell's reach byte names, in D4's order and with D6's colors. A glyph pool's slot holds the largest box its glyphs may have, so the GPU path may give glyphs whose ink passes their row pools of their own (ADR-1302 D6). Tier 2 compares the two paths byte for byte, as for every screen.

### D10: the cell's size stays

ADR-1200 D17 stays as it is: the helper sizes an outline font so its ascent and descent fill the cell, and the cell's width comes from W. The terminals measured size their rows from the font's line height and the user shortens them by a setting; vtium's cell is set by the request and the font is fitted to it, and the reach of D1 draws what passes the font's ascent and descent. Shrinking the first font by up to 5 per cent, with precomposed letters refitted through the mark rule, would have kept Latin accents in the row and could not carry Arabic, whose marks pass it by up to 0.42 of the cell; full hinting by default changes every glyph and fixes some heights only. Every outline source after the first draws at the first source's size (ADR-1200 D17, the maintainer, 2026-11-06), so Noto Sans Arabic behind DejaVu Sans Mono draws at DejaVu Sans Mono's size, where its marks pass the row by up to 0.42 of the cell (Context), and not at its own, where it drew at 0.55 of that size. ADR-1200 D17 also draws a fallback font's glyph smaller where its ink would pass its cells, across or up and down, and that stays as it is (D1).

### D11: the work, in slices

Each slice is a card of its own, branched from `integration`. The build goes into v0.1.0-alpha.6 (the maintainer, 2026-11-06).

1. **The ink box above and below** (vtium-font, vtium-config, and the console's check in `crates/vtium`): the INKX record's top and height and their round trip; the helper's ink box cut to the reach up and down; D2's move-and-scale rule at the reach's edges and the letter's own ink cut there; D8's `natural` without the cut at the text band; the console's refusal of a box past the reach. It does not block right-to-left text's drawing slice (ADR-1004 D15, slice 4), which draws the forms and marks the helper gives it either way.
2. **Composition and damage** (vtium-render): drawing ink into neighbours in all eight directions, which also builds the side-to-side drawing ADR-1302 decided and vtium-render does not draw yet, so both are built once; the reach byte, D4's order, D5's damage, D6's cursor, blink and selection rules, the stored glyph at its box, the frame read without the selection for the rows ink lands in, and the received ink in the text layer of a cell pictures reach (D7). It needs slice 1's box, or boxes its tests build.
3. **The screens** (tools/tier2): the screens of the Verification section, and the screens whose pixels move, `emoji-spaced-12` and `emoji-spaced-30` among them, each set of goldens in a commit of its own that the maintainer reviews as pictures. Right-to-left text's screens slice (ADR-1004 D15, slice 5) holds Arabic with marks, so if this slice lands first those goldens are reviewed once; if it lands after, they move again and are reviewed again.
4. **The GPU path** (vtium-render's GPU path): D9, with the GPU path's own work; it blocks nothing.

### D12: how far ink reaches side to side

*Proposed, for the maintainer to decide (2026-11-06). The rest of this record is accepted; this decision is not yet. Until it is decided, the reach side to side stays one cell, as ADR-1302 D1 has it.*

The maintainer asked, as a design input, whether ink should reach further to either side now that it reaches the rows above and below, possibly two cells each way (`considered`, 2026-11-06). This decision weighs one cell against two.

**How far ink reaches past its cell side to side,** measured on 2026-11-06 (`basis`) in DejaVu Sans Mono's 9x16 and 17x32 cells, with each glyph's advance centred in its cells (ADR-1302 D2), as a share of a cell, the larger of the two sizes:

| Glyphs | Most past the left edge | Most past the right edge |
|---|---|---|
| DejaVu Sans Mono Book and Bold, printable ASCII, Latin-1 and Latin Extended-A | 0.06 | 0.15 |
| DejaVu Sans Mono Oblique, its italic face | 0.09 | 0.20 |
| DejaVu Sans Mono Bold Oblique | 0.11 | 0.31 |
| Book and Bold slanted by 0.2, synthetic italic | 0.05 | 0.39 |
| DejaVu Sans Mono's Arabic, joining letters stretched to their cells (ADR-1302 D2) | 0.26 | 0.03 |
| Noto Sans Arabic at DejaVu Sans Mono's size, joining letters stretched to their cells | 0 | 0.19 |
| Noto Sans Arabic at that size, if its letters were not stretched (seen and sad) | 0.53 | 0.66 |
| Noto Sans Arabic's lam-alef, in its two cells | 0 | 0 |
| Noto Sans Devanagari at that size, a syllable in the one cell vtium gives a grapheme cluster (कि, क्षि, स्त्र) | 0.32 | 0.34 |
| Noto Sans Devanagari, a five-consonant conjunct in one cell (र्त्स्न्य) | 0.76 | 0.78 |
| MesloLGS DZ Nerd Font, not Mono, icons in one cell | 0.04 | 0.68 |
| Symbols Nerd Font Mono icons at DejaVu Sans Mono's size, in one cell | 0.39 | 0.34 |

No glyph measured reaches a whole cell past its own, so none would draw in a second cell. The furthest, the five-consonant conjunct, reaches 0.78 of a cell. The icons are drawn smaller to fit their cells where their font is a fallback (ADR-1200 D17), so their reach is used only where a Nerd Font is the frit's first source.

**What a reach of two cells costs against one:**

| | One cell each side | Two cells each side |
|---|---|---|
| The block a narrow glyph's ink may lie in | 3 by 3 cells, 8 neighbours | 5 by 3 cells, 14 neighbours |
| A wide glyph's block | 4 by 3 cells | 6 by 3 cells |
| The reach each cell remembers (D5) | 8 bits, one byte a cell: 8,160 bytes at 1366x768 in 8x16 cells | 14 bits, two bytes a cell: 16,320 bytes |
| Cells a changed cell can damage | 8 | 14 |
| Glyphs a pixel's composition may look at (D4) | its own and 8 neighbours, in 3 columns | its own and 14 neighbours, in 5 columns |
| The cell size at which a wide glyph's box at its full reach passes the 16 KiB chunk bound (D3) | more than 1,365 pixels a cell, 26x52 | more than 910 pixels a cell, 22x44 |
| The row scratch (D4) | unchanged | unchanged |
| A scroll, and a scroll region's margins (D4) | the rows beside a changed row | the same rows: the reach side to side adds no row |

The cost of a wider reach comes only where a glyph's ink actually reaches: a cell whose neighbours send it no ink still costs one test, of two bytes instead of one, and a glyph's box holds only the columns its ink covers. What grows is the bound: the damage block of a changed cell, the columns a composed cell looks at, the box the console's check allows, and the size at which a wide glyph's box is cut to its cells. The order of D4's step 5 would name the cells two away first, then the cells beside it, so the nearer ink lies on top.

**The lean: keep one cell.** Every glyph measured stays within it, the furthest by a fifth of a cell to spare. A second cell would double the damage bound and the reach each cell keeps, for no glyph measured. A font whose ink reaches further, such as a script italic or a font of very wide symbols, would draw that ink cut at one cell past its own, as it does today. The alternative, two cells, leaves room for such fonts and for marks placed far from their letter, at the costs above.

If the maintainer chooses two cells, D1, D3, D4, D5 and D9 take the new bound, ADR-1302 D1, D6, D7 and D9 take dated notes, and the slices grow: slice 1's ink box and the console's check allow two cells, and slice 2's composition reads five columns and keeps two bytes of reach a cell. With one cell, the slices are as D11 has them.

**These stay as they are,** as the maintainer decided them:

- **A fallback font's glyph that would pass its cells is drawn smaller** (ADR-1200 D17, the maintainer, 2026-11-06). The reach, one cell or two, is for ink that a font draws past its cell on purpose, such as an italic's lean, not for a fallback glyph too big for its cells.
- **The letter under the cursor keeps its ink beside its cell inside the cell** (ADR-1302 D8, the maintainer, 2026-11-01).

The measurements give one reason to revisit the first, which this record raises as a question and does not change (For the operator's check, open question 2): Noto Sans Arabic as a fallback at DejaVu Sans Mono's size has 4 of the 110 letter glyphs of a sentence of unvocalized Arabic passing the row's bottom, so ADR-1200 D17 draws those 4 smaller than the letters beside them in the same words, while the marks of a shaped cluster reach the row below (D2). After the stretch to their cells, the same font's joining letters still pass their cells sideways by up to 0.19 of a cell, and ADR-1200 D17 does not say whether such a letter is drawn smaller as well as stretched.

### For the operator's check

The maintainer answered on 2026-11-06 (`considered`), taking each lean, and each answer is written into the decision above:

1. Ink reaches one row up and down (D1), reversing the answer of 2026-11-01; the baseline's rise for ASCII stays (D2).
2. The move-and-scale rule keeps working, with its edge and the cut of a letter's own ink at the reach instead of the row (D2).
3. The letter under the cursor shows its ink above and below its row in both blink phases, in its colors without the cursor, and its ink beside the cell stays cut (D6).
4. `fit=room` draws as `natural` and stays a value a frit may write (D8); where two rows' ink meets, the row below draws on top (D4); a blinking accented capital shows its accent cut while it blinks (D6); an underline cursor can be crossed by an accent from the row below (D6); and the build goes into v0.1.0-alpha.6 (D11).
5. The overflow blends and is never XORed, in place of the maintainer's sketch of 2026-11-06 (D4), and a picture's cells do not cut the ink of the text around them, following ADR-1100 D4's layers (D7).

#### Open, proposed 2026-11-06

1. **How far ink reaches side to side** (D12): one cell, as ADR-1302 D1 has it, or two. The lean: one cell. Evidence: no glyph measured, italic, bold italic, Arabic, Devanagari or icon, reaches a whole cell past its own, the furthest 0.78 of a cell; two cells would raise the damage bound from 8 neighbours to 14 and double the reach each cell keeps.
2. **A fallback letter of a joining script that passes its row or its cells** (D12): ADR-1200 D17 draws it smaller, so 4 of 110 Noto Sans Arabic letters in a sentence draw smaller than their neighbours, while the marks of a shaped cluster reach the row below. The lean: let a fallback joining-script letter's own ink use the reach, as the first source's does, and keep D17's shrink for every other fallback glyph. This changes a decision the maintainer made on 2026-11-06, so the record leaves D17 as it is until the maintainer answers.

## Departures from the C reference

Each is listed in vtium-render's README, or vtium-config's for placement, when the work in D11 makes it, and each tier 2 or tier 3 golden that shows it lists the cells where it differs. No specification covers glyph ink outside its cell.

- **Glyph ink reaches into the rows above and below.** The C reference cut every glyph at its box. vtium draws a glyph's ink up to one row above and one below its own (D1), so accented capitals and Arabic marks draw whole in a cell the font's ascent and descent fill.
- **A mark draws where its font places it.** The C reference pushed a mark that did not fit back inside the cell. vtium moves or scales a mark only where it would pass a whole row past its own (D2).

## Beyond the C reference

- A changed cell redraws the neighbours its ink reaches in the rows above and below (D5); the C reference had no ink outside a cell to redraw.
- A block cursor and a selection recolor the ink that reaches them from the rows above and below (D6).

## Verification

Each test that drives this behaviour through vtium-render or vtium-config carries `// covers: text.overhang`.

- **Tier 2 screens,** vtium's own goldens, provisional until the maintainer reviews them. Tier 2 reads no font of the system's, so each draws an outline test font built in memory, as vtium-config's tests build one, with glyphs whose ink passes their row by a known number of pixels, and Arabic marks above and below:
  - `overflow-accents`: accented capitals at every cell height from 12 to 32 pixels under rows of `gjpqy___` whose backgrounds alternate between two colors, each accent whole; on the grid's first row, cut at the grid's top; in reverse video; under a selection that covers the row above and one that does not; with a block cursor, an underline cursor and a bar cursor on an accented capital and on the cell above it; and blinking, whose neighbours' pixels stay the same in both blink phases.
  - `overflow-marks`: Arabic letters with marks above and below at every cell height from 12 to 32 pixels, lam with shadda and fatha at its full height, and a stack that passes a whole row, moved or scaled to the reach.
  - `overflow-meet`: a descender in one row and an accent in the row below meeting in two different colors, the row below's on top.
  - `overflow-spaced`: the same accents and marks with line spacing added, using the added rows.
  - `overflow-pictures`: accents and Arabic marks reaching into the cells of a kitty picture at z below -1,073,741,824, at z -1 and at z 0, and of a sixel: the ink shows over the first two and is hidden by the last two.
  - Each also in 1-bit mode, at a cache of 0 glyphs, and on the GPU path once it lands, as tier 2 draws every screen.
- **Damage:** a frame draws a glyph whose ink reaches the row above, and the next frame writes a space over it; the cell above then equals a whole draw of the second frame in buffers of age 1, 2 and 0, and the damage rectangles include it. The same for the row below and for each corner; for a background changing under ink from the row below; for the cursor arriving on an accented capital and leaving it, and blinking there with no neighbour changing; for a selection whose edge lies between the rows; and for a scroll region whose margin lies under an accented row, scrolled one line both ways. A script of typing and erasing accented capitals at the rows' edges checks every frame against a whole draw at every buffer age, as `every_frame_matches_a_whole_draw_under_each_blink_option` does. Planted defects, each seen to fail: drop the damage of the cell above; drop that of the cell below; drop a corner's; draw a neighbour row's background over the ink; draw the row above's ink after the row below's; leave the cursor's cell's ink above the row out in one blink phase; draw a blinking letter's ink above its row; let ink pass the grid's top edge; draw received ink over a picture above the text; leave the received ink out of a cell a deleted picture uncovers. A picture that appears, moves, plays a frame and is deleted under an accent from the row below is checked against a whole draw at every buffer age.
- **Placement:** for the test font, a glyph's ink box equals the union of its cells and its hinted ink, cut to the reach in all four directions; a precomposed and a decomposed accented capital give the same pixels; a mark inside the reach keeps HarfRust's place, and a stack past it moves or scales, a planted rule at the row's edge failing it; a box past the reach up or down, planted in a helper's answer, is refused; an INKX record written without the new fields reads as the cell's full height.
- **Nothing else moves:** every tier 2 and tier 3 screen drawn only with bitmap fonts, or with outline text whose ink stays in its row, draws byte for byte as before; `crates/vtium-render/tests/figures.rs` shows the frame of such a screen at its present cost, and `tests/memory.rs` pins the reach byte.

## C notes

- The pango engine set the description's absolute size to the requested height (`pango_font_description_set_absolute_size(desc, PANGO_SCALE * height)`), laid out the ASCII letters and punctuation, and took `font->height` from the layout's logical extents and the baseline from `pango_layout_get_baseline` (`kmscon/src/font/font_pango.c:324, 337-347`), so the cell was the font's line height.
- Each engine cut its glyphs at their boxes, and bbulk drew each cell in its own box; ADR-1302's C notes give the lines.

## Consequences

### Positive

- Accented capitals in Latin-1, Latin Extended-A and Vietnamese draw whole in DejaVu Sans Mono at every cell height, with and without line spacing.
- Arabic marks and letters draw at the font's size, where the font puts them, instead of moved or scaled into the row, and the hamza of a precomposed alef shows whole.
- Precomposed and decomposed letters draw alike.
- The side-to-side drawing ADR-1302 decided and this record's drawing up and down are built once, in one composition and one damage rule.
- A screen of ASCII text, or one drawn in the built-in fonts, draws the same pixels at the cost of one byte and one test per cell.

### Negative

- A changed cell can redraw up to eight neighbours, where ADR-1302 redraws up to two.
- In a tight cell, an accent or a mark can touch the descenders of the line above, where today it is cut or moved; dense Arabic text with marks reads closer together.
- A blinking accented capital loses its accent's top while it blinks.
- An accent on a reverse-video capital draws in the reversed foreground over the row above, which can be close to that row's background.
- The glyph chunk format's ink box and the console's check change on both ends.

### Neutral

- ADR-1302's title, "cut at its row", describes it as decided on 2026-11-01, and its Summary says so; ADR-1302's Summary, D1, D3, D6, D7, D8, D9, D10 and D13, ADR-1300's D5 and ADR-1100's D4 each carry a dated note pointing here.
- A fallback font's glyph that would pass its cells is still drawn smaller (ADR-1200 D17); only the marks of its shaped clusters use the reach.

## Alternatives Considered

- **Keep the cut at the row.** Rejected: it removes up to 2 rows of accent in DejaVu Sans Mono at a 16-pixel cell and cuts or shrinks every Arabic mark that passes the row.
- **Shrink the first font by up to 5 per cent and refit precomposed letters through the mark rule.** Set aside: it fixes Latin accents only, moves every glyph of every frit, and Arabic marks pass the row by up to 0.42 of the cell.
- **Full hinting by default.** Set aside: it changes every glyph's shape and fixes some heights only.
- **Make the cell as tall as the font's line height,** as the C reference's pango engine did. Rejected: every outline frit's grid would lose rows, and the cell's height would no longer be the one the user or the frit asked for (ADR-1200 D17).
- **Let ink go anywhere, as Ghostty and WezTerm do.** Rejected: with no bound, a changed cell's damage has no bound, and the renderer would have to draw the whole frame each time, which the minspec build's cell-by-cell damage avoids.
- **Draw every background of the frame, then every glyph,** as Alacritty, WezTerm and Ghostty do. Rejected for the CPU path: with damage cell by cell, a second pass over a cell already drawn reads the display's buffer, the slow path ADR-1300 D5 measured at 0.16 GB/s on an integrated GPU against 87 GB/s from cache; D4's order gives the same pixels row by row.
- **Two passes with the overflow put on by XOR,** the maintainer's sketch of 2026-11-06. Its two passes and its per-cell record of which glyphs leave their cell are adopted (D4, D5); XOR is not, for the reasons D4 gives.
- **Keep the overflow in a buffer for the whole frame,** the second pass's own buffer. Rejected: 4 MiB at 1366x768, kept current through every change, where the drawn-glyph cache already holds each glyph's ink outside its cell once.
- **Draw a band of three rows, every background first and then every glyph,** as Alacritty draws a batch. Not chosen: it gives D4's pixels, but needs a scratch three rows tall or draws each row three times, and the order where ink meets follows the order glyphs were drawn in rather than a rule.
- **Bound the reach at half a row.** Not chosen: it holds every glyph measured, but costs the same, since a pixel still looks at three rows, and adds a second number to the rule.
