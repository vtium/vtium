<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty file transfer (OSC 5113)

| Sequence | What it does |
|---|---|
| `OSC 5113 ; Pt ST` | kitty's file transfer |

## What vtium does

vtium does nothing with this string. It would read and write files on the host at the request of whatever wrote to the terminal.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** it reads and writes host files at the stream's request.
- **Specifications:** kitty file-transfer-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
