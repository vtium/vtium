<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECST8C, a tab stop every 8 columns

| Sequence | What it does |
|---|---|
| `CSI ? 5 W` | DECST8C: set a tab stop every 8 columns |

## What vtium does

vtium reads the sequence and does nothing with it. RIS sets a tab stop every 8 columns.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only xterm documents it among the terminals, and RIS already restores a stop every 8 columns.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_control_functions_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
