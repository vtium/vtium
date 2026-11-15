---
contract: adr/v1
kind: decision
verb: change
capability: screen
amends:
  - "ADR-1001#The line"
  - "ADR-1001#Memory"
  - "ADR-1308#D3: how a sized run is stored"
basis:
  - operator: aaronsb
    level: directed
    said: "Design card first, then let's try and compact"
    via: "the steering session's brief for this record, 2026-11-14, relaying the maintainer's words after the repack of the line's tables found the run table uncounted"
  - operator: aaronsb
    level: directed
    said: "let's repack and use memory more efficiently"
    via: "the steering session's brief for this record, 2026-11-14, relaying the maintainer's answer when OSC 66's run table would have raised the per-row budget; the maintainer refused any rise in that budget for OSC 66"
  - standard: "kitty's text sizing protocol (sw.kovidgoyal.net/kitty/text-sizing-protocol/), read 2026-11-14: w is \"The width, in cells, in which the text should be rendered\", 0 to 7; \"The w=1 mechanism allows the program to tell the terminal what width the text should take\"; a program should \"split up the text into chunks that fit in w cells and send one escape code per chunk\", and the page's example sends each character as its own w=1 sequence; \"The use of non-zero w should mainly be restricted to non-ASCII characters and when using fractional scaling\"; the text is at most 4096 bytes; the page sets no limit on the sized blocks one line holds"
  - evidence: "measured by this record's agent on 2026-11-14 at the commit that packs the cluster arena's counts (9b7c0f85, the sized-run work with the repack), on x86-64 and on armv7 under qemu-user: a line is 48 bytes and 32, its tables 80 and 44, a vector's header 24 and 12, a style 14, a link id 4 and a sized run's entry 10 on both; a run table filled one entry at a time grows to 4, 8, 16, 32, 64, 128 and 256 entries; a 170-column screen row with a one-column OSC 66 run, its own style, its own link and a 5-byte cluster in every cell holds 12,232 bytes on x86-64 and 12,168 on armv7, against `Line::bytes_max` of 9,648 and 9,596; at 80 columns 5,912 against 4,608, and at 40 columns 3,032 against 2,368"
  - evidence: "the same commit's tests on armv7 under qemu-user: `the_minspec_grid_and_scrollback` fails, a full-width shell line costing 1,464 bytes where the test pins 1,460, and `without_images_a_placeholder_cluster_keeps_only_what_the_budget_allows` fails, `Line::bytes_max(170)` being 9,596 where the test pins the 64-bit 9,648; `make arm-test` runs aarch64 alone by default, and no gate or CI job runs vtium-term's tests on armv7"
  - evidence: "a prototype by this record's agent, never committed: compaction at one and a half times, the run table held to the line's cells and the exact trigger of D4, on x86-64 in a release build, best of five runs of 60 full-screen frames on a fresh terminal. 170 by 48, truecolor art with new colors in every cell: 278 to 290 ms and 2,784 compactions before, 248 to 263 ms and 5,616 after. 170 by 48, a new link and a three-codepoint cluster in every cell: 682 to 686 ms and 5,376 compactions before, 716 to 744 ms and 9,360 after. 170 by 48, a 16-color redraw: 62 to 66 ms and no compaction, before and after. 40 by 30, a new link and a cluster in every cell: 135 to 136 ms before, 135 to 145 ms after, and 367 ms with 212,640 compactions with today's trigger at one and a half times. The rows the prototype drove reached the new bound and never passed it, at 170, 80 and 40 columns, and `tests/flood.rs` passed"
  - evidence: "the link store (`crates/vtium-term/src/links.rs`): a `LinkId` is the entry's index in the store, renumbered densely by each sweep; the store adds a link only when its charge fits `LINKS_BYTES_MAX`, 262,144 bytes, and charges each link its id and URI bytes and 64 more, so it holds at most 4,096 links"
  - precedent: ADR-1001
  - precedent: ADR-1002
  - precedent: ADR-1308
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1001
  - ADR-1002
  - ADR-1308
  - ADR-1505
---

# ADR-1009: A line's memory bound counts its sized runs, and the line's tables compact at one and a half times what its cells name

## Summary

