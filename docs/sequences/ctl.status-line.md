<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Status line: DECSSDT, DECSASD

| Sequence | What it does |
|---|---|
| `CSI Ps $ ~` | DECSSDT: select the status line's type |
| `CSI Ps $ }` | DECSASD: select the active display |

## What vtium does

vtium reads these sequences and does nothing with them, since it draws no status line of its own.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium draws no status line of the terminal's own.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_control_functions_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
