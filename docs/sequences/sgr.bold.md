<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 0, 1 and 22: reset, bold, normal intensity

| Sequence | What it does |
|---|---|
| `CSI 0 m` | reset every attribute and both colors |
| `CSI 1 m` | bold |
| `CSI 22 m` | normal intensity: neither bold nor dim |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

SGR 0, and SGR with no parameter, return the pen to its defaults: no attribute set, the default foreground and the default background. SGR 1 draws the text that follows in the font's bold face, or in a bold that vtium makes from the regular face when the font has none. SGR 22 turns off both bold and dim.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** every program that colors its output uses bold.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) §8.3.117

## Tests

These tests check what vtium does with the sequences on this page.

- `style_asks_are_answered_in_their_style` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `synthetic_bold_emboldens_an_outline_from_side_to_side` in [crates/vtium-font-helper/tests/style.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font-helper/tests/style.rs)
- `a_sets_bold_italic_comes_from_its_regular_strike` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `bold_italic_widens_inside_the_cell_before_it_leans` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `bold_italic_widens_then_leans` in [crates/vtium-font/tests/italic.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-font/tests/italic.rs)
- `bold_and_dim_dim_the_bright_color` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `bold_brightens_before_inverse_video` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `bold_brightens_the_first_eight_palette_colors_of_the_foreground` in [crates/vtium-term/src/frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/frame.rs)
- `bold_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)
- `italic_and_bold_text_is_asked_for_in_its_style` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
