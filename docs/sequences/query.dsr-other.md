<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DSR ? 15, 25, 26, 53, 55, 56, 62, 63, 75, 85: printer, UDK, keyboard, locator, macro, checksum, session reports

| Sequence | What it does |
|---|---|
| `CSI ? 15 n` | printer status |
| `CSI ? 25 n` | user-defined keys status |
| `CSI ? 26 n` | keyboard status |
| `CSI ? 55 n` | locator status |
| `CSI ? 56 n` | locator type |
| `CSI ? 62 n` | macro space report |
| `CSI ? 63 n` | memory checksum report |
| `CSI ? 75 n` | data integrity report |
| `CSI ? 85 n` | multiple-session status |

## What vtium does

vtium reads these queries and sends no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has no printer, user-defined keys, locator or the other things these report on.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_other_device_status_reports_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
