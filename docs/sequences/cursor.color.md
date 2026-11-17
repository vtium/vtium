<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Cursor color (OSC 12)

| Sequence | What it does |
|---|---|
| `OSC 12 ; spec ST` | set the cursor's color |
| `OSC 12 ; ? ST` | ask for the cursor's color |
| `OSC 112 ST` | reset the cursor's color |

## Parameters

- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name.

## What vtium does

vtium draws the cursor in the color OSC 12 sets. A cursor with no color of its own draws its cell inverted, and the query then answers the foreground. OSC 112 returns the cursor to its configured color.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** themes set the cursor's color.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_colored_block_takes_its_color` in [crates/vtium-render/tests/cursor.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/cursor.rs)
- `a_cursor_color_change_at_rest_redraws_its_cell` in [crates/vtium-render/tests/damage.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/damage.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
