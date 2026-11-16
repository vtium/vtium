<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 5522: kitty's multi-type clipboard, and mode 5522 paste events

| Sequence | What it does |
|---|---|
| `OSC 5522 ; Pt ST` | kitty's clipboard with several types |

## What vtium does

vtium reads the string and does nothing with it, and a read gets no reply. Its reads would hand clipboard data to the program.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** its reads hand clipboard data to the program.
- **Specifications:** kitty clipboard

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
