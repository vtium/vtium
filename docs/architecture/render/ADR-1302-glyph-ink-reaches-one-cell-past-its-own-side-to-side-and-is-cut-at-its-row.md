---
contract: adr/v1
kind: decision
verb: add
capability: renderer
basis:
  - operator: aaronsb
    level: directed
    said: "Operator's decision, 2026-10-26: the cell width stays `kmscon/kmscon`'s, the larger of the advance and W's ink width, and this issue is re-scoped as the design of glyph overhang: how ink outside its cell is drawn. The C reference is frozen under ADR-304, so that design is vtium/vtium#26."
    via: "aaronsb/kmscon#64, the comment of 2026-10-26T22:32:47Z, posted for the operator; vtium/vtium#26's body carries the same decision"
  - upstream: "aaronsb/kmscon b1f7a7c5: the cell's width from W (src/font/font_freetype.c:130-138); a glyph cut to its cell box, moved left by half of what passed the box's right edge (src/font/font_freetype.c:340-380), a 1-bit glyph moved up to end in the box's last row (src/font/font_freetype.c:309-338), a narrow advance centred in a two-cell box (src/font/font_freetype.c:425-441); the pango engine drawing a cluster into a cell-sized bitmap with its ink's left edge on the cell's left edge (src/font/font_pango.c:207-228); bitmap glyphs re-boxed to their cells, centred and cut (src/font/font.c:268-308); bbulk drawing each cell's background, underline, glyph and strike-through in its own box (src/render/bbulk.c:52-55) and damaging cells one by one (src/render/bbulk.c:236-241)"
  - evidence: "DejaVu Sans Mono 2.37 (Arch's ttf-dejavu 2.37+18), its Book, Oblique, Bold and Bold Oblique faces, measured by this record's agent on 2026-11-01 with fontTools 4.67: each printable ASCII glyph's outline bounds at the cell vtium-vitrify sizes for heights 16, 22, 28 and 32 pixels (a cell request half a pixel under the height, the baseline the ascent below the top rounded up), rounded outward to whole pixels, unhinted, with the pen at the cell's left edge; and the underscore's rows at every even height from 12 to 32. Hinting can move an edge by a pixel. The script sits in the agent's scratchpad, uncommitted."
  - evidence: "foot (MIT): its foot.ini(5) `overflowing-glyphs`, on by default, lets a glyph wider than its cells draw into one more cell, for italic and for Nerd Font and Powerline symbols; its render.c draws a row's cells from right to left, clips each glyph to its cell and one more to the right, draws a block cursor before the glyphs, and before a frame marks dirty the cells to the left and right of a dirty cell until a cell whose glyph stays in its box. Read 2026-11-01 from codeberg.org/dnkl/foot at master."
  - evidence: "Ghostty (MIT): its shaders place each glyph's quad at the cell's origin moved by the glyph's bearings, with no clip to the cell, read each cell's background from a buffer of its own, and color a glyph standing on the cursor's cell in the cursor's color. Read 2026-11-01 from github.com/ghostty-org/ghostty at main, src/renderer/shaders/shaders.metal. Its renderer lets a Nerd Font icon span two cells when the cell to its right is blank (its constraint width), seen in its commit history, not in its documentation."
  - evidence: "fontconfig's 90-synthetic.conf slants a face with no italic by the matrix (1, 0.2; 0, 1); DejaVu Sans Mono Oblique's italic angle is -11 degrees, a slope of 0.19 (its post table)"
  - precedent: ADR-1300
  - precedent: ADR-1301
  - precedent: ADR-1200
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "Tier 2's `overhang-italic` screen shows an italic run whose letters lean into the next cell, unbroken across cell edges, over cells of alternating background colors"
  - "Tier 2's `overhang-underscore` screen shows a whole underscore in the bottom rows of every cell height from 12 to 32 pixels"
  - "Typing over an italic letter, and erasing it, leaves no ink behind in the cell beside it, in every buffer at every age"
  - "Every tier 2 screen drawn with a bitmap font draws byte for byte as it does today"
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related:
  - ADR-1300
  - ADR-1301
  - ADR-1200
---

# ADR-1302: Glyph ink reaches one cell past its own, side to side, and is cut at its row

## Summary

