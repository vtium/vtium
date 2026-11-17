<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty graphics feature queries (a=q with e)

| Sequence | What it does |
|---|---|
| `APC G a=q,e=Pt,keys ST` | ask which keys an action takes |

## Parameters

- `e`: The action the program asks about, such as `t` or `p`.

## What vtium does

vtium answers each of the protocol's eight actions. The answer lists each key of the query that a real command of that action takes, with its value. vtium writes every value in the answer itself, and `e` on any other action is refused.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs ask which keys and values of an action the terminal takes before they rely on them.
- **Note:** vtium answers each of the eight actions and reports the keys a real command of that action takes; e on any other action is refused.
- **Specifications:** kitty graphics-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_feature_query_ignores_its_payload_and_m_and_stores_nothing` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `a_feature_query_leaves_an_open_upload_as_it_was` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `a_feature_query_without_i_or_under_q_gets_no_answer` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `an_action_vtium_does_not_know_is_answered_without_e` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `defaults_values_vtium_refuses_and_keys_the_action_does_not_read_are_left_out` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `e_on_an_action_other_than_a_query_is_refused_and_does_nothing` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `each_action_reports_the_keys_and_values_a_command_of_it_takes` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `palette_indices_are_reported_for_a_picture_and_not_for_a_frame` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `the_specifications_two_examples_get_its_own_answers` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)
- `the_widest_feature_query_answer_fits_the_answer_bound` in [crates/vtium-image/src/protocol/kitty/tests/features.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/features.rs)

## Where to read more

To follow the work on this row, see issue [#49](https://github.com/vtium/vtium/issues/49).

To read why vtium does what it does here, see [ADR-1100 D14](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d14-vtium-presents-itself-as-a-kitty-compatible-terminal).
