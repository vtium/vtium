<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Color scheme query, CSI ? 996 n, with mode 2031

| Sequence | What it does |
|---|---|
| `CSI ? 996 n` | ask whether the colors are dark or light |

## What vtium does

vtium answers `CSI ? 997 ; 1 n` when the default background is dark and `CSI ? 997 ; 2 n` when it is light. The background is dark when its luma, by ITU-R BT.601, is below half.

While mode 2031 is set, vtium sends the same report by itself each time the default background crosses between dark and light. A change that keeps the background dark, or light, sends nothing.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program asks whether the console's palette is dark or light before it picks its colors.
- **Specifications:** contour color-palette-update-notifications; [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_scheme_query_answers_by_the_backgrounds_luma` in [crates/vtium-term/tests/inband.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/inband.rs)

## Where to read more

To follow the work on this row, see issue [#57](https://github.com/vtium/vtium/issues/57).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
