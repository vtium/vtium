<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Protection: DECSCA, DECSED, DECSEL

| Sequence | What it does |
|---|---|
| `CSI Ps " q` | DECSCA: protect the characters printed next |
| `CSI ? Ps J` | DECSED: erase the unprotected cells of the display |
| `CSI ? Ps K` | DECSEL: erase the unprotected cells of the line |

## Parameters

- `Ps`: For DECSCA, 1 protects the characters printed next, 0 and 2 end the protection, and vtium ignores other values. For DECSED and DECSEL, which part to erase, as for ED and EL. The default is 0.

## What vtium does

DECSED, DECSEL and DECSERA leave the cells that DECSCA protected, while ED, EL and DECERA erase them like any other cell. SGR 0 keeps the protection, and DECSTR and RIS end it.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** forms keep their labels from being erased.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `decsca_protects_printed_cells_from_decsed_and_decsel` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- `decsera_leaves_protected_cells` in [crates/vtium-term/tests/rect.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/rect.rs)
- [tools/conformance/vtium/cases/erase-decsed-decsel.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/erase-decsed-decsel.case)

## Where to read more

To follow the work on this row, see issue [#56](https://github.com/vtium/vtium/issues/56).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
