---
contract: adr/v1
kind: decision
verb: add
capability: screen
amends:
  - "ADR-1001#The cell"
  - "ADR-1003#Wide characters and clusters"
  - "ADR-1200#D20: keys"
extends:
  - ADR-1203
basis:
  - operator: aaronsb
    level: directed
    said: "OSC 66 draws text at a scaled size across several cells, as kitty's text sizing protocol defines it. … vtium takes the whole protocol: the scale, the fractional size and the alignment keys, with the width key first. A cell run holding scaled text is one unit for erase, overwrite and reflow"
    via: "the body of the GitHub issue that tracks the kitty text sizing protocol, posted from the operator's account; the wording is agent-written"
  - operator: aaronsb
    level: directed
    said: "Programming ligatures … an option, off by default, under which the glyph helper shapes a run of cells and a ligature's glyph spans the cells it covers. Done when a decision record says how a ligature maps onto its cells"
    via: "the body of the GitHub issue that tracks programming ligatures and the emoji source for the frit made from font-name, posted from the operator's account; the wording is agent-written"
  - operator: aaronsb
    level: guided
    said: "**Outline fonts draw at true double size.** The font helper renders each glyph at twice the cell width, and at twice the height for DECDHL, which it splits across the top and bottom lines. … **Bitmap fonts double each pixel**, as the DEC terminals did with their character ROM."
    via: "the GitHub issue that tracks double-width and double-height lines, comment headed \"Direction from the maintainer, 2026-11-14\", posted from the operator's account; the wording is agent-written"
  - standard: "kitty's text sizing protocol (sw.kovidgoyal.net/kitty/text-sizing-protocol/), read 2026-11-14, the page of kitty 0.40.0 and later: OSC 66 ; metadata ; text, ended by BEL or ESC ST; the metadata a colon-separated list of key=value; the text Escape code safe UTF-8 of at most 4096 bytes; the keys s (1 to 7, default 1), w (0 to 7, default 0), n (0 to 15, default 0), d (0 to 15, default 0, greater than n when not zero), v (0 to 2, default 0: top, bottom, centred) and h (0 to 2, default 0: left, right, centred), v and h applying only when n < d; \"the text will be rendered in a block of s * w by s cells\"; w=0 splits the text into cells as without the protocol, each an s by s block; \"If the text does not fit, the terminal is free to do whatever it feels is best, including truncating the text or downsizing the font size\"; a block larger than the screen in either dimension is discarded; the wrapping rule under DECAWM and the four overwriting rules; the cursor moves s * w cells right in the same row; cursor movement is unaffected and a large cursor covers the block; the rules for ICH, DCH, ECH, ED, EL, IL and DL; a cell whose width w set is never resized by a variation selector or a spacing mark; support is detected by CPR, with no reply of its own"
  - evidence: "vtium-term's cell, read on integration at b3955f37: `crates/vtium-term/src/cell.rs` keeps the width in bits 22 and 23 of the content word (narrow, wide, the second column of a wide cell, and a wrap's pad cell) and leaves bits 24 to 31 zero and spare; `crates/vtium-term/src/line.rs` keeps a line's size, `LineSize`, in two bits of the line's flags, with four flag bits spare; `crates/vtium-term/src/screen/edit.rs`, `split_wide`, erases a wide character whose halves an edit would separate"
  - evidence: "ADR-1203 D3, as built: the console asks the glyph helper for a key of several cells, the syllable's text, U+001F and a mark that names the piece, and the helper shapes the text once and sends one piece for each cluster's cells, because vtium-font's glyphs, the glyph sets and the GPU's glyph slots hold at most two cells"
  - precedent: ADR-1203
  - precedent: ADR-1004
  - precedent: ADR-1003
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1001
  - ADR-1003
  - ADR-1004
  - ADR-1006
  - ADR-1200
  - ADR-1202
  - ADR-1203
  - ADR-1302
  - ADR-1307
---

# ADR-1308: A run of cells that draws one scaled glyph is one unit for every edit, and a ligature is drawn across its cells under an option that is off by default

## Summary

