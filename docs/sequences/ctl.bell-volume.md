<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSWBV, DECSMBV: bell volumes

| Sequence | What it does |
|---|---|
| `CSI Ps SP t` | DECSWBV: set the warning bell's volume |
| `CSI Ps SP u` | DECSMBV: set the margin bell's volume |

## Parameters

- `Ps`: The volume. For DECSWBV, 0 and 1 are off, 2 to 4 low, and 5 to 8 high. For DECSMBV, 1 is off, 2 to 4 low, and 0 and 5 to 8 high.

## What vtium does

vtium reads these sequences and does nothing with them. vtium rings the VT bell, whose volume the kernel and the hardware set.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium rings the VT bell, whose volume the kernel and the hardware set.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_control_functions_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
