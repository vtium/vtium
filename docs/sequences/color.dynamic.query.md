<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 10 and 11 query

| Sequence | What it does |
|---|---|
| `OSC 10 ; ? ST` | ask for the default foreground |
| `OSC 11 ; ? ST` | ask for the default background |

## What vtium does

vtium answers `OSC 10 ; rgb:rrrr/gggg/bbbb` or `OSC 11 ; rgb:rrrr/gggg/bbbb` with the color in use, ended as the query was. A program can read the background this way to tell a dark screen from a light one.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask for the foreground and background to tell a dark screen from a light one.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_10_and_11_report_the_default_colors` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
