<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 51 (reserved for Emacs), OSC 176 (foot app id), OSC 555 (foot flash), OSC 3008, OSC 7501 (contour)

| Sequence | What it does |
|---|---|
| `OSC 51 ; Pt ST` | reserved for Emacs |
| `OSC 176 ; Pt ST` | set an application id |
| `OSC 555 ST` | flash the screen |
| `OSC 3008 ; Pt ST` | report a context |
| `OSC 7501 ; Pt ST` | report a program's status |

## What vtium does

vtium reads these strings and does nothing with them.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** each belongs to one program, and none has a use on a console.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); contour osc-3008-context, program-status

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
