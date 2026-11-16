<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 9;4: progress

| Sequence | What it does |
|---|---|
| `OSC 9 ; 4 ; Ps ; Ps ST` | report progress |

## Parameters

- `Ps ; Ps`: The state, then the progress from 0 to 100. The state is 0 to remove the progress, 1 to set it, 2 for an error, 3 for progress of unknown length, and 4 for paused.

## What vtium does

vtium reads the string and does nothing with it.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** a VT has no taskbar or tab to show progress in.
- **Specifications:** contour osc-9-4-progress; Ghostty conemu page

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
