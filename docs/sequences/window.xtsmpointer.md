<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTSMPOINTER (CSI > Ps p): hide the pointer while typing

| Sequence | What it does |
|---|---|
| `CSI > Ps p` | XTSMPOINTER: hide the pointer while typing |

## Parameters

- `Ps`: 0 never hides the pointer, 1 hides it while no mouse tracking mode is set, and 2 and 3 always hide it.

## What vtium does

vtium reads the sequence and does nothing. It follows its own rule for hiding the pointer while you type.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has its own rule for hiding the pointer while typing.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `title_modes_the_pointer_and_extra_cursors_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