- **Decided:** An outline glyph's ink that lies outside its cell is drawn into the cells beside it, at most one cell to the left and one to the right, where the C reference cut every glyph at its cell's edges. Ink above or below the row is cut at the row, and each strike's baseline rises just far enough that the underscore and the lowest descenders of printable ASCII end inside the cell. A glyph sits where the font puts it, its advance centred in its 1 or 2 cells. Bitmap glyphs, color glyphs, emoji and vtium's own sprites never reach past their cells. The renderer draws each row in layers: every cell's background and underline, then the ink that reaches a cell from its left and right neighbours, then the cell's own glyph, then its strike, so a neighbour's background never covers ink. A cell whose ink changes damages the neighbours its old or new ink reaches; a cursor or selection that moves damages only the cells it marks. Inside a block cursor or a selection, all ink takes the mark's text color; outside them, ink keeps the colors of the cell it came from. Synthetic italic slants the regular glyph by 0.2 about the baseline.
- **Trades away:** A changed cell can redraw up to two more cells. Each outline glyph carries its ink box, so the glyph wire format (ADR-1200 D2) and the cache's glyph size change. A neighbour's ink drawn over a block cursor or a selection takes the mark's color, not its own. Ink that leans past one cell, or past the row, is still cut.
- **One-way?** No. The rules live in vtium-vitrify's placement and vtium-render's composition, and the wire format between them changes on both ends in one release (ADR-1200 D2). Nothing is stored on disk.
- **Probes:**
  - *Confident (italic leans):* italic text leans into the next cell, as foot and Ghostty draw it, instead of being cut off at its right edge, and a plain upright font such as DejaVu Sans Mono draws exactly as before, because its cell is already as wide as its widest letter. Right?
  - *Not confident (marks):* inside a selection or under a block cursor, a neighbour's leaning ink takes the selection's or the cursor's text color, so the mark reads as one clean block; the letter under the cursor, where it leans out of the block, keeps its ordinary color. Is that the look you want?
- **Inversion:** Between "every glyph is cut to its cell", which the C reference drew, and "a glyph draws wherever its font puts it", which Ghostty's quads allow. The answer bounds the ink: one cell side to side, none up or down, so the renderer's row-at-a-time composition (ADR-1300 D5) and its cell-by-cell damage both hold.

## Context

vtium sizes an outline font's cell as the C reference did: the cell's width is the larger of the advance and W's ink width (the operator, 2026-10-26, vtium/vtium#26), so upright letters of a monospace font fit in their cells. Ink still leaves its cell in four ways: an italic letter leans past its cell's right edge and its descender past the left; bold, real or synthetic, can thicken a letter past its edges; a glyph from a fallback font or a symbol font has its own widths; and the underscore and descenders sit at the bottom of the font's descent, which rounding can put below the cell's last row. vtium/vtium#24 builds outline text, italic and bold included, and vtium/vtium#64 font merging; both wait on this record.

DejaVu Sans Mono shows how much ink leaves its cell (`basis`, measured from outline bounds rounded outward, unhinted):

| Height | Cell | Book, Bold | Oblique: glyphs past an edge, most right, most left | Book slanted by 0.2 | Underscore's last ink |
|---|---|---|---|---|---|
| 16 | 9x16 | none | 18 of 94, 0 px, 1 px | 14 of 94, 1 px, 1 px | 0.14 px below the cell |
| 22 | 12x22 | none | 21 of 94, 1 px, 2 px | 34 of 94, 2 px, 1 px | 0.36 px below |
| 28 | 15x28 | none | 23 of 94, 1 px, 2 px | 44 of 94, 3 px, 2 px | inside, 0.43 px clear |
| 32 | 17x32 | none | 23 of 94, 2 px, 2 px | 47 of 94, 4 px, 2 px | 0.38 px below |

