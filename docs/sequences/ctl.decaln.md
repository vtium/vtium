<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECALN, the screen alignment test (ESC # 8)

| Sequence | What it does |
|---|---|
| `ESC # 8` | DECALN: fill the screen with E |

## What vtium does

vtium fills every cell with an E in the default style, so a person can check the screen's alignment. It turns origin mode off, returns all four margins to the whole screen, and moves the cursor home.

The pen keeps its colors and its protection from DECSCA, and loses its other attributes.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** vttest and alignment checks fill the screen with E.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decaln_returns_a_double_width_line_to_single_size` in [crates/vtium-term/src/screen/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/screen/rect.rs)
- `decaln_fills_the_screen_with_e_and_resets_the_margins_and_cursor` in [crates/vtium-term/tests/modes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/modes.rs)

## Where to read more

To follow the work on this row, see issue [#54](https://github.com/vtium/vtium/issues/54).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
