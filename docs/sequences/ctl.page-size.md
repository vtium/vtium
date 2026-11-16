<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Page size: DECSCPP, DECSLPP, DECSNLS

| Sequence | What it does |
|---|---|
| <code>CSI Ps $ &#124;</code> | DECSCPP: set columns per page |
| `CSI Ps t` | DECSLPP: set lines per page, for Ps of 24 or more |
| <code>CSI Ps * &#124;</code> | DECSNLS: set lines per screen |

## Parameters

- `Ps`: For DECSCPP, the columns per page: 80 or 132, where 0 means 80. For DECSLPP, the lines per page, 24 or more. A smaller value is an XTWINOPS operation, on the page window.manipulate. For DECSNLS, the lines on the screen. The default is 80 for DECSCPP.

## What vtium does

vtium reads these sequences and does nothing with them. The screen's size comes from the display and the font.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** DECCOLM is refused, and the page size comes from the display and the font.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_control_functions_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