- **Decided:** A run is a block of cells that draws one piece of text as a whole. vtium has two kinds. A sized run is what kitty's text sizing protocol, OSC 66, writes: text drawn at a scale across a block of cells, up to 49 columns wide and 7 rows high. vtium-term stores a sized run as one character, and every edit treats it as one unit: an edit that would move, clear or overwrite some of its cells and not the rest erases the whole run. vtium takes all six of the protocol's keys, s, w, n, d, v and h, and follows the protocol's rules for wrapping, overwriting, the cursor and the editing controls. A ligature is the other kind. It is a drawing only, as a joined syllable is: the terminal keeps each character in its own cell, and the console draws the font's ligature across those cells, one piece in each cell. Ligatures are off by default, and the vtium.conf key `font-ligatures = on` turns them on. A double-size line is not a run: each of its characters is one cell, drawn twice as wide, and the VT510's rules for it stand.
- **Trades away:** A program that writes over any cell of a sized run's top row loses the whole run, as the protocol asks. Rows that hold part of a run taller than one row do not rewrap at a width change. A ligature draws apart when the cursor stands on it, so the drawing changes as the cursor moves through code. Only ligatures made of ASCII symbols draw, so a font's ligatures of letters or digits, such as `www` or `0x`, do not.
- **One-way?** No, not yet. The run is stored only in memory, in the screen and the scrollback, and nothing on disk holds it. The name of the key `font-ligatures` becomes expensive to change once a release ships it, since people write it into vtium.conf.
- **Probes:** *Not confident (cursor in a ligature):* when the cursor stands on any cell of a ligature, vtium draws the ligature's characters apart, so you can see which character the cursor is on, as it already does for joined syllables and the Arabic lam-alef. Is that what you want, or should the ligature stay whole under the cursor? *Confident (big text is one piece):* a heading a program writes in big text behaves like one large character. When anything writes over its first row, or an erase touches any part of it, the whole heading goes, never half of it. Right?
- **Inversion:** The choice sat between storing every cell of a run as an ordinary character and drawing the scale on top, which lets an edit leave half a heading, and making the whole line a special line, as a double-size line is, which cannot hold big text beside normal text. A run stored as one character keeps both: normal text beside big text, and no half of a heading. A ligature sits at the other end on purpose. The program wrote separate characters, so the terminal keeps them separate and only the drawing joins them.

## Context

A terminal is a grid of cells of one size. A wide character, such as a CJK ideograph or an emoji, already takes two cells, and vtium-term treats those two cells as one unit: an edit that would separate them erases both (`split_wide` in `crates/vtium-term/src/screen/edit.rs`). Three features draw a glyph across more cells than that.

- kitty's text sizing protocol, OSC 66, lets a program draw text at up to seven times the normal size, as a block of cells up to 7 rows high. It also lets a program state how many cells a piece of text takes, so that the program and the terminal agree on its width. The maintainer asked for the whole protocol, with a run of scaled text as one unit for erase, overwrite and reflow.
- Double-width and double-height lines (DECDWL, DECDHL and DECSWL, `ESC # 3` to `ESC # 6`) make a whole line draw at twice the width, and at twice the height across two lines. vtium-term already keeps a line's size in the line's flags (`LineSize` in `crates/vtium-term/src/line.rs`), and the work on double-size lines builds the rest by the VT510's rules.
- Programming fonts, such as Fira Code, JetBrains Mono, Cascadia Code and Iosevka, draw operators such as `->`, `!=` and `>=` as one shape. ADR-1000's `text.ligatures` row lists them as planned, off by default, and asks for this record first.

vtium already draws one shape across several cells in two places. ADR-1004 D6 draws the Arabic lam-alef ligature across the cells of lam and alef. ADR-1203 draws a syllable that the grapheme rules cut into several clusters across all their cells: the console asks the glyph helper for the syllable's whole text, the helper shapes it once, and each cell draws its own piece. Both draw apart when the cursor stands on them. ADR-1307 squeezes an Indic cluster wider than its cell sideways to fit it.

This record decides how a sized run sits in the cell model and how each edit treats it, how a ligature maps onto its cells, and where the ligature option lives. It also says which of OSC 66's keys vtium takes and what each one does.

## Decision

### D1: two kinds of run

A run is a block of cells that draws one piece of text as a whole.

- **A sized run** is the block that one OSC 66 sequence writes (D2). The terminal stores it as one character that covers its block (D3), and every edit treats it as one unit (D5).
- **A ligature** is a drawing only (D8). The terminal keeps each of its characters in a cell of its own, and every edit acts on those cells as on any others.
- **A double-size line is not a run.** Each character on it is one cell, which the renderer draws at twice the width, and on a double-height line at twice the height, split across the line and the next. The VT510's rules for the cursor, the margins and the editing controls apply to it, as the work on double-size lines builds them, and this record adds nothing to them. D7 says how a double-size line meets a sized run.

