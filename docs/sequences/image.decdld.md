<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECDLD soft fonts

| Sequence | What it does |
|---|---|
| `DCS Pm { Pt ST` | DECDLD: load a soft font |

## What vtium does

vtium reads the string and does nothing with it. Its fonts come from outline and bitmap font files.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** soft fonts are a DEC hardware feature that outline and bitmap fonts replace.
- **Specifications:** [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `control_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
