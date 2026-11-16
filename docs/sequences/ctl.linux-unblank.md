<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# The Linux console's unblank, CSI 13 ]

| Sequence | What it does |
|---|---|
| `CSI 13 ]` | wake the screen |

## What vtium does

vtium wakes its displays, as a key press does, and starts the blank's countdown again. It acts on the sequence only while the `program-blank` key is on.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs written for the Linux console wake the displays with it.
- **Note:** a program wakes the displays, as a key does, while `program-blank` is on.
- **Specifications:** [console_codes(4)](https://man7.org/linux/man-pages/man4/console_codes.4.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `csi_13_wakes_the_screen` in [crates/vtium-term/tests/blank.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/blank.rs)
- `a_programs_timeout_blanks_past_the_pointers_hide_and_its_wake_ends_it` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_programs_wake_ends_the_blank` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issue [#79](https://github.com/vtium/vtium/issues/79).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
