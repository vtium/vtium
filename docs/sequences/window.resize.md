<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 8: resize the text area in characters

| Sequence | What it does |
|---|---|
| `CSI 8 ; Ph ; Pw t` | XTWINOPS: resize the text area in characters |

## Parameters

- `Ph`: The number of rows. An omitted value keeps the rows there are, and 0 takes as many as the display holds.
- `Pw`: The number of columns. An omitted value keeps the columns there are, and 0 takes as many as the display holds.

## What vtium does

vtium changes the grid only when the `allow-window-ops` key is on, and the key is off by default. Otherwise it reads the sequence and changes nothing.

vtium meets the request by changing the font size. It draws at the largest zoom step whose grid holds the rows and columns asked for, on a grid of exactly that size. When the displays cannot hold a side even at the smallest step, that side takes what they hold.

vtium resizes the pty before it answers a size report that follows in the same write, such as `CSI 18 t`. The grid lasts until the next zoom, or until the session that asked for it ends.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program sets the grid's size, which vtium meets by changing the font size.
- **Note:** behind allow-window-ops, off by default: vtium draws at the largest zoom step whose grid holds the rows and columns asked for, on exactly that grid, and resizes the pty before CSI 18 t is answered; a side left out keeps its length and 0 fills the display; a side the displays cannot hold at the smallest step takes what they hold there; the grid lasts until a zoom or the end of the session that asked; the grid is exact, and the cell size vtium reports (CSI 16 t, the pty's pixel size) stays the base font's cell while a display may draw larger cells, as a head with several displays does (ADR-1401 D1).
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_request_for_a_text_area_waits_for_the_host` in [crates/vtium-term/tests/window.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/window.rs)
- `a_request_is_ignored_until_the_host_turns_window_operations_on` in [crates/vtium-term/tests/window.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/window.rs)
- `a_programs_request_for_a_grid_takes_the_zoom_path` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_request_with_allow_window_ops_off_changes_nothing` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_size_the_display_cannot_hold_takes_what_it_holds_at_the_smallest_size` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_text_area_request_draws_the_strike_whose_grid_holds_it` in [crates/vtium/tests/strike_size.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/strike_size.rs)

## Where to read more

To follow the work on this row, see issue [#44](https://github.com/vtium/vtium/issues/44).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
