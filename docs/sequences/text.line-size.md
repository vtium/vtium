<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Double-width and double-height lines (DECDWL, DECDHL, DECSWL: ESC # 3-6)

| Sequence | What it does |
|---|---|
| `ESC # 3` | DECDHL: top half of a double-height line |
| `ESC # 4` | DECDHL: bottom half of a double-height line |
| `ESC # 5` | DECSWL: single-width line |
| `ESC # 6` | DECDWL: double-width line |

## What vtium does

vtium sets the size of the cursor's line, and draws the line at that size. A double-size line holds half the screen's columns, and the characters right of its center are lost. The cursor never stands right of the line's last column, and printing wraps after it.

ED returns each line it erases whole to single size, and IL, DL and the scrolls move a line with its size. While DECLRMM is set, every line is single size and vtium ignores these sequences. The pointer's columns on a double-size line are still the screen's.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** double-size banner lines, as DEC consoles drew them, and the vttest screens that check them.
- **Note:** vtium-term sets the cursor's line's size from ESC # 3 to 6; a double-size line holds half the columns, and printing, autowrap, the cursor, tabs, CPR, ICH, DCH, ED, EL, IL, DL, the scrolls, DECALN, DECLRMM and a resize follow the VT100 User Guide and the VT510 manual; vtium-render draws such a line at double size, and the font helper draws its outline glyphs at true double size; selection, the pointer and reverse wrap map onto the doubled columns; OSC 66 on a double-size line takes only its width, and a size change erases the sized runs on the line (ADR-1308 D7).
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_double_ask_draws_a_color_glyph_at_its_size` in [crates/vtium-config/src/serve.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/serve.rs)
- `a_double_ask_places_a_color_glyph_held_for_text_in_the_doubled_cell` in [crates/vtium-config/src/serve.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/serve.rs)
- `a_double_ask_shapes_a_cluster_at_its_size` in [crates/vtium-config/src/serve.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/serve.rs)
- `a_sprite_keeps_to_the_text_band_at_the_normal_and_each_double_size` in [crates/vtium-font/src/sprite.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/sprite.rs)
- `a_color_glyph_on_a_line_of_double_size_takes_unifonts_bitmap_off_color` in [crates/vtium-render/src/cache.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/cache.rs)
- `a_shade_on_a_line_of_double_size_keeps_its_1_bit_pattern` in [crates/vtium-render/src/cache.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/cache.rs)
- `each_piece_of_a_doubled_glyph_is_a_glyph_of_its_own` in [crates/vtium-render/src/cache.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/cache.rs)
- `the_pieces_of_a_cell_of_double_size_are_cut_from_one_drawn_glyph` in [crates/vtium-render/src/cache.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/cache.rs)
- `a_box_drawing_line_on_a_double_height_pair_takes_the_taller_cells_weight` in [crates/vtium-render/src/renderer/double.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/renderer/double.rs)
- `a_box_drawing_line_on_a_double_width_line_keeps_its_weight` in [crates/vtium-render/src/renderer/double.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/renderer/double.rs)
- `a_glyph_rendered_at_double_size_keeps_its_accent_above_the_line` in [crates/vtium-render/tests/bitmap_reach.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/bitmap_reach.rs)
- `every_buffer_gets_a_doubled_glyphs_ink_as_it_changes` in [crates/vtium-render/tests/bitmap_reach.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/bitmap_reach.rs)
- `every_gpu_frame_of_a_doubled_glyphs_ink_matches_a_whole_draw` in [crates/vtium-render/tests/bitmap_reach.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/bitmap_reach.rs)
- `ink_from_a_normal_row_never_reaches_into_a_line_of_double_size` in [crates/vtium-render/tests/bitmap_reach.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/bitmap_reach.rs)
- `every_gpu_frame_of_double_size_lines_matches_a_whole_draw` in [crates/vtium-render/tests/gpu_line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/gpu_line_size.rs)
- `a_double_height_pair_draws_the_upper_half_above_the_lower` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `a_double_width_line_draws_each_pixel_twice_as_wide` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `a_glyph_rendered_at_double_size_draws_its_pieces_as_they_are` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `a_line_of_double_size_receives_no_ink_from_its_neighbours` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `a_wide_character_on_a_double_width_line_spans_four_cells` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `each_frame_of_the_cursor_on_a_pair_matches_a_whole_draw` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `every_buffer_gets_what_it_lacks_as_lines_change_size` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `every_decoration_and_color_doubles_with_the_line` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `the_cursor_and_the_selection_double_with_the_line` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `the_cursor_covers_both_lines_of_a_double_height_pair` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `the_cursor_covers_the_wide_characters_columns_on_the_other_line` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `the_cursor_on_a_line_with_no_partner_covers_its_own_line` in [crates/vtium-render/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size.rs)
- `a_selection_on_either_line_of_a_pair_draws_both_halves_selected` in [crates/vtium-render/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size_select.rs)
- `the_gpu_path_draws_both_halves_of_a_selected_pair_character` in [crates/vtium-render/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/line_size_select.rs)
- `a_line_made_double_size_loses_the_characters_right_of_the_center` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `a_resize_cuts_a_double_size_line_at_half_the_new_width` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `cursor_movement_stops_at_the_lines_last_column` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `decaln_returns_every_line_to_single_size` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `declrmm_returns_lines_to_single_size_and_keeps_them_there` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `decrc_restores_a_wrap_pending_at_the_lines_last_column` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `ed_returns_each_line_it_erases_whole_to_single_size` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `esc_hash_3_to_6_set_the_size_of_the_cursors_line` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `ich_and_dch_move_the_cells_up_to_the_lines_last_column` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `lines_move_with_their_size_and_blank_lines_come_in_single_size` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `printing_on_a_double_size_line_wraps_at_half_the_width` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `reverse_wraparound_goes_back_to_a_double_size_lines_last_column` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `the_cursor_keeps_its_column_or_moves_to_the_lines_last_one` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `without_autowrap_printing_overwrites_the_lines_last_column` in [crates/vtium-term/tests/line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size.rs)
- `a_pair_copies_its_text_once_and_selects_as_one_line` in [crates/vtium-term/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_select.rs)
- `a_pair_cut_by_the_top_of_the_view_selects_and_copies_its_text` in [crates/vtium-term/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_select.rs)
- `the_alternate_screen_does_not_pair_its_first_row_with_the_scrollback` in [crates/vtium-term/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_select.rs)
- `the_link_under_the_pointer_on_a_double_width_line_is_the_one_drawn_there` in [crates/vtium-term/tests/line_size_select.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_select.rs)
- `the_face_gives_a_cluster_drawn_at_a_double_size` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `a_drag_on_a_double_width_line_selects_the_doubled_characters_under_it` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- [tools/conformance/vtium/cases/cursor-dhl-alone.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-dhl-alone.case)
- [tools/conformance/vtium/cases/cursor-dwl-underline.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-dwl-underline.case)
- [tools/conformance/vtium/cases/cursor-dwl-wide.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-dwl-wide.case)
- [tools/conformance/vtium/cases/cursor-pair-bottom-bar.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-pair-bottom-bar.case)
- [tools/conformance/vtium/cases/cursor-pair-top-block.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-pair-top-block.case)
- [tools/conformance/vtium/cases/line-size-edit.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-edit.case)
- [tools/conformance/vtium/cases/line-size-erase.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-erase.case)
- [tools/conformance/vtium/cases/line-size-irm-tall.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-irm-tall.case)
- [tools/conformance/vtium/cases/line-size-irm-wide.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-irm-wide.case)
- [tools/conformance/vtium/cases/line-size-irm-widen.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-irm-widen.case)
- [tools/conformance/vtium/cases/line-size-reverse-wrap.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-reverse-wrap.case)
- [tools/conformance/vtium/cases/line-size-select.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-select.case)
- [tools/conformance/vtium/cases/line-size-wrap.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/line-size-wrap.case)

## Where to read more

To follow the work on this row, see issue [#61](https://github.com/vtium/vtium/issues/61).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
