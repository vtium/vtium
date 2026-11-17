<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty multiple cursors (CSI > Ps ; ... SP q)

| Sequence | What it does |
|---|---|
| `CSI > Ps ; Pt SP q` | add or remove extra cursors |

## Parameters

- `Ps`: The shape of the extra cursors: 0 none, which removes them, 1 a block, 2 a bar, 3 an underline, and 29 the main cursor's shape. kitty's specification gives 30, 40 and 100 other uses: the colors of the extra cursors, and a query.
- `Pt`: Where the cursors go, as groups separated by `;`. Each group is a coordinate type and its coordinates, joined by `:`: 0 for the main cursor's cell, 2 for cells written as row and column pairs, and 4 for rectangles written as top, left, bottom and right.

## What vtium does

vtium reads the sequence and does nothing with it. The screen keeps its one cursor.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only kitty documents it.
- **Specifications:** kitty multiple-cursors-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `title_modes_the_pointer_and_extra_cursors_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
