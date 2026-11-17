<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECUDK (DCS Ps ; Ps | ...): user-defined keys

| Sequence | What it does |
|---|---|
| <code>DCS Ps ; Ps &#124; Pt ST</code> | DECUDK: define user-defined keys |

## What vtium does

vtium does nothing with this string, and every key keeps sending what it sent before. A key that program output could define would later type that output into the shell when the user presses the key.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** program output would program a key that later types into the shell.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `key_formats_user_defined_keys_and_hilite_tracking_are_not_taken` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
