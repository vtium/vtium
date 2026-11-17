<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECREQTPARM, DECRQDE, DECRQUPSS, DECRQTSR

| Sequence | What it does |
|---|---|
| `CSI Ps x` | DECREQTPARM: report the serial line's parameters |
| `CSI " v` | DECRQDE: report the window's extent |
| `CSI & u` | DECRQUPSS: report the user-preferred character set |
| `CSI Ps $ u` | DECRQTSR: report the terminal's state |

## Parameters

- `Ps`: For DECREQTPARM, 0 or 1, which says whether the terminal may send the report unasked. For DECRQTSR, which state to report: 1 for the terminal's whole state, and 2 for its color table. The default is 0 for DECREQTPARM.

## What vtium does

vtium reads these sequences and sends no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** these report a serial line and a terminal's own settings, which a console does not have.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_reports_vtium_does_not_give_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
