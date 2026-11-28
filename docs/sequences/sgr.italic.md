<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 3 and 23: italic

| Sequence | What it does |
|---|---|
| `CSI 3 m` | italic |
| `CSI 23 m` | not italic |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws italic text in the font's italic face. A font without one, such as a bitmap font, draws the regular face slanted.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors and pagers show comments and emphasis in italic.
- **Note:** the slanted face comes with outline fonts.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_bitmap_box_passes_the_right_edge_and_stays_inside_a_chunk` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `style_asks_are_answered_in_their_style` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `synthetic_italic_slants_an_outline_by_a_fifth_about_the_baseline` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `the_helper_shears_a_bitmap_as_vtium_font_does` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `a_scaled_psf_glyph_leans_by_twice_the_pixels` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `an_8x16_psf_glyph_leans_2_1_and_0` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `from_16_pixels_a_letters_last_rows_lean_together` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `ink_sheared_past_the_right_edge_widens_the_box_right` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `the_builtin_unifont_leans_about_its_baseline` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `italic_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)
- `a_style_draws_its_own_glyph_else_the_built_in_fonts_in_that_style` in [crates/vtium/src/deferred.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/deferred.rs)
- `an_italic_ask_names_its_style_and_its_glyphs_are_held_as_italic` in [crates/vtium/src/helper/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/helper/tests.rs)
- `italic_and_bold_text_is_asked_for_in_its_style` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issues [#12](https://github.com/vtium/vtium/issues/12) and [#23](https://github.com/vtium/vtium/issues/23).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
