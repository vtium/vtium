<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 14, 16, 18, 19: text area pixels, cell pixels, text area and screen characters

| Sequence | What it does |
|---|---|
| `CSI 14 t` | XTWINOPS: report the text area's size in pixels |
| `CSI 16 t` | XTWINOPS: report the cell's size in pixels |
| `CSI 18 t` | XTWINOPS: report the text area's size in characters |
| `CSI 19 t` | XTWINOPS: report the screen's size in characters |

## What vtium does

vtium answers `CSI 14 t` with `CSI 4 ; height ; width t` and `CSI 16 t` with `CSI 6 ; height ; width t`, both in pixels. It answers `CSI 18 t` with `CSI 8 ; rows ; columns t`.

The screen is the display the grid fills, so `CSI 19 t` gets the same numbers as `CSI 18 t`, in `CSI 9 ; rows ; columns t`. A report in pixels answers only when vtium knows the cell's size.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask the size of the text area and the cell to place pictures and layouts.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `xtwinops_reports_the_sizes_vtium_has` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `the_size_report_after_a_request_answers_the_size_the_host_made` in [crates/vtium-term/tests/window.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/window.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
