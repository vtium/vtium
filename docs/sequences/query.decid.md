<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECID (ESC Z)

| Sequence | What it does |
|---|---|
| `ESC Z` | DECID: identify the terminal |

## What vtium does

vtium answers DECID as it answers DA1: `CSI ? 62 ; 22 c`, or `CSI ? 62 ; 4 ; 22 c` while it offers sixel images.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** old programs ask what terminal this is with ESC Z.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `da1_reports_a_vt220_with_color` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
