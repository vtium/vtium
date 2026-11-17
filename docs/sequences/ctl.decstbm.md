<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSTBM

| Sequence | What it does |
|---|---|
| `CSI Ps ; Ps r` | DECSTBM: set the top and bottom margins |

## Parameters

- `Ps ; Ps`: The top and bottom rows of the scroll region, counted from 1. The default is the whole screen, from the first row to the last.

## What vtium does

DECSTBM sets the scroll region that scrolling, line feeds, and inserting and deleting lines keep within. It then moves the cursor home, which in origin mode is the region's top left corner.

vtium ignores a region whose top is not above its bottom, and keeps the region and the cursor as they were. A bottom margin past the screen's last row stops at that row.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** every full-screen program with a status line scrolls inside a region.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decstbm_sets_the_scroll_region` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- [tools/conformance/vtium/cases/margins-decstbm.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/margins-decstbm.case)

## Where to read more

To follow the work on this row, see issue [#11](https://github.com/vtium/vtium/issues/11).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
