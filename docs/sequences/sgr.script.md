<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 73, 74, 75: superscript, subscript, baseline

| Sequence | What it does |
|---|---|
| `CSI 73 m` | superscript |
| `CSI 74 m` | subscript |
| `CSI 75 m` | neither superscript nor subscript |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws superscript and subscript text smaller, at two thirds of the normal size, raised to the top of the line or lowered to its bottom. Setting one of the two replaces the other.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** mathematical and chemical text draws superscripts and subscripts with it.
- **Note:** vtium draws a superscript or subscript glyph at two thirds of the normal glyph's own size, centred across its cells, with its top on the text band's top for SGR 73 and its bottom on the text band's bottom for SGR 74; SGR 75 draws the normal glyph (mintty's Tips page, its table of text attributes, https://github.com/mintty/mintty/wiki/Tips#text-attributes-and-rendering, and its changelog for 3.0.1, which name the codes and leave the size and place to the terminal); a bitmap font draws the small glyph from its own smaller size where one is from half to three quarters of the normal glyph's height: itself at a smaller whole multiple when vtium draws it at 2x or more, a CPI font's other heights, and the smaller fonts a frit's script-source= names, each glyph placed whole; a bitmap font with none scales its 1-bit glyph to two thirds, every pixel fully on or off, with bold and italic added to the small glyph; a color glyph is scaled smoothly; a sprite is drawn at the small size; the glyph cache keeps each variant of a text apart (ADR-1202).
- **Specifications:** WezTerm es (mintty's codes)

## Tests

These tests check what vtium does with the sequences on this page.

- `script_source_names_smaller_sizes_of_the_source_above_it` in [crates/vtium-config/src/frit.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/frit.rs)
- `an_ask_made_says_how_each_script_glyph_was_made` in [crates/vtium-config/src/serve.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/serve.rs)
- `the_script_line_draws_the_glyphs_the_engine_made` in [crates/vtium-config/src/specimen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/specimen.rs)
- `the_font_views_captions_say_how_the_font_makes_superscripts` in [crates/vtium-config/src/tui/words.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/tui/words.rs)
- `the_script_caption_names_each_way` in [crates/vtium-config/src/tui/words.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/src/tui/words.rs)
- `check_notes_a_script_source_outside_the_range` in [crates/vtium-config/tests/commands.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/commands.rs)
- `away_from_a_terminal_each_font_is_a_line_of_text` in [crates/vtium-config/tests/preview.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/preview.rs)
- `a_bitmap_script_glyph_is_its_normal_glyph_made_small_by_vtium_font` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `a_bitmap_script_glyph_takes_its_style_after_it_is_made_small` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `a_color_script_glyph_fits_two_thirds_of_its_cells_against_an_edge` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `a_script_ask_is_answered_in_strikes_of_its_script` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `a_script_glyph_comes_from_the_source_of_the_normal_glyph` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `an_outline_script_glyph_is_rasterised_at_two_thirds_of_its_size` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `an_outline_script_stands_on_the_first_sources_own_box` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `an_outline_script_strike_reports_lines_found_at_the_script_size` in [crates/vtium-config/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script.rs)
- `a_cpi_source_takes_another_of_its_heights` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `a_script_glyph_never_comes_from_a_font_not_named_for_its_source` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `script_source_gives_the_smaller_fonts_glyph` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `script_source_gives_the_smaller_fonts_lines` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `script_source_takes_the_sources_unimap_only_without_a_table_of_its_own` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `the_engine_says_how_it_made_each_script_glyph` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `the_helper_draws_unifonts_smaller_multiples_as_the_console_does` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `the_helpers_script_lines_are_the_consoles_for_unifont` in [crates/vtium-config/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-config/tests/script_sizes.rs)
- `a_one_pixel_stroke_stays_one_pixel_wide_at_each_phase` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_script_glyph_sits_against_the_text_band_not_the_cell` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_smaller_glyph_is_placed_whole_against_the_band` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_smaller_size_lies_from_half_to_three_quarters` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_source_drawn_at_a_multiple_takes_the_tables_smaller_multiple` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_strike_past_three_quarters_is_refused` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `bold_widens_the_small_glyph` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `five_times_takes_three_not_four` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `italic_shears_the_small_glyph_about_its_baseline` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `script_ink_is_cut_above_and_below_its_row_only` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `superscript_sits_at_the_top_and_subscript_at_the_bottom` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `the_glyphs_own_box_scales_not_its_cell` in [crates/vtium-font/src/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/src/script.rs)
- `a_face_draws_its_own_script_glyph` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `a_script_glyph_sits_against_the_faces_text_band` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `a_sprite_is_drawn_at_the_small_size` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `a_wide_script_glyph_is_centred_across_its_cells` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `bold_is_added_after_the_glyph_is_made_small` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `the_bitmap_scale_keeps_the_shape_of_91_of_100_unifont_glyphs` in [crates/vtium-font/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script.rs)
- `a_superscripts_underline_lies_under_the_raised_glyph` in [crates/vtium-font/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_lines.rs)
- `d4s_map_takes_each_line_where_it_takes_the_glyphs_rows` in [crates/vtium-font/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_lines.rs)
- `every_line_of_every_script_set_lies_inside_the_cell` in [crates/vtium-font/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_lines.rs)
- `line_spacing_moves_a_script_set_with_the_script_box` in [crates/vtium-font/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_lines.rs)
- `unifonts_script_lines_are_the_rows_of_d8s_table` in [crates/vtium-font/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_lines.rs)
- `a_script_cell_takes_its_sources_glyph_else_the_built_in_fonts` in [crates/vtium-font/tests/script_set.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_set.rs)
- `a_script_strike_reports_its_own_lines` in [crates/vtium-font/tests/script_set.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_set.rs)
- `a_strike_says_its_script_and_draws_only_for_a_cell_in_it` in [crates/vtium-font/tests/script_set.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_set.rs)
- `a_smaller_multiples_glyph_is_stored_at_its_own_size` in [crates/vtium-font/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_sizes.rs)
- `bold_at_a_smaller_multiple_widens_by_one_of_the_fonts_pixels` in [crates/vtium-font/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_sizes.rs)
- `each_scale_takes_the_tables_multiple` in [crates/vtium-font/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_sizes.rs)
- `unifont_at_three_times_takes_its_scripts_at_twice_in_whole_blocks` in [crates/vtium-font/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_sizes.rs)
- `unifont_at_twice_its_size_takes_its_scripts_at_its_own_size` in [crates/vtium-font/tests/script_sizes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/script_sizes.rs)
- `each_script_is_a_glyph_of_its_own` in [crates/vtium-render/src/cache.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/cache.rs)
- `the_sprites_lie_in_a_grid_that_fits_a_largest_texture_a_row_of_them_passes` in [crates/vtium-render/src/gpu/glyphs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/gpu/glyphs.rs)
- `each_script_attribute_picks_its_variant` in [crates/vtium-render/src/variant.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/variant.rs)
- `a_bitmap_script_glyph_at_1_bit_is_its_small_glyph_exactly` in [crates/vtium-render/tests/mono.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/mono.rs)
- `each_variant_draws_vtium_fonts_scaled_glyph` in [crates/vtium-render/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script.rs)
- `superscript_and_subscript_draw_the_glyph_small_at_the_top_and_bottom` in [crates/vtium-render/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script.rs)
- `a_1_bit_curl_in_a_script_cell_is_joined_whole_pixels` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `an_underline_steps_between_a_normal_and_a_superscript_cell` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `at_2x_and_3x_the_lines_follow_the_smaller_multiple` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `each_line_lies_on_its_scripts_rows` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `the_gpu_path_draws_a_script_cells_lines_as_the_cpu_path_does` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `the_underline_cursor_hides_a_script_cells_own_underline` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `what_covers_the_whole_cell_draws_in_a_script_cell_as_in_a_normal_one` in [crates/vtium-render/tests/script_lines.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/script_lines.rs)
- `superscript_and_subscript_are_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)
- `a_script_cell_draws_the_built_in_script_glyph_until_its_own_arrives` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `a_script_cells_lines_come_from_its_scripts_strike` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `a_superscript_cluster_draws_from_the_frits_superscript_parts` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `before_the_frits_glyphs_a_script_cells_lines_are_the_built_in_fonts` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `a_superscript_ask_names_its_script_and_its_glyphs_are_held_in_it` in [crates/vtium/src/helper.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/helper.rs)
- `italic_and_bold_text_is_asked_for_in_its_style` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_built_in_font_at_twice_its_size_takes_its_scripts_at_its_own_size` in [crates/vtium/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/script.rs)
- `a_built_in_script_glyph_sits_against_the_text_band` in [crates/vtium/tests/script.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/script.rs)

## Where to read more

To follow the work on this row, see issue [#37](https://github.com/vtium/vtium/issues/37).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
