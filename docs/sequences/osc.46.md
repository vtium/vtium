<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 46: change the log file

| Sequence | What it does |
|---|---|
| `OSC 46 ; Pt ST` | change the log file |

## Parameters

- `Pt`: The name of the file xterm logs to.

## What vtium does

vtium does nothing with this string. It would write a file on the host at a program's request.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** it writes a file on the host.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
