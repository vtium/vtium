<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DEC locator: DECELR, DECSLE, DECEFR, DECRQLP

| Sequence | What it does |
|---|---|
| `CSI Ps ; Pu ' z` | DECELR: enable locator reports |
| `CSI Pm ' {` | DECSLE: select locator events |
| `CSI Pt ; Pl ; Pb ; Pr ' w` | DECEFR: enable a filter rectangle |
| <code>CSI Ps ' &#124;</code> | DECRQLP: request the locator's position |

## Parameters

- `Ps`: For DECELR, 0 turns locator reports off, 1 turns them on, and 2 turns them on for one report. For DECRQLP, 0, 1 or nothing asks for one report. The default is 0.
- `Pu`: For DECELR, the unit of the reports: 1 for pixels, and 0 or 2 for character cells. The default is 0, character cells.
- `Pm`: For DECSLE, the events to report: 0 only reports the program asks for, 1 and 2 turn reports of a button going down on and off, and 3 and 4 turn reports of a button going up on and off. The default is 0.
- `Pt ; Pl ; Pb ; Pr`: For DECEFR, the filter rectangle's top, left, bottom and right. An omitted one is the locator's position.

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