### D2: OSC 66 and its keys

vtium reads `OSC 66 ; metadata ; text`, ended by BEL or by ESC `\` (ST), as kitty's text sizing protocol defines it. The metadata is a list of `key=value` pairs separated by colons, and either part may be empty. vtium takes all six keys:

| Key | Values | Default | The protocol's statement | What vtium does |
|---|---|---|---|---|
| `s` | 1 to 7 | 1 | "The overall scale, the text will be rendered in a block of s * w by s cells" | The block is s times as high and s times as wide as the text's own cells. |
| `w` | 0 to 7 | 0 | "The width, in cells, in which the text should be rendered. When zero, the terminal should calculate the width as it would for normal text, splitting it up into scaled cells." | With w from 1 to 7, all of the text is one run, s × w columns wide and s rows high. With w = 0, vtium cuts the text into grapheme clusters as it cuts printed text (ADR-1006), and each cluster becomes a run of its own, s rows high and s times its own width. |
| `n` | 0 to 15 | 0 | "The numerator for the fractional scale." | With d, it scales the text inside the block. |
| `d` | 0 to 15 | 0 | "The denominator for the fractional scale. Must be > n when non-zero." | When d is not zero, the text draws at n/d of the block's size, inside the block. When d is 0, the text fills the block. |
| `v` | 0 to 2 | 0 | "The vertical alignment to use for fractionally scaled text (n < d). 0 - top, 1 - bottom, 2 - centered" | It places the smaller text at the block's top, bottom or middle. |
| `h` | 0 to 2 | 0 | "The horizontal alignment to use for fractionally scaled text (n < d). 0 - left, 1 - right, 2 - centered" | It places the smaller text at the block's left, right or middle. |

- **A key vtium does not know** is ignored, so that a program written for a later version of the protocol still draws. A known key whose value is not a whole number in its range makes vtium discard the whole sequence, text included. So does d not 0 with n not below d. vtium cannot know what space such a sequence meant, and drawing its text in some other space would break the program's layout.
- **The text** is UTF-8, which vtium decodes as it decodes printed text, with U+FFFD for each ill-formed part. A control character inside the text is dropped. Text over 4096 bytes is discarded with its sequence, as the protocol limits the text to 4096 bytes.
- **A sequence that sizes nothing**, with s = 1, w = 0 and d = 0, writes its text as ordinary printed text, and makes no run.
- **A run of one cell is still a run.** With s = 1 and w = 1, the text takes exactly one cell. vtium marks the cell's width as set by the program, so that neither VS15, VS16 nor a spacing mark that arrives later changes it, as the protocol requires.
- **No reply.** OSC 66 has no reply. A program finds support by the cursor reports that the protocol describes, and vtium's cursor moves as D4 says, so those reports answer correctly.

### D3: how a sized run is stored

A sized run keeps ADR-1001's cell of 8 bytes. Each line that one of the run's rows lies on holds that row's share of the run in a table of its own, as a line holds its styles and its links. So a line never refers to another line, and a line that leaves the scrollback takes its share of the run with it.

- **The line's run table** has one entry for each run that crosses the line. The entry holds the run's first column and its width in columns, which row of the run this line holds, and the run's s, n, d, v and h. It also records whether w set the width. A line with no run has an empty table and pays nothing for it.
- **Each cell of the run** is marked as part of a run with one of the content word's spare bits, bits 24 to 31 (ADR-1001, "The cell"). This record takes one of those bits.
- **The first cell of each row** of the run holds the run's text, as a cluster in the line's cluster arena. Each row holds its own copy, so each line can draw its row on its own. The other cells of the row hold no text.
- **The text's bound.** A run's text keeps as many whole grapheme clusters as fit two bounds: the line's cluster budget of 6 bytes for each of the row's cells, and the 32 codepoints that one cluster keeps (ADR-1001, "The line"). Clusters past that are dropped, as the protocol allows a terminal to truncate text that does not fit. A run keeps at least its first cluster.
- **The style** of the run is the style that was current when the program wrote it, held in every cell of the run, as printed text holds it.

### D4: writing a sized run

The run's block is s × w columns wide and s rows high, with w the width that D2 gives it.

- **Too large for the screen.** A block wider than the screen's columns or taller than its rows is discarded, as the protocol says.
- **At the right margin.** When the block does not fit before the right margin and DECAWM is set, the cursor moves to the start of the next line and the run is written there, as the protocol says. When DECAWM is reset, the cursor moves left until the block fits and the run is written there, by the overwriting rules below.
- **At the bottom.** When the block's last row would lie below the scrolling region's bottom margin, vtium first scrolls the region up, as that many line feeds at the bottom margin would, and the cursor moves up with the text. A block taller than the scrolling region is discarded, because it cannot fit without passing the region's margins. The protocol does not say what happens near the bottom of the screen, and this is vtium's reading.
- **The cursor** moves s × w columns to the right, in the row where the run starts, as the protocol says. With a pending wrap at the right margin, it waits there as it does after a wide character.
- **Overwriting.** When text or a run would write into the cells of an existing run, vtium applies the protocol's rules, the first one that holds:
  1. A combining character, one that the grapheme rules join to the cluster before it, is added to the run's text, within D3's bound.
  2. Text that would write into the run's top-left cell erases the whole run.
  3. Text that would write into any other cell of the run's top row erases the whole run. The protocol says "replaced by spaces" here, for compatibility with wide characters. vtium clears every cell of the run to the blank style, as it clears a wide character that an overwrite splits, so rules 2 and 3 draw the same.
  4. Text that would write into a row of the run below its first moves the cursor past the run's cells on that row first, whatever DECAWM says, and is then written. When the run's cells reach the right margin on that row, the text goes where text written past the margin goes: to the next line when DECAWM is set, and nowhere when it is reset. The protocol does not cover a run that reaches the margin, and this is vtium's reading.
- **Erasing a run** clears every one of its cells on every row the screen holds. Rows of the run that have scrolled into the scrollback stay as they are, because a program never changes the scrollback.
- **REP after a run.** REP repeats the graphic character written last. A run is not a graphic character for it, so after a run REP repeats nothing.

### D5: every edit treats a sized run as one unit

**The rule.** An edit that would move, clear or overwrite some of a run's cells and not the others first erases the whole run. An edit that moves all of a run's cells together keeps the run.

This rule gives each of the protocol's rules for the editing controls:

- **ICH, and printing in insert mode,** shift the cells from the cursor right. A run with a row on the cursor's line and other rows on other lines is erased, since only one of its rows would move. A run of one row that the insertion splits, or pushes partly past the right margin, is erased.
- **DCH** shifts the cells after the cursor left, and the same two cases erase a run.
- **ECH, EL and ED** erase each run that the erased cells touch. Mode 22 of ED (`CSI 22 J`) copies the screen into the scrollback first, runs included, as the protocol says.
- **IL** erases a run whose second or later row is on the cursor's line, and a run that the bottom margin would cut. **DL** erases each run that a deleted line holds.

The same rule covers the edits the protocol does not list:

- **Scrolling** within a scrolling region, and a region bounded left and right by DECSLRM, erases a run that the region's edges cut, and moves a run that lies wholly inside the region.
- **DECIC and DECDC,** which insert and delete columns, erase a run that the moved columns cut.
- **The rectangle edits,** DECCRA, DECFRA, DECERA and DECSERA, erase each run that the rectangle touches. DECCRA copies a run only when the rectangle holds the whole run, and the copy is a run of its own.
- **DECALN** fills the screen with E, which erases every run.
- **Selective erase** (DECSED, DECSEL and DECSERA) erases a run only when the run's style is not protected, and then erases the whole run.

A change to any cell of a run damages every cell of the run, on every row, so the renderer draws the whole run again (ADR-1302 D9).

### D6: the cursor, selection and reflow of a sized run

- **Cursor movement** moves one cell at a time, as the protocol says, so the cursor can stand on any cell inside a run.
- **The cursor's drawing.** When the cursor stands on any cell of a run, vtium draws the cursor over the whole block, as the protocol asks. A block cursor covers every cell of the run, a bar cursor stands along the run's left edge on all its rows, and an underline cursor runs under its bottom row. The run stays drawn whole under the cursor.
- **A mouse report** names the cell under the pointer, as it does for any cell.
- **Selection and copy.** A selection that touches any cell of a run selects the whole run. Copying gives the run's text once, from its top row, and the run's other cells copy as nothing.
- **Right-to-left text.** The bidirectional algorithm sees a run as one U+FFFC OBJECT REPLACEMENT CHARACTER, which is neutral. So a run moves as one block when its paragraph is reordered, and its text is drawn in the order its own shaping gives (ADR-1004).
- **Reflow at a width change (ADR-1003).** A run of one row moves whole to the next row when the row no longer holds it, as a wide character does, and leaves pad cells behind it. A line that holds any row of a run taller than one row does not rewrap: it keeps its columns, as a line written while mode 2028 is reset keeps them. A narrower margin that cuts such a run erases the whole run.
- **A resize that makes the screen too small** for a run, narrower than its columns or shorter than its rows, erases the run, as the protocol says a terminal may.

### D7: sized runs on double-size lines

- **OSC 66 on a double-size line** takes only its width: vtium writes the text as a run of s = 1 with the width w gives, in the line's half-width columns, and ignores s, n, d, v and h. A block of several rows across lines of different sizes has no meaning that the protocol or the VT510 gives.
- **A line's size change erases the runs on it.** `ESC # 3`, `# 4`, `# 5` and `# 6` erase every run that crosses the line whose size changes, on all its rows, before the line takes its new size.