At every even height from 12 to 32 the underscore's ink passes the cell's bottom edge at 12, 16, 22, 26 and 32 pixels. At 12 pixels the part of the underscore left inside the cell is 6% of a pixel tall, so it all but vanishes, and `g`'s descender passes the edge too. The C reference's pango engine drew no underscore at all with DejaVu Sans Mono 24 in a 14x28 cell (aaronsb/kmscon#254). The most any of these letters reaches past its cell is 4 pixels of a 17-pixel cell.

### Where vtium stands

- **Glyphs are cut to their cells today.** vtium-vitrify places an outline glyph by its own bearing on the baseline, moves it left by half of what passes the cell's right edge, and cuts it at the cell's edges (`crates/vtium-vitrify/src/outline.rs`, `place_drawn` and `copy`). The baseline is the ascent below the cell's top, rounded up.
- **A glyph is exactly its 1 or 2 cells** in the glyph chunk format the helper sends (ADR-1200 D2, `crates/vtium-font/src/set/format.rs`), in the store, and in the drawn-glyph cache (`crates/vtium-render/src/stored.rs`). Nothing on the way carries ink outside the cell.
- **Italic draws upright and bold widens in the cell.** No frit draws a style but regular yet (`crates/vtium-vitrify/src/frit.rs`, "style= other than regular is not built yet"); a bitmap face, a PSF font and a set's strike draw bold by setting each pixel whose left neighbour is set, inside the cell, and draw italic upright (`crates/vtium-font/src/face.rs`, `crates/vtium-font/src/psf.rs`, `crates/vtium-font/src/set/read.rs`). ADR-1200 D21 intends the helper to draw italic and bold from the frit's own faces, and to embolden the regular glyph where a frit has no bold face.
- **The row scratch is ready for it.** ADR-1300 D5 composes every run of a row's drawn cells in the row scratch in main memory and copies it out, so that a neighbour's overhang can be drawn without reading the display's memory; it names vtium/vtium#26 as one of its reasons. Each cell is composed in its own box today (`crates/vtium-render/src/compose.rs`, `cell_line`).
- **Damage is cell by cell.** Each cell keeps the serial of the frame that last changed what it shows, and a buffer gets the cells newer than its age (`crates/vtium-render/src/damage.rs`). No cell's change reaches its neighbours.
- **Color glyphs are fitted inside their cells.** vtium-vitrify draws a color glyph larger than its cells again at the size that fits, centred across them and kept inside the cell's height (`crates/vtium-vitrify/src/outline.rs`).
- **ADR-1000 lists the row** `text.overhang`, supported, in the text layer, carried by vtium/vtium#26, with no test yet.

### The C reference's flow

The C reference cut every glyph to its box of 1 or 2 cells, in each of its font engines. Its FreeType engine drew a glyph by its bearing on the baseline into a cell-sized buffer, moved it left by half of what passed the right edge, and cut it at the top, bottom and both sides; a 1-bit glyph that passed the bottom was moved up so that it ended in the last row. A narrow glyph in a two-cell box was centred by its advance. Its pango engine drew a cluster into a cell-sized bitmap with the ink's left edge moved onto the cell's left edge. Its bitmap engines re-boxed each glyph to its cells, centred and cut. Its renderer drew each cell's background, underline, glyph and strike-through inside that cell's own box, and its damage was cell by cell. The C reference drew italic only through pango, and its default FreeType engine drew italic upright and could not embolden a face with no bold (aaronsb/kmscon#100). The C notes give the functions.

### How other terminals draw it

foot lets a glyph draw into one more cell to its right, on by default, for italic and for symbol fonts, and cuts what passes its left edge. It draws a row's cells from right to left, so a glyph's overhang lands over the neighbour already drawn, and it marks a changed cell's neighbours dirty until it meets a cell whose glyph stays in its box. Ghostty places each glyph's quad by its bearings with no clip, draws cell backgrounds from a buffer of their own, apart from the glyphs' quads, and lets a Nerd Font icon span two cells when the cell to its right is blank (`basis`). xterm and WezTerm also draw overhanging ink into the next cell (vtium/vtium#26).

## Decision

The figure shows one row of four cells, A to D. Cell B holds an italic glyph whose ink reaches into A and C, inside the dashed box that marks how far B's ink may go; the ink above the row is cut. C is selected, so B's ink inside C takes C's text color. Below the row, the order in which each of the row's pixels is drawn.

![Four cells A to D of one row. Cell B holds a slanted glyph whose ink reaches a little into A and further into C, inside a dashed box spanning A to C; the part of the stroke above the row's top edge is outlined but not drawn. C has a blue selected background, and the ink inside C is white, C's text color; the ink in A and B is B's own amber. D, outside the dashed box, holds no ink of B's. Below, the order each pixel is drawn in: the background of its cell and its underline, the ink of the glyph to its left and then to its right, the cell's own glyph, the cell's strike.](figures/ADR-1302-overhang.svg)

### D1: which ink reaches past its cell, and how far

- **The reach** of a glyph is its own 1 or 2 cells and one cell more on each side, along its row. Ink inside the reach is drawn; ink outside it is cut. A narrow glyph's ink spans at most 3 cells and a wide glyph's at most 4.
- **Outline glyphs reach.** Regular, bold and italic glyphs of an outline font, real faces and synthetic styles alike (D4), and a shaped cluster's glyph (ADR-1301 D1), draw all of their ink within the reach.
- **These never reach past their cells:** bitmap glyphs (PSF, BDF, unifont, Unscii and every 1-bit strike), whose bold widening stays inside the cell as today; color glyphs and emoji, which vtium-vitrify fits inside their cells (D5); vtium's own sprites, box drawing, blocks, shades, Braille, Legacy Computing and the decorations, which are drawn to their cells so that neighbours join exactly (ADR-1301 D7); and the bar cursor's glyph.
- **No ink reaches up or down.** A glyph's ink is cut at its row's top and bottom edges (D3).
- **The grid's edge cuts ink.** Ink that would reach left of the first column or right of the last is cut there, so the border around the grid never shows ink. A wide glyph cut at the margin keeps the rule vtium-render already has for it (its README's departures).

The bound of one cell holds every letter measured in Context, the most being 4 pixels of a 17-pixel cell, and it bounds what one changed cell can damage (D7). A symbol font's glyph that is wider still is fitted by vtium/vtium#64 before this rule applies.

### D2: where a glyph sits in its cells

- An outline glyph is placed by the font: its pen starts at its cell's left edge moved right by half of what its cells' width exceeds its advance, rounded down, and its ink lies where its bearings put it from the pen, on the strike's baseline. A glyph whose advance is wider than its cells moves left by half the difference, so its ink spreads to both sides.
- For a monospace font whose cell is its advance, or a pixel wider because W's ink is wider (the operator's rule), the pen stays on the cell's left edge, so an upright letter lands where the C reference put it.
- A narrow glyph in two cells, such as a text-style symbol widened by VS16, is centred across the two cells by the same rule, as the C reference centred it.
- Nothing moves a glyph to keep its ink inside its cell. The C reference moved a glyph that passed its right edge left by half of the overflow; vtium draws the overflow instead.

