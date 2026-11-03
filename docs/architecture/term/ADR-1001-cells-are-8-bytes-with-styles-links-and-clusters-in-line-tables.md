---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: directed
    said: "It bounds scrollback by the machine's memory with the operator's caps (2026-10-26): `sb-size` is accepted up to 10,000 lines below 2 GiB, 30,000 from 2 GiB, and 100,000 from 4 GiB."
    via: the body of the GitHub issue for vtium-term's cell, line and frame model
  - upstream: "kmscon ADR-304 §4 and §5 (aaronsb/kmscon): the cell model is designed fresh, budgeted for reflow's wrapped-line flag, hyperlink ids, overline and conceal, and measured against the minspec targets: resident after the first screen at most 10.3 MiB with unifont, at peak with the scrollback full at most 15.7 MiB"
  - upstream: "kmscon ADR-102 (aaronsb/kmscon): cells carry clusters and unresolved colour; the C reference's cell was 32 bytes, held there by a static assert, with an 8-byte change age; clusters keep at most 32 codepoints"
  - upstream: "kmscon ADR-103 (aaronsb/kmscon): an image placement is anchored to the line holding its top row and travels with it; kmscon's README, \"Minimal build\": the 15.7 MiB peak held a full 1,000-line scrollback"
  - upstream: "the C fork's issues on a region scroll that flagged every cell, no mark for the default background, padding drawn with the pen, the cursor color, and no continuation flag for reflow"
  - standard: "kitty graphics protocol, z-index: images below -1,073,741,824 are drawn under cells with non-default background colors"
  - standard: "ECMA-48 §8.3.117 (SGR); xterm control sequences (DECSCUSR, DECSCA, OSC 12); mintty's SGR 73-75; FinalTerm's OSC 133"
  - precedent: ADR-1000
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "I reviewed adr 1001 and it seems sane with the memory estimates and approach."
    via: operator, in session 2026-10-27, read before the cluster bound was added
    covers: [default memory]
  - operator: aaronsb
    said: "10000 lines is fine but it should be configurable"
    via: operator, in session 2026-10-26, after the corrected worst case
    covers: [the cap]
  - operator: aaronsb
    said: "Placeholder cells: widen the cluster budget for U+10EEEE cells only, to at most three diacritics (up to 17 bytes); every other cell keeps the 6-byte cap."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
  - operator: Aaron Bockelie
    said: "accept"
    via: steering session, 2026-11-02
    covers: [placeholder memory]
  - operator: aaronsb
    said: "I think A"
    via: "the maintainer, 2026-11-02, in the steering session, answering option A for the minspec peak test that priced the two screens as empty: the scrollback default follows the display; at start, vtium sizes the scrollback's memory budget from the real grid so the 16 MiB peak always holds; small displays keep the full scrollback; only a 1366x768 screen gets a smaller budget; the rule holds by construction, with a test"
  - operator: aaronsb
    said: "I really have no issues with an 8k display scrolling crazy glyphs, characters, and images on a terminal consuming hundreds of megabytes of ram on a sufficicently capable machine (like this one). it's not a hard gate that everything must fit within 16mb. I hope we didn't record it like that"
    via: the maintainer, 2026-11-02, in the steering session, on what option A meant for displays larger than minspec's
  - operator: aaronsb
    said: "we should be kind to normal computers - a lightweight laptop or tablet, or some other typical machine that would get regular use of a terminal, can of course go beyond minspec but we should be mindful not to guzzle ram"
    via: the maintainer, 2026-11-02, in the steering session, on whether the scrollback default grows past minspec
  - operator: aaronsb
    said: "above minspec I value usability and performance over ram footprint. (within reason)"
    via: the maintainer, 2026-11-02, in the steering session, the principle behind the two answers before it
observable:
  - "`cargo test -p vtium-term --test memory -- --nocapture` prints the resident figures this record quotes, and fails when the model's size changes"
  - "Changing the cell or the line so it grows fails the build at the size assertions in `src/cell.rs` and `src/line.rs`"
status: accepted
date: 2026-10-26
deciders:
  - aaronsb
related: []
---

# ADR-1001: Cells are 8 bytes, with styles, links and clusters in line tables

## Summary

