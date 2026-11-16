<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 15, 16, 18 and their resets: Tektronix colors

| Sequence | What it does |
|---|---|
| `OSC 15 ; spec ST` | set the Tektronix foreground |
| `OSC 16 ; spec ST` | set the Tektronix background |
| `OSC 18 ; spec ST` | set the Tektronix cursor's color |
| `OSC 115 ST` | reset the Tektronix foreground |
| `OSC 116 ST` | reset the Tektronix background |
| `OSC 118 ST` | reset the Tektronix cursor's color |

## Parameters

- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.

## What vtium does

vtium reads these strings and does nothing with them, and a query gets no reply. In a list that starts at a lower number, such as `OSC 10`, vtium skips their places and sets the colors after them.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** vtium has no Tektronix mode for these colors to apply to.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
