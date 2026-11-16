<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Rectangular areas: DECCRA, DECFRA, DECERA, DECSERA, DECCARA, DECRARA, DECSACE

| Sequence | What it does |
|---|---|
| `CSI Pt ; Pl ; Pb ; Pr ; Pp ; Pt ; Pl ; Pp $ v` | DECCRA: copy a rectangle |
| `CSI Pc ; Pt ; Pl ; Pb ; Pr $ x` | DECFRA: fill a rectangle with a character |
| `CSI Pt ; Pl ; Pb ; Pr $ z` | DECERA: erase a rectangle |
| `CSI Pt ; Pl ; Pb ; Pr $ {` | DECSERA: erase the unprotected cells of a rectangle |
| `CSI Pt ; Pl ; Pb ; Pr ; Pm $ r` | DECCARA: change the attributes of a rectangle |
| `CSI Pt ; Pl ; Pb ; Pr ; Pm $ t` | DECRARA: reverse the attributes of a rectangle |
| `CSI Ps * x` | DECSACE: choose what DECCARA and DECRARA act on |

## Parameters

- `Pt ; Pl ; Pb ; Pr`: The rectangle's top row, left column, bottom row and right column, counted from 1. In origin mode they count from the margins. DECCRA's second Pt and Pl give the top left corner it copies to. The default is the whole screen.
- `Pp`: A page number. vtium has one page, so DECCRA's page numbers change nothing. The default is 1.
- `Pc`: The code of the character that DECFRA fills with. vtium takes a character one cell wide, and 0 means a space.
- `Pm`: For DECCARA, SGR codes, which it applies as SGR applies them to the pen. For DECRARA, the attributes to reverse: 1 bold, 4 underline, 5 blink, 7 inverse and 8 conceal, or 0 for the first four. The default is 0.
- `Ps`: For DECSACE, 2 makes DECCARA and DECRARA act on the rectangle, and any other value on the stream of text from the first corner to the last. The default is 0, the stream.

## What vtium does

vtium clips each rectangle to the screen, or in origin mode to the region inside all four margins. It ignores a rectangle whose top is below its bottom or whose left is right of its right.

DECERA and DECSERA erase as ED does, to empty cells in the pen's background color, and DECSERA leaves the cells that DECSCA protects. When a rectangle's edge cuts a wide character, the whole character goes.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors and vttest copy, fill and erase rectangles of the screen.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); kitty deccara (DECCARA for every SGR attribute)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_copy_keeps_clusters_that_fit_the_line` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `a_rectangle_in_origin_mode_keeps_to_the_margins` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `a_rectangle_marks_only_its_cells` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `a_rectangle_over_the_last_column_ends_the_wrap` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `a_wide_character_cut_at_a_rectangle_edge_goes_whole` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `deccara_sets_attributes_in_a_rectangle_or_a_stream` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `deccara_takes_every_sgr_attribute` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `deccra_copies_a_rectangle_whole` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `decera_erases_a_rectangle_clipped_to_the_screen` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `decfra_fills_a_rectangle_with_a_character` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `decrara_reverses_attributes` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `decsera_leaves_protected_cells` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)

## Where to read more

To follow the work on this row, see issue [#56](https://github.com/vtium/vtium/issues/56).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
