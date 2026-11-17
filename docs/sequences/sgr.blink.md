<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 5 and 25: blink

| Sequence | What it does |
|---|---|
| `CSI 5 m` | blink |
| `CSI 25 m` | no blink |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium blinks the text that follows SGR 5. The `blink-text` key sets how the text looks in the blink's off phase, and `blink-decorations` sets whether its underline and strike blink with it. SGR 25 stops both SGR 5's blink and SGR 6's rapid blink.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs flash a warning or a prompt with it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `blinking_text_hides_as_blank_cells_but_under_the_cursor` in [crates/vtium-render/tests/blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/blink.rs)
- `blink_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