### D8: ligatures

A ligature is a shape that a font draws in place of several characters. vtium draws it across the cells of those characters. The terminal does not change for a ligature: it keeps each character in its own cell, and it moves the cursor, erases, wraps, selects and copies by those cells. The console finds ligatures each time it draws a row, so nothing about them is stored.

- **The option.** `font-ligatures` in vtium.conf turns ligatures on with `on`. Its default, `off`, draws no ligatures. The key sits at the top level and in a `[look NAME]` section, where a look's value wins, as `font-hinting` does (ADR-1200 D16). A program cannot change it. A frit has no key for it.
- **Which cells can join.** With the option on, the console looks for runs of two to fourteen cells on one row that each hold one ASCII symbol: U+0021 to U+002F, U+003A to U+0040, U+005B to U+0060 and U+007B to U+007E. These are the characters that programming ligatures are made of, such as `->`, `=>`, `!=`, `===`, `<=>`, `&&` and `/*`. A run of such cells longer than fourteen draws each cell alone. The bound of fourteen comes from the glyph helper's key, as ADR-1203 D1 bounds a syllable.
- **The console asks for the run as one key.** It asks the helper for each piece of the run, with ADR-1203 D3's key: the run's text, then U+001F, then the piece's mark. The helper shapes the whole text once with HarfRust and the font's default features, which include the ligature features `liga` and `calt`. It places the drawing across all the run's cells and cuts it at each cell's edges, and each cell draws its own piece. This works whether a font draws a ligature as one glyph across the cells or as a different glyph in each cell, as most programming fonts do.
- **A run the font does not join.** When shaping gives each character the same glyph at the same place that it has alone, the helper answers that the run does not join. The console then draws each cell alone, and remembers that answer for that text, so the store holds no copies of ordinary glyphs.
- **When a ligature draws apart.** A ligature draws as its separate characters, each in its own cell, under the conditions ADR-1203 D4 gives for a syllable: when the cursor stands on any of its cells, in either phase of its blink; when its cells differ in foreground color, style or script; when a selection or a link's overlay covers part of it; and until the helper has answered. A change to any of its cells damages all of them (ADR-1302 D9).
- **Where ligatures never draw.** Cells that the frame draws in an order other than their stored order, left to right, never join. So a ligature never draws backwards in a right-to-left paragraph. Cells of a double-size line, and cells of a sized run, never join into a ligature. A bitmap font draws no ligatures, since the helper does not shape bitmap fonts, and neither do the built-in fonts. A build that does not draw one key across several cells draws each cell alone, as ADR-1203 D4 says of syllables in the minspec build.
- **A sized run's own text** is shaped whole with the font's default features, whatever `font-ligatures` says. A heading that a program writes in big text is the program's own typography, so it shows the font's ligatures, such as `fi`, where the font draws them by default.

