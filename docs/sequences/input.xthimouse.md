<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTHIMOUSE: the program's answer to hilite tracking

| Sequence | What it does |
|---|---|
| `CSI Ps ; Ps ; Ps ; Ps ; Ps T` | XTHIMOUSE: the program's answer to hilite mouse tracking |

## What vtium does

vtium does nothing with this sequence. It refuses hilite mouse tracking, mode 1001, so there is no question for a program to answer. The five parameters keep the sequence apart from SD, which takes one.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** vtium refuses hilite tracking, mode 1001, which it answers.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `key_formats_user_defined_keys_and_hilite_tracking_are_not_taken` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
