<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTSAVE and XTRESTORE (CSI ? Pm s, r), with kitty's empty list meaning every mode

| Sequence | What it does |
|---|---|
| `CSI ? Pm s` | XTSAVE: save DEC private modes |
| `CSI ? Pm r` | XTRESTORE: restore DEC private modes |

## Parameters

- `Pm`: The numbers of the DEC private modes to save or restore. The default is every mode that is free of side effects.

## What vtium does

XTSAVE saves the state of each mode it lists, and XTRESTORE sets each mode back to its saved state. A mode that nothing was saved for stays as it is.

With an empty list, the two act on every supported DEC private mode except DECOM, the alternate screen modes 47, 1047 and 1049, mode 1048 and mode 2026. So the empty list never moves the cursor or switches screens. DECSTR keeps what XTSAVE saved, and RIS clears it.

[Modes](../modes.md) says which modes share a saved state.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs save the modes they change and put them back as they leave.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); kitty misc-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `decstr_keeps_what_xtsave_saved_and_ris_clears_it` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `the_empty_list_leaves_the_cursor_and_the_screen_alone` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `the_empty_list_saves_and_restores_every_mode` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `xtrestore_restores_each_mode_xtsave_saved` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `xtsave_keeps_one_state_for_modes_that_share_one` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)

## Where to read more

To follow the work on this row, see issue [#54](https://github.com/vtium/vtium/issues/54).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
