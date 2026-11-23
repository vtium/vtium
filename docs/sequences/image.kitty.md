<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty graphics (APC G)

| Sequence | What it does |
|---|---|
| `APC G keys ; Pt ST` | a kitty graphics command |

## Parameters

- `keys`: The command's keys, `key=value` pairs separated by commas, such as `a=T,f=100,i=1`. The key `a` names the action.
- `Pt`: The payload, in base64: the picture's data, or the name of a file or of shared memory to read it from.

## What vtium does

vtium shows pictures only in a build with the `images` feature, and the minspec build, the default, ignores the string. A build with pictures offers a protocol only while its switch is on, `image-storage-limit` is above 0, and every display of the head can draw pictures.

vtium takes every action of the protocol: transmit, transmit and show, put, delete, query, and the animation actions. A program may send a picture in chunks, and vtium reserves the whole picture's memory at the first chunk. vtium decodes PNG and zlib data, and opens files and shared memory, only in its sandboxed image helper.

A command that carries an image id or number gets an answer, `OK` or an error, unless the key `q` quiets it. The answer holds only numbers and fixed text that vtium writes itself.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as kitten icat, ranger and chafa show pictures in kitty's protocol.
- **Specifications:** kitty graphics-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_delete_naming_an_image_id_and_number_answers_as_a_put_does` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_named_medium_goes_to_the_helper_and_its_answer_is_stored` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_names_unpadded_last_group_decodes_with_it` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_palette_index_picture_is_charged_for_its_indices` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_palette_index_picture_takes_the_new_colors_when_they_change` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_payload_cut_anywhere_stores_the_same_image` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_raw_upload_without_s_or_v_is_refused_as_kitty_refuses_it` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_replacement_keeps_the_same_images_whether_or_not_it_takes_the_spare` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_reply_file_too_short_or_too_long_is_ebadf_and_gives_its_bytes_back` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_reply_leaves_out_i_when_i_named_the_image_as_kittys_does` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_reservation_for_an_id_keeps_the_old_image_when_the_new_one_is_refused` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_run_fed_a_few_bytes_of_work_at_a_time_stores_what_it_stores_whole` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_transmission_stores_premultiplied_pixels_and_places_at_the_cursor` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `frames_on_palette_index_pictures_are_refused_as_kitty_refuses_them` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `palette_index_pictures_are_refused_where_vtium_cannot_take_them` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `palette_index_pictures_come_through_every_medium_and_with_zlib` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `palette_index_pictures_store_the_colors_their_indices_give` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `point_deletes_take_kitty_placements_and_leave_a_sixel_beside_them` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `raw_data_short_of_its_picture_is_enodata_in_every_raw_format` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `refusals_carry_kittys_texts` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `replaced_frames_of_one_size_reuse_the_buffer_freed_last` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `replies_follow_q_and_need_an_id_or_a_number` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `the_cursor_after_a_tall_image_is_bounded_by_the_margin_and_a_screen` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `the_cursor_goes_after_the_box_and_wraps_at_the_right_edge` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `the_longest_error_answer_fits_the_answer_bound` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `without_a_helper_named_media_are_ebadf_and_png_and_zlib_einval` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_chunk_cut_off_ends_the_upload_and_fails_it_with_einval` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_color_change_between_chunks_takes_effect_for_the_whole_picture` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_delete_ends_the_upload_with_no_answer_then_deletes` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_direct_transmission_while_an_upload_is_open_is_its_next_chunk` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_first_chunk_refused_by_its_keys_is_answered_at_once_and_opens_nothing` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_frame_while_a_pictures_upload_is_open_is_its_next_chunk` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_put_a_composition_and_a_control_leave_the_upload_open` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_query_in_chunks_is_answered_at_its_last_chunk_and_stores_nothing` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_query_while_an_upload_is_open_is_answered_from_its_own_keys` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_transmission_from_a_named_medium_ends_the_upload_and_runs_on_its_own` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `an_a_capital_t_upload_places_at_the_cursor_of_its_last_chunk_with_its_first_keys` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `an_upload_in_chunks_answers_once_at_its_last_chunk` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `an_uploads_data_is_capped_at_its_reservation` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `an_uploads_q_is_its_first_chunks_until_a_later_one_other_than_0` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `an_uploads_reservation_counts_beside_the_old_image` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `each_buffer_holds_its_own_upload` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `each_chunk_decodes_its_own_base64_unpadded` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `reset_and_withdraw_end_the_upload_with_no_answer` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `the_old_image_stays_until_the_upload_is_accepted` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `cell_selectors_choose_by_the_cells_a_placement_covers` in [crates/vtium-image/src/store/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/kitty.rs)
- `d_a_removes_every_kitty_placement_on_the_screen_and_keeps_the_sixel` in [crates/vtium-image/src/store/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/kitty.rs)
- `a_palette_index_picture_takes_the_colors_of_now_whenever_it_arrived` in [crates/vtium-term/src/terminal/images.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/images.rs)
- `every_color_change_and_reset_redraws_a_palette_index_picture` in [crates/vtium-term/src/terminal/images.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/images.rs)
- `osc_colors_reach_a_palette_index_picture` in [crates/vtium-term/src/terminal/images.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/images.rs)
- `a_kitty_placement_moves_only_when_wholly_inside_the_rectangle` in [crates/vtium-term/tests/image_margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/image_margins.rs)
- `the_cursor_after_a_kitty_placement_wraps_at_the_right_margin` in [crates/vtium-term/tests/image_margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/image_margins.rs)
- `a_clear_leaves_the_upload_open` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `a_sixel_between_chunks_leaves_the_upload_open` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `each_screen_holds_its_own_upload` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `kitty_switched_off_drops_the_upload_at_the_next_apc` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `output_between_chunks_leaves_the_upload_open` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `replacing_the_handler_drops_its_open_upload` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `ris_and_the_sessions_end_drop_the_upload` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `the_work_bound_stops_an_input_call_inside_an_apc_run` in [crates/vtium-term/tests/kitty.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty.rs)
- `a_picture_in_a_cell_a_double_width_line_hides_is_not_drawn` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_picture_on_a_double_height_pair_starts_where_its_cell_is_drawn` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `a_picture_on_a_double_width_line_starts_where_its_cell_is_drawn` in [crates/vtium-term/tests/line_size_pictures.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/line_size_pictures.rs)
- `the_childs_exit_drops_a_kitty_upload_it_left_open` in [crates/vtium/src/term.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/term.rs)
- [tools/conformance/vtium/cases/kit-g14-margins.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-g14-margins.case)
- [tools/conformance/vtium/cases/kit-g14-point-deletes.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-g14-point-deletes.case)
- [tools/conformance/vtium/cases/kit-g14-su-sd.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-g14-su-sd.case)
- [tools/conformance/vtium/cases/kit-g14-tall-cursor.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-g14-tall-cursor.case)
- [tools/conformance/vtium/cases/kit-margins.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-margins.case)
- [tools/conformance/vtium/cases/kit-png-frames.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-png-frames.case)
- [tools/conformance/vtium/cases/kit-spec-indexed-media.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-indexed-media.case)
- [tools/conformance/vtium/cases/kit-spec-indexed-recolor.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-indexed-recolor.case)
- [tools/conformance/vtium/cases/kit-spec-indexed.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-indexed.case)
- [tools/conformance/vtium/cases/kit-spec-no-id-order.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-no-id-order.case)
- [tools/conformance/vtium/cases/kit-spec-replies.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-replies.case)

## Where to read more

To follow the work on this row, see issue [#49](https://github.com/vtium/vtium/issues/49).

To read why vtium does what it does here, see [ADR-1100 D14](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d14-vtium-presents-itself-as-a-kitty-compatible-terminal).
