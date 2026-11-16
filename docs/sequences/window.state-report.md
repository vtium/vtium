<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 11, 13, 14;2: iconified state, window position, window pixels

| Sequence | What it does |
|---|---|
| `CSI 11 t` | XTWINOPS: report whether the window is iconified |
| `CSI 13 t` | XTWINOPS: report the window's position |
| `CSI 14 ; 2 t` | XTWINOPS: report the window's size in pixels |

## What vtium does

vtium has no window, so it reads these queries and sends no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has no window whose state or position it could report.
- **Note:** no reply.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `window_reports_vtium_has_no_window_for_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