### D3: the baseline, and ink above and below the row

- **The baseline** of a strike is the font's ascent below the cell's top, rounded up, as today, unless the lowest ink of the strike's printable ASCII glyphs, as rasterised, would pass the cell's bottom edge. Then the baseline rises by the whole pixels that ink passes the edge, rounded up, for every glyph of the strike, so the underscore and the descenders of `g`, `j`, `p`, `q` and `y` end in the cell's bottom rows and every letter keeps one baseline.
- **The underline and strike positions** the strike reports follow the baseline (ADR-1200 D2).
- **Ink above the cell's top or below its bottom is cut,** for every glyph. A row is composed on its own (ADR-1300 D5), and ink reaching into the rows above and below would make each row's pixels depend on three rows of glyphs and damage.
- The C reference cut ink at both edges, and moved each 1-bit glyph that passed the bottom up on its own, so a single glyph could stand off the baseline. vtium moves the whole strike, and only for ASCII's lowest ink, so a font whose ascent is tight loses a pixel at the top of its tallest marks rather than the underscore.

### D4: italic and bold

- **A real italic or bold face** draws its glyphs as the font draws them, and their ink reaches by D1.
- **Synthetic italic,** for a frit with no italic face (ADR-1200 D21), slants the regular glyph's outline by 0.2 about the baseline before it is rasterised: each point moves right by 0.2 of its height above the baseline, so ink above the baseline leans right and descenders lean left. 0.2 is fontconfig's slant for a face with no italic, and DejaVu Sans Mono's own Oblique leans by 0.19. The slant changes neither the advance nor the cell.
- **Synthetic bold** of an outline glyph is ADR-1200 D21's and vtium/vtium#24's to build; the ink it adds reaches by D1, and it changes neither the advance nor the cell.
- **Bitmap glyphs** keep drawing italic upright and bold widened inside the cell, as the C reference drew them, so every bitmap screen draws as today.

