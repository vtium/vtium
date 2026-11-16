<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTQALLOWED, XTQDISALLOWED, XTQALLOWABLE (OSC 60-62)

| Sequence | What it does |
|---|---|
| `OSC 60 ST` | XTQALLOWED: list the allowed features |
| `OSC 61 ; Pt ST` | XTQDISALLOWED: list the disallowed features |
| `OSC 62 ST` | XTQALLOWABLE: list the features that may be allowed |

## What vtium does

vtium reads these strings and sends no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only xterm documents these, and OSC 60 is also contour's font settings.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
