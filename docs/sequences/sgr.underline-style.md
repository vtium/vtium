<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 4:0 to 4:5: underline styles

| Sequence | What it does |
|---|---|
| `CSI 4 : 0 m` | no underline |
| `CSI 4 : 1 m` | single underline |
| `CSI 4 : 2 m` | double underline |
| `CSI 4 : 3 m` | curly underline |
| `CSI 4 : 4 m` | dotted underline |
| `CSI 4 : 5 m` | dashed underline |

## What vtium does

The colon form of SGR 4 chooses the underline's style. vtium draws each of the five styles, and a cell keeps one style at a time, so a new style replaces the last one.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors such as neovim mark errors with curly and dotted underlines.
- **Specifications:** kitty underlines; WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `a_dotted_underline_keeps_one_period_across_cells_on_the_cpu_path` in [crates/vtium-render/tests/dotted_phase.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/dotted_phase.rs)
- `a_letter_beside_a_color_glyph_still_breaks_the_line_around_its_descender` in [crates/vtium-render/tests/emoji_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/emoji_ink.rs)
- `every_decoration_draws_on_the_gpu_path_as_on_the_cpu_path` in [crates/vtium-render/tests/gpu_decorations.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/gpu_decorations.rs)
- `a_doubled_underline_skips_the_ink_of_the_whole_doubled_character` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `a_magnified_underline_skips_the_ink_of_its_whole_character_on_the_gpu_path` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `a_sized_runs_underline_skips_the_ink_of_the_whole_run` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `a_turned_underline_skips_the_ink_of_the_letter_the_next_frame_shows` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `an_underline_skips_the_ink_at_every_turn_of_the_display` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `every_underline_style_skips_the_ink_of_descenders` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `every_underline_style_skips_the_ink_of_descenders_on_the_gpu_path` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `the_underlined_bench_rows_draw_the_same_at_every_turn_of_the_display` in [crates/vtium-render/tests/skip_ink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/skip_ink.rs)
- `underline_styles_take_a_sub_parameter` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
