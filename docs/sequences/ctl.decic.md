<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECIC, DECDC, DECBI, DECFI: column insert and delete, back and forward index

| Sequence | What it does |
|---|---|
| `CSI Ps ' }` | DECIC: insert columns |
| `CSI Ps ' ~` | DECDC: delete columns |
| `ESC 6` | DECBI: back index |
| `ESC 9` | DECFI: forward index |

## Parameters

- `Ps`: How many rows, columns, characters or lines the function acts on. A count of 0 counts as 1. The default is 1.

## What vtium does

DECIC and DECDC insert and delete columns at the cursor's column. They act on each row from the top margin to the bottom margin, up to the right margin, and do nothing while the cursor is outside those margins.

DECBI and DECFI move the cursor one column left or right. At the left or right margin, they move the columns between the margins instead.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors that edit a column of text insert and delete columns with these.
- **Note:** DECIC and DECDC in the rows between the top and bottom margins, up to the right margin; DECBI and DECFI move the columns at the left and right margins.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decbi_and_decfi_move_the_cursor_or_the_columns` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `decic_and_decdc_insert_and_delete_columns` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)

## Where to read more

To follow the work on this row, see issue [#55](https://github.com/vtium/vtium/issues/55).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
