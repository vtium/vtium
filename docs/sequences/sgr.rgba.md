<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 38:6, 48:6, 58:6: RGBA color

| Sequence | What it does |
|---|---|
| `CSI 38 : 6 : Pr : Pg : Pb : Pa m` | foreground with an alpha value |
| `CSI 48 : 6 : Pr : Pg : Pb : Pa m` | background with an alpha value |
| `CSI 58 : 6 : Pr : Pg : Pb : Pa m` | underline color with an alpha value |

## What vtium does

vtium reads these forms and leaves the colors as they were. The rest of the SGR sequence still applies.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only WezTerm documents it.
- **Specifications:** WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `the_ignored_sgr_forms_leave_the_pen` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
