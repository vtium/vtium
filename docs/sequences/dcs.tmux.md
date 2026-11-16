<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# tmux control mode (DCS 1000 p)

| Sequence | What it does |
|---|---|
| `DCS 1000 p Pt ST` | tmux's control mode |

## Parameters

- `Pt`: tmux's control mode output, which starts with `%begin`.

## What vtium does

vtium reads the string and does nothing with it. tmux runs on the console as a plain program.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only WezTerm documents its use of it, and tmux runs on a console as a plain program.
- **Specifications:** iTerm2 tmux integration; WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `control_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