### D9: how a sized run is drawn

- **The renderer asks for the run's drawing as one key.** The key is the run's text, its style, its scale s, its fraction n/d, its alignment v and h, and its block's columns and rows. This adds the scale, the fraction, the alignment and the block to ADR-1200 D20's key. The helper draws the whole block once and sends it in pieces, one for each cell, as ADR-1203 D3 sends a syllable's pieces, since a glyph holds at most two cells. Each cell of the run draws its own piece.
- **An outline font draws at true size.** The helper renders the text at s times the size of the frit's first source, times n/d where d is not zero, so strokes stay sharp and nothing is stretched. This is the maintainer's rule for double-size lines, applied to every scale.
- **A bitmap font doubles its pixels.** A bitmap source draws at s times its own size by whole factors, every pixel fully on or off, as the DEC terminals drew double-size text. A fraction draws the 1-bit glyph at n/d of that size, by ADR-1202's scaling of a bitmap glyph to a smaller size. A built-in font draws the same way in the console, while the helper has not answered and where no helper runs.
- **A color glyph** is scaled smoothly to the block, as a color glyph is scaled elsewhere.
- **Text wider than its block.** Where the text's advance at that size is wider than the block, the helper draws the text smaller, keeping its shape, until it fits, as the protocol allows. It stands on the block's baseline. With d = 0, text narrower than the block is centred across it.

