<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Tabs: HTS, TBC, CHT, CBT

| Sequence | What it does |
|---|---|
| `ESC H` | HTS: set a tab stop at the cursor's column |
| `CSI Ps g` | TBC: clear tab stops |
| `CSI Ps I` | CHT: forward to a following tab stop |
| `CSI Ps Z` | CBT: back to a preceding tab stop |

## Parameters

- `Ps`: For TBC, 0 clears the stop at the cursor's column and 3 clears every stop, and vtium ignores other values. For CHT and CBT, how many stops to move, where 0 counts as 1. The default is 0 for TBC, and 1 for CHT and CBT.

## What vtium does

vtium starts with a tab stop every 8 columns. HT, CHT and CBT move the cursor to the next or previous stop, and HT stops at the right margin when no stop is left. The 8-bit control 0x88 works as HTS.

DECSTR keeps the tab stops, and RIS returns them to a stop every 8 columns.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs set and use tab stops to line up columns.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/); [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `tab_stops_are_set_cleared_and_followed` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- [tools/conformance/vtium/cases/tabs-cht-cbt-tbc.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/tabs-cht-cbt-tbc.case)
- [tools/conformance/vtium/cases/tabs-hts.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/tabs-hts.case)

## Where to read more

To follow the work on this row, see issue [#11](https://github.com/vtium/vtium/issues/11).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
