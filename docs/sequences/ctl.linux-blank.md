<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# The Linux console's blank timeout, CSI 9 ; n ] (setterm --blank)

| Sequence | What it does |
|---|---|
| `CSI 9 ; Ps ]` | set the screen blank's timeout |

## Parameters

- `Ps`: The timeout in minutes, at most 60. A value of 0 turns blanking off. The default is 0.

## What vtium does

vtium blanks its displays after that many idle minutes, and the sequence also wakes the displays. vtium acts on it only while the `program-blank` key is on. RIS returns the timeout to the configured one.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** setterm --blank and programs written for the Linux console time the screen blank with it.
- **Note:** a program times the DPMS blank, in minutes, while `program-blank` is on.
- **Specifications:** [console_codes(4)](https://man7.org/linux/man-pages/man4/console_codes.4.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `csi_9_sets_the_timeout_in_minutes_and_wakes_the_screen` in [crates/vtium-term/tests/blank.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/blank.rs)
- `a_programs_minute_blanks_past_the_pointers_hide` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_programs_timeout_holds_until_ris` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issue [#79](https://github.com/vtium/vtium/issues/79).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
