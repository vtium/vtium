<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 0, 1, 2: icon name and title

| Sequence | What it does |
|---|---|
| `OSC 0 ; Pt ST` | set the icon name and the title |
| `OSC 1 ; Pt ST` | set the icon name |
| `OSC 2 ; Pt ST` | set the title |

## Parameters

- `Pt`: The text of the icon name or the title.

## What vtium does

vtium fills the screen and shows no title bar, so it reads these strings and keeps nothing. `CSI 21 t` therefore always reports an empty title.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** a console has no title bar.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
