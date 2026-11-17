<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DA1 (CSI c)

| Sequence | What it does |
|---|---|
| `CSI Ps c` | DA1: primary device attributes |

## Parameters

- `Ps`: Must be 0. vtium answers no other value. The default is 0.

## What vtium does

vtium answers `CSI ? 62 ; 22 c`: a VT220-level terminal (62) with ANSI color (22). While vtium offers sixel images, it adds attribute 4 and answers `CSI ? 62 ; 4 ; 22 c`.

DECID and the 8-bit control 0x9A get the same answer.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask what kind of terminal this is before they use its features.
- **Note:** one of xterm's defined levels in place of 60 (a departure), 22 for color, and 4 only when sixel is built.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `da1_reports_a_vt220_with_color` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `da1_reports_sixel_while_sixel_is_offered` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- [tools/conformance/scenarios/reply-device-attributes.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reply-device-attributes.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
