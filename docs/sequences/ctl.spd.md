<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SPD, select presentation directions (CSI Ps SP S)

| Sequence | What it does |
|---|---|
| `CSI Ps ; Ps SP S` | SPD: select the presentation directions |

## Parameters

- `Ps`: 0 sets the cursor's paragraph left to right, as SCP 1 does, and 3 right to left, as SCP 2 does. vtium ignores other values and the second parameter. The default is 0.

## What vtium does

vtium takes SPD 0 and 3 as other names for SCP 1 and 2. Only a build with the `bidi` feature has SPD, and a build without it ignores the sequence.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs written to ECMA-48's other direction control reach the same paragraph direction.
- **Note:** 0 and 3 only, as aliases of SCP 1 and 2, as the recommendation allows and mintty documents; other values and the second parameter are ignored (ADR-1004 D2); only a build with the bidi feature has it, and a build without it ignores the sequence.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) §8.3.137

## Tests

These tests check what vtium does with the sequences on this page.

- `a_build_without_bidi_answers_0_and_changes_nothing` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)
- `spd_0_and_3_are_scp_1_and_2_and_the_rest_are_ignored` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)

## Where to read more

To follow the work on this row, see issue [#43](https://github.com/vtium/vtium/issues/43).

To read why vtium does what it does here, see [ADR-1004 D2](../architecture/term/ADR-1004-right-to-left-text-the-screen-keeps-logical-order-and-each-paragraph-is-drawn-in-the-order-the-bidirectional-algorithm-gives.md#d2-paragraphs-and-their-properties).
