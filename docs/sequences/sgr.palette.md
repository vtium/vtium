<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 30-37, 39, 40-47, 49, 90-97, 100-107: palette colors

| Sequence | What it does |
|---|---|
| `CSI 30 m to CSI 37 m` | foreground from palette colors 0 to 7 |
| `CSI 39 m` | default foreground |
| `CSI 40 m to CSI 47 m` | background from palette colors 0 to 7 |
| `CSI 49 m` | default background |
| `CSI 90 m to CSI 97 m` | foreground from the bright palette colors 8 to 15 |
| `CSI 100 m to CSI 107 m` | background from the bright palette colors 8 to 15 |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium keeps the palette color's number in the cell and looks the color up when it draws the cell. So a program that changes a palette color with OSC 4 recolors the text already on the screen. The `palette` key sets the colors that the palette starts with.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** every program that colors its output uses the 16 palette colors.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `palette_colors_are_set_and_returned_to_the_default` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
