<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 221 and 222: reset bold or faint alone

| Sequence | What it does |
|---|---|
| `CSI 221 m` | turn off bold alone |
| `CSI 222 m` | turn off dim alone |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium reads these codes and changes nothing. SGR 22 turns off bold and dim together.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only kitty documents it.
- **Note:** the work on SGR overline, superscript and subscript could take it.
- **Specifications:** kitty misc-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_sgr_forms_leave_the_pen` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