- **Decided:** `Line::bytes_max`, the most a screen row may cost, counts the row's run table. The run table holds at most one entry for each of the row's cells, since runs on a line never overlap. vtium pays for it without raising the bound. A line's style table, link table and cluster arena compact at one and a half times what its cells can name, where they compacted at twice. A run's entry shrinks from 10 bytes to 6, and a line's link table keeps 16-bit link numbers. A 170-column row's bound falls from 9,648 bytes to 8,142 on x86-64, and from 15,428 to 12,477 in a build with images. The memory tests run on armv7 as well as aarch64, through a new make target that joins the gate line and CI.
- **Trades away:** A line compacts its tables about twice as often. In the prototype that cost up to 9% on a redraw that gives every cell a new link and a cluster, and nothing on ordinary redraws. The scrollback's worst row now counts its runs, so `sb-memory`'s default holds fewer of the worst rows at the minspec grid: 650 where it held 693.
- **One-way?** No. Everything here lives in memory, and nothing on disk or on the wire holds it. The compaction factor and the entry sizes can change again in one release.
- **Probes:**
  - *Confident (no cap below the cells):* a line may hold one sized run in every cell, because kitty's text sizing protocol shows a program sending one w=1 sequence per character. vtium refuses none of them. Right?
  - *Not confident (16-bit links):* the 16-bit link table saves 510 bytes a row. The other savings already pay for the runs, so it is not needed. The author leans to taking it, because the link store can never hold more than 4,096 links. Do you want it now?
  - *Not confident (fewer worst rows):* at the minspec grid the default scrollback holds 650 of the new worst rows, where it held 693 of the old ones. Ordinary shell lines are unchanged, and the default budget itself grows by 128 KiB. Is that acceptable?
- **Inversion:** The choice sits between raising the per-row bound to fit the run table, which the maintainer refused, and capping the runs a line may hold, which would refuse what the protocol asks a program to do. Paying for the table out of the slack the line's other tables keep for compaction avoids both. The slack shrinks, and compaction runs more often.

## Context

ADR-1001 bounds the memory of every screen row. `Line::bytes_max` gives the most a row of W cells may hold: its struct, its cells, and its style table, link table and cluster arena, each at the size where the line compacts it. `sb-memory`'s default is what the minspec peak of 16 MiB leaves after the rest of the process, a full link store, and both screens with every row at that bound. So the peak holds by construction, as long as no screen row passes `Line::bytes_max`.

ADR-1308 D3 gives each line a run table, which holds the line's share of each sized run that OSC 66 writes. The work that repacked the line's tables made the run table's pointer cost nothing: the cluster arena became a boxed slice with 24-bit counts, and the tables struct stayed at 80 bytes on x86-64. But `Line::bytes_max` does not count the table's entries, and nothing limits how many a line holds. The runs on a line never overlap, and a run takes at least one column. So a row of W cells holds at most W runs, and a run table grows by doubling to the next power of two at or above W. At 170 columns that is 256 entries of 10 bytes and a 24-byte vector header: 2,584 bytes past the bound. The measured row costs 12,232 bytes against a bound of 9,648.

The maintainer refused any rise in the per-row budget for OSC 66, and asked for the line's memory to be used more efficiently. The line's tables keep slack for compaction. A table compacts when it reaches twice what the line's cells can name, so up to half of each table is entries no cell names any more. That slack is the largest saving at hand.

Separately, the 32-bit figures that the memory tests pin are out of date. An earlier change, the one that keeps a kitty placeholder cluster's marks outside the budget, added a 32-bit placeholder count to the line's tables. That count costs 4 bytes on armv7, and two pinned tests fail there. Nobody saw it, because the default `make arm-test` runs aarch64 alone, and no gate or CI job runs vtium-term's tests on armv7.

## Decision

### D1: the bound counts the run table

`Line::bytes_max` counts the run table at its most: the table's vector header and one entry for each of the row's cells. No program reaches more, because runs on a line never overlap and each takes at least one column.

- **No cap below the cells.** The protocol sets no limit on the sized blocks a line holds. It asks a program to send one sequence per chunk of text, and its example sends one w=1 sequence per character. So vtium accepts a run in every cell.
- **The table stops at the line's cells.** The run table grows by doubling, as the line's other tables do, but stops at the line's cell count. A line cut to fewer cells, on a screen that narrows or a row a rewrap builds, gives back the run table's room past its new width.
- **The scrollback counts runs too.** A scrollback row already costs its struct and its heap, run table included, so `sb-memory` holds the scrollback to its bytes without a change. The memory test's worst scrollback row gains a run in every cell.

### D2: an entry of 6 bytes

