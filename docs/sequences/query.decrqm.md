<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECRQM, ANSI and DEC

| Sequence | What it does |
|---|---|
| `CSI Pa $ p` | DECRQM: report an ANSI mode |
| `CSI ? Pd $ p` | DECRQM: report a DEC private mode |

## Parameters

- `Pa`: The number of an ANSI mode, as SM and RM take it.
- `Pd`: The number of a DEC private mode, as DECSET and DECRST take it.

## What vtium does

vtium answers `CSI Pa ; Ps $ y`, or `CSI ? Pd ; Ps $ y` for a DEC private mode. `Ps` is 1 when the mode is set and 2 when it is reset. It is 3 for a mode that is always set and 4 for one that is always reset. A mode vtium ignores or refuses reports its fixed state: 3 when set, 4 when reset. It is 0 for a number vtium does not recognize.

vtium answers from its mode table, which [Modes](../modes.md) lists.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask whether a mode is set before they rely on it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- [tools/conformance/scenarios/reply-decrqm.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reply-decrqm.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
