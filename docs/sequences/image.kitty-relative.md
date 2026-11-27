<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty graphics relative placements (P, Q, H, V)

| Sequence | What it does |
|---|---|
| `APC G a=p,P=Ps,Q=Ps,H=Ps,V=Ps,keys ST` | place a picture relative to another |

## Parameters

- `P`: The parent's image id.
- `Q`: The parent's placement id. Without it, vtium takes the parent image's placement made last that has no placement id.
- `H, V`: How many cells the placement sits to the right of and below its parent. A negative value moves it left or up. The default is 0.

## What vtium does

vtium places the picture relative to its parent, so the two move together. A put whose parent does not exist answers ENOPARENT. A chain that loops answers ECYCLE, and a chain that is too deep answers ETOODEEP.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs place a picture relative to another, as kitty's protocol allows.
- **Specifications:** kitty graphics-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_chain_deeper_than_eight_relative_placements_is_etoodeep` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_parent_that_does_not_exist_is_enoparent` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_placement_that_would_follow_itself_is_ecycle` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_relative_placement_draws_at_its_parents_origin_plus_its_offset` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_relative_placement_follows_its_parent_through_a_scroll_and_the_scrollback` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_relative_placement_follows_its_parent_when_the_parent_is_replaced` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_relative_placement_moves_with_its_parent_in_a_region_scroll` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `deleting_a_parent_takes_its_children_and_images_left_without_placements` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `notes_past_their_bound_report_the_buffer_changed_whole` in [crates/vtium-image/src/store/runs/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/runs/tests.rs)
- `a_child_above_a_parent_below_the_view_draws_from_the_lines_below` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `a_child_of_a_root_that_lost_a_column_starts_from_its_drawn_origin` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `a_relative_placement_reports_its_box_from_its_roots_line` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `a_child_above_a_parent_below_a_scrolled_back_view_is_drawn` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `a_relative_placement_marks_its_own_rows_when_it_comes_and_goes` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `a_child_keeps_its_column_when_its_double_width_root_is_below_the_view` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_child_of_a_picture_on_a_double_width_line_is_offset_in_screen_columns` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_child_of_a_virtual_parent_on_a_double_width_line_is_offset_in_screen_columns` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_child_on_a_double_width_line_is_offset_in_screen_columns` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_delete_of_every_placement_removes_a_child_of_a_virtual_parent` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `a_virtual_parents_child_marks_where_it_draws_when_placed_edited_and_deleted` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `deleting_a_virtual_parent_marks_its_cells_and_where_its_child_drew` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `ed_2_on_the_alternate_screen_leaves_the_main_buffers_child` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `ed_2_removes_a_child_of_a_virtual_parent` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- `on_the_alternate_screen_a_delete_of_every_placement_removes_a_virtual_parents_child` in [crates/vtium-term/tests/placeholder_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/placeholder_pictures.rs)
- [tools/conformance/vtium/cases/kit-relative-refusals.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-relative-refusals.case)
- [tools/conformance/vtium/cases/kit-relative.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-relative.case)
- [tools/conformance/vtium/cases/kit-spec-replies.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-replies.case)

## Where to read more

To follow the work on this row, see issue [#49](https://github.com/vtium/vtium/issues/49).

To read why vtium does what it does here, see [ADR-1100 D13](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d13-relative-placements-and-unicode-placeholders).
