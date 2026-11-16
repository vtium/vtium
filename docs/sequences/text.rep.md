<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# REP

| Sequence | What it does |
|---|---|
| `CSI Ps b` | REP: repeat the last character |

## Parameters

- `Ps`: How many times to repeat the character. The default is 1.

## What vtium does

vtium prints the last character it printed Ps more times, even when controls came between, so `y z CR LF CSI 3 b` prints `zzz` on the next row. When a single shift mapped that character, REP repeats the mapped character.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** ncurses repeats a character with it to save bytes.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/)

## Tests

These tests check what vtium does with the sequences on this page.

- `rep_repeats_the_last_printed_base` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- [tools/conformance/vtium/cases/print-rep.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/print-rep.case)

## Where to read more

To follow the work on this row, see issue [#11](https://github.com/vtium/vtium/issues/11).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
