---
contract: adr/v1
kind: decision
verb: add
capability: renderer
basis:
  - operator: aaronsb
    level: directed
    said: "Operator's decision, 2026-10-26: the cell width stays `kmscon/kmscon`'s, the larger of the advance and W's ink width, and this issue is re-scoped as the design of glyph overhang: how ink outside its cell is drawn. The C reference is frozen under ADR-304, so that design is [vtium's glyph-overhang work]."
    via: "the comment of 2026-10-26T22:32:47Z on the C fork's issue about the FreeType cell's width, posted for the operator; the glyph-overhang issue's body carries the same decision"
  - operator: aaronsb
    level: directed
    said: "right we stretch not squeeze now. let's make sure that choice is consistent"
    via: "the maintainer in the steering session, 2026-11-02, relayed by the team lead, on the scale of a letter with its marks in place of the squeeze"
  - upstream: "aaronsb/kmscon b1f7a7c5: the cell's width from W (src/font/font_freetype.c:130-138), the larger of the regular and bold faces' cells (src/font/font_freetype.c:262-278); a gray glyph cut to its cell box, moved left by half of what passed the box's right edge (src/font/font_freetype.c:340-380); a 1-bit glyph moved up to end in the box's last row, down when it started above the box, and right when it passed the box's left edge (src/font/font_freetype.c:309-338); a narrow advance centred in a two-cell box (src/font/font_freetype.c:425-441); a mark that did not fit pushed back inside the cell (src/font/font_freetype.c:505-520); the pango engine drawing a cluster into a cell-sized bitmap from the pen at the cell's left edge and cutting it at the bitmap's edges (src/font/font_pango.c:207-228); bitmap glyphs re-boxed to their cells, centred and cut (src/font/font.c:268-308); bbulk drawing each cell's background, underline, glyph and strike-through in its own box (src/render/bbulk.c:52-55) and damaging cells one by one (src/render/bbulk.c:236-241)"
  - evidence: "DejaVu Sans Mono 2.37 (Arch's ttf-dejavu 2.37+18), its Book, Oblique, Bold and Bold Oblique faces, measured by this record's agent on 2026-11-01 with fontTools 4.67: each printable ASCII glyph's outline bounds at the cell the W rule gives at heights 16, 22, 28 and 32 pixels (a cell request half a pixel under the height, the baseline the ascent below the top rounded up), rounded outward to whole pixels, with the pen at the cell's left edge; and the underscore's rows at every even height from 12 to 32. The geometry is unhinted, where vtium hints outlines, so a hinted edge can differ by a pixel. The script sits in the agent's scratchpad, uncommitted."
  - evidence: "kbd's default8x16 and Terminus ter-v16n and ter-v32n console fonts, sheared by 0.2 about the baseline row by row, each row moved by 0.2 times the height of its middle above the baseline rounded to the nearest pixel, measured by this record's agent on 2026-11-01 with the shear script of the steering session's Ghostty study: 46, 38 and 56 of 94 printable ASCII glyphs pass the cell's right edge, by at most 2, 1 and 3 pixels, and none passes the left edge: only each font's last row or rows move left, by 1 pixel, and no glyph has ink in the leftmost column of those rows"
  - evidence: "foot 152487e (MIT): foot.ini(5) lines 1868-1871, `overflowing-glyphs`, on by default, lets a glyph wider than its cells draw into one more cell; render.c:999-1014 clips each glyph to its cell and one more to the right and cuts its left edge; render.c:1211 draws a row's cells from right to left; render.c:3460-3525 marks dirty, before a frame, the cells to the left and right of a dirty cell until a cell whose glyph stays in its box. Read by the steering session's Ghostty study on 2026-11-01."
  - evidence: "Ghostty 2f967e3a6ab2 (MIT), read by the steering session's Ghostty study on 2026-11-01: src/renderer/shaders/shaders.metal:556-619 places each glyph's quad at its cell moved by the glyph's bearings with no clip, so ink crosses rows as well as cells; shaders.metal:660-672 draws a whole glyph whose own cell is the cursor's in the cursor's color; src/renderer/generic.zig:1716 onward draws the whole frame each time; src/renderer/cell.zig:230-290 lets a symbol, a private-use character or one of eight symbol blocks, take up to two cells when the cell to its right is blank, src/font/face/freetype.zig:538-541 sets a synthetic italic slant of 12 degrees, a slope of 0.213, and line 564 applies it to each outline point (checked with git at 2f967e3a6ab231188c29a97fb422e479a254e97c); Ghostty draws a bitmap font's italic upright."
  - evidence: "fontconfig's 90-synthetic.conf, lines 21-23 as installed on the steering session's machine, slants a face with no italic by the matrix (1, 0.2; 0, 1); DejaVu Sans Mono Oblique's italic angle is -11 degrees, a slope of 0.19 (its post table)"
  - evidence: "The steering session's right-to-left study (rtl-study), 2026-11-01, fontTools and HarfBuzz over Liberation Mono 2, Miriam Mono CLM 0.133, Cascadia Mono 2407, Kawkab Mono 0.501, DejaVu Sans Mono 2.37, and Noto Sans Hebrew, Noto Sans Arabic and Noto Sans Arabic UI as fallbacks, at cells by the W rule for heights 16 to 32 pixels; its measurements are in its scratch files vertical2.txt, marks.txt, horiz.txt and joins.txt, uncommitted. Hebrew fits the row in every monospace font measured but for 1 pixel of a few cantillation marks. Arabic in DejaVu Sans Mono loses vowel marks at every height, the damma of alef with hamza and damma 11 pixels above a 28-pixel row and the kasratan under jeem 7 below it, and its precomposed alef with hamza loses 2 to 3 pixels at the top. Arabic wide letters reach 2 to 5 pixels past a cell's left edge in DejaVu Sans Mono, and Hebrew cantillation at a word's edge up to 9 pixels into a 17-pixel neighbour in Liberation Mono. Joined Arabic letters leave seams of 0.13 to 0.72 pixels in DejaVu Sans Mono and Kawkab Mono and overlap by 0.38 to 1.03 pixels in Cascadia Mono. Lam-alef draws in one cell's advance in every monospace font measured."
  - evidence: "WezTerm (MIT): wezterm-font/src/rasterizer/mod.rs:8 sets `FAKE_ITALIC_SKEW` to 0.2, the slant its synthetic italic uses, read by the reviewer of this record on 2026-11-01"
  - standard: "Unicode Character Database, ArabicShaping.txt: the Joining_Type property, D (dual-joining), R (right-joining), L (left-joining), C (join-causing), U and T; and BidiMirroring.txt's Bidi_Mirroring_Glyph"
  - precedent: ADR-1300
  - precedent: ADR-1301
  - precedent: ADR-1200
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Outline ink is drawn into the neighbouring cells: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [italic leans]
  - operator: aaronsb
    said: "Reach: one cell each side, decided after the Ghostty study. Ghostty-style per-codepoint symbol fitting goes into font merging; Ghostty's two-cell-next-to-blank rule is font merging's own decision."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "No vertical reach; the baseline rises so ASCII's lowest ink fits: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Inside a cursor or selection, all ink takes the mark's text color; outside, ink keeps its own: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [marks]
  - operator: aaronsb
    said: "Synthetic italic slants by 0.2 about the baseline: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Bitmap italic: shear at 0.2 about the baseline, for a bitmap font with no italic face, using the same ink box and neighbour damage as outline fonts; neither the C reference nor Ghostty shears bitmaps, so it is a departure. One rule for every font, one cell each side; the record notes that sheared bitmap italic happens to use only the right side, with no separate right-only rule."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Color glyphs and emoji stay fitted in their cells: yes. The advance is centered in the glyph's cells: yes. No key to turn overhang off, for now: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Ink is cut at the grid edge. He wants the better image, but not the complexity of managing the border."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Right-to-left text gets the same decisions as left-to-right text, as its mirror image: every rule applies to right-to-left characters as it does to left-to-right ones, mirrored wherever a rule names a side or a direction. Synthetic italic, bitmap shear and outline slant alike, is 0.2, leaning the mirror way for right-to-left characters, decided per character by its own direction. bidirectional text and shaping build on this and do not reopen it. A research pass checks the mirror rule for peculiarities before the record is final."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "An outline frit that states no cell= takes its fonts' own cell by the 2026-10-26 W rule, as wide as the wider of the advance and W's ink (the C reference also took the bold face into the max), asked of the helper before the frit's first frame, the same wait a bitmap frit with no cell= already has. So every frit without cell= takes its fonts' own cell."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Blink: a cell that is not blinking must never appear to blink because it hosts a blinking neighbour's lean. A blinking letter's ink stays inside its own cell; its lean is cut while it blinks, and neighbours never change with the blink phase."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Bar cursor: while the bar cursor is on a letter, the letter's lean into its neighbours is not drawn; the lean goes with its letter: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Baseline: decided as the record leans, each font's baseline from its whole printable ASCII set, whatever a request held."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Hidden text: ink invisible in its own cell is invisible wherever it leans, on every display: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Selection edge: rows where a selected letter leans out are read a second time for its ordinary color; pay the cost: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Huge glyphs: a glyph that won't fit the 16 KiB chunk with its lean is cut to its own cells, not dropped: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Synthetic italic leans right by 0.2 for every script, Hebrew and Arabic included, as fontconfig, WezTerm, Ghostty, foot, browsers and Word do; a frit's real italic face draws as its designer leaned it. This supersedes the mirrored lean. The mirror rule still holds for everything else that names a side or a direction. The pivot stays at the baseline, for the bitmap shear too."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Marks: a mark glyph whose ink passes the row's top or bottom moves toward the baseline until it fits; a stack that still cannot fit is squeezed vertically about the edge nearest its base letter; base letters and joining strokes never move. It applies to combining marks of every script; Hebrew marks already fit and are unchanged. Shrinking all Arabic to fit, about 60%, was not chosen. This answers the record's question on marks moving back inside the row."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Join seams: letters of joining scripts (Joining_Type D, R, L or C in ArabicShaping.txt) are stretched or squeezed sideways to exactly their cells' width before rasterising, so joining strokes meet with no seam: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Adopted without a question: a cell whose glyph changes because a neighbour changed its joining form counts as an ink change, and the three-cell keystroke figure holds for left-to-right text; the bar cursor's side is bidirectional text's to choose; no glyph or picture is ever drawn flipped, and a mirrored character draws its counterpart's glyph with its own ink box."
    via: the maintainer's answers in the steering session, 2026-11-01, third round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Join stretch: the letter's slot (its advance) is scaled to fill its cells, not its ink, so a designed overlap such as Cascadia Mono's stays and draws into the neighbour under the one-cell reach: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, fourth round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Tall letters: a base letter whose own ink passes the row is cut at the row, as decided (letters never change shape); a frit naming an Arabic font that fits the row, such as Kawkab Mono, avoids it: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, fourth round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Cursor blink: while the cursor sits on a cell, that cell's letter keeps its ink inside its own cell, whatever the cursor's blink phase or shape, so no neighbour changes with the cursor's blink; the lean returns when the cursor moves away: yes. This extends the text-blink ruling."
    via: the maintainer's answers in the steering session, 2026-11-01, fifth round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Hebrew marks: one rule for every script; Liberation Mono's telisha gedola and shalshelet and Miriam Mono's tipeha move 1 px inside the row; everyday Hebrew is untouched: yes. This corrects the earlier premise that Hebrew marks were unchanged."
    via: the maintainer's answers in the steering session, 2026-11-01, sixth round, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "1302 is written well, let's accept it"
    via: the maintainer in the steering session, 2026-11-01, after reading the record at 4a631f9 on GitHub with its figure, the ADR-1200 amendment beside it, and its four observables listed
  - operator: aaronsb
    said: "Shrink the whole stack"
    via: the maintainer's answers in the steering session's interview, 2026-11-01
  - operator: aaronsb
    said: "Stretch as decided"
    via: the maintainer's answers in the steering session's interview, 2026-11-01
  - operator: aaronsb
    said: "Additional line spacing, the user's choice: vtium adds none by default; the extra height is split evenly above and below the text; letters keep their size on their baseline, a bitmap glyph its pixel grid; line and block characters stretch to fill the taller cell, not letters; the stack shrink still applies inside the taller row."
    via: the maintainer's answers in the steering session's interview, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "not stretch emojis, but allow arabic extra room, leave roman chars normal, keep numbering normal … and let text character drawings stretch"
    via: the maintainer's answers in the steering session's interview, 2026-11-01
