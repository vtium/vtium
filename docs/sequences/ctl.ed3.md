<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# ED 3 empties the scrollback

| Sequence | What it does |
|---|---|
| `CSI 3 J` | ED 3: erase the scrollback |

## What vtium does

vtium empties the scrollback and leaves the screen as it is. The `clear` command sends ED 3 after it erases the screen, so the history goes too.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** the clear command empties the scrollback with it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `ed_3_empties_the_scrollback` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)

## Where to read more

To follow the work on this row, see issue [#11](https://github.com/vtium/vtium/issues/11).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
