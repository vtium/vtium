<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# VT52 mode and its escape sequences

| Sequence | What it does |
|---|---|
| `CSI ? 2 l` | DECANM reset: enter VT52 mode |

## What vtium does

vtium stays in ANSI mode. Resetting DECANM does nothing, and vtium does not read VT52's escape sequences. DECRQM reports DECANM as permanently set.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** VT52 mode waits on its own work.
- **Specifications:** [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html) (DECANM); [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) (VT52 mode)

## Tests

No test covers this row yet.

## Where to read more

To follow the work on this row, see issue [#65](https://github.com/vtium/vtium/issues/65).

To read why vtium does what it does here, see [ADR-1000, "Declined for now"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#declined-for-now).
