<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTSHIFTESCAPE (CSI > Ps s): may Shift reach a mouse report

| Sequence | What it does |
|---|---|
| `CSI > Ps s` | XTSHIFTESCAPE: let Shift reach mouse reports |

## Parameters

- `Ps`: 1 lets a pointer event with Shift held reach the program's mouse reports, and 0 keeps Shift for the user's own selection. The default is 0.

## What vtium does

With XTSHIFTESCAPE 0, a click or a drag with Shift held selects and pastes text even while a program tracks the mouse. With XTSHIFTESCAPE 1, vtium reports that event to the program instead. DECSTR keeps the setting, and RIS returns it to 0.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program lets Shift reach its mouse reports, or keeps Shift for the user's selection.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); Ghostty xtshiftescape page

## Tests

These tests check what vtium does with the sequences on this page.

- `xtshiftescape_says_whether_shift_reaches_the_mouse_reports` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)
- `a_program_that_asks_for_shift_gets_it_in_its_reports` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issue [#54](https://github.com/vtium/vtium/issues/54).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
