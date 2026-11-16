<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 38, 48: 256-color and direct color, semicolon and colon forms

| Sequence | What it does |
|---|---|
| `CSI 38 : 5 : Ps m` | foreground from the 256-color palette |
| `CSI 48 : 5 : Ps m` | background from the 256-color palette |
| `CSI 38 : 2 : Pi : Pr : Pg : Pb m` | foreground as red, green and blue |
| `CSI 48 : 2 : Pi : Pr : Pg : Pb m` | background as red, green and blue |

## Parameters

- `Ps`: An index into the 256-color palette, from 0 to 255.
- `Pi`: The color space's id, which T.416 places before the color. vtium reads it and does not use it, and it may be left empty.
- `Pr, Pg, Pb`: The red, green and blue values, each from 0 to 255.

## What vtium does

vtium reads both the colon forms above and the semicolon forms that most programs send, `CSI 38 ; 5 ; Ps m` and `CSI 38 ; 2 ; Pr ; Pg ; Pb m`. In the semicolon forms the color space's id is left out. vtium also reads `CSI 38 : 2 : Pr : Pg : Pb m`, which some programs send, with no field for the color space's id, and tells it apart by its count of fields. A direct color is kept as it is, and a palette index is looked up when the cell is drawn.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** themes, editors and prompts use 256 colors and direct color.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) (T.416 colon form)

## Tests

These tests check what vtium does with the sequences on this page.

- `extended_colors_take_both_forms` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
