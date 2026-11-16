<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 3, and Sun's OSC I, l, L

| Sequence | What it does |
|---|---|
| `OSC 3 ; Pt ST` | set an X property on the window |
| `OSC I Pt ST` | set the icon from a file, in Sun's form |
| `OSC l Pt ST` | set the title, in Sun's form |
| `OSC L Pt ST` | set the icon name, in Sun's form |

## What vtium does

vtium has no X window, so it reads these strings and does nothing.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has no X window whose properties these set.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
