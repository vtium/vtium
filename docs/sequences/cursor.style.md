<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSCUSR cursor styles, values outside 0-6 taken as xterm takes them

| Sequence | What it does |
|---|---|
| `CSI Ps SP q` | DECSCUSR: set the cursor style |

## Parameters

- `Ps`: The style: 0 the default style, 1 a blinking block, 2 a steady block, 3 a blinking underline, 4 a steady underline, 5 a blinking bar and 6 a steady bar. The default is 0.

## What vtium does

vtium draws the cursor in the style DECSCUSR sets. The default style is a block that blinks while the `blink` key is on. vtium ignores a value past 6 and keeps the style it has.

DECSC does not save the cursor style, so DECRC leaves it as it is. DECRQSS reports the style that DECSCUSR set.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors such as vim show a bar in insert mode and a block in normal mode.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_bar_draws_over_the_letter_along_the_cells_left_edge` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `a_blinking_bar_keeps_its_letter_in_both_phases` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `a_block_on_a_trailing_half_covers_the_wide_character` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `a_block_swaps_the_cells_colors` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `an_underline_cursor_toggles_a_single_underline` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `a_cursor_shape_change_at_rest_redraws_its_cell` in [crates/vtium-render/tests/damage.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/damage.rs)
- `a_bar_cursor_keeps_its_letter_on_the_gpu_path` in [crates/vtium-render/tests/gpu_line_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/gpu_line_size.rs)
- `decscusr_sets_the_cursor_style` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issues [#12](https://github.com/vtium/vtium/issues/12) and [#16](https://github.com/vtium/vtium/issues/16).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