### D10: the records this amends

- **ADR-1001, "The cell":** one of the content word's spare bits, bits 24 to 31, marks a cell as part of a sized run, and a line gains a run table beside its style and link tables (D3).
- **ADR-1003, "Wide characters and clusters":** a sized run of one row moves whole at a narrower width, as a wide character does, and a line that holds a row of a taller run does not rewrap (D6).
- **ADR-1200 D20:** a sized run's key adds its scale, its fraction, its alignment and its block of cells (D9).
- **ADR-1203 D3 and D4:** the key of several cells and the rules for drawing apart now serve ligatures too (D8).

## Verification

- vtium-term, the replay corpus: one case for each key and each of its bounds; a key out of its range and an unknown key; w = 0 against w from 1 to 7; a run at the right margin with DECAWM set and reset; a run near the bottom of the scrolling region; each of the four overwriting rules; ICH, DCH, ECH, EL, ED, IL and DL against a run of one row and a run of several rows; scrolling in a region that cuts a run; a line's size changed under a run; VS16 after a cell whose width w set; and the cursor reports a program uses to find support. Each case's `# spec:` line quotes the protocol, or names this record where vtium reads a gap in it.
- vtium-term, reflow: a run of one row moves whole at a narrower width, and a line that holds a row of a taller run keeps its columns.
- vtium-render and the helper: a run at two scales, on a bitmap font and on an outline font, and a fraction at each alignment, as tier 2 screens, provisional until the maintainer reviews them at 1:1.
- Ligatures: with `font-ligatures = on`, a programming font's `->`, `!=` and `===` draw as the font's ligature across their cells; with the cursor on one of them they draw apart; with the option off they draw as single characters. A run the font does not join draws its cells alone. The tests carry `covers: text.ligatures`.

## Consequences

### Positive

- A program can write a heading or a banner in big text beside normal text, and a program can state the width of any piece of text, so the two always agree on columns.
- No edit ever leaves half of a heading on the screen.
- Ligatures use the same pieces that joined syllables use, so the helper and the renderer gain no second path for drawing across cells.
- The terminal's cells do not change for ligatures, so a program that counts one cell per character keeps its columns with ligatures on.

### Negative

- Each line gains a run table, and the renderer's and the store's keys gain the scale, the fraction and the block.
- A run of seven rows and 49 columns draws as 343 pieces, each one a glyph in the store.
- A line with a row of a taller run does not rewrap.
- Ligatures of letters and digits do not draw.

### Neutral

- A double-size line keeps the VT510's rules, and this record adds only how it meets a sized run.
- Lines written before this record hold no runs, and nothing about them changes.

## Alternatives Considered

- **Store each cell of a run as an ordinary character and draw the scale on top.** Not taken: an edit to one cell would leave the rest of a heading on the screen, which the protocol forbids, and the maintainer asked for the run to be one unit.
- **Store the run's parameters in every cell,** as wide extra bits of the cell. Not taken: the scale, the fraction, the alignment and the position inside the block need more bits than the cell's spare byte holds, and ADR-1001 keeps the cell at 8 bytes. A table in the line costs nothing on a line without runs.
- **Keep the run's text only in its top row.** Not taken: a line would then refer to another line, and when the top row left the scrollback the rows below would lose their text.
- **Take only the width key,** as some terminals do. Not taken: the maintainer asked for the whole protocol.
- **Store ligatures in the terminal,** as a cell that covers several characters. Not taken: the program wrote separate characters, and the cursor, the selection and copying must still reach each of them. A ligature drawn from the frame, as ADR-1203 draws a syllable, needs nothing stored.
- **Keep a ligature whole under the cursor and clip the cursor to one cell of it.** Not taken in this draft: a terminal must show which character the cursor is on, and the maintainer chose to split a syllable under the cursor for that reason. This is the first probe.
- **Shape every run of text with the same style, letters included,** as a word processor does. Not taken: every distinct word would become a key in the helper and in the store, and a programming font's letter ligatures are rare. Runs of ASCII symbols cover the operators that programming ligatures exist for.
