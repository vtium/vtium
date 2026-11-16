<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# BEL

| Sequence | What it does |
|---|---|
| `BEL` | ring the bell (0x07) |

## What vtium does

While mode 1042 is set, vtium rings the VT bell for a BEL, when its bell setting is on. Mode 1042 starts set, so a program's BEL rings without the program asking. A BEL from any console rings the bell, whether that console is shown or hidden.

While mode 1042 is reset, vtium ignores BEL.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ring the bell to call the user.
- **Note:** a BEL from any console rings the VT bell, the console shown or a hidden one.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/)

## Tests

These tests check what vtium does with the sequences on this page.

- `bell_rings_the_vt_at_a_bel_while_mode_1042_lets_it_through` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issues [#19](https://github.com/vtium/vtium/issues/19) and [#21](https://github.com/vtium/vtium/issues/21).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
