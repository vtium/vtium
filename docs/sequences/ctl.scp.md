<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SCP, select character path (CSI Ps SP k)

| Sequence | What it does |
|---|---|
| `CSI Ps ; Ps SP k` | SCP: select the character path |

## Parameters

- `Ps`: 1 sets the cursor's paragraph left to right, and 2 right to left. 0 returns it to the direction that vtium.conf's `text-direction` sets. vtium ignores other values and the second parameter. The default is 0.

## What vtium does

vtium sets the direction of the paragraph that holds the cursor, and draws the paragraph in that order. Only a build with the `bidi` feature has SCP, and a build without it ignores the sequence.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program showing right-to-left text sets its paragraphs right to left.
- **Note:** 1 sets the cursor's paragraph left to right and 2 right to left, and 0 or no parameter returns it to vtium.conf's `text-direction`; other values and the second parameter are ignored (ADR-1004 D2); only a build with the bidi feature has it, and a build without it ignores the sequence.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) §8.3.111

## Tests

These tests check what vtium does with the sequences on this page.

- `a_build_without_bidi_answers_0_and_changes_nothing` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)
- `decstr_and_ris_return_the_paragraph_to_its_defaults` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)
- `scp_sets_the_paragraph_direction_at_its_first_position` in [crates/vtium-term/tests/bidi.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi.rs)
- `a_mixed_line_draws_from_the_right_edge_in_a_right_to_left_paragraph` in [crates/vtium-term/tests/bidi_frame.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/bidi_frame.rs)

## Where to read more

To follow the work on this row, see issue [#43](https://github.com/vtium/vtium/issues/43).

To read why vtium does what it does here, see [ADR-1004 D2](../architecture/term/ADR-1004-right-to-left-text-the-screen-keeps-logical-order-and-each-paragraph-is-drawn-in-the-order-the-bidirectional-algorithm-gives.md#d2-paragraphs-and-their-properties).
