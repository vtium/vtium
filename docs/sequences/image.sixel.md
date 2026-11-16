<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Sixel (DCS q)

| Sequence | What it does |
|---|---|
| `DCS P1 ; P2 ; P3 q Pt ST` | a sixel picture |

## Parameters

- `P1`: The pixel aspect ratio. vtium reads it and draws each dot as one square pixel. The default is 0.
- `P2`: 1 leaves the pixels that no dot sets transparent. Any other value fills them with color register 0. The default is 0.
- `P3`: The horizontal grid size, which vtium ignores.
- `Pt`: The picture: sixel characters, color register commands, repeats and raster attributes.

## What vtium does

vtium shows pictures only in a build with the `images` feature, and the minspec build, the default, ignores the string. A build with pictures offers a protocol only while its switch is on, `image-storage-limit` is above 0, and every display of the head can draw pictures.

vtium decodes the picture as it arrives and places it at the cursor. The cursor ends on the picture's last row, or after its right edge with mode 8452. With mode 80 set, the picture sits at the screen's top left and the cursor stays where it is. Mode 1070 gives each picture its own color registers.

vtium reserves a picture's memory under the console's image budget before it decodes the pixels. A picture that cannot fit is refused.

vtium draws each sixel dot as one square pixel, where DEC's VT340 manual draws a dot twice as tall as it is wide. See [How sixel works in vtium-image's README](https://github.com/vtium/vtium/blob/main/crates/vtium-image/README.md#how-sixel-works).

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as img2sixel, mpv and lsix show pictures in sixel.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); VT330/VT340 graphics manual

## Tests

These tests check what vtium does with the sequences on this page.

- `a_picture_ends_at_its_lowest_dot_and_its_furthest_column` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_picture_fed_in_pieces_is_the_picture_fed_whole` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_picture_past_the_budget_is_refused_and_gives_its_bytes_back` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_picture_sized_first_draws_each_dot_in_its_row` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_picture_that_grows_holds_both_buffers_then_its_own` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_register_defined_again_changes_only_what_is_drawn_after` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_repeat_draws_its_sixel_that_many_times_and_0_once` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_sixel_draws_six_dots_bit_0_on_top_in_its_registers_color` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_string_with_no_dot_and_no_size_has_no_picture` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `carriage_return_overprints_the_band_in_another_color` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `hls_in_integers_matches_floating_point_but_at_exact_halves` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `raster_attributes_reserve_the_picture_before_any_sixel` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `raster_attributes_size_the_picture_and_reserve_it_first` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `rgb_and_hls_define_registers_and_a_number_past_them_is_the_last` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `the_largest_picture_cuts_what_lies_past_it` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `the_second_parameter_1_leaves_undrawn_pixels_clear` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `the_work_counts_the_pixels_a_short_string_writes` in [crates/vtium-image/src/decode/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/decode/sixel.rs)
- `a_sixel_inside_the_margins_scrolls_with_the_rectangle` in [crates/vtium-term/tests/image_margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/image_margins.rs)
- `a_sixel_outside_the_margins_scrolls_nothing` in [crates/vtium-term/tests/image_margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/image_margins.rs)
- `a_cancelled_string_places_nothing_and_gives_its_bytes_back` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_lower_max_graphic_size_mid_string_drops_the_picture` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_picture_goes_at_the_cursor_and_the_cursor_ends_on_its_last_row` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_picture_holds_its_pixels_of_the_budget_while_its_string_streams` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_picture_past_the_bottom_margin_scrolls_the_region_first_and_moves_whole` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_picture_past_the_budget_is_dropped_and_raster_attributes_refuse_it_first` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_picture_that_writes_past_the_work_bound_stops_the_input_call` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_register_count_xtsmgraphics_sets_takes_effect_at_the_next_picture` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `a_sixel_stream_fed_a_byte_at_a_time_ends_as_fed_whole` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `padded_frames_played_over_each_other_leave_one_picture` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `the_check_of_hidden_pieces_is_bounded_by_the_new_picture` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `the_work_bound_stops_an_input_call_inside_a_run` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)

## Where to read more

To follow the work on this row, see issue [#48](https://github.com/vtium/vtium/issues/48).

To read why vtium does what it does here, see [ADR-1100 D3](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d3-the-protocol-tag-and-the-rules-function).