A run's entry keeps its first column in 16 bits and packs the rest into 4 bytes: its width in columns (1 to 49, 6 bits), which row of the run the line holds (0 to 6, 3 bits), and the keys s (3 bits), w (3 bits), n (4 bits), d (4 bits), v (2 bits) and h (2 bits). That is 27 of the 32 bits. The entry is aligned to 2 bytes, so it takes 6 bytes where it took 10. The `SizedRun` that the frame hands the renderer keeps its accessors and reads them from the packed entry.

### D3: tables compact at one and a half times

A line's style table, link table and cluster arena compact at one and a half times what the line's cells can name, rounded up, where they compacted at twice.

- **Styles and links:** the limit is W + ⌈W / 2⌉ entries, and at least W + 2. Right after a compaction a table may still hold one entry for each cell and one for the line's fill, so the limit must leave room for one more.
- **The cluster arena:** the limit is ⌈1.5 × N⌉ bytes, where N is the line's budget of 6 bytes a cell, plus 17 bytes for each cell that names a kitty placeholder cluster in a build with images.
- **A row a rewrap lands on a screen** is held to the same limit, where it was held to twice its budget.
- Every other bound of ADR-1001 stays: the 6 bytes of clusters a cell, the 32 codepoints a cluster, and the 16-bit style and link indices, which a smaller limit only makes safer.

### D4: the arena compacts on the bytes a write adds

The cluster arena compacts when its length plus the bytes of the cluster being written would pass its limit. The overwritten cell gives back its cluster first, so a compaction never keeps it. Today the arena compacts when there is less room than the longest cluster, 129 bytes. At one and a half times, a row of fewer than 43 columns that names its full budget would then compact on every write. The prototype measured 2.95 compactions per cell written at 40 columns, and the redraw took 2.7 times as long. With the exact trigger it compacted 0.079 times per cell, and the redraw took 135 to 145 ms, against 135 to 136 ms today.

### D5: the link table keeps 16-bit numbers

A line's link table keeps each link's number in 16 bits, where it kept the store's 32-bit `LinkId`. The link store holds at most `LINKS_BYTES_MAX` / 64 links, 4,096, because it charges each link at least 64 bytes and adds none past its bound. A link's number is its index in the store, renumbered densely at each sweep, so it stays below 4,096. A compile-time check ties the two: `LINKS_BYTES_MAX` / `ENTRY_OVERHEAD` must stay below 65,536. `LinkId` stays 32 bits outside the line.

### D6: the figures

The bound of a row of W cells is the line's struct, 8 bytes a cell, the tables' struct, g(W) styles of 14 bytes and links of 2, an arena of g(6W) bytes, or g(23W) with images, and a run table of a vector header and 6 bytes a cell. Here g(x) is x + ⌈x / 2⌉. On x86-64 that is 48 + 8W + 80 + 16 g(W) + g(6W) + 24 + 6W. On armv7 the struct, the tables and the header take 32, 44 and 12 bytes.

| Figure | x86-64, before | x86-64, now | armv7, before | armv7, now |
|---|---|---|---|---|
| `Line::bytes_max`, 170 / 80 / 40 columns | 9,648 / 4,608 / 2,368 | 8,142 / 3,912 / 2,032 | 9,596 / 4,556 / 2,316 | 8,078 / 3,848 / 1,968 |
| The same, with images | 15,428 / 7,328 / 3,728 | 12,477 / 5,952 / 3,052 | 15,376 / 7,276 / 3,676 | 12,413 / 5,888 / 2,988 |
| Both screens at their worst, 170 by 48 | 992,256 | 847,680 | 987,264 | 841,536 |
| The same, with images | 1,547,136 | 1,263,840 | 1,542,144 | 1,257,696 |
| `sb-memory`'s default, 170 by 48 / 80 by 30 / 40 by 30 | 5,242,880 / 5,898,240 / 6,029,312 | 5,373,952 / 5,963,776 / 6,094,848 | as x86-64 | as x86-64 |
| The same, with images | 4,653,056 / 5,767,168 / 5,963,776 | 4,915,200 / 5,832,704 / 6,029,312 | as x86-64 | as x86-64 |
| The worst scrollback row at 170 columns | 7,560 | 8,264 | 7,508 | 8,200 |
| The same, with images | 10,450 | 11,154 | 10,398 | 11,090 |
| Worst rows the default holds at 170 by 48 | 693 | 650 | 698 | 655 |
| The same, with images | 445 | 440 | 447 | 443 |
| Worst peak at 170 by 48 | 16,776,192 | 16,762,688 | 16,771,200 | 16,756,544 |
| The same, with images | 16,741,248 | 16,720,096 | 16,736,256 | 16,713,952 |