observable:
  - "Tier 2's `overhang-italic` screen shows an italic run whose letters lean into the next cell, unbroken across cell edges, over cells of alternating background colors"
  - "Tier 2's `overhang-underscore` screen shows a whole underscore in the bottom rows of every cell height from 12 to 32 pixels"
  - "Typing over an italic letter, and erasing it, leaves no ink behind in the cell beside it, in every buffer at every age"
  - "Every tier 2 and tier 3 screen drawn with a bitmap font draws byte for byte as it does today, but the screens that hold italic text, which lean in a commit of their own that lists their cells"
status: accepted
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

- **Decided:** A glyph from a font draws ink that lies outside its cell into the cells beside it, at most one cell to the left and one to the right, where the C reference cut every glyph at its cell's edges. One rule holds for every font: outline glyphs, and bitmap glyphs slanted for italic. Ink above or below the row is cut at the row, and each font's baseline in a cell rises just far enough that its underscore and lowest descenders end inside the cell. A glyph sits where its font puts it, its advance centred in its 1 or 2 cells. Italic with no italic face is the regular glyph slanted right by 0.2 about the baseline, for outline and bitmap fonts and every script alike. A combining mark that would pass the row's top or bottom moves toward the baseline until it fits, and where the marks cannot move in alone, the letter and its marks scale together vertically to fit the row. Letters of joining scripts, such as Arabic, are fitted sideways to their cells' width so their joins meet. Color glyphs, emoji and vtium's own drawn symbols never reach past their cells. The renderer draws each pixel of a row in layers: its cell's background and underline, then the ink that reaches it from the cells beside it, then the cell's own glyph, then its strike-through, so a neighbour's background never covers ink. A cell whose ink changes damages the neighbours its old or new ink reaches. A blinking letter's ink stays inside its own cell, so no neighbour ever blinks. The letter under the cursor keeps its ink inside its own cell too. Inside a block cursor or a selection, all ink takes the mark's text color; outside them, ink keeps the colors of the cell it came from as that cell draws unmarked. Right-to-left characters take every rule that names a side or a direction mirrored, but synthetic italic, which leans right for every script; no glyph is ever drawn flipped. An outline frit that states no cell takes its fonts' own cell by the W rule (ADR-1200 D17).
- **Trades away:** A changed cell can redraw up to two more cells. Each glyph that leans carries its ink box, so the glyph wire format (ADR-1200 D2) and the cache's stored glyph change. A neighbour's ink drawn over a block cursor or a selection takes the mark's color, not its own. Ink past one cell, or past the row, is still cut. Bitmap italic, upright in the C reference, leans, so the italic cells of vtium's bitmap goldens move.
- **One-way?** No. The rules live in vtium-vitrify's placement and vtium-render's composition, and the wire format between them changes on both ends in one release (ADR-1200 D2). Nothing is stored on disk.
- **Probes:**
  - *Confident (italic leans):* italic text leans into the next cell, as foot and Ghostty draw it, instead of being cut off at its right edge, and a plain upright font such as DejaVu Sans Mono draws exactly as before, because its cell is already as wide as its widest letter. Right? (Answered 2026-11-01; see `considered`. The second half holds where an outline frit with no cell takes the W rule's cell, which ADR-1200 D17 now gives it, and where the baseline rises the whole line sits a pixel higher; D3.)
  - *Not confident (marks):* inside a selection or under a block cursor, a neighbour's leaning ink takes the selection's or the cursor's text color, so the mark reads as one clean block; the letter under the cursor, where it leans out of the block, keeps its ordinary color. Is that the look you want? (Answered 2026-11-01; see `considered`. A later answer keeps the letter under the cursor inside its own cell; D8.)
- **Inversion:** The decision sits between "every glyph is cut to its cell", which the C reference drew, and "a glyph draws wherever its font puts it", which Ghostty draws because it draws the whole frame each time. vtium bounds the ink, one cell side to side and none up or down, so that the renderer can keep composing one row at a time (ADR-1300 D5) and keep damage cell by cell.

## Context

The cell's width is the larger of the advance and W's ink width (the maintainer, 2026-10-26), so upright letters of a monospace font fit in their cells. vtium applies that rule to every outline frit that states no cell (the maintainer, 2026-11-01; ADR-1200 D17). Ink still leaves its cell in four ways: an italic letter leans past its cell's right edge, and its descender past the left; bold, real or synthetic, can thicken a letter past its edges; a glyph from a fallback font or a symbol font has its own widths; and the underscore and descenders sit at the bottom of the font's descent, which rounding can put below the cell's last row. The outline-font build draws outline text, italic and bold included, and font merging fits symbol fonts to the cell; both wait on this record.

DejaVu Sans Mono shows how much ink leaves its cell (`basis`; unhinted outline bounds rounded outward, so a hinted edge can differ by a pixel):

| Height | Cell | Book, Bold | Oblique: glyphs past an edge, most right, most left | Book slanted by 0.2 | Underscore's last ink |
|---|---|---|---|---|---|
| 16 | 9x16 | none | 18 of 94, 0 px, 1 px | 14 of 94, 1 px, 1 px | 0.14 px below the cell |
| 22 | 12x22 | none | 21 of 94, 1 px, 2 px | 34 of 94, 2 px, 1 px | 0.36 px below |
| 28 | 15x28 | none | 23 of 94, 1 px, 2 px | 44 of 94, 3 px, 2 px | inside, 0.43 px clear |
| 32 | 17x32 | none | 23 of 94, 2 px, 2 px | 47 of 94, 4 px, 2 px | 0.38 px below |

At every even height from 12 to 32, the underscore's outline passes the cell's bottom edge at 12, 16, 22, 26 and 32 pixels. At 12 pixels the part of it left inside the cell is 6% of a pixel tall, and `g`'s descender passes the edge too. The C reference's pango engine drew no underscore at all with DejaVu Sans Mono 24 in a 14x28 cell. The most any of these letters reaches past its cell is 4 pixels of a 17-pixel cell. Bitmap console fonts slanted by 0.2 reach at most 3 pixels past the right edge of a 16-pixel-wide cell, and none past the left (`basis`).

### Where vtium stands

- **Glyphs are cut to their cells today.** vtium-vitrify places an outline glyph by its own bearing on the baseline, moves it left by half of what passes the cell's right edge, and cuts it at the cell's edges (`crates/vtium-vitrify/src/outline.rs`, `place_drawn` and `copy`). The baseline is the font's ascent below the cell's top, rounded up, but every strike the helper serves reports the cell's height as its baseline (`crates/vtium-vitrify/src/serve.rs`, `glyphs`).
- **An outline frit with no cell draws in the built-in font's cell.** The console gives such a frit a cell as wide as the built-in font's at that height (`crates/vtium/src/font.rs`, `frit_cell_at`), so DejaVu Sans Mono draws 8 pixels wide at 16, and vtium-vitrify's W rule runs only for a request that leaves the width to the font, which only tests send. ADR-1200 D17 now gives such a frit its fonts' own cell.
- **A glyph is exactly its 1 or 2 cells** in the glyph chunk format the helper sends (ADR-1200 D2, `crates/vtium-font/src/set/format.rs`), in the store, and in the drawn-glyph cache (`crates/vtium-render/src/stored.rs`). Nothing on the way carries ink outside the cell.
- **Italic draws upright and bold widens in the cell.** No frit draws a style but regular yet (`crates/vtium-vitrify/src/frit.rs`); a bitmap face, a PSF font and a set's strike draw bold by setting each pixel whose left neighbour is set, inside the cell, and draw italic upright (`crates/vtium-font/src/face.rs`, `crates/vtium-font/src/psf.rs`, `crates/vtium-font/src/set/read.rs`).
- **Marks are not pushed back into the cell.** The helper composes a shaped cluster's glyphs where the shaper puts them and cuts the result at the cell (`crates/vtium-vitrify/src/shaping/shape.rs`).
- **The row scratch is ready for overhang.** ADR-1300 D5 composes every run of a row's drawn cells in the row scratch in main memory, and names glyph overhang as one of its reasons. Each cell is composed in its own box today (`crates/vtium-render/src/compose.rs`, `cell_line`).
- **Damage is cell by cell.** Each cell keeps the serial of the frame that last changed what it shows, and a buffer gets the cells newer than its age (`crates/vtium-render/src/damage.rs`).
- **The renderer knows a selected cell's unselected colors only at 1 bit,** where it reads the frame again without its selection (`crates/vtium-render/src/renderer.rs`, `Pass::plain`).
- **Color glyphs are fitted inside their cells.** vtium-vitrify draws a color glyph larger than its cells again at the size that fits, centred across them and kept inside the cell's height (`crates/vtium-vitrify/src/outline.rs`).
- **ADR-1000 lists the row** `text.overhang`, supported, in the text layer, with no test yet.

### The C reference's flow

The C reference cut every glyph to its box of 1 or 2 cells, in each of its font engines. Its FreeType engine sized the cell from W in the regular and the bold face and took the larger. It drew a gray glyph by its bearing on the baseline into a cell-sized buffer, moved it left by half of what passed the right edge, and cut it at the top, bottom and both sides. It moved a 1-bit glyph up when it passed the bottom, down when it started above the top, and right when it passed the left edge. It centred a narrow glyph in a two-cell box by its advance, and pushed a mark that did not fit back inside the cell. Its pango engine drew a cluster into a cell-sized bitmap from a pen at the cell's left edge, keeping the glyph's bearings, and cut it at the bitmap's edges. Its bitmap engines re-boxed each glyph to its cells, centred and cut, and drew italic upright. Its renderer drew each cell's background, underline, glyph and strike-through inside that cell's own box, and its damage was cell by cell. The C reference drew italic only through pango, and its default FreeType engine drew italic upright and could not embolden a face with no bold. The C notes give the functions.

### How other terminals draw it

foot lets a glyph draw into one more cell to its right, on by default, for italic and for symbol fonts, and cuts what passes its left edge. It draws a row's cells from right to left, so a glyph's overhang lands over the neighbour already drawn, and before a frame it marks a changed cell's neighbours dirty until it meets a cell whose glyph stays in its box. Ghostty places each glyph by its bearings with no clip at all, so its ink crosses rows as well as cells; it needs no bound because it draws the whole frame each time. It lets a symbol take up to two cells when the cell to its right is blank, draws a whole glyph standing on the cursor's cell in the cursor's color, slants synthetic italic by 0.213, and draws bitmap italic upright (`basis`).

## Decision

The figure shows one row of five cells, A to E, in a 9x16 cell drawn large. B holds an italic letter, slanted by 0.2, whose top leans into C and whose tail leans a little into A, inside the dashed box that marks B's reach. C is selected, with its colors set to dark text on light blue, and holds an italic letter of its own that leans out of the selection into D. E holds an underscore. The baseline is drawn where D3 puts it for a font whose lowest ASCII ink would pass the cell's bottom, a pixel above the font's own, so the underscore lies inside the cell. The figure draws that case to show the rule. DejaVu Sans Mono at 16 pixels is such a case under the default `light` hinting (ADR-1200 D1): its outline alone passes the edge by 0.14 pixels, as the table in Context gives it, its light-hinted underscore no longer fits the cell either, and vtium raises its baseline from row 13 to row 12. Under `full` hinting its underscore fits the cell, and the baseline stays at row 13 (D3). Below the row, the order in which each pixel is drawn.

> Edited 2026-11-02: the hinting work, on the branch that adds hinting to the helper, measured DejaVu Sans Mono at 16 pixels under each value of `hinting=`: under `light`, the default, its underscore no longer fits and the baseline rises, and its glyphs come close to the C reference's goldens. The sentence read: "The figure draws that case to show the rule; DejaVu Sans Mono's outline alone passes the edge by 0.14 pixels at 16 pixels, as the table in Context gives it, but its hinted underscore fits the cell, so vtium does not raise its baseline (D3)." The earlier text is in git at d45b7d0. Corrected the same day after the fonts review and the records re-check: the comparison with the goldens moved to D3's baseline bullet, with the figures `crates/vtium-vitrify/README.md` gives for its cell set; it read "113 of the 231 inked cells are identical under `light` and none under `full`, and the largest difference at any pixel is 17 levels of 255 under `light` against 255 under `full`", where 113 counted cells within 2 levels, and then gave 77 of 234 identical; the earlier text is in git at 0e3cce1 and 04de5a0.

> Edited 2026-11-01: the review of the helper's baseline: the figure's raised baseline illustrates the rule, not DejaVu Sans Mono at 16 pixels, whose hinted glyphs do not rise. The sentence read: "so the underscore lies inside the cell, where at the font's own baseline its lowest part passed the bottom edge by 0.14 pixels, as the table in Context gives it at 16 pixels." The figure's caption and its description say which font's case it draws. The earlier text is in git at fa62f7f.

![Five cells A to E of one row, drawn large, dark background. A dashed line marks the font's own baseline and a solid line one pixel higher marks the baseline vtium uses for a font whose lowest ASCII ink would pass the cell's bottom. B holds an amber italic letter, slanted by 0.2, whose top leans into C and whose tail leans a little into A, all inside a dashed box spanning A to C. C is selected, light blue with dark text: B's ink inside C is dark, and C's own italic letter is dark inside C and light gray where it leans out into D. E holds an underscore just above the cell's bottom edge, with an outline one pixel lower showing where the font's own baseline would have put it, its lowest part past the cell's bottom edge. Below, the order each pixel is drawn in: its cell's background, its cell's underline, the ink of the glyphs beside it that reaches it, its cell's own glyph, its cell's strike-through.](figures/ADR-1302-overhang.svg)

Two words are used carefully below. A **strike** is a font's set of glyphs at one cell size and style, as the glyph chunk format holds them (ADR-1200 D2). A **strike-through** is the line drawn through text by SGR 9.

### D1: which ink reaches past its cell, and how far

- **The reach** of a glyph is its own 1 or 2 cells and one cell more on each side, along its row. Ink inside the reach is drawn; ink outside it is cut. A narrow glyph's ink spans at most 3 cells and a wide glyph's at most 4.
- **One rule holds for every glyph a font draws:** outline glyphs, regular, bold and italic, real faces and synthetic styles alike; a shaped cluster's glyph (ADR-1301 D1); and a bitmap glyph slanted for italic (D4) (the maintainer, 2026-11-01). Slanted bitmap italic happens to reach only to the right in the console fonts measured: only their last row or rows move left, by 1 pixel, and no glyph has ink in the leftmost column of those rows. The rule itself names both sides.
- **These never reach past their cells:** color glyphs and emoji, which vtium-vitrify fits inside their cells (D5); vtium's own drawn symbols, box drawing, blocks, shades, Braille, Legacy Computing and the underline and strike-through sprites, which vtium draws to fill their cells (`crates/vtium-font/src/sprite.rs`, `crates/vtium-render/src/deco.rs`); the bar cursor's glyph; and an upright bitmap glyph, whose ink lies in its cells by construction, bold widening included.
- **No ink reaches up or down.** A glyph's ink is cut at its row's top and bottom edges (D3).
- **The grid's edge cuts ink.** Ink that would reach left of the first column or right of the last is cut there, so the border around the grid never shows ink (the maintainer, 2026-11-01). A wide glyph cut at the margin keeps the rule vtium-render already has for it (its README's departures).
- **Right-to-left characters** take this rule as it stands, since the reach is the same on both sides (D11).

The bound of one cell holds every letter measured in Context, the most being 4 pixels of a 17-pixel cell, and it bounds what one changed cell can damage (D9). Ghostty lets a symbol take two cells beside a blank one and fits symbols per codepoint; font merging decides both for merged symbol fonts, and fits a symbol before this rule applies (the maintainer, 2026-11-01).

### D2: where a glyph sits in its cells

- An outline glyph is placed by the font. Its advance is rounded to the nearest whole pixel, as the C reference rounded it when it centred a glyph; its pen starts at its cell's left edge, moved right by half of what its cells' width exceeds that advance, rounded down, which is a move left when the advance is wider than the cells; and its ink lies where its bearings put it from the pen, on the baseline of D3. Rounding down keeps a one-pixel difference, as between DejaVu Sans Mono's 9-pixel cell and its 8-pixel advance at 16 pixels, at no move.
- For a monospace font whose cell is its advance, or a pixel wider because W's ink is wider (the W rule), the pen stays on the cell's left edge, so an upright letter lands where the C reference put it.
- A narrow glyph in two cells, such as a text-style symbol widened by VS16, is centred across the two cells by the same rule, as the C reference centred it.
- Nothing moves a glyph sideways to keep its ink inside its cell. The C reference moved a gray glyph that passed its right edge left by half of the overflow, and a 1-bit glyph that passed its left edge right; vtium draws the overflow instead, within the reach.
- A bitmap glyph keeps its place in its cell, as its font draws it.
- **Letters of joining scripts in an outline font are fitted sideways to their cells** (the maintainer, 2026-11-01), in the helper, on HarfRust's output (D12). A glyph whose character has the Joining_Type D, R, L or C in the Unicode Character Database's `ArabicShaping.txt` (Arabic and its tatweel, Syriac, N'Ko, Mongolian, Mandaic, Adlam and the other joining scripts) is scaled sideways, before it is rasterised, by its cells' width over its advance as the font gives it, unrounded, and its pen sits on its cell's left edge, so D2's rounding does not apply to it. The whole shaped cluster is scaled together, the base letter and the marks HarfRust placed on it, offsets included, so each mark stays over its base. Its advance therefore fills its cells exactly and its joining strokes meet the next letter's with no seam. The advance is what is scaled, not the ink, so the overlap a font designed past the advance stays and D1's reach draws it into the neighbour (the maintainer, 2026-11-01). Unscaled, a medial beh's joins leave a gap of 0.13 to 0.72 pixels in DejaVu Sans Mono and Kawkab Mono, and overlap by 0.38 to 1.03 pixels on purpose in Cascadia Mono (rtl-study, `basis`). Every joining letter is stretched now, though vtium draws each letter's standalone form until shaping across cells exists (the maintainer, 2026-11-01). Until then the stretch changes letter shapes without any joins meeting: a fallback font's alef, narrower than its cell, widens, and seen, wider than its cell, is squeezed.

> Edited 2026-11-01: the maintainer's answer in the steering session's interview (`considered`, 2026-11-01): the stretch stays as decided, and D2 states what it changes before shaping across cells exists. The bullet ended at the measured gaps. The earlier text is in git at 37f8563.
- **A ligature that draws two cells' letters in one cell's advance,** as lam-alef does in every monospace font measured, is not placed by this decision: centring its advance across its two cells is the wrong default for it, and shaping places it.
- **Devanagari and other scripts whose letters join through a headline** (a stroke along the top) need a rule of their own for the headline to meet, which a later record decides; this decision does not stretch them.

### D3: the baseline, and ink above and below the row

- **The baseline** of each font in a cell is the font's ascent below the cell's top, rounded up, unless the lowest ink of that font's printable ASCII glyphs, as rasterised at that cell, would pass the cell's bottom edge. Then the baseline rises by the whole pixels that ink passes the edge, rounded up, for every glyph that font draws at that cell, so the underscore and the descenders of `g`, `j`, `p`, `q` and `y` end in the cell's bottom rows and every letter of the font keeps one baseline.
- **The baseline does not depend on what was asked.** The helper rasterises the font's whole printable ASCII set to find its lowest ink each time it draws that font at a cell, whatever characters the request holds, so glyphs drawn for different requests, or again after an eviction, sit on one baseline. A font with no printable ASCII keeps the ascent rounded up.
- **Each font's baseline is in its glyphs' pixels.** The helper rasterises each font's glyphs against that font's own baseline into their cell box, so glyphs from several fonts in one strike each stand on their own font's baseline. The strike reports the baseline of the frit's first source, the font that sets the cell, and the underline and strike-through positions it reports follow that baseline (ADR-1200 D2). The helper always takes the first source for these, and where it cannot read that source it reports the plain lines: the baseline at the cell's height, as every strike the helper serves today reports.

> Edited 2026-11-01: the review of the helper's ink box and baseline: the first source always gives the strike's baseline and lines, and an unreadable one gives the plain lines. The bullet ended: "every strike the helper serves today reports the cell's height." The earlier text is in git at fa62f7f.
- **Where the baseline rises, the whole line sits a pixel higher** than the C reference drew it. The helper measures the lowest ink on the hinted glyphs it draws, not on the outline's own geometry. Measured that way with the installed fonts, from 12 to 32 pixels, DejaVu Sans Mono's baseline rises at 12, 14, 16, 17, 25, 26, 27, 31 and 32 pixels under the default `light` hinting (ADR-1200 D1), and only at 27 under `full`. JetBrains Mono and Liberation Mono rise at no height from 12 to 32 under either, since their hinting puts the underscore's last ink on the cell's bottom row. DejaVu Sans Mono's outline alone would pass the edge at 12, 16, 22, 26 and 32 pixels, which is what the table in Context measures. `light_hinting_fits_rows_only_and_full_hinting_both_ways` (`crates/vtium-vitrify/tests/outline.rs`) records DejaVu Sans Mono at 16 pixels: the baseline rises from row 13 to row 12 under `light`, and stays at row 13 under `full`. Light hinting also draws DejaVu Sans Mono close to the C reference's tier 2 goldens `ascii` and `bold` (kmscon b1f7a7c5), regular and bold at 16 pixels. The comparison draws the helper's glyphs on the goldens' own baseline, row 13, so the rise does not shift it, and takes the 231 cells with ink in those screens' DejaVu text: all of `ascii`'s, and the bold words of `bold`'s first three lines, as `crates/vtium-vitrify/README.md` states them. Under `light`, 79 of the 231 cells are identical, 121 are within 2 levels of 255 at every pixel, the 79 among them, and no pixel differs by more than 15 levels. Under `full`, no cell is within 2 levels, and pixels differ by up to 229 levels.

> Edited 2026-11-02: the hinting work measured the rise under each value of `hinting=`, on the branch that adds hinting to the helper: under `light`, the default, DejaVu Sans Mono rises at nine heights from 12 to 32 pixels, and under `full` at one. The bullet read: "Measured that way, DejaVu Sans Mono, JetBrains Mono and Liberation Mono rise at no height from 12 to 32 pixels, since hinting puts the underscore's last ink on the cell's bottom row", and "No test yet records such a height for a real font". The earlier text is in git at 16c8ccc.

> Edited 2026-11-01: the review of the helper's ink box and baseline measured the rise on vtium's own hinted glyphs, where none of the three fonts rises from 12 to 32 pixels. The bullet read: "With DejaVu Sans Mono's outline geometry that happens at 12, 16, 22, 26 and 32 pixels; the hinted glyphs vtium draws decide the heights, which the outline-font build's tests record. The C reference's DejaVu goldens at 16 pixels list it when they are re-baselined." The earlier text is in git at fa62f7f.
- **Ink above the cell's top or below its bottom is cut,** for every glyph. A row is composed on its own (ADR-1300 D5), and ink reaching into the rows above and below would make each row's pixels depend on three rows of glyphs and damage.
- **A mark the shaper left unplaced is placed before the mark rule runs.** HarfRust leaves a mark with no advance and no offset where the font has no anchor for it on its base, and such a mark would draw at the pen, past its base, in the next cell. The helper first places each such mark whose combining class puts it above or below its base: centred across the ink of the base letter before it, a mark above with one empty pixel row between it and all the ink drawn since that letter, and a mark below directly under that ink. The classes it places are 214, 216, 228, 230, 232 and 234 above, and 202, 218, 220, 222 and 233 below. Only then does the mark rule below run, moving stacks of marks into the row or scaling a letter with its marks, on the positions this placement gave. A mark placed above a tall letter can therefore pass the row's top and be moved in, or make the letter and its marks scale together. Neither step runs on a cluster that holds a color glyph.
- **A right-to-left mark of combining class 0 to 36 that the shaper left at the pen keeps its place at the pen from side to side.** The Hebrew points have classes 10 to 26, the Arabic harakat 27 to 35 and the Syriac superscript alaph 36, and none of these is among the above and below classes the helper places. Where a font with GPOS positioning has no anchor for such a mark on its base, the mark keeps its place at the pen, past its base. In right-to-left text HarfRust returns a cluster's marks before their letter, so the helper finds no letter before an unplaced mark and places none, whatever its class. The mark rule still moves such a mark up or down into the row, and its ink past the one-cell reach is cut, as any ink is (D1). HarfRust's fallback placement, ported from HarfBuzz's, runs only for a font with no GPOS positioning. It maps the Hebrew, Arabic and Syriac classes 10 to 36, all but dagesh's 21, to places above or below the base, and the helper's own placement does not take that mapping. The C reference did not re-place such a mark either.

> Edited 2026-11-01: the steering session's brief for the second wave of records, after the review round of the helper's mark rule, checked against the helper's code (`crates/vtium-vitrify/src/shaping/shape.rs`, `compose`, `place_unplaced` and `fit_marks`, on the branch that builds the overhang in the helper) and the C reference (`src/font/font_freetype.c` at kmscon b1f7a7c5): D3 gains the two bullets above, before the mark rule's, on the placement of a mark the shaper left unplaced and on the right-to-left marks it does not place, and the C notes gain the bullet on that placement. D3 did not say where an unplaced mark goes; the earlier text is in git at 725c369.

- **A combining mark is moved rather than cut at the row** (the maintainer, 2026-11-01). In an outline font, where HarfRust has placed it (D12), a mark glyph whose ink passes the row's top or bottom moves toward the baseline until it fits. The marks above the middle of the base letter's ink move together as one stack, and the marks below it as another. Where the move would push the stack past the row's other edge, or lay more of its ink on the letter's, the marks do not move alone: the letter and its marks scale together, as the next bullet says. vtium never squeezes a stack of marks on its own (the maintainer, 2026-11-02). Base letters and joining strokes never move except when they scale with their marks in that way. A glyph is a mark by its character, not by its place in the cluster: a character whose General_Category is Mn or Me, or whose canonical combining class is not 0, in the Unicode Character Database, since HarfRust returns a right-to-left cluster in visual order, its mark first. The rule holds for the combining marks of every script. Sideways, a mark reaches as any ink does, and is cut past the reach as any ink is. For example, in DejaVu Sans Mono at 28 pixels the damma over an alef with hamza (أُ) lies 11 pixels above the row and is cut away whole today; it moves down until it fits, and where moving it down would lay it on the hamza, the letter and its marks scale together instead. In DejaVu Sans Mono every vowel mark measured on Arabic letters loses part or all of its ink at every height from 16 to 32 pixels, the kasratan under jeem 7 pixels below the row at 28; the Hebrew monospace fonts measured fit their rows but for three cantillation marks, Liberation Mono's telisha gedola and shalshelet and Miriam Mono's tipeha, which the rule moves 1 pixel inside the row; everyday Hebrew, its letters and vowel points, is untouched (the maintainer, 2026-11-01; rtl-study, `basis`). The C reference pushed a mark that did not fit back inside the cell and never scaled a letter with its marks.
- **A letter whose marks cannot move into the row on their own scales with its marks as one stack** (the maintainer, 2026-11-01). The rule applies when a stack of marks passes the row's top or bottom, and moving the marks in alone would lay more of their ink on the letter's ink than lies there already, or push them past the row's other edge. It applies whether or not the letter's own ink passes the row (the maintainer, 2026-11-02). The helper then scales the whole cluster, the letter and all its marks, vertically, just enough to fit the row. The scale is anchored at the ink's edge opposite the one that passes the row: when the ink passes the row's top, its lowest ink stays where it is and its top comes down to the row's top, and when it passes the row's bottom, its highest ink stays and its bottom comes up to the row's bottom. When the ink passes both edges, it scales to fill the row exactly. The scale keeps the font's own arrangement of the marks against the letter, and the helper applies it to HarfRust's output before rasterising, as it does the rest of D3. Outside a terminal the font's anchors place such a mark above the letter's ink and the line grows to fit, which a fixed row cannot do; scaling the stack keeps that arrangement in the row's height. In DejaVu Sans Mono, alef with hamza and a damma (أُ) at 16 pixels scales down so the damma stays above the hamza, and jeem's tail with kasratan (جٍ) scales down so the kasratan stays below the tail. Lam with shadda and fatha (لَّ) shows the case where the letter's own ink fits: the lam fits its row, but the shadda and the fatha above it pass the top and cannot move down without landing on the lam's stem, so the cluster scales, and measured with DejaVu Sans Mono the stem loses 2 to 4 rows of its height at 16 to 28 pixels.
- **A base letter's own ink past the row, with no mark that must go past it too, is cut,** as every glyph's is, so such letters never change shape (the maintainer, 2026-11-01); a frit that names an Arabic font whose letters fit the row, such as Kawkab Mono, avoids the cut. The hamza that is part of the precomposed letter alef with hamza (أ) in DejaVu Sans Mono loses 2 to 3 pixels at the top at every height measured, unvocalized text included.
> Edited 2026-11-01: the maintainer's answers in the steering session's interview (`considered`, 2026-11-01), after a fresh review of the helper's mark rule: a letter whose own ink passes the row shrinks with the marks that must go past it as one stack, and a mark is recognised by its character. The mark bullet said only that "Base letters and joining strokes never move", and the cut bullet read "A base letter's own ink past the row is cut, as every glyph's is, so letters never change shape". The earlier text is in git at 37f8563.

> Edited 2026-11-01: the steering session's brief, from the review of the helper's mark rule, checked against `fit_marks` in `crates/vtium-vitrify/src/shaping/shape.rs` on the branch that builds the overhang in the helper. The maintainer answered "Shrink the whole stack" in the steering session's interview of 2026-11-01, and confirmed on 2026-11-02 that the scale replaces the squeeze everywhere (`basis`): vtium scales a letter with its marks whenever the marks cannot move into the row alone, whether or not the letter's own ink fits the row, and never squeezes a stack of marks on its own. The helper does both. The mark bullet read "A mark, or a stack of marks, that still cannot fit after that move is squeezed vertically, about the row's edge nearest its base letter, until it fits. Base letters and joining strokes never move, but for one case, the next bullet's." and ended its example "the pair is squeezed toward the letter"; the stack bullet was headed "A letter whose own ink passes the row, with marks that must also go above or below it, shrinks with its marks as one stack" and applied "when the stack's ink, the letter and its marks together, passes the row's top or bottom, and moving the marks in alone would make them collide with the letter's own ink". The Summary, D12, D13, the departures, "Beyond the C reference", the verification's `overhang-marks`, the negative consequences and the rejected alternative of shrinking all Arabic text named the squeeze, and now name the scale. The earlier text is in git at 39419dd.

> Edited 2026-11-02: the maintainer confirmed in the steering session "right we stretch not squeeze now. let's make sure that choice is consistent" (`basis`, 2026-11-02). The scale of a letter with its marks replaces the squeeze in every record, and the rule's wider reach, to letters whose own ink fits the row, is the maintainer's. The answer of 2026-11-01 that describes the squeeze, "a stack that still cannot fit is squeezed vertically about the edge nearest its base letter" (`considered`, third round), stays in the record as history and is superseded by this answer. The mark and stack bullets credited the wider rule to "the steering session's reading of the maintainer's answer", and the alef and jeem examples said "shrinks" where the record's one word for this behaviour is "scales"; the earlier text is in git at bbd88f8.

> Edited 2026-11-01: the steering session's brief, from the review of the helper's mark rule, checked against `fit_marks` and `remap` in `crates/vtium-vitrify/src/shaping/shape.rs` on the branch that builds the overhang in the helper: a scale about the edge the ink passes could never bring that ink inside the row, and the helper anchors the scale at the ink's opposite edge. The stack bullet read "vertically about the row's edge it passes, just enough to fit the row, keeping the font's own arrangement of the marks against the letter; it does so on HarfRust's output before rasterising". The earlier text is in git at fd73fca.

- The C reference cut gray ink at both edges and moved each 1-bit glyph that passed the bottom up on its own, so a single glyph could stand off the baseline. vtium moves the whole font's baseline, and only for ASCII's lowest ink, so a font whose ascent is tight loses a pixel at the top of its tallest marks rather than the underscore.

### D4: italic and bold

- **A real italic or bold face** draws its glyphs as the font draws them, and their ink reaches by D1.
- **Synthetic italic leans right, for every script** (the maintainer, 2026-11-01). Hebrew, Arabic and every other right-to-left script lean right as left-to-right scripts do, as fontconfig, Ghostty and WezTerm slant them (`basis`); a frit's real italic face draws as its designer leaned it. Digits, punctuation and symbols drawn from a font lean like letters, the steering session's call under the maintainer's rule that every script leans right; color glyphs and vtium's own drawn symbols do not lean. The pivot is the baseline, for the outline slant and the bitmap shear alike.
- **Synthetic italic of an outline font,** for a frit with no italic face (ADR-1200 D21), slants the regular glyph's outline by 0.2 about the baseline before it is rasterised: each point moves right by 0.2 of its height above the baseline, so ink above the baseline leans right and descenders lean left. 0.2 is fontconfig's slant for a face with no italic, and DejaVu Sans Mono's own Oblique leans by 0.19; Ghostty leans by 0.213. The slant changes neither the advance nor the cell.
- **Synthetic italic of a bitmap font,** for a bitmap font with no italic face, shears the glyph by the same 0.2 about its baseline, one pixel row at a time: each row moves right by 0.2 times the height of the row's middle above the baseline, rounded to the nearest pixel, a half rounding right (the maintainer, 2026-11-01). In an 8x16 console font the top five rows move 2 pixels, the next five 1 pixel, the five around the baseline none, and the last row 1 pixel left. The slanted glyph reaches by D1 and carries an ink box (D6). The C reference and Ghostty both drew bitmap italic upright; this is a departure.
- **Where the bitmap shear runs.** The shear runs wherever a bitmap glyph is drawn: in the helper for a frit's bitmap sources, and in vtium-font for the faces the console draws itself, the built-in unifont and Unscii and a PSF font (`crates/vtium-font/src/face.rs`, `psf.rs`, `unifont.rs`), whose glyph then carries its ink box as the helper's does. It runs at the font's own size, before the glyph is scaled by its whole factor (ADR-1300 D2), so each sheared pixel becomes a square of the scale.
- **Synthetic bold** of an outline glyph is ADR-1200 D21's and the outline-font build's to build; the ink it adds reaches by D1, and it changes neither the advance nor the cell.
- **Bold widening of a bitmap or set glyph** sets each pixel whose left neighbour is set, as today, and stays inside the glyph's box: its cells for an upright glyph, its ink box for one that carries a box. For bitmap bold italic, the widening comes first and the shear moves the widened rows, the steering session's call under the maintainer's rules.

### D5: wide glyphs, color glyphs, emoji and symbols

- **A wide glyph,** an ideograph or any glyph 2 cells wide, reaches by D1: its own two cells and one cell on each side.
- **Color glyphs and emoji** stay inside their cells (the maintainer, 2026-11-01): vtium-vitrify draws one larger than its cells again at the size that fits, centred across them, as today. Color glyphs are pictures fitted to a box, and cutting or spreading them would change the picture.
- **Symbol fonts merged by range,** Nerd Font and Powerline glyphs (font merging), are fitted to their cells by font merging's rule, and whatever ink the fit leaves outside the cell reaches by D1.

### D6: how a glyph carries ink past its cell

- A glyph whose ink passes its cells carries its ink box: the columns its cells and its ink cover, cut to its reach, so the box always holds the glyph's own cells, given as the left column's offset from its cell's left edge in pixels, which may be negative, and the box's width, over the cell's full height. A glyph whose ink lies inside its cells keeps its cells as its box. This holds for outline strikes and for bitmap strikes slanted for italic; upright bitmap strikes and color strikes keep every glyph exactly its 1 or 2 cells.
- The glyph chunk format (ADR-1200 D2) carries the box, and the console's check of each answer from the helper (ADR-1200 D15) refuses a box that passes the reach. The encoding is the outline-font build's to make.
- **A glyph whose box would pass the chunk bound,** 16 KiB (ADR-1200 D2), counted over the box, its cells and its ink together, is cut to its own cells, as every glyph is today, instead of being left out. Only very large cells reach the bound: a wide glyph's box of 4 cells passes it once a cell holds more than 4,096 pixels, as 48x96 does.
> Edited 2026-11-01: the review of the helper's ink box: the box is the union of the glyph's cells and its ink, as the build stores it, and the chunk bound applies to that union. The first bullet read "the columns its ink covers, cut to its reach". The earlier text is in git at fa62f7f.

- **The console's overstrike** of a mark on its base (ADR-1200 D22, step 3) gives the composed glyph the union of the base's and each mark's boxes.
- The drawn-glyph cache (ADR-1300 D2) stores each glyph at its box. Its pools' byte ceilings (ADR-1300 D4) stay as they are, so a pool of glyphs that all reached full cells on both sides would hold a third as many narrow glyphs; DejaVu Sans Mono's letters reach at most 4 pixels past a 17-pixel cell. On the GPU path a pool's slot holds the largest box its glyphs may have, which the GPU path sizes, and it may give glyphs that reach past their cells pools of their own.
- The cache's key does not change (ADR-1300 D1): the box is part of the glyph.

### D7: the order the row is drawn in

Each pixel of a composed run (ADR-1300 D5) is drawn in this order, with the per-pixel arithmetic ADR-1300 carries forward:

1. The background of the cell the pixel lies in.
2. That cell's underline.
3. The ink of the glyphs beside the cell whose reach covers the pixel: the glyph before the cell in the cell's direction, then the glyph after it. A cell's direction is right to left when its character's Bidi_Class is R or AL, and left to right for every other cell, an empty cell, a space, a digit and punctuation included, so the order never depends on anything but the cell's own character.
4. That cell's own glyph.
5. That cell's strike-through.

- Steps 2 to 5 form the pixel's text layer, premultiplied, which goes over the background in one step, as a cell of several layers goes today.
- A cell's own glyph is drawn last of the glyphs, so where two glyphs' ink meets, the cell's own letter lies on top.
- A neighbour's underline and strike-through stay in their own cells; only glyph ink reaches.
- A cell that no neighbour's ink reaches, and whose own ink stays inside it, draws exactly as today. A screen drawn only in bitmap fonts with no italic is made of such cells; a frit that mixes bitmap and outline fonts, or a bitmap italic, puts reaching ink beside bitmap cells.
- To compose a cell, the renderer reads the glyphs of the cells beside it from the frame and the cache; it writes only the cells it draws.

### D8: the colors overhanging ink takes

- **Ink takes the colors of the cell it came from,** its foreground as that cell draws it, dimmed or hidden with that cell, wherever it lands in an unmarked cell.
- **A blinking letter's ink stays inside its own cell** (the maintainer, 2026-11-01). While a cell's text blinks (SGR 5 or 6), its glyph's ink outside the cell is not drawn in either phase, so a cell that does not blink never appears to blink because a blinking neighbour leans into it. For example, an italic word set to blink draws its letters cut at their cells' edges, and the word beside it stays steady. With `blink-text=off`, which draws SGR 5 and 6 steady, the letter's lean draws like any steady letter's, the steering session's call under the maintainer's rule.
- **The letter under the cursor keeps its ink inside its own cell** (the maintainer, 2026-11-01), whatever the cursor's shape and blink phase, so no neighbour changes with the cursor's blink. Its lean returns when the cursor moves away. A cursor that does not show, hidden by DECTCEM or out of view while the screen is scrolled back, neither shows nor blinks, so it does not cut its cell's lean, the steering session's call under the maintainer's rule. The renderer folds the cursor's visibility and its blink into one value today (`crates/vtium-render/src/renderer.rs`, `draw_cells`), so the build separates them. For example, with the cursor on an italic `f`, the `f`'s top does not lean over the next letter until the cursor moves on.
- **A mark takes over the ink inside it** (the maintainer, 2026-11-01). Inside a block cursor's cell, or a selected cell, every glyph's ink, a neighbour's included, draws in the text color the mark gives that cell: the cursor's text color, or the selection's (OSC 17 and 19, or the cell inverted). A selected cell's own glyph draws its ink that lands outside the selection in the colors that cell would have unselected; the letter under the cursor has no ink outside its cell (above). A mark therefore reads as one block, and nothing of a mark's color leaks into the cells beside it. Ghostty draws a whole glyph standing on the cursor's cell in the cursor's color, inside the block and out; foot draws every ink in its own cell's colors.
- **The colors a selected cell would have unselected** come from the frame read without its selection, which the renderer reads today only at 1 bit. On every display, the renderer reads it for the rows where a selected cell's glyph leans into an unselected cell, and for no other row.
- **Ink that is invisible in its own cell is invisible everywhere** (the maintainer, 2026-11-01). A glyph whose foreground equals its own cell's background, as text written in the background color, draws no ink in the cells beside it, on every display.
- **A bar or underline cursor** is drawn as today: the bar is its cell's glyph and lies over a neighbour's ink (D7), and the underline is its cell's underline. Under a bar cursor the cell's own letter is not drawn, as today, and neither is its lean. Which side of a right-to-left letter's cell the bar stands on is bidirectional text's to choose.
- **On a 1-bit display** ink that lands in another cell is drawn as the opposite of that cell's background, lit on unlit or unlit on lit, so it never vanishes into a background of its own state; ink invisible in its own cell stays invisible.
- **Pictures** over and under text (the image pipeline) treat a neighbour's ink in a cell as part of that cell's text layer.

### D9: damage reaches a neighbour's ink

- **Each cell remembers its reach:** whether the glyph it last drew had ink in the cell before it and in the cell after it.
- **A cell whose ink changes damages its neighbours** that its old reach or its new reach names. A cell's ink changes when any of these change: its glyph or its style; its foreground; its own background, where dim text (SGR 2) mixes the background into the foreground, or where the background comes to equal or differ from the foreground, which hides or shows its ink in its neighbours (D8); whether it blinks, which takes its lean away (D8), though the blink's phase changes no neighbour; and whether the cursor stands on it, which keeps its ink inside its cell (D8), though the cursor's blink phase changes no neighbour. Those neighbours are stamped as changed on the same frame (`crates/vtium-render/src/damage.rs`), so every buffer, at whatever age, gets them, and each is composed again with the changed cell's new ink, or without it. A cell whose ink stayed inside it, before and after, damages nothing beside it.
- **A selection that appears, moves or goes** damages only the cells it marks, since D8 draws a selected letter's ink outside the selection as it would be unselected. The cursor arriving on a letter that leans, or leaving it, damages the cells the lean reaches, by the rule above.
- **Damage never chains.** A neighbour drawn again because of its neighbour has not changed its own ink, so its own reach damages nothing further.
- **The same holds for every source of change:** program output, the cursor, the selection, a glyph's arrival from the helper replacing the built-in drawing (ADR-1200 D23), and scrolling, which redraws whole rows.
- **The damage rectangles** handed to the display cover the neighbours drawn this way.
- A keystroke in an italic run draws the cell it wrote, the cell the cursor entered, which is the written cell's right neighbour, and the written cell's left neighbour when the glyph leans into it: three cells, where two are drawn today, when the cell the cursor enters is blank or holds an upright letter. When it holds a leaning letter, as in typing or overwriting in the middle of an italic line, that letter loses its lean under the cursor (D8), so the cell after it is drawn too: four cells.
- **A neighbour's new joining form is an ink change.** In a joining script a letter's form depends on its neighbours in the text's logical order, so writing one letter can change the glyph of the letter beside it; that cell's glyph changes, so it counts as an ink change by the rule above and damages its own neighbours as its reach names. The three-cell figure above holds for left-to-right text. In right-to-left text, bidirectional text's implicit mode may reorder and redraw a whole paragraph when one character changes.

### D10: the GPU path

The GPU path draws the CPU path's pixels (ADR-1300 D6): a cell's fragment shader samples its own glyph and the glyphs of its neighbours whose reach covers it, in the order of D7, with the colors of D8. Tier 2 compares the two paths byte for byte, as for every screen.

### D11: right-to-left text

Right-to-left text takes the same decisions as left-to-right text, as their mirror image (the maintainer, 2026-11-01). Every rule above applies to a right-to-left character as it does to a left-to-right one, mirrored wherever the rule names a side or a direction, and each character's direction is its own. The one exception is synthetic italic, which leans right for every script (D4). Bidirectional text, and shaping and joining, build on this and do not reopen it.

- **The reach of one cell on each side** covers right-to-left ink. Arabic wide letters, seen, sheen, sad and dad, reach 2 to 5 pixels past the left edge of a 9- to 17-pixel cell in DejaVu Sans Mono, and up to 11 pixels of a 17-pixel cell in Noto Sans Arabic UI as a fallback; Hebrew cantillation marks placed at a word's edge, zinor, pashta and telisha, reach up to 9 pixels into a 17-pixel neighbour in Liberation Mono. All lie inside one cell (rtl-study, `basis`).
- **The order of D7** runs in the cell's direction, as D7 defines it.
- **No glyph or picture is ever drawn flipped.** A character the bidirectional algorithm mirrors, such as a parenthesis in right-to-left text, draws its counterpart's glyph, the one its Bidi_Mirroring_Glyph names, with that glyph's own ink box.
- **Left to shaping:** lam-alef's place across its two cells (D2), where an Arabic underline sits against Arabic descenders, and the damage a joining form spreads beyond its neighbours.
- **Left to bidirectional text:** the side of a right-to-left letter's cell the bar cursor stands on (D8), and the redraw its implicit mode needs (D9).

### D12: where shaping and fitting run

vtium shapes text with HarfRust, the HarfBuzz project's Rust port, only where it parses fonts: in `vtium-vitrify`'s sandboxed child and in the run-time helper, never in the console (ADR-1200 D14).

- **Outline fonts.** HarfRust places each mark against its base through the font's GPOS anchors, and picks the joining forms and lam-alef from the font's own tables. The mark rule (D3: move a stack of marks in, or scale a letter and its marks together) and the joining-script stretch (D2) act on HarfRust's output, in the helper, before the glyphs are rasterised, so the console receives glyphs already fitted and never shapes or measures a font.
- **Bitmap fonts.** No shaper runs. An upright bitmap glyph is exactly its cell, and a sheared one carries an ink box that reaches sideways only (D4, D6), so a bitmap glyph's ink never leaves the row. The mark rule and the stretch act on outline glyphs only and not on bitmap glyphs, the steering session's reading of the maintainer's rules. How bitmap text joins is shaping's to decide.

### D13: added line spacing, the user's choice

*Not built yet.* A decision of its own, not a bullet of D3, because it changes the row's height, which every other decision here takes as given: D3 still decides where ink sits and where it is cut, now inside a taller row, and this decision says how each kind of glyph uses the height added.

- **The user may make each row taller; vtium adds nothing by default** (the maintainer, 2026-11-01). A frit's `line-spacing=`, and kmscon.conf's `font-line-spacing` or a profile's over it (ADR-1200 D1, D16), add a whole percentage of the text's height, the height the cell has without spacing, from `0`, the default, to `100`, rounded to whole pixels. With `0` the minspec look and the C reference's look stay as they are. The added height joins the cell, so the grid has fewer rows. A frit's stated `cell=` is exact and takes no spacing, the record's call: a `cell=` already names the height its writer wants. kitty (`modify_font cell_height`), WezTerm (`line_height`), foot (`line-height`), Ghostty (`adjust-cell-height`), Alacritty (its offsets) and VTE all let the user make rows taller.
- **The added height is split evenly above and below the text** (the maintainer, 2026-11-01), so marks above and descenders below both get room; an odd pixel goes above, the record's call, since marks above are what D3's stack scale most often meets. The middle of the row, the text's own height, is the text band.
- **How a glyph uses the taller row** is set per set of characters in the frit, by `fit=` (ADR-1200 D1), as a frit already gives sets of characters to fonts, so one frit can leave emoji unstretched, give Arabic extra room, keep Latin letters and digits as they are, and let line drawings stretch (the maintainer, 2026-11-01). A frit that says nothing gets these defaults:
  - *`fill`, for line and block characters:* box drawing, blocks and shades, Powerline, sextants, octants, Braille and Legacy Computing stretch to the whole taller cell, so they still join from row to row, as terminals that draw box drawing themselves fill the cell. vtium already draws these as sprites at the cell's size. Where a frit's sprite policy has the font draw them (`glyph-source=`), a bitmap glyph is stretched by repeating its top and bottom pixel rows outward and an outline glyph is scaled vertically to the cell (the maintainer, 2026-11-01).
  - *`natural`, for everything else:* the glyph keeps the font's size and sits on its baseline in the text band, with the added height blank above and below it, and its ink is cut at the text band as D3 cuts it at the row. A bitmap glyph keeps its pixel grid, scaled by whole factors only, with blank rows above and below. Color glyphs and emoji are always `natural`, whatever `fit=` says: fitted inside the text band and never stretched, as D5 keeps them pictures.
  - *`room`, for Arabic, Hebrew, Syriac, Thaana, N'Ko and the combining marks of every script:* the letter keeps its size and its baseline, as under `natural`, and its marks may use the added height above and below the text band before D3 moves or scales anything.
- **D3's rules hold inside the taller row.** A stack of marks moves in, or a letter scales with its marks (D3), only when it still does not fit the space its `fit=` gives it: the text band, or the whole cell under `room`.

One row with spacing added, from the top: the added band, the text band, the added band. A Latin letter sits in the text band; an Arabic letter's marks reach into the added bands; a box-drawing line runs the whole height; an emoji stays inside the text band.

```text
 +---------+---------+---------+---------+
 |         |  mark   |    |    |         |   <- added band: blank, or room for marks
 |    A    |   beh   |    |    | (emoji) |   <- text band: the font's size, on its baseline
 |         |  mark   |    |    |         |   <- added band
 +---------+---------+---------+---------+
   natural    room      fill    color glyph
```

> Edited 2026-11-01: the maintainer added line spacing, the user's choice, and per-set fitting of the row's height (`considered`, 2026-11-01). The record had no D13 and took the row's height as the cell's; the earlier text is in git at 38ee007.

### Records this amends

- **ADR-1200 D17, D10 and D2,** amended in their own commits with this revision (the maintainer, 2026-11-01): an outline frit that states no cell takes its fonts' own cell by the W rule, asked of the helper before its first frame and at each zoom, and D10's judgement of which frames wait follows; and the glyph chunk format carries an ink box for outline strikes and for bitmap strikes slanted for italic, cut to its cells past the chunk bound.

The maintainer accepted this record on 2026-11-01, and each of these passages now carries a dated note pointing here:

- ADR-1200 D15: the console's check refuses a box past the reach. ADR-1200 D21: a cluster's glyph is fitted to its 1 or 2 cells and the reach of D1, and synthetic italic is D4's slant. ADR-1200 D23: an arrival damages the cells that show the arrived character and the neighbours their ink reaches (D9).
- ADR-1301 D1, step 3: "a shaped cluster is composed into one bitmap fitted to its 1 or 2 cells" becomes its 1 or 2 cells and the reach of D1. ADR-1301's Verification gains this record's screens.
- ADR-1300 D2: a stored glyph is its ink box (D6); D5 names this record where it names glyph overhang.
- ADR-1000's `text.overhang` row: its foot and ghostty columns, now answered by this record's evidence.

> Edited 2026-11-01: each passage above took its dated note once the maintainer accepted this record. The sentence before the list read: "This record is proposed, and these passages take dated notes pointing here once the maintainer accepts it:". The earlier text is in git at 1d51e46.

### For the operator's check

The maintainer answered this section's questions on 2026-11-01 in four rounds (`considered`), and each answer is written into the decision above. One remains, written as the decision until the maintainer confirms or overrules it:

1. **The `observable` lines.**

## Departures from the C reference

Each is listed in vtium-render's README, or vtium-vitrify's for placement, when the outline-font build makes it, and each tier 2 or tier 3 golden the C reference drew that shows it lists the cells where it differs in `tools/tier2/departures.tsv` or `tools/tier3/departures.tsv`. No specification covers glyph overhang.

- **Glyph ink reaches into the cells beside it.** The C reference cut every glyph at its cell's edges. vtium draws a glyph's ink up to one cell to each side (D1). The reason: italic and symbol glyphs are drawn whole, as foot and Ghostty draw them.
- **A glyph sits where its font puts it.** The C reference's FreeType engine moved a gray glyph that passed its cell's right edge left by half of the overflow, and a 1-bit glyph that passed its left edge right; its pango engine kept the bearings and cut. vtium centres a glyph's advance in its cells, keeps its bearings, and draws the overflow (D2). The reason: moving a glyph to fit its cell bends an italic run's spacing and a letter's place.
- **The underscore stays inside its cell.** The C reference's pango engine drew no underscore with DejaVu Sans Mono 24 in a 14x28 cell, and its FreeType engine cut a gray glyph's ink at the cell's bottom edge as at every edge, and moved a 1-bit glyph that passed the bottom up on its own. vtium raises a font's baseline so its printable ASCII's lowest ink ends in the cell (D3); where it rises, the whole line sits a pixel higher than the C reference drew it.
- **Italic is drawn slanted.** The C reference's default FreeType engine drew italic upright and could not slant a face with no italic, and its bitmap engines drew italic upright. vtium draws a frit's italic face, or slants the regular glyph by 0.2, outline and bitmap alike (D4). vtium's own goldens that hold italic text in a bitmap font are redrawn in a commit of their own that lists their cells, the maintainer reviewing it: tier 2's `small-640x480`, the cells of its italic word `ital` and any cell its lean reaches; and tier 3's `sgr` screen, whose `italic` word draws on `2d-320x240-psf8`, `2d-640x480-psf16`, `sysvinit-640x480`, `sysvinit-no-udev`, `two-outputs-mirror` and its second output, and `two-cards-mirror` and its second card (`tools/tier3/guest/screens/sgr.sh`, `tools/tier3/t3_evaluate.py`, `tools/tier3/t3_scenario.py`). A tier 3 departure lists the italic cells of `sgr` and `sixel-article` in the C reference's goldens on `2d-potato-min` and `2d-potato-min-psf`.
- **A mark that does not fit moves toward the baseline, and a letter whose marks cannot move in alone scales with them.** The C reference pushed a mark that did not fit back inside the cell, and never scaled a letter with its marks. vtium moves each stack of marks toward the baseline, and scales the letter and its marks together where the marks cannot move in alone (D3), so Arabic vowel marks show whole at every size.
- **Arabic letters join, and fill their cells.** The C reference drew a single codepoint by its glyph index without shaping (`kmscon/src/font/font_freetype.c:443`), so Arabic letters drew in their isolated forms, unjoined, except where a program wrote Unicode's presentation forms itself. vtium shapes them in the helper (D12) and scales each joining letter's advance to its cells' width (D2), which closes the seam of up to 0.72 pixels that shaped DejaVu Sans Mono letters leave at the W rule's cell.
- **An outline frit with no cell takes its fonts' own cell** (ADR-1200 D17), as the C reference took its font's; the departure is from vtium's earlier rule, and vtium's provisional tier 3 golden `2d-640x480-frit/small-console` moves with the cell.

## Beyond the C reference

- A changed cell redraws the neighbours its ink reaches, in every buffer at its age (D9); the C reference had no ink outside a cell to redraw.
- A block cursor and a selection recolor the ink inside them, a neighbour's included (D8).
- Right-to-left text takes every rule that names a side or a direction mirrored, and never draws a glyph flipped (D11); Arabic marks move in, or scale with their letter, to fit the row (D3), and joining letters fill their cells (D2).

## Verification

Each test that drives this behaviour through vtium-render or vtium-vitrify carries `// covers: text.overhang`. The outline-font build makes the screens with the outline feature.

- **Tier 2 screens,** vtium's own goldens, provisional until the maintainer reviews them. Tier 2 reads no font of the system's, so each draws an outline test font built in memory, as vtium-vitrify's tests build one (`crates/vtium-vitrify/tests/common/ttf.rs`), through vtium-vitrify's engine, with glyphs whose ink reaches a known number of pixels past their cells, or `tools/testfont`'s PSF fonts:
  - `overhang-italic`: an italic run, from an italic face and from the regular face slanted, over cells whose backgrounds alternate between two colors, with an underline and a strike-through across part of it, and a block cursor and a selection on and beside letters that lean; a right-to-left run whose synthetic italic leans right, and a blinking italic word beside a steady one, whose neighbour's pixels stay the same in both blink phases.
  - `overhang-wm`: a run of `W` and `m`, regular and bold, at a cell set by W's ink, beside a glyph that reaches left.
  - `overhang-underscore`: the underscore and `gjpqy` at every cell height from 12 to 32 pixels, each with all of its ink in the cell.
  - `overhang-wide`: a wide glyph whose ink reaches both neighbours, beside narrow text, and at the grid's last column.
  - `overhang-bitmap`: a PSF font's italic, sheared, beside upright text and across a selection's edge.
  - `overhang-marks`: Arabic and Hebrew letters with vowel marks and cantillation at every cell height from 12 to 32 pixels, alef with hamza and damma and jeem with kasratan among them, each mark with ink inside the row, and a letter scaled with its marks where they cannot move in alone; a planted cut of marks at the row fails it.
  - `overhang-joins`: a run of joined Arabic letters, beh four times and a word with tatweel, in a font that leaves seams and one that overlaps on purpose, with no column between two joined letters left without ink on the joining stroke's rows; a planted placement without the stretch fails it.
  - Each also in 1-bit mode, and on the GPU path under llvmpipe once the GPU path lands; each at a cache of 0 glyphs and the performance budgets, as tier 2 draws every screen (ADR-1300).
- **Damage:** a frame draws a glyph whose ink reaches its right neighbour, and the next frame writes a space over it; the neighbour's pixels then equal a whole draw of the second frame, in buffers of age 1, 2 and 0, and the damage rectangles include the neighbour. The same for a glyph that reaches its left neighbour; for the neighbour's background changing under ink; for a dim leaning letter whose own background changes; for a blinking leaning letter, whose neighbours stay the same through both phases; for the cursor arriving on a leaning letter and leaving it, in each shape, and blinking there with no neighbour changing; for a cell's own background coming to equal its foreground under a leaning letter; and for a selection whose edge crosses a leaning letter. A script of typing and erasing in an italic run checks every frame against a whole draw at every buffer age, as `every_frame_matches_a_whole_draw_under_each_blink_option` does. Planted defects, each seen to fail: drop the left neighbour's damage; drop the right neighbour's; draw a neighbour's background over the ink; leave the cursor out of what changes a cell's ink; draw the lean of the letter under the cursor; leave a background that comes to equal the foreground out of what changes a cell's ink; draw a blinking letter's lean; leave a neighbour's new joining form out of what changes a cell's ink; leave the cell's own background out of what changes dim ink.
- **Placement:** for the test font, a glyph's ink box equals the union of its cells and its hinted ink placed at the pen of D2, cut to the reach; the baseline rises at exactly the heights where printable ASCII's lowest hinted ink passes the bottom; the same baseline comes back from requests holding only `_`, only `a`, and only a character outside ASCII, a planted baseline taken from the request's own characters failing it; a slanted glyph's top row moves right by 0.2 of its height above the baseline, for a left-to-right and a right-to-left character alike; a mirrored parenthesis draws its counterpart's glyph, unflipped; a bitmap glyph's rows move by D4's rounding. A box past the reach, planted in a helper's answer, is refused.

> Edited 2026-11-01: the review of the helper's ink box and baseline: the box is the union of the glyph's cells and its ink, and the rise is measured on hinted ink. The bullet read "a glyph's ink box equals its outline bounds placed at the pen of D2" and "where printable ASCII's lowest ink passes the bottom". The earlier text is in git at fa62f7f.
- **Nothing else moves:** every tier 2 and tier 3 screen drawn with a bitmap font, and the C reference's goldens that judge them, draw byte for byte as before, but the italic cells the departure lists; `tests/reference.rs`'s cell digests hold.

## C notes

- `font_get_width` sized the cell as the larger of W's advance and its bitmap's width (`kmscon/src/font/font_freetype.c:130-138`), and the font took the larger of the regular and bold faces' cells (`kmscon/src/font/font_freetype.c:262-278`).
- `copy_map` placed a coverage map by `bitmap_left` and the ascender, moved it left by half of what passed the box's right edge, and cut it at all four edges (`kmscon/src/font/font_freetype.c:340-380`). `copy_mono` moved a 1-bit glyph up when it passed the bottom, down when its top was above the box, and right when it passed the left edge (`kmscon/src/font/font_freetype.c:309-338`). `centre_dx` rounded the advance, `(adv + 32) >> 6`, and centred a narrow advance in a two-cell box (`kmscon/src/font/font_freetype.c:425-441`). `render_cluster` pushed a mark that did not fit back inside the cell (`kmscon/src/font/font_freetype.c:505-520`). The engine loaded glyphs with `FT_LOAD_TARGET_LIGHT` and never slanted or emboldened them.
- The pango engine called `pango_layout_line_get_extents(line, &logical_rec, &rec)`, whose second argument receives the ink rectangle and third the logical one, so `rec` held the logical rectangle, whose x is normally 0; it rendered the line into a cell-sized `FT_Bitmap` at `-rec.x`, so the pen sat at the cell's left edge and ink outside the bitmap was dropped (`kmscon/src/font/font_pango.c:207-228`).
- `render_cluster` placed a mark that HarfBuzz left with no advance and no offset only when `ccc_above` or `ccc_below` named its class, the same eleven classes the helper places, each of them 202 or higher; it centred the mark over the base's ink and set it one row above or directly below the ink drawn since the base, clamped to the cell, modelled on HarfBuzz's `position_around_base` (`kmscon/src/font/font_freetype.c:477-505, 578-598`). It took the base only from the glyphs before the mark, so it placed no mark in a right-to-left cluster. It took no part of HarfBuzz's `recategorize_combining_class`, so a Hebrew, Arabic or Syriac mark of class 10 to 36, or a mark of class 0, stayed at the pen.
- `fit_glyph` re-boxed a bitmap engine's glyph to its cells, centred, cutting what overflowed (`kmscon/src/font/font.c:268-308`).
- bbulk drew a cell's background, underline, glyph and strike-through as one request in the cell's box (`kmscon/src/render/bbulk.c:52-55`), and `damage_cell` marked one cell (`kmscon/src/render/bbulk.c:236-241`); its damage reached the display as `FB_DAMAGE_CLIPS`.

## Consequences

### Positive

- Italic text draws whole, in outline and bitmap fonts, and a font with italic or symbol glyphs wider than its cells draws as the font intends.
- The underscore shows at every size.
- An outline frit with no cell takes its fonts' own cell, so a plain upright monospace font draws in its own cell with no ink past its sides, and every bitmap font without italic draws exactly as before.
- A cell whose ink changes damages at most two more, never a chain.
- Right-to-left text needs no rules of its own beyond the mirror, and Arabic vowel marks and joins draw whole and seamless in fonts not made for a terminal's cells.

### Negative

- A changed cell whose ink reaches redraws up to two more cells, and composing a cell can mean blending two more glyphs' lines into it. The cost is not measured; it falls only on cells beside ink that reaches.
- Rows where a selection's edge crosses a leaning letter are read twice.
- The glyph chunk format, the console's check and the cache's stored glyph gain an ink box, a change in three crates.
- vtium-render's README departure "A keystroke damages only the cells it changes" gains the cells beside it that a leaning glyph reaches: three cells, or four in the middle of an italic line (D9).
- A neighbour's ink inside a selection or a block cursor loses its own color.
- Where a font's baseline rises, its whole line sits a pixel higher than the C reference drew it.
- Ink past one cell, or past the row, is still cut, a base letter's included, so the hamza on alef loses its top in DejaVu Sans Mono.
- A blinking italic letter is cut at its cell's edges while it blinks.
- Letters scaled with their marks and stretched joining letters are drawn at shapes their fonts did not draw.

### Neutral

- The outline-font build makes D2 to D6 in vtium-vitrify and the outline feature, and the tier 2 screens; vtium-render takes D7 to D9 in the same build or a slice of its own.
- Font merging fits merged symbol glyphs before D1 applies; the GPU path builds D10; bidirectional text and shaping build on D11.
- The C reference's tier 2 and tier 3 goldens drawn with outline fonts, re-baselined in the outline-font build, list the cells where ink now reaches, and the rows where the baseline rose, as this record's departures.

## Alternatives Considered

- **Cut every glyph at its cell, as the C reference did.** Rejected: italic letters lose their tops and descenders at every size measured (18 to 47 of 94 ASCII letters in DejaVu Sans Mono's Oblique and its slanted Book), and symbol glyphs lose their edges.
- **Shrink or move a glyph until it fits its cell.** Rejected: a slanted letter shrunk to its cell no longer matches its neighbours' size, and moving it, as the C reference moved glyphs that passed an edge, bends the run's spacing.
- **Widen every cell until italic fits.** Rejected: the maintainer kept the cell at the larger of the advance and W's ink, and a wider cell spaces every upright letter apart for the sake of italic.
- **No bound on the reach, as Ghostty draws.** Not taken: Ghostty draws the whole frame each time, where vtium draws only what changed; with no bound, one changed cell could damage any number of cells, and the row scratch would need the whole row's glyphs to compose one cell. One cell holds every letter measured (the maintainer, 2026-11-01).
- **Reach one cell to the right only, as foot does.** Not taken: italic descenders, `j` and `y`, and slanted outline glyphs lean left of their cells by 1 to 2 pixels at every size measured.
- **A separate right-only reach for bitmap italic,** since its shear reaches only right in the fonts measured. Not taken by the maintainer: one rule holds for every font.
- **Let ink reach the rows above and below, as Ghostty draws.** Not taken: a row would depend on the glyphs of three rows, its damage would cross rows, and the row scratch (ADR-1300 D5) composes one row; raising the baseline keeps the underscore, which is the ink that most needs it.
- **Draw a row's cells from right to left, as foot does,** so each glyph lands over its right neighbour. Not taken: a cell's background then covers the ink its right neighbour leans into it, which italic descenders do, and a cell's ink lands over its right neighbour's own letter.
- **Keep each glyph's ink in its own cell's colors under a mark, as foot does, or color a whole glyph on the cursor's cell in the cursor's color, as Ghostty does.** Not taken: a selection or block cursor would show stray ink in another color, or leak its color outside the block, and at 1 bit the ink could match the mark's background and vanish.
- **Draw bitmap italic upright, as the C reference and Ghostty do.** Rejected by the maintainer: a bitmap font with no italic face shears, so italic reads as italic on every font.
- **Mirror synthetic italic for right-to-left text,** its top leaning left, as this record first decided. Replaced by the maintainer: every script leans right, as fontconfig, Ghostty and WezTerm slant it (`basis`).
- **Shrink all Arabic text until its marks fit the row,** to about 60% of its size. Not chosen by the maintainer: the letters would be too small to read beside the text around them; moving the marks, and scaling a letter only where its marks cannot move in alone, keeps every other letter at its size.
- **Let a blinking letter's lean blink with it.** Not taken by the maintainer: a steady neighbour would then appear to blink.
- **A configuration key to cut ink at the cell,** as foot's `overflowing-glyphs`. Not taken for now (the maintainer, 2026-11-01); vtium adds one only if a font shows the need.
