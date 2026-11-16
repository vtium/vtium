<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 10-20, 26, 50-52, 54, 60-65: alternate fonts, Fraktur, spacing, frames, ideogram marks

| Sequence | What it does |
|---|---|
| `CSI 10 m to CSI 20 m` | primary and alternative fonts, and Fraktur |
| `CSI 26 m` | proportional spacing |
| `CSI 50 m to CSI 52 m` | no proportional spacing, framed and encircled |
| `CSI 54 m` | neither framed nor encircled |
| `CSI 60 m to CSI 65 m` | ideogram underlines, overlines and stress marks |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium reads these codes and changes nothing. The other codes of the same SGR sequence still apply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** alternate fonts, Fraktur and the other ECMA-48 renditions have no common use on a console.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) §8.3.117

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_sgr_forms_leave_the_pen` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
