<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DCS = 1 s and = 2 s: begin and end a synchronized update

| Sequence | What it does |
|---|---|
| `DCS = 1 s ST` | begin a synchronized update |
| `DCS = 2 s ST` | end a synchronized update |

## What vtium does

The two strings set and reset mode 2026. While an update is open, vtium holds its frames, so the display keeps showing the last whole picture until the program ends the update. vtium also ends an update that stays open longer than `sync-timeout`, 200 ms by default. A `sync-timeout` of 0 turns mode 2026 off.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs hold drawing until an update is whole, as with mode 2026.
- **Note:** the same hold as mode 2026.
- **Specifications:** [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); contour synchronized-output

## Tests

These tests check what vtium does with the sequences on this page.

- `a_dcs_synchronized_update_holds_as_mode_2026` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
