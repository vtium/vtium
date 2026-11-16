<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTWINOPS 1-7, 9, 10 and 24 and above: move, resize, raise, lower, iconify, maximize, full screen, lines per screen

| Sequence | What it does |
|---|---|
| `CSI 1 t` | XTWINOPS: de-iconify the window |
| `CSI 2 t` | XTWINOPS: iconify the window |
| `CSI 3 ; Px ; Py t` | XTWINOPS: move the window |
| `CSI 4 ; Ph ; Pw t` | XTWINOPS: resize the window in pixels |
| `CSI 5 t` | XTWINOPS: raise the window |
| `CSI 6 t` | XTWINOPS: lower the window |
| `CSI 7 t` | XTWINOPS: refresh the window |
| `CSI 9 ; Ps t` | XTWINOPS: maximize or restore the window |
| `CSI 10 ; Ps t` | XTWINOPS: full screen |

## Parameters

- `Px ; Py`: Where to move the window, in pixels from the screen's top left corner.
- `Ph ; Pw`: The window's height and width in pixels. An omitted value keeps that side, and 0 takes the display's.
- `Ps`: For `CSI 9`, 0 restores the window, 1 maximizes it, and 2 and 3 maximize it vertically and horizontally. For `CSI 10`, 0 leaves full screen, 1 enters it, and 2 toggles it.

## What vtium does

vtium fills the whole screen and has no window, so these sequences have nothing to act on. vtium reads them and does nothing.

A program that wants a grid of a given size in characters sends `CSI 8 ; Ph ; Pw t`, which vtium supports when the configuration allows it. `CSI Ps t` with a Ps of 24 or more is DECSLPP, on the page ctl.page-size.

## Status

- **Status:** refused. vtium deliberately does not implement this.
- **Why:** a console has one screen and no window to move, raise, iconify or resize.
- **Note:** a program's resize is the XTWINOPS 8 row.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `window_reports_vtium_has_no_window_for_are_ignored` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Refusals"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#refusals).
