<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECSCUSR cursor styles, 7 the configured style

| Sequence | What it does |
|---|---|
| `CSI Ps SP q` | DECSCUSR: set the cursor style |

## Parameters

- `Ps`: The style: 0 and 1 a blinking block, 2 a steady block, 3 a blinking underline, 4 a steady underline, 5 a blinking bar, 6 a steady bar, and 7 the style vtium's configuration gives. The default is 0.

## What vtium does

A session starts with the cursor style that vtium's configuration gives: a block that blinks while the `blink` key is on and stays steady while it is off. A style that DECSCUSR selects blinks or stays steady as its number says, whatever `blink` says. DECSCUSR changes the style as the VT510 manual decides, so `CSI 0 SP q` and `CSI SP q` select a blinking block. `CSI 7 SP q` returns to the configured style, as ctlseqs gives it. vtium ignores a value past 7 and keeps the style it has.

DECSTR and RIS return the cursor to the configured style.

DECSC does not save the cursor style, so DECRC leaves it as it is. DECRQSS reports the style the console shows. It reports the configured style as `1 q` while it blinks and `2 q` while it is steady.

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
- `a_style_a_program_chose_blinks_as_it_says` in [crates/vtium/src/app/cursor_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/cursor_blink.rs)

## Where to read more

To follow the work on this row, see issues [#12](https://github.com/vtium/vtium/issues/12) and [#16](https://github.com/vtium/vtium/issues/16).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
