<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSTR and RIS; DECSTR resets the mouse modes

| Sequence | What it does |
|---|---|
| `CSI ! p` | DECSTR: soft terminal reset |
| `ESC c` | RIS: reset to initial state |

## What vtium does

DECSTR returns the modes, the pen, the cursor's style, the character sets, the saved cursors, the top and bottom margins, and the 256 palette colors to their initial state. The mouse modes and mode 1004 are among the modes it resets. It keeps the text, the cursor's position, the tab stops, and the left and right margins.

RIS does all of that, then erases the screen and the scrollback and moves the cursor home. It also sets a tab stop every 8 columns, returns every color a program set, and empties the SGR and color stacks.

DECSTR turns autowrap on, where the VT510 manual's DECSTR table turns it off ("No autowrap"). Terminfo entries that say the terminal wraps send DECSTR to reset it, so autowrap has to stay on. See [Resets in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#resets).

## Status

- **Status:** supported. vtium implements this as the specifications below define it, apart from the departure above.
- **Why:** programs and the reset command put the terminal back in a known state.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decstr_and_ris_return_the_paragraph_to_its_defaults` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)
- `decstr_and_ris_reset_the_character_sets` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `decstr_keeps_the_tab_stops_and_ris_resets_them` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `decstr_leaves_autowrap_on` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `decstr_returns_the_modes_to_their_defaults` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `ris_erases_and_goes_home` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- [tools/conformance/scenarios/reset-decstr.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reset-decstr.scenario)
- [tools/conformance/scenarios/reset-ris.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reset-ris.scenario)
- [tools/conformance/scenarios/reset-state.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reset-state.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