- **Decided:** A cell is 8 bytes: what it shows and its width, and two 16-bit indices, one into its line's style table and one into its line's link table. A grapheme cluster of several codepoints lives in its line's cluster arena, which holds at most 6 bytes a cell; a cluster past that keeps the codepoints that fit. Each line owns its tables, a permanent id that images and prompt marks anchor to, and 16 bits of flags: the wrapped flag reflow reads, the three shell-integration marks and the line size, with ten spare for bidirectional text. Change ages are kept per screen position, so neither cells nor the scrollback carry them. A frame resolves colors through the palette when it is drawn. `sb-size` defaults to 1,000 lines and is capped by default at 10,000, 30,000 or 100,000 lines by the machine's memory; a configured cap replaces that default, up to 100,000.
- **Trades away:** A cell no longer holds its style itself, so reading one goes through its line, and moving cells between lines (reflow, rectangle copies) renumbers their indices. A line whose cells each carry their own style, their own link and a long cluster costs 32 bytes a cell where plain text costs 8; a line full of long clusters keeps only part of each past its budget. At the 10,000-line cap the scrollback costs more than the C reference's measured peak allowed, which held 1,000 lines.
- **One-way?** No. Reversible now; expensive once the screen and reflow are built on these types.
- **Probes:**
  - *Confident (default memory):* a console left at the default 1,000 lines of scrollback spends about 1.5 MiB on it, against the C reference's 5.2 MiB, so the minspec build keeps about 4 MiB of headroom under its 15.7 MiB peak.

  > Note 2026-10-26: the 4 MiB holds for full shell lines. The worst a program can write into the default scrollback leaves about 0.3 MiB (Memory).

  - *Not confident (the cap):* that on a machine under 2 GiB, setting `sb-size` to the 10,000-line cap may cost about 14.5 MiB of full shell lines, and at worst about 100 MiB, when a program fills its rows with long combining clusters and kitty placeholder cells, each cell with its own colors and its own link, and a rewrap packs the clusters together; at the 100,000-line cap, from 4 GiB, that worst case is about 997 MiB. Squeezing plain text to about a byte a character is possible, at the cost of more code and of unpacking lines as they are scrolled back to.

  > Edited 2026-11-02, with placeholder cells (Memory, the note of 2026-11-02): the probe read "and at worst 53 MiB, when a program gives every cell its own colors, its own link and a long combining cluster; at the 100,000-line cap, from 4 GiB, that worst case is 531 MiB".

  > Edited 2026-10-26: review found that clusters, not truecolor art, were the worst case, with no bound on a line's cluster bytes (24.6 KB a line), and the arena is now bounded. This probe read: "that on a machine under 2 GiB, setting `sb-size` to the 10,000-line cap may cost about 15 MiB of full shell lines, and up to 37 MiB if the scrollback fills with pictures printed as coloured text. Squeezing it to about a byte a character for plain text is possible, at the cost of more code and of unpacking lines as they are scrolled back to."
  - *Not confident (placeholder memory):* a picture drawn with kitty's placeholder cells keeps up to 17 bytes a cell, so the worst row of scrollback the memory test prices grows from 7,560 to 10,450 bytes. The 16 MiB peak and `sb-memory`'s default stay as you set them, and the scrollback still stops at that default, so the cost is fewer of the worst rows kept: about 570 where it was about 788. Ordinary shell lines are unchanged. Is that acceptable?

  > Added 2026-11-02, with placeholder cells (Memory, the note of 2026-11-02).

  > Note 2026-11-02, later: `sb-memory`'s default now follows the console's grid, and at the minspec grid it is 5 MiB, where it was 5,963,776 bytes, so the scrollback keeps fewer of the worst rows there (Memory, the note of 2026-11-02 on the screens).
