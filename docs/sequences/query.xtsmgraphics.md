<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTSMGRAPHICS (CSI ? Pi ; Pa ; Pv S)

| Sequence | What it does |
|---|---|
| `CSI ? Pi ; Pa ; Pv S` | XTSMGRAPHICS: set or read a graphics attribute |

## Parameters

- `Pi`: The item: 1 for the number of sixel color registers, 2 for the largest sixel image in pixels.
- `Pa`: The action: 1 reads the value in force, 2 resets it, 3 sets it to `Pv`, and 4 reads the largest value allowed.
- `Pv`: The value that action 3 sets.

## What vtium does

vtium answers `CSI ? Pi ; Ps ; Pv S`, with the value or values after the status `Ps`. The status is 0 for success, 1 for an item vtium does not know, 2 for an action it does not know, and 3 for a value it cannot take.

A program can set the number of color registers from 2 up to the `num-color-registers` key. The new number takes effect at the next image. The image size reads `max-graphic-size`, and a program cannot reset or set it.

While vtium does not offer sixel, and in a build without images, every item answers status 1.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** sixel programs ask the number of color registers and the picture size.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `a_register_count_xtsmgraphics_sets_takes_effect_at_the_next_picture` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `the_shared_registers_start_again_after_each_change_of_their_settings` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `xtsmgraphics_answers_status_1_in_a_build_without_images` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)
- `xtsmgraphics_reads_sets_and_resets_the_register_count` in [crates/vtium-term/tests/sixel.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sixel.rs)

## Where to read more

To follow the work on this row, see issue [#48](https://github.com/vtium/vtium/issues/48).

To read why vtium does what it does here, see [ADR-1100 D10](../architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md#d10-the-images-feature-and-what-minspec-carries).
