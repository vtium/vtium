<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DEC locator: DECELR, DECSLE, DECEFR, DECRQLP

| Sequence | What it does |
|---|---|
| `CSI Ps ; Pu ' z` | DECELR: enable locator reports |
| `CSI Pm ' {` | DECSLE: select locator events |
| `CSI Pt ; Pl ; Pb ; Pr ' w` | DECEFR: enable a filter rectangle |
| <code>CSI Ps ' &#124;</code> | DECRQLP: request the locator's position |

## What vtium does

vtium reads these sequences, changes nothing and sends no reply. A program that wants the pointer's position turns on mouse reporting through the mouse modes, which [Modes](../modes.md) lists.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium reports the mouse through the mouse modes, which programs use instead.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_reports_vtium_does_not_give_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
