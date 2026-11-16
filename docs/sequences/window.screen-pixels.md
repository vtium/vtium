<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 15: screen size in pixels

| Sequence | What it does |
|---|---|
| `CSI 15 t` | XTWINOPS: report the screen's size in pixels |

## What vtium does

vtium answers `CSI 5 ; height ; width t` with the real size, in pixels, of the display the grid fills. It answers only when it knows that size.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask the size of the display the grid fills.
- **Note:** the real size of the display the grid fills.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `xtwinops_reports_the_sizes_vtium_has` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
