<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty graphics Unicode placeholders (U=1 virtual placements)

| Sequence | What it does |
|---|---|
| `APC G a=p,U=1,keys ST` | make a virtual placement |

## Parameters

- `keys`: The command's keys, `key=value` pairs separated by commas, such as `a=T,f=100,i=1`. The key `a` names the action.

## What vtium does

A virtual placement draws nothing on its own. A program shows it by writing kitty's placeholder character, U+10EEEE, into the text, with marks that name the picture's row and column. vtium draws the part of the picture under each run of placeholder cells. So the picture moves with the text, as inside a multiplexer's pane.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** keeps a picture in its pane under a multiplexer, as kitty video in tmux needs.
- **Specifications:** kitty graphics-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_virtual_placement_is_stored_on_no_line_and_only_a_delete_by_image_takes_it` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_delete_by_cell_takes_a_virtual_parents_child_by_the_runs_handed_with_it` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_frame_takes_a_virtual_parents_origin_only_from_runs_on_the_rows_it_shows` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_frame_takes_at_most_a_run_for_each_cell_it_shows` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_run_draws_its_slice_of_the_picture` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_run_on_a_double_size_line_covers_twice_its_cells` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_run_shows_the_part_of_the_fitted_picture_under_its_cells` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_run_whose_image_or_placement_does_not_exist_draws_nothing` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_run_with_no_placement_id_takes_the_virtual_placement_made_last` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_virtual_parents_children_draw_from_the_origin_its_runs_give` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_virtual_placement_evicted_or_replaced_by_one_on_a_line_is_reported` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `an_animated_images_run_shows_its_current_frame` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `deleting_a_virtual_placement_empties_its_runs_and_reports_a_change` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `ed_2_and_d_a_take_a_virtual_parents_child_by_the_runs_handed_with_them` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `has_virtual_follows_the_buffers_virtual_placements` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `only_an_entry_made_from_a_run_says_it_comes_from_a_placeholder_run` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `run_entries_of_one_placement_draw_by_row_then_by_column` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `run_entries_take_their_placements_place_in_the_draw_order` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `the_cells_of_a_run_outside_the_grid_draw_nothing` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `the_changes_nobody_takes_stay_within_the_images_the_store_can_hold` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `the_count_of_a_virtual_parents_children_follows_deletes_and_replacements` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_virtual_placement_has_its_size_in_cells_and_no_line` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `an_image_shown_only_by_a_virtual_placement_is_evicted_as_one_on_the_screen` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `a_cluster_that_holds_the_placeholder_later_draws_as_text` in [crates/vtium-render/tests/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/placeholder.rs)
- `a_hovered_link_draws_no_dash_on_a_placeholder_cell` in [crates/vtium-render/tests/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/placeholder.rs)
- `a_placeholder_cell_draws_only_its_background` in [crates/vtium-render/tests/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/placeholder.rs)
- `the_cursor_draws_over_a_placeholder_cell` in [crates/vtium-render/tests/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/placeholder.rs)
- `a_placeholder_cell_takes_a_blank_cells_colors_under_reverse_video` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `every_diacritic_is_an_above_mark_in_the_pinned_ucd` in [crates/vtium-term/src/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/placeholder.rs)
- `the_table_is_ghosttys_list_entry_for_entry` in [crates/vtium-term/src/placeholder.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/placeholder.rs)
- `a_frame_reads_runs_only_while_the_shown_buffer_holds_a_virtual_placement` in [crates/vtium-term/src/screen/images.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/screen/images.rs)
- `a_placeholder_run_on_a_double_width_line_covers_its_cells` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_virtual_placement_draws_its_slices_where_its_cells_are` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `an_edit_of_the_frame_a_virtual_placement_shows_marks_its_cells` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `deleting_a_virtual_placement_marks_its_cells` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `moving_the_cells_as_text_moves_the_picture` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `nothing_reads_runs_while_no_virtual_placement_has_a_child` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `one_read_marks_the_cells_of_every_virtual_placement_an_event_changes` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `a_cell_that_names_no_image_belongs_to_no_run` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_change_to_a_virtual_placement_marks_its_images_placeholder_cells` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_picture_late_in_a_long_line_keeps_its_marks` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_picture_row_rewrapped_by_a_resize_reads_the_same_cells` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_picture_row_wrapped_down_from_the_scrollback_keeps_its_columns` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_rewrap_moves_the_runs_with_the_cells` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `a_two_by_two_grid_reads_as_the_pictures_four_quarters` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `cells_that_leave_their_marks_out_read_as_cells_that_carry_them` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- `the_walk_back_through_wrapped_rows_stops_at_the_longest_picture_row` in [crates/vtium-term/tests/placeholders.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholders.rs)
- [tools/conformance/vtium/cases/kit-placeholders.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-placeholders.case)

## Where to read more

To follow the work on this row, see issue [#49](https://github.com/vtium/vtium/issues/49).

To read why vtium does what it does here, see [ADR-1100 D13](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d13-relative-placements-and-unicode-placeholders).
