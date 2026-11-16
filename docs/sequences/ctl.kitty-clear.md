<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty: move the screen into the scrollback (CSI 22 J)

| Sequence | What it does |
|---|---|
| `CSI 22 J` | move the screen's text into the scrollback, then clear the screen |

## What vtium does

vtium reads the sequence and does nothing with it.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only kitty documents it.
- **Specifications:** kitty misc-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_control_functions_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
