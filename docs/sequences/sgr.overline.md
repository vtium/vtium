<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 53 and 55: overline

| Sequence | What it does |
|---|---|
| `CSI 53 m` | overline |
| `CSI 55 m` | no overline |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws a line along the top of each cell of overlined text.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** ECMA-48 defines it, and tables and status lines draw a line above text with it.
- **Note:** drawn along the cell's top rows in the strike's color.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/); WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `an_overline_names_its_sprite_in_the_strikes_slot` in [crates/vtium-render/tests/gpu_compose.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/gpu_compose.rs)
- `an_overline_and_a_strike_draw_together_in_the_strikes_color` in [crates/vtium-render/tests/renditions.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/renditions.rs)
- `an_overline_blinks_with_the_decorations` in [crates/vtium-render/tests/renditions.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/renditions.rs)
- `an_overline_draws_the_cells_top_row` in [crates/vtium-render/tests/renditions.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/renditions.rs)
- `overline_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#37](https://github.com/vtium/vtium/issues/37).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
