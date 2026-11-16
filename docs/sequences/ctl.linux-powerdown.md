<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# The Linux console's VESA powerdown interval, CSI 14 ; n ] (setterm --powerdown)

| Sequence | What it does |
|---|---|
| `CSI 14 ; Ps ]` | set the VESA powerdown interval |

## What vtium does

vtium reads the sequence and does nothing with it. vtium has one blanking step, which `CSI 9 ; Ps ]` times.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has one blanking step, which the blank timeout times.
- **Specifications:** [console_codes(4)](https://man7.org/linux/man-pages/man4/console_codes.4.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `csi_14_is_ignored` in [crates/vtium-term/tests/blank.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/blank.rs)

## Where to read more

To follow the work on this row, see issue [#79](https://github.com/vtium/vtium/issues/79).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
