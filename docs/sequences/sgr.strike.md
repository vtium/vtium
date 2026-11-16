<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 9 and 29: strike

| Sequence | What it does |
|---|---|
| `CSI 9 m` | strike through |
| `CSI 29 m` | no strike |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws a line through the middle of the text that follows SGR 9.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors and to-do lists strike through finished or deleted text.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `strike_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
