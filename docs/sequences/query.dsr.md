<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DSR 5, DSR 6 (CPR), DECXCPR (CSI ? 6 n)

| Sequence | What it does |
|---|---|
| `CSI 5 n` | DSR: device status report |
| `CSI 6 n` | CPR: report the cursor's position |
| `CSI ? 6 n` | DECXCPR: report the cursor's position and page |

## What vtium does

vtium answers `CSI 5 n` with `CSI 0 n`, which means the terminal works. It answers `CSI 6 n` with `CSI Pr ; Pc R`, the cursor's row and column counted from 1. DECXCPR adds the page, which is always 1: `CSI ? Pr ; Pc ; 1 R`. The page parameter follows the VT510 manual's DECXCPR, since xterm's control sequences give the reply as `CSI ? Pr ; Pc R`.

In origin mode, the row counts from the top of the scrolling region and the column from the left margin. While a wrap is pending, the cursor stands on the last column, and vtium reports that column.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask whether the terminal is ready and where the cursor is.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `dsr_reports_the_status_and_the_cursor` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- [tools/conformance/scenarios/reply-status-reports.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reply-status-reports.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
