<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSCL

| Sequence | What it does |
|---|---|
| `CSI Pl ; Pc " p` | DECSCL: set the conformance level |

## Parameters

- `Pl`: The level: 61 for a VT100, and 62 to 64 for a VT200 to a VT400.
- `Pc`: Whether replies use 7-bit or 8-bit controls. vtium reads it and keeps its replies 7-bit.

## What vtium does

vtium first does a soft reset, as DECSTR does. Level 61 then selects 7-bit input, and levels 62 to 64 select 8-bit input with DEC supplemental graphics in G2 and G3. Any other level leaves the input as UTF-8.

DECRQSS reports the level that DECSCL set, and 62 until a program sets one.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs set the terminal's level and choose 7-bit or 8-bit replies.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decscl_resets_and_sets_the_input` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- [tools/conformance/vtium/cases/charset-7bit-8bit.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-7bit-8bit.case)
- [tools/conformance/vtium/cases/charset-decscl.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-decscl.case)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
