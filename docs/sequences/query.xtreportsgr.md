<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTREPORTSGR, XTCHECKSUM, DECRQCRA

| Sequence | What it does |
|---|---|
| <code>CSI Pt ; Pl ; Pb ; Pr # &#124;</code> | XTREPORTSGR: report the SGR of an area |
| `CSI Ps # y` | XTCHECKSUM: choose how DECRQCRA computes its checksum |
| `CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y` | DECRQCRA: report the checksum of an area |

## What vtium does

vtium reads these sequences and sends no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only xterm documents these reports.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_reports_vtium_does_not_give_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
