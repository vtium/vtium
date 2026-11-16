<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 58 and 59: underline color

| Sequence | What it does |
|---|---|
| `CSI 58 : 5 : Ps m` | underline color from the 256-color palette |
| `CSI 58 : 2 : : Pr : Pg : Pb m` | underline color as red, green and blue |
| `CSI 59 m` | underline in the text's color |

## Parameters

- `Ps`: An index into the 256-color palette, from 0 to 255.
- `Pr, Pg, Pb`: The red, green and blue values, each from 0 to 255.

## What vtium does

vtium draws every underline style in the color SGR 58 sets, and SGR 59 returns the underline to the text's color. vtium also reads the semicolon forms, `CSI 58 ; 5 ; Ps m` and `CSI 58 ; 2 ; Pr ; Pg ; Pb m`.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors color an error's underline apart from its text.
- **Specifications:** kitty underlines

## Tests

These tests check what vtium does with the sequences on this page.

- `sgr_58_and_59_set_the_underline_color` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
