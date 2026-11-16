<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 5, 6, 105, 106: special colors (bold, underline, blink, reverse)

| Sequence | What it does |
|---|---|
| `OSC 5 ; c ; spec ST` | set a special color |
| `OSC 6 ; c ; f ST` | turn a special color on or off |
| `OSC 105 ; c ST` | reset a special color |
| `OSC 106 ; c ; f ST` | turn a special color on or off |

## Parameters

- `c`: The special color: 0 bold, 1 underline, 2 blink, 3 reverse and 4 italic.
- `f`: 1 turns the color on, and 0 turns it off.

## What vtium does

vtium reads these strings and does nothing with them. Bold, underlined, blinking and inverse text keep their own colors.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only xterm documents the colors it shows in place of bold, underline, blink and reverse.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
