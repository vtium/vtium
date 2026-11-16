<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 22, 23: title stack

| Sequence | What it does |
|---|---|
| `CSI 22 ; Ps t` | XTWINOPS: push the title on a stack |
| `CSI 23 ; Ps t` | XTWINOPS: pop the title from the stack |

## Parameters

- `Ps`: Which title: 0 both the icon name and the window title, 1 the icon name, and 2 the window title. An optional third parameter from 1 to 10 names a stack entry to store into or restore from. The default is 0.

## What vtium does

vtium keeps no title, so it reads these sequences and does nothing.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium keeps no title to push or pop.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd)

## Tests

These tests check what vtium does with the sequences on this page.

- `window_reports_vtium_has_no_window_for_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
