<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTVERSION (CSI > q)

| Sequence | What it does |
|---|---|
| `CSI > Ps q` | XTVERSION: report the terminal's name and version |

## Parameters

- `Ps`: Must be 0. vtium answers no other value. The default is 0.

## What vtium does

vtium answers `DCS > | vtium(VERSION) ST`, where VERSION is vtium's version, as in `vtium(0.1.0)`.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask the terminal's name and version.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `xtversion_reports_the_hosts_text` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
