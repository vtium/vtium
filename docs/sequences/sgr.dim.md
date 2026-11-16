<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 2: dim

| Sequence | What it does |
|---|---|
| `CSI 2 m` | dim, also called faint |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws dim text in a color halfway between the text's color and its background. SGR 22 turns dim off together with bold.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as shells and git dim secondary text.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `bold_and_dim_draw_the_bold_glyph_dimmed` in [crates/vtium-render/tests/dim.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/dim.rs)
- `dim_text_draws_halfway_to_its_background` in [crates/vtium-render/tests/dim.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/dim.rs)
- `reverse_video_dims_the_swapped_foreground` in [crates/vtium-render/tests/dim.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/dim.rs)
- `the_cursor_draws_over_the_dimmed_cell` in [crates/vtium-render/tests/dim.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/dim.rs)
- `bold_and_dim_dim_the_bright_color` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `dim_draws_the_foreground_halfway_to_the_background` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `dim_follows_inverse_video` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `dim_follows_the_selection` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `dim_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
