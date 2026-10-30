---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: directed
    said: "It bounds scrollback by the machine's memory with the operator's caps (2026-10-26): `sb-size` is accepted up to 10,000 lines below 2 GiB, 30,000 from 2 GiB, and 100,000 from 4 GiB."
    via: vtium/vtium#10, the issue body
  - upstream: "kmscon ADR-304 §4 and §5 (aaronsb/kmscon): the cell model is designed fresh, budgeted for reflow's wrapped-line flag, hyperlink ids, overline and conceal, and measured against the minspec targets: resident after the first screen at most 10.3 MiB with unifont, at peak with the scrollback full at most 15.7 MiB"
  - upstream: "kmscon ADR-102 (aaronsb/kmscon): cells carry clusters and unresolved colour; the C reference's cell was 32 bytes, held there by a static assert, with an 8-byte change age; clusters keep at most 32 codepoints"
  - upstream: "kmscon ADR-103 (aaronsb/kmscon): an image placement is anchored to the line holding its top row and travels with it; kmscon's README, \"Minimal build\": the 15.7 MiB peak held a full 1,000-line scrollback"
  - upstream: "aaronsb/libtsm#34 (a region scroll flagged every cell), #35 (no mark for the default background), #47 (padding drawn with the pen), #22 (the cursor color), #28 (no continuation flag for reflow)"
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
- **One-way?** No. Reversible now; expensive once vtium/vtium#11 builds the screen and vtium/vtium#41 builds reflow on these types.
- **Probes:**
  - *Confident (default memory):* a console left at the default 1,000 lines of scrollback spends about 1.5 MiB on it, against the C reference's 5.2 MiB, so the minspec build keeps about 4 MiB of headroom under its 15.7 MiB peak.

  > Note 2026-10-26: the 4 MiB holds for full shell lines. The worst a program can write into the default scrollback leaves about 0.3 MiB (Memory).

  - *Not confident (the cap):* that on a machine under 2 GiB, setting `sb-size` to the 10,000-line cap may cost about 14.5 MiB of full shell lines, and at worst 53 MiB, when a program gives every cell its own colors, its own link and a long combining cluster; at the 100,000-line cap, from 4 GiB, that worst case is 531 MiB. Squeezing plain text to about a byte a character is possible, at the cost of more code and of unpacking lines as they are scrolled back to.

  > Edited 2026-10-26: review found that clusters, not truecolor art, were the worst case, with no bound on a line's cluster bytes (24.6 KB a line), and the arena is now bounded. This probe read: "that on a machine under 2 GiB, setting `sb-size` to the 10,000-line cap may cost about 15 MiB of full shell lines, and up to 37 MiB if the scrollback fills with pictures printed as coloured text. Squeezing it to about a byte a character for plain text is possible, at the cost of more code and of unpacking lines as they are scrolled back to."
