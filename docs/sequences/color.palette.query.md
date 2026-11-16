<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 4 query

| Sequence | What it does |
|---|---|
| `OSC 4 ; c ; ? ST` | ask for palette color c |

## Parameters

- `c`: The palette color's index, from 0 to 255.

## What vtium does

vtium answers `OSC 4 ; c ; rgb:rrrr/gggg/bbbb`, with each byte of the color written twice, and ends the reply with BEL or ST as the query ended. One string can ask for several colors, and each gets a reply of its own.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs read the palette to match their colors to it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_4_reports_palette_colors` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
