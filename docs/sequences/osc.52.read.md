<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 52 read

| Sequence | What it does |
|---|---|
| `OSC 52 ; Pc ; ? ST` | ask for the clipboard's text |

## What vtium does

vtium sends no reply. The reply would hand the clipboard's text to whatever program reads the terminal's input, which may not be the program the user copied for.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** the reply would hand clipboard text to whatever reads the pty.
- **Note:** docs/architecture.md keeps it off.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `osc_52_query_gets_no_reply` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
