<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSC, DECRC

| Sequence | What it does |
|---|---|
| `ESC 7` | DECSC: save the cursor |
| `ESC 8` | DECRC: restore the cursor |

## What vtium does

DECSC saves the cursor's position and a pending wrap, the pen, autowrap, origin mode and the character sets. The main screen and the alternate screen each keep a saved cursor of their own.

DECRC restores what DECSC saved for the screen in use. With nothing saved, it moves the cursor home and returns the pen and those modes to their defaults.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs save and restore the cursor around what they draw.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decsc_saves_the_character_sets` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `decsc_saves_and_decrc_restores_the_cursor_state` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- [tools/conformance/vtium/cases/cursor-decsc-decrc.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-decsc-decrc.case)
- [tools/conformance/vtium/cases/cursor-decsc-origin.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-decsc-origin.case)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
