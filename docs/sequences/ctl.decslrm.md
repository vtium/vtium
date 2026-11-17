<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSLRM, SCOSC and SCORC (DECSLRM and SCOSC share CSI s)

| Sequence | What it does |
|---|---|
| `CSI Ps ; Ps s` | DECSLRM: set the left and right margins, while mode 69 is set |
| `CSI s` | SCOSC: save the cursor, while mode 69 is reset |
| `CSI u` | SCORC: restore the cursor |

## Parameters

- `Ps ; Ps`: The left and right columns of the margins, counted from 1. The default is the whole width, from the first column to the last.

## What vtium does

DECSLRM and SCOSC share `CSI s`. While DECLRMM, mode 69, is set, `CSI s` sets the left and right margins. While it is reset, `CSI s` with no parameter saves the cursor as DECSC does, and `CSI u` restores it as DECRC does.

Printing, cursor movement, editing, scrolling and origin mode keep inside the left and right margins. DECSLRM moves the cursor home, and DECRQSS reports the margins. Resetting mode 69 returns the margins to the whole width.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors and multiplexers that split the screen side by side keep text inside left and right margins.
- **Note:** DECSLRM while DECLRMM is set and SCOSC while it is reset; printing, cursor movement, editing, scrolling, origin mode and the rectangle operations in origin mode keep inside the margins, and DECRQSS reports them; vtium-term's README lists where it departs from xterm.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_wide_character_across_a_margin_goes_whole_when_the_region_scrolls` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `cursor_movement_stops_at_the_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `decaln_and_a_resize_return_the_margins_to_the_whole_screen` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `decrqss_reports_the_left_and_right_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `decsc_keeps_a_pending_wrap_at_the_right_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `erases_ignore_the_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `ich_and_dch_move_the_cells_up_to_the_right_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `il_and_dl_move_the_cells_between_the_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `insert_mode_moves_the_cells_up_to_the_right_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `line_feeds_scroll_only_between_the_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `mode_69_makes_csi_s_set_margins_and_resetting_it_clears_them` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `origin_mode_counts_columns_from_the_left_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `printing_wraps_at_the_right_margin_to_the_left_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `rectangles_in_origin_mode_count_from_the_left_margin` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `su_and_sd_scroll_the_region_between_the_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `scosc_and_scorc_save_and_restore_the_cursor_as_decsc_and_decrc_do` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#55](https://github.com/vtium/vtium/issues/55).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
