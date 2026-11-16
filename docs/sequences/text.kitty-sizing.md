<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty text sizing (OSC 66)

| Sequence | What it does |
|---|---|
| `OSC 66 ; Pm ; Pt ST` | write text at a larger size |

## Parameters

- `Pm`: `key=value` pairs separated by `:`. `s` is the scale, 1 to 7, and `w` the width in cells, 0 to 7. `n` over `d` is a fraction of the scale, each 0 to 15. `v` and `h` align the text vertically and horizontally, each 0 to 2. The default is an empty list, which writes the text at its normal size.
- `Pt`: The text, at most 4096 bytes of UTF-8.

## What vtium does

vtium stores the block of cells that one sequence writes as one unit, so an edit that would move, clear or overwrite only part of it erases it whole. vtium ignores a key it does not know. A known key with a value out of its range discards the whole sequence, text included.

The renderer does not draw a block at its size yet. OSC 66 has no reply.

## Status

- **Status:** partial. vtium implements part of this. The note says what is missing.
- **Why:** large text, such as a heading or a banner at the login prompt, on a grid whose font size serves every program.
- **Note:** ADR-1308 decides it: vtium takes the keys s, w, n, d, v and h; vtium-term interprets OSC 66 and stores the block one sequence writes as one character, and an edit that would move, clear or overwrite only part of it erases it whole; the renderer does not draw a run at its size yet.
- **Specifications:** kitty text-sizing-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_run_whose_rows_scroll_off_with_no_scrollback_goes_whole` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `a_run_whose_top_row_scrolled_into_the_scrollback_keeps_it_there` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `a_scroll_inside_the_margins_moves_a_run_inside_them_and_erases_one_a_margin_cuts` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `a_selective_erase_spares_a_protected_run_and_erases_another_whole` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `decic_moves_a_tall_run_whole_and_ich_on_one_of_its_rows_erases_it` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `il_erases_a_run_that_the_bottom_margin_cuts` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `rep_after_a_run_repeats_nothing` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `the_frame_carries_each_runs_place_scale_width_fraction_and_alignment` in [crates/vtium-term/src/terminal/text_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/text_size.rs)
- `a_mark_that_one_row_cannot_hold_leaves_the_runs_text_on_every_row` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_narrower_screen_keeps_whole_clusters_of_a_runs_text` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_run_below_the_screen_that_growing_would_cut_goes_whole` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_run_on_a_full_line_keeps_whole_clusters` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_run_over_clusters_has_their_room_for_its_text` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_run_that_a_rewrap_pushes_below_the_screen_goes_whole` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_screen_shorter_than_a_run_erases_it` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_shorter_alternate_screen_erases_a_run_it_cuts` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_shorter_main_screen_erases_a_run_it_cuts_and_growing_brings_back_none` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_shorter_screen_erases_a_run_it_cuts_on_the_screen_not_shown` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_shorter_screen_keeps_a_run_it_does_not_cut` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `a_tall_run_over_a_double_width_line_is_one_row_high` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccara_over_part_of_a_run_keeps_the_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccra_copies_a_whole_run_as_a_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccra_copies_a_whole_tall_run_as_a_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccra_of_part_of_a_run_copies_empty_cells` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccra_onto_a_runs_lower_row_erases_the_whole_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `deccra_whose_target_clips_a_run_copies_no_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `decfra_onto_a_runs_lower_row_erases_the_whole_run` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `declrmm_keeps_a_tall_run_beside_a_double_width_line_whole` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- `random_runs_and_edits_keep_every_run_whole` in [crates/vtium-term/tests/sized_runs.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sized_runs.rs)
- [tools/conformance/vtium/cases/osc66-bottom.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-bottom.case)
- [tools/conformance/vtium/cases/osc66-edit.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-edit.case)
- [tools/conformance/vtium/cases/osc66-erase.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-erase.case)
- [tools/conformance/vtium/cases/osc66-fraction.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-fraction.case)
- [tools/conformance/vtium/cases/osc66-line-size.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-line-size.case)
- [tools/conformance/vtium/cases/osc66-margin.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-margin.case)
- [tools/conformance/vtium/cases/osc66-overwrite.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-overwrite.case)
- [tools/conformance/vtium/cases/osc66-reflow.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-reflow.case)
- [tools/conformance/vtium/cases/osc66-scale.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-scale.case)
- [tools/conformance/vtium/cases/osc66-width.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/osc66-width.case)

## Where to read more

To follow the work on this row, see issue [#60](https://github.com/vtium/vtium/issues/60).

To read why vtium does what it does here, see [ADR-1308 D2](../architecture/render/ADR-1308-a-run-of-cells-that-draws-one-scaled-glyph-is-one-unit-for-every-edit-and-a-ligature-is-drawn-across-its-cells-under-an-option-that-is-off-by-default.md#d2-osc-66-and-its-keys).