- **Inversion:** Between "a fat cell that holds everything inline" (the C reference's 32 bytes) and "a tiny cell indexing tables the whole screen shares" (style ids with reference counts). The answer is the tiny cell with its tables owned by its line: no shared table to count references in, hash, or fill, and a line that leaves the scrollback takes everything it used with it.

## Context

vtium designs its cell model fresh (kmscon ADR-304 §4) before the screen is built on it. The C reference's cell was 32 bytes, held there by a static assert: the codepoint, a 4-byte width holding 0 to 2, a 12-byte attribute struct, and an 8-byte change age. Its tracker queued claimants the cell had no room for: overline, superscript and subscript, conceal, a hyperlink id, a mark for the default background; per line, a continuation flag for reflow, prompt marks, bidirectional state and image anchors; per frame, the cursor's shape and color.

The minspec machine (1366x768, 512 MB) sets the budget. In unifont's 8x16 cells the screen is 170 by 48. The C reference's measured peak, 15.7 MiB with unifont, held its default 1,000-line scrollback after 500,000 lines of text; its README measured only the totals. Derived from its layout, those 1,000 lines were 5.19 MiB of 32-byte cells, and its main and alternate screens 0.50 MiB. The operator's caps let a machine under 2 GiB keep ten times that many lines.

> Added 2026-10-27, after minspec became a single-board computer (docs/architecture.md). Its display floor is a 320x240 panel, 40x30 in an 8x8 font, and 1366x768 stays tier 3's x86 variant; the budget below still measures the 170 by 48 screen, the largest minspec grid. The byte figures here are 64-bit layouts. On armv7 they are smaller (the screens 265,728 bytes against 267,264, a worst-case line 5,512 against 5,568), and `tests/memory.rs` pins each per pointer width.

## Decision

### The cell

A cell is 8 bytes:

| Bits | Holds |
|---|---|
| content, bits 0-20 | A codepoint, or the byte offset of a cluster in the line's cluster arena; 0 is empty |
| content, bit 21 | Set for a cluster |
| content, bits 22-23 | The width: narrow, wide, or the second column of a wide cell |
| content, bits 24-31 | Zero, spare |
| style, 16 bits | The index of its style in the line's style table; 0 is the default style |
| link, 16 bits | The index of its hyperlink in the line's link table; 0 is no link |

The all-zero cell is the blank cell: empty, narrow, default style, no link. A cell erased in a color (BCE) is empty in that style, and is not blank.

### Styles

A style is the two colors, the underline color and 16 bits of attributes, 14 bytes, held in the line's table. A color is the default, a palette index or RGB, kept as the program set it until a frame resolves it. The attributes are bold, dim, italic, blink, rapid blink, inverse, conceal, strike, overline and protected (DECSCA), each a bit; the underline style in three bits (none, single, double, curly, dotted, dashed); and the script in two (baseline, superscript, subscript). One bit is spare.

### The line

A line owns its cells and three tables: styles, hyperlinks (each entry a link id into the link store that the work on OSC 8 hyperlinks builds) and clusters. Clusters sit in one arena per line, each as a length byte and its codepoints in UTF-8, at most `CLUSTER_MAX`, 32, codepoints; the cell holds the cluster's byte offset. A line with only default, unlinked, single-codepoint cells allocates no tables.

The clusters a line's cells name take at most `CLUSTER_BYTES_PER_CELL`, 6, bytes per cell of the width the line was written at: a base and two combining marks a cell on average. A cluster that would pass the budget keeps the longest run of its first codepoints that fits, and at least its first codepoint; overwriting or erasing a cluster gives its bytes back. Ordinary text spends little of it: an emoji with a skin tone or a flag takes 9 bytes across its two cells, a letter with an accent 4.

> Added 2026-10-26, after review measured a line of 32-codepoint clusters at 24.6 KB, 26.4 MiB at the default 1,000 lines. The bound holds the worst case at the default inside the minspec peak (Memory).

> Edited 2026-10-26, after review of the screen and scrollback work: a line trimmed for the scrollback keeps the clusters its cells name, which may pass the budget of its fewer cells but never that of the width it was written at, so the memory bound holds. This paragraph read "bytes per cell of the line".

**Placeholder cells.** A cluster whose first codepoint is U+10EEEE, the kitty graphics protocol's Unicode placeholder (ADR-1100 D13), keeps its first three combining marks, and such a cluster takes at most 17 bytes: its length byte, the 4-byte placeholder and three marks of up to 4 bytes each (the maintainer, 2026-11-02). A placeholder carries its picture's row, column and the top byte of its image id in those three marks, and cutting them made a picture that starts late in a line, such as one in a tmux pane to the right of another, draw each of its rows as its first. Placeholder clusters sit outside the line's budget of 6 bytes a cell: the budget counts every other cluster, which it cuts as before, and a placeholder keeps its marks whatever the other clusters take, so text beside a picture keeps the accents it would keep anywhere else. A line of W cells, P of which name a placeholder, therefore names at most 6W + 17P cluster bytes, and it counts its placeholder cells to know P. The line compacts its arena against twice that, 2 × (6W + 17P), where P counts the placeholder cells as of the line's last check for room, which comes before every write and every erase. An erase that removes placeholders gives their bytes back but leaves the arena as it was until the next write finds it full and compacts it, so the arena holds at most 2 × (6W + 17P) + 129 bytes, the threshold and one longest cluster, for P as of that check, and never more than 46W + 129 bytes whatever the line holds now. vtium-term's flood test, which checks each line's heap against the bound its tables compact at, and the line's unit test of that bound both take the placeholder term. Compacting against twice the budget alone, 12W, would not do: a line of placeholders names up to 17W, which stays above 12W after a compaction, so every later write would compact the line again, and each compaction walks the whole line. Rewriting a full line of 8,192 placeholders would then take 8,192 compactions, about 67 million cell visits and over a gigabyte of copied bytes, where it now takes at most one. At `COLUMNS_MAX` the largest arena offset is 2 × 23 × 8,192 + 129 = 376,961, inside the cell's 21 bits (2,097,152), and a compile-time assertion holds it. The Memory section's note of 2026-11-02 gives what placeholder cells add to the worst line and the worst row. The C reference had no byte budget and kept a cluster's first 32 codepoints, so it kept a placeholder's marks too, and for placeholder cells vtium's departure in vtium-term's README no longer applies.

> Added 2026-11-02: the maintainer widened the budget for placeholder cells only (`considered`), from the image records' question on kitty placeholders. Until then a placeholder's marks were cut at the budget like any other cluster's.

A write looks its style and link up in the line's table, newest entry first, and appends them if absent, so a write is linear in the line's width at worst. Once the style or link table reaches twice the line's cells, or the arena twice the cluster bytes the line's cells may name, its budget and its placeholder cells' bytes (Placeholder cells, above), the line compacts: it keeps the entries a cell still names and renumbers the cells. A compaction costs time linear in the line's width, as a lookup does, and no table holds more than twice what its line can name, whatever a program writes. Shrinking a line compacts it too. `COLUMNS_MAX` (8192) keeps twice the widest line inside a 16-bit index, and twice the cluster bytes it may name inside the cell's 21 bits; compile-time assertions hold both. Nothing is shared between lines, so no table needs a reference count or a hash.

> Edited 2026-11-02, with placeholder cells: the line compacted when "the arena [reached] twice the line's cluster budget", and `COLUMNS_MAX` kept "twice its cluster budget inside the cell's 21 bits".

A line has a `LineId`, unique within its screen buffer for the buffer's life and never reused, and 16 bits of flags:

- **Wrapped:** the line's text continues on the next line, because printing wrapped at the right margin. A line the program ended with a newline, or left by moving the cursor, does not carry it. Reflow rejoins only wrapped lines.
- **Prompt, input, output:** OSC 133's A, B and C marks start on this line.
- **Line size:** single, double width, or the top or bottom of double height, in two bits.
- Ten bits are spare for bidirectional state.

A line may hold fewer cells than the screen is wide. A line entering the scrollback is trimmed: it drops its trailing blank cells, compacts, and gives back spare capacity. Every cell past a line's end reads as the blank cell in the default style, never the pen.

### Where each claimant lives

| Claimant | Where | Issue that uses it |
|---|---|---|
| The glyph or grapheme cluster | Cell content; the line's cluster arena | the screen, scrollback and Unicode |
| The width | Cell content, bits 22-23 | the screen, scrollback and Unicode |
| Overline, conceal, rapid blink | Style attributes | SGR overline, superscript and subscript |
| Superscript and subscript | Style attributes, the script | SGR overline, superscript and subscript |
| A hyperlink id | Cell link index; the line's link table | OSC 8 hyperlinks and OSC 133 prompt marks |
| The default background mark | The style's background is `Color::Default`; the frame cell's `default_bg` | the C fork's issue on marking the default background |
| The change age | `Damage`, by screen position | the screen, scrollback and Unicode |
| The continuation flag | Line flags, wrapped | reflow on resize |
| Prompt marks | Line flags, prompt, input and output | OSC 8 hyperlinks and OSC 133 prompt marks |
| Bidirectional state | Line flags, ten spare bits | bidirectional text |
| Double width and height | Line flags, line size | double-width and double-height lines |
| Image anchors | The `LineId` | the image pipeline |
| The cursor's shape and color | The frame's `Cursor` | modes, SGR and replies; the OSC color sequences |
| Change tracking renderers read | The frame cell's `changed`, the frame's `row_changed` | the CPU compositor |

### Change tracking

`Damage` holds a change age for every screen position and every row, and one for the whole screen. Each mark stamps the current age. A display takes an age when it draws, and a position has changed for it when its stamp is newer, so each of several displays keeps its own. What marks what:

- writing or erasing cells marks those cells;
- scrolling marks the rows that moved: inside margins, the region's rows only, and for the whole screen, every row;
- a resize, a palette change, reverse video (DECSCNM), switching between the main and alternate screens, and moving the view through the scrollback mark everything;
- an image placement that is added, moved, clipped or removed, or whose image changes, marks the cells under its old and its new box;
- moving the cursor marks nothing, since the frame carries the cursor and a renderer redraws the cells it left and covers;
- a pen change alone (SGR) marks nothing.

Damage tracks at most `COLUMNS_MAX` (8192) columns by `ROWS_MAX` (2048) rows, and a larger size is clamped, so a resize the stream asks for cannot allocate without limit.

### The frame

A frame is the visible lines, the damage, the palette, the age the reading display last drew at, the cursor and DECSCNM. For each cell it gives the glyph (a codepoint or a cluster's codepoints), the width, the foreground and background resolved through the palette and inverse video, the underline color SGR 58 set (resolved, and kept through inverse video) or none to follow the configured one, the attributes, whether the background is the default one and not inverted, the link id, and whether the cell changed for this display. The cursor carries its position, visibility, DECSCUSR shape and blink, and OSC 12's color. Resolving at draw time is what lets a palette change recolor text already on the screen (ADR-1000's row "Colors resolved at draw time").

Bold as bright and dim join the resolution with the work on resolving bold, the selection with the work on the screen and scrollback, and the placement list with the image pipeline. vtium-image anchors a placement by the `LineId` of the line holding its top row, and vtium-term marks in `Damage` the boxes the image store reports changed, so vtium-image never depends on vtium-term (ADR-1100 D1); the line holds nothing for images.

> Edited 2026-11-01: ADR-1100, accepted 2026-11-01, gives the marking of a placement's boxes to vtium-term. The sentence read: "vtium-image anchors a placement by the `LineId` of the line holding its top row and marks its boxes through `Damage`; the line holds nothing for images." The earlier text is in git at 1d51e46.

### Scrollback size

`sb-size` defaults to 1,000 lines, the C reference's default. By default it is accepted up to 10,000 lines below 2 GiB of memory, 30,000 from 2 GiB and 100,000 from 4 GiB. A cap the configuration sets replaces that default, higher or lower, and is clamped to `SB_SIZE_MAX`, 100,000 lines; its key is wired with the configuration. A larger `sb-size` is clamped to the cap.

> Added 2026-10-26: the operator, on the corrected worst case, said "10000 lines is fine but it should be configurable". The caps are rows of vtium-term's bounds table.

### Memory

Computed from the structures by capacity in `crates/vtium-term/tests/memory.rs`, which pins each figure. A line costs its 48-byte struct and its heap. The main and alternate screens, 170 by 48 blank lines each, and the one set of change ages they share, are 201,216 bytes (0.19 MiB), against the C reference's 0.50 MiB.

> Edited 2026-11-02 (the note of 2026-11-02 below): the screens counted their change ages twice, where a screen holds one set of 66,048 bytes for both. The sentence read "The main and alternate screens, 170 by 48 lines each with their damage, are 267,264 bytes (0.25 MiB)".

| Scrollback content | Bytes a line | 1,000 lines | 10,000 lines | 30,000 lines | 100,000 lines |
|---|---|---|---|---|---|
| Plain text, full width | 1,408 | 1.34 MiB | 13.43 MiB | 40.28 MiB | 134.28 MiB |
| A shell's lines (three styles), full width | 1,516 | 1.45 MiB | 14.46 MiB | | |
| A shell's lines, 60 cells | 636 | 0.61 MiB | 6.07 MiB | | |
| Truecolor art, every cell its own colors, full width | 3,868 | 3.69 MiB | 36.89 MiB | | |
| The worst case: every cell its own style, its own link and a 32-codepoint cluster, the cluster budget full | 5,568 | 5.31 MiB | 53.10 MiB | 159.30 MiB | 531.01 MiB |

The C reference's figures are its measured totals, 10.3 MiB after the first screen and 15.7 MiB at peak with unifont, and what its layout gives: 0.50 MiB for its two screens and 5.19 MiB for 1,000 lines of scrollback. Assuming everything else in vtium's process costs what everything else in the C reference's did, 9.80 MiB:

- **After the first screen**, about 9.99 MiB, against the target of 10.3 MiB.

  > Edited 2026-11-02, with the screens' figure above: this read "about 10.06 MiB".
- **At the default 1,000 lines**, about 11.5 MiB full of full-width shell lines, 4.2 MiB under the 15.7 MiB peak target, and about 15.4 MiB in the worst case, 0.3 MiB under. `the_worst_default_scrollback_fits_the_minspec_peak` checks the worst case against the peak.
- **At the 10,000-line cap**, below 2 GiB, about 24.5 MiB full of full-width shell lines and 63.2 MiB in the worst case, 12% of a 512 MB machine. The 15.7 MiB target was measured with 1,000 lines, so the cap passes it; meeting it at 10,000 lines would take about 3.4 bytes a cell, which only packing the text reaches (Alternatives).
- **At the 30,000-line cap**, from 2 GiB, the worst case is about 169 MiB, 8% of 2 GiB; **at the 100,000-line cap**, from 4 GiB, about 541 MiB, 13% of 4 GiB.

Tier 3 measures the real figures from the first working build (kmscon ADR-304 §5).

> Added 2026-10-28, after ADR-1003, and corrected the same day: a rewrap moves clusters with their cells, so a rewrapped row may name more than its own cells' budget. The scrollback and the screen name at most 3,012 cluster bytes a row at 170 columns, 2.95 times the 1,020 above (`crates/vtium-term/tests/flood.rs` derives it), so the worst case a row grows to 7,560 bytes: 7.21 MiB at 1,000 lines, 72.10 MiB at 10,000, 216.29 MiB at 30,000 and 720.98 MiB at 100,000. At the default 1,000 lines the worst case is about 17.3 MiB, 1.6 MiB over the 15.7 MiB minspec peak, where it was 0.3 MiB under; full shell lines are unchanged. `the_worst_default_scrollback_fits_the_minspec_peak` measures a line as written and does not see it.

> Added 2026-10-28, the operator's decision: the minspec peak figure is 16 MiB, and the scrollback is bounded by its memory as well as its lines. `sb-memory` sets its byte budget, each row costing its struct and its heap; the oldest lines go past it, as past `sb-size`, which can only lower what the budget allows. The default, 5,963,776 bytes, is what 16 MiB leaves after the first screen (about 10.06 MiB, above) and a full link store, rounded down to 64 KiB: about 788 rows at 7,560 bytes, so the worst default scrollback comes to about 15.99 MiB, and 1,000 full-width shell lines take about 1.45 MiB of it. `the_worst_default_scrollback_fits_the_minspec_peak` prices each row at 7,560 bytes and asserts the 16 MiB figure. The budget is fenced from 1 MiB to the lesser of 1 GiB and a quarter of the machine's memory, so at the larger line caps the fence, not the line count, bounds the worst case.

> Added 2026-11-02, with placeholder cells (Placeholder cells, under The line): a placeholder keeps up to 17 bytes outside the budget, so the worst line and the worst row grow, and the table's worst case is the worst line without placeholders. Written at 170 columns, a line whose budget of 1,020 bytes sits on 8 cells and whose other 162 cells are placeholders names 3,774 bytes; with its own style and link in every cell it costs 8,322 bytes, where the table's worst case costs 5,568. A rewrap moves a placeholder with its cell, as it moves every cluster, and a row of at most W cells holds at most 17W bytes of placeholders, so the bound that `crates/vtium-term/tests/flood.rs` derives for a row grows by 17W: 5,902 bytes at 170 columns, where it was 3,012. The worst row `the_worst_default_scrollback_fits_the_minspec_peak` prices grows to 10,450 bytes, where it was 7,560. `sb-memory` still holds the scrollback to its default of 5,963,776 bytes, so the worst default peak stays about 15.99 MiB, inside 16 MiB; that budget now holds about 570 of the worst rows, where it held about 788, and 1,000 full-width shell lines still take about 1.45 MiB of it. Without the placeholder term the flood test's bound fails on placeholders alone at five columns and fewer, where a row of placeholders names 17W, 85 bytes at five columns, against a bound of 84, so the flood test runs placeholder cells among long clusters at five columns as well as at its usual twelve. The table's last row and the cap figures under it therefore no longer give the worst case, and neither do the figures of the note of 2026-10-28: with `sb-memory` raised to hold them, rows of 10,450 bytes cost about 9.97 MiB at 1,000 lines, 99.66 MiB at 10,000, 298.98 MiB at 30,000 and 996.59 MiB at 100,000. With the first screen's 10.06 MiB, the worst case comes to about 109.7 MiB at the 10,000-line cap, 21% of a 512 MB machine, where it was 63.2 MiB; about 309 MiB at the 30,000-line cap, 15% of 2 GiB; and about 1,007 MiB at the 100,000-line cap, 25% of 4 GiB, which the fence of a quarter of the machine's memory still holds.

> Added 2026-10-29: with heads (ADR-1401 D1), each head holds a stack of virtual consoles, and each console's session has a scrollback of its own under `sb-memory`, so the scrollback across every virtual console on minspec needs a total budget. No decision sets it yet: it is an open question for the heads build, beside ADR-1401's probe 6 on the caps of consoles per head and in total.

> Added 2026-10-29: the operator set the caps at 4 consoles a head and 8 in total by default, with hard maxima of 50 consoles and 16 heads (ADR-1401 D10); the heads build's slice H3b sets the total scrollback budget from measurement.

> Added 2026-11-02, the maintainer's decisions (`considered`): the 16 MiB peak holds on minspec's grids, up to the largest, 170 by 48, and `sb-memory`'s default follows the console's grid there. The peak is minspec's promise, not a limit on vtium everywhere. The memory test had priced the two screens as empty, so it missed what a program can make them hold: with every cell of every row its own style, its own link and a cluster, a screen row of 170 cells grew to 14,768 bytes, because each of its tables, which compact at twice what the row can name, sat in a vector that may have doubled past that limit. The two screens then held 1,483,776 bytes at the minspec grid, and the worst peak came to about 17.16 MiB, over 16 MiB. vtium does three things about it.
>
> - **A row's tables stop at the limit they compact at.** The style table and the link table grow by doubling, as a vector does, but stop at twice the row's cells, which is where the row compacts them; the cluster arena stops the same way at twice the row's budget. A row that a rewrap built may name more than that while it waits in the scrollback, and compacting such a row sizes its arena to what its cells name, once; on a screen it is held to the arena's limit (the note of 2026-11-02 after review of the scrollback budget). The most a screen row of W cells holds is then its 48-byte struct, 8 bytes a cell, the tables' 80-byte struct and 2W entries of 14 bytes of style, 4 of link and 6 of arena: 128 + 56W bytes, which is 9,648 at 170 columns, 4,608 at 80 and 2,368 at 40. `Line::bytes_max` gives it, and the memory test drives rows through `write` and through a rewrap to it and no further.
> - **`sb-memory`'s default is worked out from the grid.** It is no longer one fixed number. On a grid up to 170 by 48, vtium takes 16 MiB, less the rest of the process (the C reference's measured 10.3 MiB after its first screen, less its two screens, 10,038 KiB rounded up), less the two screens at their worst at the console's grid, every row at its bound with the change ages, less a full link store, and rounds the result down to 64 KiB, so a smaller grid keeps more. On a larger grid the default is the 170 by 48 value, 5,242,880 bytes (5 MiB), and never less: the 1 MiB floor of the fence no longer decides a 4K or 8K console. The fence applies on top, as before: at least 1 MiB, and at most the lesser of 1 GiB and a quarter of the machine's memory. An explicit `sb-memory` still wins, up to that fence, because the user chose it. When the grid changes, through a hotplug, a resize or a zoom, the default is worked out again from the rows the two screens then hold, which include the rows a screen keeps below its last row after it shrank, and the oldest lines past the new budget go, as when `sb-memory` changes. The 170 by 48 value, where it was 5,963,776, is also what the scrollback starts with and what `sb-memory` may be at most when the machine's memory cannot be read.
> - **The test checks the peak at three grids.** `the_worst_default_scrollback_fits_the_minspec_peak` prices the screens with rows driven to their worst, checks that price against the one the default is worked out from, and holds the worst default scrollback, rows at the bound a rewrap allows, inside 16 MiB at the minspec grid, at 80 by 30 (a 640x480 display in 8x16 cells) and at 40 by 30 (a 320x240 display in 8x8 cells). A unit test and the memory test hold a 3840x2160 display in 8x16 cells, 480 by 135, to the 170 by 48 value.
>
> | Grid | Screens at their worst | `sb-memory` default | Worst rows it holds | Worst peak, before | Worst peak, now |
> |---|---|---|---|---|---|
> | 170 by 48 | 992,256 bytes, where it was 1,483,776 | 5,242,880 bytes | 693 of 7,560 bytes | 17,988,608 bytes (17.16 MiB) | 16,776,192 bytes (16.00 MiB) |
> | 80 by 30 | 296,160 bytes, where it was 455,520 | 5,898,240 bytes | 1,627 of 3,624 bytes | 14,620,576 bytes (13.94 MiB) | 14,461,216 bytes (13.79 MiB) |
> | 40 by 30 | 152,160 bytes, where it was 230,880 | 6,029,312 bytes | 3,213 of 1,876 bytes | 12,647,936 bytes (12.06 MiB) | 12,569,216 bytes (11.99 MiB) |
>
> The smaller grids keep the default 1,000 lines whatever a program writes into them, and at the minspec grid the default 1,000 full-width shell lines take about 1.45 MiB of the 5 MiB, so only the worst rows lose lines there. Placeholder cells raise a row's arena limit once the line keeps them (Placeholder cells, under The line), and `Line::bytes_max` then takes the placeholder term, so the default follows without another rule.
>
> **Above minspec's grids.** The maintainer's three answers make one rule. On minspec's grids, up to 170 by 48, vtium keeps the 16 MiB peak. Above them, usability and performance come before memory footprint, within reason: a console on a larger display holds larger screens and uses more memory than the peak by design, and the maintainer accepts that on machines that can afford it, up to hundreds of megabytes for an 8K display full of images. But the defaults stay modest on ordinary laptops and tablets, so the scrollback default does not grow with the display or with the machine's memory: above 170 by 48 it stays at 5,242,880 bytes a console. A user who wants a deeper scrollback raises `sb-memory`, up to the lesser of 1 GiB and a quarter of the machine's memory. At 480 by 135, for example, the two screens at their worst hold 7,812,720 bytes and the default scrollback 5 MiB, so the worst console comes to about 22.5 MiB with the rest of the process.
>
> The peak counts one console: the rest of the process once, and one console's two screens, scrollback and link store. Each further virtual console holds its own two screens, its own link store and its own scrollback under the same default, so each further console at the minspec grid can hold about 6.2 MiB more: 992,256 bytes of screens, 5,242,880 of scrollback and 262,144 of links. What bounds them together today is ADR-1401 D10's cap, a quarter of the machine's memory shared among the most consoles the heads may hold, and the total budget across consoles is still the heads build's open question (the notes of 2026-10-29).

> Edited 2026-11-02, after a re-check found a screen widened from 28 columns to 33 holding a row of 2,048 bytes against its bound of 1,976: the first point read "A row that a rewrap built may name more than that, and compacting such a row sizes its arena to what its cells name, once."

> Added 2026-11-02, after review of the scrollback budget that follows the grid: a row narrowed from a much wider grid kept every cluster it was written with, because a rewrap moves clusters whole and cutting a row to fewer cells kept the clusters of the cells left. On a grid 4,000 columns wide, 52 lines of 170 cells, each cell with its own color, its own link and a cluster of 125 bytes, then a resize to 170 by 48, left a screen row of 25,968 bytes against the 9,648 of `Line::bytes_max`, and the main screen alone held 1,201,456 bytes, more than the 992,256 both screens are priced at. Each line now records the width its clusters were written within: the width it was written at, or, for a row a rewrap builds, the widest of the rows its clusters came from. The record takes 2 bytes of the line's padding, so the line stays 48 bytes. A row cut to fewer cells, and a row that lands on a screen narrower than that width, from a rewrap or from the scrollback, keep the clusters that writing them at the new width keeps: from the first cell on, a cluster that would pass what is left of the 6 bytes a cell keeps the longest run of its first codepoints that fits, and at least its first codepoint. A row that lands on a screen at least as wide as that width, from a rewrap or from the scrollback, is held the same way to its arena's limit, twice its budget, 12 bytes a cell. A widening by a factor that is not a whole number can gather onto one row the end of one source row, the whole of the next and the start of the one after, and when each of those source rows holds its whole budget in a few cells, the row names up to about 16.5 bytes a cell: widened from 28 columns to 33, such a row named 504 bytes, about 15.3 a cell, and with its own style and link in every other cell it held 2,048 bytes against its bound of 1,976. The scrollback keeps every cluster of each row that rewraps, and `sb-memory` holds it to its bytes; it cuts a row that never rewraps as a screen does. So no screen row passes 128 + 56W bytes, whatever width it was written at, and the figures above do not move. `tests/memory.rs` narrows both screens from 4,000 and from 1,300 columns to 170, widens a screen from 15 to 17, 28 to 33, 53 to 65 and 110 to 129 columns and pulls the widened row back from the scrollback, edits, cuts and widens rows at random, and holds every row to the bound after each step.

> Edited 2026-11-02, after the same re-check: the note held only a row that lands on a narrower screen, and its sentence on the rest read "A widening keeps every cluster, as before, and so does the scrollback, which `sb-memory` holds to its bytes." Its test widened no screen.

> Note 2026-11-02, later, with placeholder cells (Placeholder cells, under The line): a placeholder cluster raises a row's arena limit by 17 bytes for each cell that names one, so a screen row that `write` fills, every cell a placeholder in its own style and link, holds its 48-byte struct, 8 bytes a cell, the tables' 80-byte struct and 2W entries of 14 bytes of style, 4 of link, 6 of arena and 17 of placeholder: 128 + 90W bytes, where the notes above give 128 + 56W. `Line::bytes_max` gives it: 15,428 bytes at 170 columns, 7,328 at 80 and 3,728 at 40, where they were 9,648, 4,608 and 2,368. `sb-memory`'s default follows without another rule. At 170 by 48 the two screens at their worst are 1,547,136 bytes, where they were 992,256; the default is 4,653,056 bytes, where it was 5,242,880; it holds 445 of the worst rows of 10,450 bytes, where it held 693 of 7,560; and the worst peak is 16,741,248 bytes (15.97 MiB), inside 16 MiB. At 80 by 30 the screens are 459,360 bytes, the default 5,767,168 and the worst peak 15,984,416; at 40 by 30 the screens are 233,760 bytes, the default 5,963,776 and the worst peak 13,330,816. Both still hold 1,000 of the worst rows, and 1,000 full-width shell lines take about 33% of the minspec grid's default. The table and the figures above keep the values they were written with; `tests/memory.rs` pins these.

## Consequences

### Positive

- The default scrollback costs about a quarter of the C reference's, and the screen half.
- Every claimant on the C reference's tracker has a place, with bits to spare in the content word, the attributes and the line flags.
- A program cannot grow a line's tables past twice what the line can name, and nothing it writes into one line holds memory in another. The worst a program can write into the default scrollback fits the minspec peak.
- Reflow has the wrapped flag, and image placements and prompt marks have a stable line identity, without either costing a cell anything.
- A region scroll leaves the rest of the screen unchanged for the renderer.

### Negative

- Moving cells between lines, as reflow and rectangle copies do, renumbers their style, link and cluster indices into the destination line's tables.
- A write searches its line's style table, at most twice the line's cells long, so it is linear in the line's width. A program that styles every cell differently pays that search on every write.
- Truecolor art costs about 23 bytes a cell, more than a 16-byte inline cell would, and the worst case 32.
- A line full of long clusters keeps only part of each past its budget of 6 bytes a cell: a line of 85 three-person ZWJ families, 19 bytes each, keeps about 53 of them whole.
- The 10,000-line cap, used in full, passes the C reference's measured peak.

### Neutral

- The cluster arena is per line, so the screen has no shared cluster table, and the C reference's hash seed fallback has no counterpart.
- The screen and scrollback work applies the change-tracking rule in the screen and holds it with its property test of the frame's changed set.
- The three departures this model carries are listed in vtium-term's README.

## Alternatives Considered

- **An inline cell of 16 to 20 bytes**, both colors and the attributes in the cell, the underline color and the link beside them. Rejected: it doubles or more the cost of the plain and shell lines that fill most scrollbacks, and wins only on truecolor art.
- **Style ids into one table the screen shares, with reference counts.** Rejected: 1.7 million cells of truecolor art in a 10,000-line scrollback overflow a 16-bit id, so the table needs a policy for when it is full; every write updates two counts; and lookup needs an ordered map, since `HashMap` is denied (docs/architecture.md, "Determinism"). Ghostty answers the overflow with a style set per memory page; vtium's line is the same idea at the size of a line, with no counts.
- **A change age in each cell**, as the C reference had. Rejected: it doubles the cell, and the scrollback never reads it.
- **Clusters with no budget per line**, bounded only by `CLUSTER_MAX` per cell. Rejected: a line of 32-codepoint clusters held 24.6 KB, so 1,000 such lines passed the minspec peak alone.
- **Packed scrollback lines**, about a byte a character for plain text with runs of styles. Not now: it reaches the 15.7 MiB peak at 10,000 lines, at the cost of a second line form and unpacking as the view scrolls back. It is the step to take if the operator wants the 10,000-line cap inside that peak, or if tier 3 measures the default over its target.
