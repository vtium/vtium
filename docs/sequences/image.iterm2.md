<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# iTerm2 inline images (OSC 1337 File=)

| Sequence | What it does |
|---|---|
| `OSC 1337 ; File=keys : Pt ST` | a picture in one string |
| `OSC 1337 ; MultipartFile=keys ST` | start a picture sent in parts |
| `OSC 1337 ; FilePart=Pt ST` | a part of the picture |
| `OSC 1337 ; FileEnd ST` | end the picture sent in parts |

## Parameters

- `keys`: `key=value` pairs separated by `;`. vtium shows only a picture with `inline=1`. `width` and `height` size it, and `preserveAspectRatio=0` lets it stretch.
- `Pt`: The file, in base64.

## What vtium does

vtium shows pictures only in a build with the `images` feature, and the minspec build, the default, ignores the string. A build with pictures offers a protocol only while its switch is on, `image-storage-limit` is above 0, and every display of the head can draw pictures.

vtium's image helper decodes the file as PNG, JPEG or GIF, and vtium places the picture at the cursor. The text scrolls when the picture needs room, and the cursor ends on the picture's last row. An animated GIF plays its frames.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as imgcat show pictures in iTerm2's protocol.
- **Note:** as a handler.
- **Specifications:** WezTerm es, imgcat

## Tests

These tests check what vtium does with the sequences on this page.

- `a_gif_past_its_frame_bounds_comes_back_as_its_first_frame` in [crates/vtium-image-helper/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/src/decode.rs)
- `disposal_method_2_clears_the_frames_own_rectangle_to_transparent` in [crates/vtium-image-helper/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/src/decode.rs)
- `disposal_method_3_restores_the_canvas_the_frame_was_laid_on` in [crates/vtium-image-helper/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/src/decode.rs)
- `disposal_methods_0_and_1_lay_the_next_frame_on_the_frame_before` in [crates/vtium-image-helper/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/src/decode.rs)
- `the_loop_count_comes_from_the_netscape_extension_or_its_absence` in [crates/vtium-image-helper/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/src/decode.rs)
- `the_real_helper_returns_an_animated_gifs_frames_past_its_budget_of_one_picture` in [crates/vtium-image-helper/tests/helper.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/tests/helper.rs)
- `frames_fit_only_a_request_that_asked_for_them_within_its_bounds` in [crates/vtium-image/src/decode.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode.rs)
- `a_request_for_frames_comes_back_and_its_bounds_are_held` in [crates/vtium-image/src/decode/wire.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/wire.rs)
- `the_records_examples_draw_at_the_sizes_it_gives` in [crates/vtium-image/src/protocol/iterm2/keys.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/keys.rs)
- `a_loop_count_of_0_loops_forever_and_one_of_l_plays_l_plus_one_times` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `a_picture_asks_for_frames_within_the_frame_allowance` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `a_picture_in_one_string_goes_to_the_helper_and_is_placed_under_the_sixel_tag` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `a_picture_taller_than_the_rows_below_scrolls_as_a_sixel_and_ignores_the_sixel_modes` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `a_transfer_in_parts_gathers_its_file_across_parts_of_any_length` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `an_animated_gif_is_stored_with_its_frames_gaps_and_loops_and_runs` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `an_animated_gif_past_the_frame_allowance_is_stored_as_its_first_frame_still` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `only_the_first_frame_is_copied_when_the_frame_allowance_refuses` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `width_and_height_size_the_picture_and_its_box` in [crates/vtium-image/src/protocol/iterm2/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/iterm2/tests.rs)
- `a_drawn_size_draws_the_picture_at_it_and_the_box_covers_the_cells_it_reaches` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `a_drawn_size_ignores_a_box_in_cells_the_place_names` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `an_animated_picture_covers_a_cell_only_where_every_frame_covers_it` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `an_animated_picture_draws_nothing_only_where_every_frame_draws_nothing` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `frames_reserved_ahead_of_their_image_free_other_animations_and_split_or_go_back` in [crates/vtium-image/src/store/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/tests.rs)
- `the_parts_of_a_transfer_reach_the_iterm2_handler` in [crates/vtium-term/src/terminal/images/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/images/tests.rs)
- `iterm2_is_on_while_sixel_is_offered_and_a_helper_can_start` in [crates/vtium/src/head/images.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/head/images.rs)

## Where to read more

To follow the work on this row, see issue [#62](https://github.com/vtium/vtium/issues/62).

To read why vtium does what it does here, see [ADR-1103 D1](../architecture/image/ADR-1103-iterm2-pictures-are-sized-by-their-keys-and-decoded-in-the-image-helper.md#d1-what-vtium-takes).