- **Inversion:** Between "a fat cell that holds everything inline" (the C reference's 32 bytes) and "a tiny cell indexing tables the whole screen shares" (style ids with reference counts). The answer is the tiny cell with its tables owned by its line: no shared table to count references in, hash, or fill, and a line that leaves the scrollback takes everything it used with it.

## Context

vtium designs its cell model fresh (kmscon ADR-304 §4) before the screen is built on it (vtium/vtium#11). The C reference's cell was 32 bytes, held there by a static assert: the codepoint, a 4-byte width holding 0 to 2, a 12-byte attribute struct, and an 8-byte change age. Its tracker queued claimants the cell had no room for: overline, superscript and subscript, conceal, a hyperlink id, a mark for the default background; per line, a continuation flag for reflow, prompt marks, bidirectional state and image anchors; per frame, the cursor's shape and color.

The minspec machine (1366x768, 512 MB) sets the budget. In unifont's 8x16 cells the screen is 170 by 48. The C reference's measured peak, 15.7 MiB with unifont, held its default 1,000-line scrollback after 500,000 lines of text; its README measured only the totals. Derived from its layout, those 1,000 lines were 5.19 MiB of 32-byte cells, and its main and alternate screens 0.50 MiB. The operator's caps let a machine under 2 GiB keep ten times that many lines.

> Added 2026-10-27, after minspec became a single-board computer (docs/architecture.md, vtium/vtium#75). Its display floor is a 320x240 panel, 40x30 in an 8x8 font, and 1366x768 stays tier 3's x86 variant; the budget below still measures the 170 by 48 screen, the largest minspec grid. The byte figures here are 64-bit layouts. On armv7 they are smaller (the screens 265,728 bytes against 267,264, a worst-case line 5,512 against 5,568), and `tests/memory.rs` pins each per pointer width.

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

A line owns its cells and three tables: styles, hyperlinks (each entry a link id into the link store vtium/vtium#40 builds) and clusters. Clusters sit in one arena per line, each as a length byte and its codepoints in UTF-8, at most `CLUSTER_MAX`, 32, codepoints; the cell holds the cluster's byte offset. A line with only default, unlinked, single-codepoint cells allocates no tables.

The clusters a line's cells name take at most `CLUSTER_BYTES_PER_CELL`, 6, bytes per cell of the width the line was written at: a base and two combining marks a cell on average. A cluster that would pass the budget keeps the longest run of its first codepoints that fits, and at least its first codepoint; overwriting or erasing a cluster gives its bytes back. Ordinary text spends little of it: an emoji with a skin tone or a flag takes 9 bytes across its two cells, a letter with an accent 4.

> Added 2026-10-26, after review measured a line of 32-codepoint clusters at 24.6 KB, 26.4 MiB at the default 1,000 lines. The bound holds the worst case at the default inside the minspec peak (Memory).

> Edited 2026-10-26, after review of vtium/vtium#11: a line trimmed for the scrollback keeps the clusters its cells name, which may pass the budget of its fewer cells but never that of the width it was written at, so the memory bound holds. This paragraph read "bytes per cell of the line".

A write looks its style and link up in the line's table, newest entry first, and appends them if absent, so a write is linear in the line's width at worst. Once the style or link table reaches twice the line's cells, or the arena twice the line's cluster budget, the line compacts: it keeps the entries a cell still names and renumbers the cells. A compaction costs time linear in the line's width, as a lookup does, and no table holds more than twice what its line can name, whatever a program writes. Shrinking a line compacts it too. `COLUMNS_MAX` (8192) keeps twice the widest line inside a 16-bit index, and twice its cluster budget inside the cell's 21 bits; compile-time assertions hold both. Nothing is shared between lines, so no table needs a reference count or a hash.

A line has a `LineId`, unique within its screen buffer for the buffer's life and never reused, and 16 bits of flags:

- **Wrapped:** the line's text continues on the next line, because printing wrapped at the right margin. A line the program ended with a newline, or left by moving the cursor, does not carry it. Reflow (vtium/vtium#41) rejoins only wrapped lines.
- **Prompt, input, output:** OSC 133's A, B and C marks start on this line (vtium/vtium#40).
- **Line size:** single, double width, or the top or bottom of double height, in two bits (vtium/vtium#61).
- Ten bits are spare for bidirectional state (vtium/vtium#43).

A line may hold fewer cells than the screen is wide. A line entering the scrollback is trimmed: it drops its trailing blank cells, compacts, and gives back spare capacity. Every cell past a line's end reads as the blank cell in the default style, never the pen (aaronsb/libtsm#47).

### Where each claimant lives

| Claimant | Where | Issue that uses it |
|---|---|---|
| The glyph or grapheme cluster | Cell content; the line's cluster arena | vtium/vtium#11 |
| The width | Cell content, bits 22-23 | vtium/vtium#11 |
| Overline, conceal, rapid blink | Style attributes | vtium/vtium#37 |
| Superscript and subscript | Style attributes, the script | vtium/vtium#37 |
| A hyperlink id | Cell link index; the line's link table | vtium/vtium#40 |
| The default background mark | The style's background is `Color::Default`; the frame cell's `default_bg` | aaronsb/libtsm#35 |
| The change age | `Damage`, by screen position | vtium/vtium#11 |
| The continuation flag | Line flags, wrapped | vtium/vtium#41 |
| Prompt marks | Line flags, prompt, input and output | vtium/vtium#40 |
| Bidirectional state | Line flags, ten spare bits | vtium/vtium#43 |
| Double width and height | Line flags, line size | vtium/vtium#61 |
| Image anchors | The `LineId` | vtium/vtium#28 |
| The cursor's shape and color | The frame's `Cursor` | vtium/vtium#12, #38 |
| Change tracking renderers read | The frame cell's `changed`, the frame's `row_changed` | vtium/vtium#16 |

### Change tracking

`Damage` holds a change age for every screen position and every row, and one for the whole screen. Each mark stamps the current age. A display takes an age when it draws, and a position has changed for it when its stamp is newer, so each of several displays keeps its own. What marks what:

- writing or erasing cells marks those cells;
- scrolling marks the rows that moved: inside margins, the region's rows only (aaronsb/libtsm#34), and for the whole screen, every row;
- a resize, a palette change, reverse video (DECSCNM), switching between the main and alternate screens, and moving the view through the scrollback mark everything;
- an image placement that is added, moved, clipped or removed, or whose image changes, marks the cells under its old and its new box;
- moving the cursor marks nothing, since the frame carries the cursor and a renderer redraws the cells it left and covers;
- a pen change alone (SGR) marks nothing.

Damage tracks at most `COLUMNS_MAX` (8192) columns by `ROWS_MAX` (2048) rows, and a larger size is clamped, so a resize the stream asks for (vtium/vtium#44) cannot allocate without limit.

### The frame

A frame is the visible lines, the damage, the palette, the age the reading display last drew at, the cursor and DECSCNM. For each cell it gives the glyph (a codepoint or a cluster's codepoints), the width, the foreground and background resolved through the palette and inverse video, the underline color SGR 58 set (resolved, and kept through inverse video) or none to follow the configured one, the attributes, whether the background is the default one and not inverted, the link id, and whether the cell changed for this display. The cursor carries its position, visibility, DECSCUSR shape and blink, and OSC 12's color. Resolving at draw time is what lets a palette change recolor text already on the screen (ADR-1000's row "Colors resolved at draw time").

Bold as bright and dim join the resolution with vtium/vtium#83, the selection with vtium/vtium#11, and the placement list with vtium/vtium#28. vtium-image anchors a placement by the `LineId` of the line holding its top row and marks its boxes through `Damage`; the line holds nothing for images.

### Scrollback size

`sb-size` defaults to 1,000 lines, the C reference's default. By default it is accepted up to 10,000 lines below 2 GiB of memory, 30,000 from 2 GiB and 100,000 from 4 GiB. A cap the configuration sets replaces that default, higher or lower, and is clamped to `SB_SIZE_MAX`, 100,000 lines; its key is wired with the configuration (vtium/vtium#21). A larger `sb-size` is clamped to the cap.

> Added 2026-10-26: the operator, on the corrected worst case, said "10000 lines is fine but it should be configurable". The caps are rows of vtium-term's bounds table.

### Memory

Computed from the structures by capacity in `crates/vtium-term/tests/memory.rs`, which pins each figure. A line costs its 48-byte struct and its heap. The main and alternate screens, 170 by 48 lines each with their damage, are 267,264 bytes (0.25 MiB), against the C reference's 0.50 MiB.

| Scrollback content | Bytes a line | 1,000 lines | 10,000 lines | 30,000 lines | 100,000 lines |
|---|---|---|---|---|---|
| Plain text, full width | 1,408 | 1.34 MiB | 13.43 MiB | 40.28 MiB | 134.28 MiB |
| A shell's lines (three styles), full width | 1,516 | 1.45 MiB | 14.46 MiB | | |
| A shell's lines, 60 cells | 636 | 0.61 MiB | 6.07 MiB | | |
| Truecolor art, every cell its own colors, full width | 3,868 | 3.69 MiB | 36.89 MiB | | |
| The worst case: every cell its own style, its own link and a 32-codepoint cluster, the cluster budget full | 5,568 | 5.31 MiB | 53.10 MiB | 159.30 MiB | 531.01 MiB |

The C reference's figures are its measured totals, 10.3 MiB after the first screen and 15.7 MiB at peak with unifont, and what its layout gives: 0.50 MiB for its two screens and 5.19 MiB for 1,000 lines of scrollback. Assuming everything else in vtium's process costs what everything else in the C reference's did, 9.80 MiB:

- **After the first screen**, about 10.06 MiB, against the target of 10.3 MiB.
- **At the default 1,000 lines**, about 11.5 MiB full of full-width shell lines, 4.2 MiB under the 15.7 MiB peak target, and about 15.4 MiB in the worst case, 0.3 MiB under. `the_worst_default_scrollback_fits_the_minspec_peak` checks the worst case against the peak.
- **At the 10,000-line cap**, below 2 GiB, about 24.5 MiB full of full-width shell lines and 63.2 MiB in the worst case, 12% of a 512 MB machine. The 15.7 MiB target was measured with 1,000 lines, so the cap passes it; meeting it at 10,000 lines would take about 3.4 bytes a cell, which only packing the text reaches (Alternatives).
- **At the 30,000-line cap**, from 2 GiB, the worst case is about 169 MiB, 8% of 2 GiB; **at the 100,000-line cap**, from 4 GiB, about 541 MiB, 13% of 4 GiB.

Tier 3 measures the real figures from the first working build (kmscon ADR-304 §5).

> Added 2026-10-28, after ADR-1003, and corrected the same day: a rewrap moves clusters with their cells, so a rewrapped row may name more than its own cells' budget. The scrollback and the screen name at most 3,012 cluster bytes a row at 170 columns, 2.95 times the 1,020 above (`crates/vtium-term/tests/flood.rs` derives it), so the worst case a row grows to 7,560 bytes: 7.21 MiB at 1,000 lines, 72.10 MiB at 10,000, 216.29 MiB at 30,000 and 720.98 MiB at 100,000. At the default 1,000 lines the worst case is about 17.3 MiB, 1.6 MiB over the 15.7 MiB minspec peak, where it was 0.3 MiB under; full shell lines are unchanged. `the_worst_default_scrollback_fits_the_minspec_peak` measures a line as written and does not see it.

> Added 2026-10-28 (vtium/vtium#131), the operator's decision: the minspec peak figure is 16 MiB, and the scrollback is bounded by its memory as well as its lines. `sb-memory` sets its byte budget, each row costing its struct and its heap; the oldest lines go past it, as past `sb-size`, which can only lower what the budget allows. The default, 5,963,776 bytes, is what 16 MiB leaves after the first screen (about 10.06 MiB, above) and a full link store, rounded down to 64 KiB: about 788 rows at 7,560 bytes, so the worst default scrollback comes to about 15.99 MiB, and 1,000 full-width shell lines take about 1.45 MiB of it. `the_worst_default_scrollback_fits_the_minspec_peak` prices each row at 7,560 bytes and asserts the 16 MiB figure. The budget is fenced from 1 MiB to the lesser of 1 GiB and a quarter of the machine's memory, so at the larger line caps the fence, not the line count, bounds the worst case.

> Added 2026-10-29: with heads (ADR-1401 D1), each head holds a stack of virtual consoles, and each console's session has a scrollback of its own under `sb-memory`, so the scrollback across every virtual console on minspec needs a total budget. No decision sets it yet: it is an open question for the heads build, beside ADR-1401's probe 6 on the caps of consoles per head and in total.

> Added 2026-10-29: the operator set the caps at 4 consoles a head and 8 in total by default, with hard maxima of 50 consoles and 16 heads (ADR-1401 D10); the heads build's slice H3b sets the total scrollback budget from measurement.

## Consequences

### Positive

- The default scrollback costs about a quarter of the C reference's, and the screen half.
- Every claimant on the C reference's tracker has a place, with bits to spare in the content word, the attributes and the line flags.
- A program cannot grow a line's tables past twice what the line can name, and nothing it writes into one line holds memory in another. The worst a program can write into the default scrollback fits the minspec peak.
- Reflow (vtium/vtium#41) has the wrapped flag, and image placements (vtium/vtium#28) and prompt marks (vtium/vtium#40) have a stable line identity, without either costing a cell anything.
- A region scroll leaves the rest of the screen unchanged for the renderer.

### Negative

- Moving cells between lines, as reflow and rectangle copies do, renumbers their style, link and cluster indices into the destination line's tables.
- A write searches its line's style table, at most twice the line's cells long, so it is linear in the line's width. A program that styles every cell differently pays that search on every write.
- Truecolor art costs about 23 bytes a cell, more than a 16-byte inline cell would, and the worst case 32.
- A line full of long clusters keeps only part of each past its budget of 6 bytes a cell: a line of 85 three-person ZWJ families, 19 bytes each, keeps about 53 of them whole.
- The 10,000-line cap, used in full, passes the C reference's measured peak.

### Neutral

- The cluster arena is per line, so vtium/vtium#11 has no shared cluster table, and the C reference's hash seed fallback (aaronsb/libtsm#41) has no counterpart.
- vtium/vtium#11 applies the change-tracking rule in the screen and holds it with its property test of the frame's changed set.
- The three departures this model carries (aaronsb/libtsm#34, #35 and #47) are listed in vtium-term's README.

## Alternatives Considered

- **An inline cell of 16 to 20 bytes**, both colors and the attributes in the cell, the underline color and the link beside them. Rejected: it doubles or more the cost of the plain and shell lines that fill most scrollbacks, and wins only on truecolor art.
- **Style ids into one table the screen shares, with reference counts.** Rejected: 1.7 million cells of truecolor art in a 10,000-line scrollback overflow a 16-bit id, so the table needs a policy for when it is full; every write updates two counts; and lookup needs an ordered map, since `HashMap` is denied (docs/architecture.md, "Determinism"). Ghostty answers the overflow with a style set per memory page; vtium's line is the same idea at the size of a line, with no counts.
- **A change age in each cell**, as the C reference had. Rejected: it doubles the cell, and the scrollback never reads it.
- **Clusters with no budget per line**, bounded only by `CLUSTER_MAX` per cell. Rejected: a line of 32-codepoint clusters held 24.6 KB, so 1,000 such lines passed the minspec peak alone.
- **Packed scrollback lines**, about a byte a character for plain text with runs of styles. Not now: it reaches the 15.7 MiB peak at 10,000 lines, at the cost of a second line form and unpacking as the view scrolls back. It is the step to take if the operator wants the 10,000-line cap inside that peak, or if tier 3 measures the default over its target.