The "before" columns leave the run table out, so they understate what a row with runs holds. Every worst peak stays inside 16 MiB, 16,777,216 bytes, on each of minspec's grids, in both builds and on both targets. At 80 by 30 and 40 by 30 the default still holds 1,000 of the worst scrollback rows, in both builds and on both targets, as ADR-1001 asks of the smaller grids. With images on x86-64, compaction at one and a half times alone would miss that at 80 by 30: 1,000 rows of 5,808 bytes against a default of 5,767,168. The 6-byte entry or the 16-bit link table each brings it back inside.

### D7: the memory tests run on armv7

`make arm-memory` runs vtium-term's tests for armv7-unknown-linux-gnueabihf under qemu-user, through the wrappers and sysroot that `make arm` uses, once without images and once with all features. It joins the gate line right after `make arm`, and CI's arm-cross job runs it after `make arm`. aarch64 keeps its native run in the arm-native job. The tests pin the 32-bit figures beside the 64-bit ones, so a change that moves either fails on its own target.

## Verification

- vtium-term, `tests/memory.rs`: the worst screen row gains rows of one-column runs, each cell with its own style, its own link and a cluster, and reaches `Line::bytes_max` exactly at 170, 80 and 40 columns. The worst scrollback row gains a run in every cell. The tests pin the figures of D6 for both builds and both targets.
- vtium-term, the bound at every width: the screen-row test sweeps every width from 1 to 48 columns as well as the three grids, so the W + 2 floor of D3 and the trigger of D4 hold at the narrowest widths.
- vtium-term, narrowing: a row of runs written at 170 columns and cut to 40, on the alternate screen and through a rewrap, holds to `Line::bytes_max(40)`.
- vtium-term, the link store: the compile-time check of D5 fails when `LINKS_BYTES_MAX` is raised past 65,535 links' worth of charge.
- Each new or changed check is seen failing on a planted defect: the run table left out of the bound, the table let grow past the cells, the trigger reverted to 129 bytes.
- `make arm-memory` passes on armv7 in both builds.

## Consequences

### Positive

- "No screen row holds more than `Line::bytes_max`" is true again with runs in use, so the 16 MiB peak holds by construction.
- Every screen row's bound falls, by 1,506 bytes at 170 columns and by 2,951 with images. `sb-memory`'s default grows by 128 KiB at the minspec grid, and by 256 KiB with images.
- Interning a new style searches a shorter table, so a redraw that gives every cell new colors runs about 10% faster in the prototype.
- A change that moves a 32-bit figure fails on armv7 in the gate line, where it passed unseen.

### Negative

- Tables compact about twice as often. A redraw with a new link and a cluster in every cell cost 5 to 9% more in the prototype.
- The scrollback's worst row grows by its run table, so the minspec grid's default holds fewer of the worst rows: 650 where it held 693, and 440 where it held 445 with images.
- A run's entry is bit-packed, so reading it takes shifts and masks.
- The gate line gains a run under qemu-user.

### Neutral

- Ordinary shell lines, which hold no runs, cost what they cost today.
- `LinkId` and `SizedRun` keep their public shapes; only the line's storage changes.
- The figures in ADR-1001's notes keep the values they were written with. This record gives the current ones.

## Alternatives Considered

- **Raise the per-row bound by the run table.** Not taken: the maintainer refused any rise in the per-row budget for OSC 66. It would also lower `sb-memory`'s default at the minspec grid by 192 KiB, and with images the default would miss 1,000 worst rows at 80 by 30.
- **Cap the runs a line holds below its cells.** Not taken: the protocol asks a program to send one sequence per character when it states widths, so any cap below the cells would refuse correct output.
- **Store a one-cell run with no fraction as a mark in its cell, with no table entry.** Not taken now: it needs a second cell bit and a second path through every edit, and the run table at 6 bytes a cell already fits the bound.
- **Compact at twice and save elsewhere.** Not taken: 16-bit link numbers, boxed style and link tables, and a boxed cell array save 680, 8 and 8 bytes a row, less than the run table's 1,044.
- **Keep the 10-byte entry.** Not taken: with images on x86-64, the default at 80 by 30 would then hold fewer than 1,000 of the worst rows.
- **Keep today's trigger, room for the longest cluster.** Not taken: at one and a half times it compacts on every write of a narrow row that names its full budget.
- **Run the whole workspace's tests on armv7.** Not taken as a gate: the figures that differ by pointer width live in vtium-term, and qemu-user makes a full run slow. `make arm-test ARM_TEST_TARGETS=armv7-unknown-linux-gnueabihf` still runs it on request.