### D5: wide glyphs, color glyphs and emoji

- **A wide glyph,** an ideograph or any glyph 2 cells wide, reaches by D1: its own two cells and one cell on each side.
- **Color glyphs and emoji** stay inside their cells: vtium-vitrify draws one larger than its cells again at the size that fits, centred across them, as today. A COLR glyph's outline layers are drawn at that size too. Color glyphs are pictures fitted to a box, and their edges are part of the picture, so cutting or spreading them would change the picture.
- **Symbol fonts merged by range,** Nerd Font and Powerline glyphs (vtium/vtium#64), are fitted to their cells by that issue's rule, and whatever ink the fit leaves outside the cell reaches by D1.

### D6: how a glyph carries ink past its cell

- An outline glyph carries its ink box: the columns its ink covers, cut to its reach, given as the left column's offset from its cell's left edge in pixels, which may be negative, and the box's width, over the cell's full height. A glyph whose ink lies inside its cells may keep its cells as its box.
- Bitmap and color strikes keep each glyph exactly its 1 or 2 cells, as the format holds them today.
- The glyph chunk format (ADR-1200 D2) gains the box for outline strikes, and the helper's check of each glyph (ADR-1200 D15) refuses a box that passes the reach. The encoding is ADR-1200's to amend.
- The drawn-glyph cache (ADR-1300 D2) stores each glyph at its box. Its pools' byte ceilings (ADR-1300 D4) stay as they are, so a pool of glyphs that all reach full cells on both sides would hold a third as many narrow glyphs; DejaVu Sans Mono's letters reach at most 4 pixels past a 17-pixel cell. On the GPU path a pool's slot holds the largest box its glyphs may have, which vtium/vtium#33 sizes, and it may give glyphs that reach past their cells pools of their own.
- The cache's key does not change (ADR-1300 D1): the box is part of the glyph.

### D7: the order the row is drawn in

Each pixel of a composed run (ADR-1300 D5) is drawn in this order, with the per-pixel arithmetic ADR-1300 carries forward:

1. The background of the cell the pixel lies in.
2. That cell's underline.
3. The ink of the glyph to its left whose reach covers the pixel, then of the glyph to its right whose reach covers it.
4. That cell's own glyph.
5. That cell's strike.

- Steps 2 to 5 form the pixel's text layer, premultiplied, which goes over the background in one step, as a cell of several layers goes today.
- A cell's own glyph is drawn last of the glyphs, so where two glyphs' ink meets, the cell's own letter lies on top.
- A neighbour's underline and strike stay in their own cells; only glyph ink reaches.
- A cell with no neighbour reaching it, and whose own ink stays inside it, draws exactly as today. Every bitmap-font cell is one, so a screen in a bitmap font changes no pixel.
- To compose a cell, the renderer reads the glyphs of the cells beside it from the frame and the cache; it writes only the cells it draws.

### D8: the colors overhanging ink takes

- **Ink takes the colors of the cell it came from,** its foreground, as dimmed, blinking or hidden with that cell, wherever it lands in an unmarked cell.
- **A mark takes over the ink inside it.** Inside a block cursor's cell, or a selected cell, every glyph's ink, a neighbour's included, draws in the text color the mark gives that cell: the cursor's text color, or the selection's (OSC 17 and 19, or the cell inverted). The cell under a block cursor, or a selected cell, draws its own glyph's ink that lands outside the mark in the colors that cell would have unmarked. A mark therefore reads as one block, and nothing of a mark's color leaks into the cells beside it.
- **A bar or underline cursor** is drawn as today: the bar is its cell's glyph and lies over a neighbour's ink (D7), and the underline is its cell's underline.
- **On a 1-bit display** ink that lands in another cell is drawn as the opposite of that cell's background, lit on unlit or unlit on lit, by the same rule that keeps a cell's own text visible (ADR-1301 D7), so it never vanishes into a background of its own state.
- **Pictures** over and under text (vtium/vtium#28) treat a neighbour's ink in a cell as part of that cell's text layer.

### D9: damage reaches a neighbour's ink

- **Each cell remembers its reach:** whether the glyph it last drew had ink in the cell to its left, and in the cell to its right.
- **A cell whose ink changes damages its neighbours** that its old reach or its new reach names. Its ink changes when its glyph, its style or its foreground changes, or when the text blink hides or dims it. Those neighbours are stamped as changed on the same frame (`crates/vtium-render/src/damage.rs`), so every buffer, at whatever age, gets them, and each of them is composed again with the changed cell's new ink, or without it. A cell whose ink stayed inside it, before and after, damages nothing beside it.
- **A mark damages only the cells it marks.** A block cursor or a selection that appears, moves or goes changes no pixel outside the cells it marks, since D8 keeps a mark's colors inside it and draws a marked glyph's ink outside the mark as it would be unmarked.
- **Damage never chains.** A neighbour drawn again because of its neighbour has not changed its own ink, so its own reach damages nothing further.
- **The same holds for every source of change:** program output, the text blink, a glyph's arrival from the helper replacing the built-in drawing (ADR-1200 D23), and scrolling, which redraws whole rows.
- **The damage rectangles** handed to the display (`FB_DAMAGE_CLIPS`) cover the neighbours drawn this way.
- A keystroke in an italic run draws the cell it wrote, the cell the cursor entered, which is the written cell's right neighbour, and the written cell's left neighbour when the glyph leans into it: three cells, where two are drawn today.

### D10: the GPU path

The GPU path (vtium/vtium#33) draws the CPU path's pixels (ADR-1300 D6): a cell's fragment shader samples its own glyph and the glyphs of its neighbours whose reach covers it, in the order of D7, with the colors of D8. Tier 2 compares the two paths byte for byte, as for every screen.

### Records this amends once accepted

This record is proposed and edits no accepted record. Once the operator accepts it, these passages take dated notes pointing here:

- ADR-1200 D2, the glyph chunk format, gains the ink box of D6 for outline strikes; ADR-1200 D15's check refuses a box past the reach; ADR-1200 D21 points to D4 for synthetic italic.
- ADR-1301 D1, step 3: "a shaped cluster is composed into one bitmap fitted to its 1 or 2 cells" becomes its 1 or 2 cells and the reach of D1.
- ADR-1300 D2: a stored outline glyph is its ink box (D6); D5 names this record where it names vtium/vtium#26.

ADR-1200 lies in the font domain, outside this record's author's files; the steering session routes that edit.

### For the operator's check

These are the agent's leans, written into the decision above. The operator confirms or overrules each one.

1. Outline ink is drawn into the cells beside it rather than cut at the cell, as foot, Ghostty, xterm and WezTerm draw it (D1).
2. Ink reaches at most one cell to each side; foot reaches one cell to the right only, Ghostty sets no bound (D1).
3. Ink never reaches the rows above or below, and the strike's baseline rises just far enough that printable ASCII's lowest ink, the underscore included, ends in the cell (D3).
4. Inside a block cursor or a selection, every glyph's ink takes the mark's text color, and the marked cell's own glyph keeps its unmarked colors where it leans outside the mark (D8). Foot draws each glyph's ink in its own cell's colors everywhere; Ghostty colors a glyph standing on the cursor in the cursor's color.
5. Synthetic italic slants by 0.2 about the baseline, fontconfig's slant (D4).
6. Bitmap glyphs never reach past their cells, draw italic upright and widen bold inside the cell, so every bitmap screen draws as today (D1, D4).
7. Color glyphs and emoji stay fitted inside their cells and never reach (D5).
8. A glyph's advance is centred in its cells, in place of the C reference's move left by half of what passed the right edge (D2).
9. Ink is cut at the grid's edges, and the border never shows it (D1).
10. No configuration key turns overhang off. foot has one, `overflowing-glyphs`, on by default; vtium adds one only if a font shows the need, since a key's name is expensive to change after the first release.
11. The `observable` lines.

## Departures from the C reference

Each is listed in vtium-render's README, or vtium-vitrify's for placement, when vtium/vtium#24 builds it, and each tier 2 or tier 3 golden the C reference drew with an outline font lists the cells where it differs in `tools/tier2/departures.tsv` or `tools/tier3/departures.tsv`. No specification covers glyph overhang.

- **Glyph ink reaches into the cells beside it.** The C reference cut every glyph at its cell's edges. vtium draws an outline glyph's ink up to one cell to each side (D1). The reason: italic and symbol glyphs are drawn whole, as other terminals draw them.
- **A glyph sits where its font puts it.** The C reference's FreeType engine moved a glyph that passed its cell's right edge left by half of the overflow, and its pango engine moved every glyph's ink onto the cell's left edge. vtium centres a glyph's advance in its cells and keeps its bearings (D2). The reason: moving a glyph to fit its cell bends an italic run's spacing and a letter's place.
- **The underscore stays inside its cell.** The C reference's FreeType engine cut ink below the cell, so at some sizes the underscore was faint or gone (aaronsb/kmscon#254 records it under pango), and moved a 1-bit glyph that passed the bottom up on its own. vtium raises the strike's baseline so printable ASCII's lowest ink ends in the cell (D3).
- **Italic is drawn slanted.** The C reference's default FreeType engine drew italic upright and could not slant a face with no italic (aaronsb/kmscon#100). vtium draws a frit's italic face, or slants the regular glyph by 0.2 (D4).

## Beyond the C reference

- A changed cell redraws the neighbours its ink reaches, in every buffer at its age (D9); the C reference had no ink outside a cell to redraw.
- A block cursor and a selection recolor the ink inside them, a neighbour's included (D8).

## Verification

Each test that drives this behaviour through vtium-render or vtium-vitrify carries `// covers: text.overhang`. vtium/vtium#24 builds the screens with the outline feature.

- **Tier 2 screens,** vtium's own goldens, provisional until the operator reviews them. Tier 2 reads no font of the system's, so each draws an outline test font built in memory, as vtium-vitrify's tests build one (`crates/vtium-vitrify/tests/common/ttf.rs`), through vtium-vitrify's engine, with glyphs whose ink reaches a known number of pixels past their cells:
  - `overhang-italic`: an italic run, from an italic face and from the regular face slanted, over cells whose backgrounds alternate between two colors, with an underline and a strike across part of it, and a block cursor and a selection on and beside letters that lean.
  - `overhang-wm`: a run of `W` and `m`, regular and bold, at a cell set by W's ink, beside a glyph that reaches left.
  - `overhang-underscore`: the underscore and `gjpqy` at every cell height from 12 to 32 pixels, each with all of its ink in the cell.
  - `overhang-wide`: a wide glyph whose ink reaches both neighbours, beside narrow text, and at the grid's last column.
  - Each also in 1-bit mode, and on the GPU path under llvmpipe once vtium/vtium#33 lands; each at a cache of 0 glyphs and the performance budgets, as tier 2 draws every screen (ADR-1300).
- **Damage:** a frame draws a glyph whose ink reaches its right neighbour, and the next frame writes a space over it; the neighbour's pixels then equal a whole draw of the second frame, in buffers of age 1, 2 and 0, and the damage rectangles include the neighbour. The mirror case changes the neighbour's background under a glyph that reaches into it. A script of typing and erasing in an italic run checks every frame against a whole draw at every buffer age, as `every_frame_matches_a_whole_draw_under_each_blink_option` does. Planted defects: drop the neighbour's damage, and draw the neighbour's background over the ink; each fails.
- **Placement:** for the test font, a glyph's ink box equals its outline bounds placed at the pen of D2, cut to the reach; the baseline rises at exactly the heights where printable ASCII's lowest ink passes the bottom; a slanted glyph's top row moves right by 0.2 of its height above the baseline. A box past the reach, planted in a helper's answer, is refused.
- **Nothing else moves:** every tier 2 screen drawn with a bitmap font, and the C reference's goldens that judge them, draw byte for byte as before; `tests/reference.rs`'s cell digests hold.

## C notes

- `font_get_width` sized the cell as the larger of W's advance and its bitmap's width (`kmscon/src/font/font_freetype.c:130-138`).
- `copy_map` placed a coverage map by `bitmap_left` and the ascender, moved it left by half of what passed the box's right edge, and cut it at all four edges (`kmscon/src/font/font_freetype.c:340-380`). `copy_mono` moved a 1-bit glyph up when it passed the bottom (`kmscon/src/font/font_freetype.c:309-338`). `centre_dx` centred a narrow advance in a two-cell box (`kmscon/src/font/font_freetype.c:425-441`). The engine loaded glyphs with `FT_LOAD_TARGET_LIGHT` and never slanted or emboldened them.
- The pango engine rendered a layout line into a cell-sized `FT_Bitmap` at `-rec.x`, the ink rectangle's left edge, so ink outside the box was dropped (`kmscon/src/font/font_pango.c:207-228`).
- `fit_glyph` re-boxed a bitmap engine's glyph to its cells, centred, cutting what overflowed (`kmscon/src/font/font.c:268-308`).
- bbulk drew a cell's background, underline, glyph and strike-through as one request in the cell's box (`kmscon/src/render/bbulk.c:52-55`), and `damage_cell` marked one cell (`kmscon/src/render/bbulk.c:236-241`).

## Consequences

### Positive

- Italic text draws whole, and a font with italic or symbol glyphs wider than its cells draws as the font intends, as in the terminals people compare vtium with.
- The underscore shows at every size.
- A plain upright monospace font and every bitmap font draw exactly as before.
- A cell whose ink changes damages at most two more, never a chain, and a moving cursor or selection damages only what it marks.

### Negative

- A changed cell whose ink reaches redraws up to two more cells, and composing a cell can mean blending two more glyphs' lines into it. The cost is not measured; it falls only on cells beside ink that reaches.
- The glyph chunk format, the helper's check and the cache's stored glyph gain an ink box, a change in three crates.
- vtium-render's README departure "A keystroke damages only the cells it changes" gains the cell beside it that a leaning glyph reaches.
- A neighbour's ink inside a selection or a block cursor loses its own color.
- Ink past one cell, or past the row, is still cut.

### Neutral

- vtium/vtium#24 builds D2 to D6 in vtium-vitrify and the outline feature, and the tier 2 screens; vtium-render takes D7 to D9 in the same issue or a slice of its own.
- vtium/vtium#64 fits merged symbol glyphs before D1 applies, and vtium/vtium#33 builds D10.
- The C reference's tier 2 and tier 3 goldens drawn with outline fonts, re-baselined in vtium/vtium#24, list the cells where ink now reaches as this record's departure.

## Alternatives Considered

- **Cut every glyph at its cell, as the C reference did.** Rejected: italic letters lose their tops and descenders at every size measured (18 to 47 of 94 ASCII letters in DejaVu Sans Mono's Oblique and its slanted Book), and symbol glyphs lose their edges.
- **Shrink or move a glyph until it fits its cell.** Rejected: a slanted letter shrunk to its cell no longer matches its neighbours' size, and moving it, as the C reference moved glyphs that passed the right edge, bends the run's spacing.
- **Widen every cell until italic fits.** Rejected: the operator kept the cell at the larger of the advance and W's ink, and a wider cell spaces every upright letter apart for the sake of italic.
- **No bound on the reach, as Ghostty's quads allow.** Not taken: a changed cell could then damage any number of cells, and the row scratch would need the whole row's glyphs to compose one cell. One cell holds every letter measured.
- **Reach one cell to the right only, as foot does.** Not taken: italic descenders, `j` and `y`, and slanted glyphs lean left of their cells by 1 to 2 pixels at every size measured.
- **Let ink reach the rows above and below.** Not taken: a row would depend on the glyphs of three rows, its damage would cross rows, and the row scratch (ADR-1300 D5) composes one row; raising the baseline keeps the underscore, which is the ink that most needs it.
- **Draw a row's cells from right to left, as foot does,** so each glyph lands over its right neighbour. Not taken: a cell's background then covers the ink its right neighbour leans into it, which italic descenders do, and a cell's ink lands over its right neighbour's own letter.
- **Keep each glyph's ink in its own cell's colors under a mark, as foot does.** Not taken: a selection or block cursor would show stray ink in another color, and at 1 bit the ink could match the mark's background and vanish.
- **A configuration key to cut ink at the cell,** as foot's `overflowing-glyphs`. Not taken for now (For the operator's check, item 10).
