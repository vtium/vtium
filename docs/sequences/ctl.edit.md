<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Cursor movement and editing: CUU, CUD, CUF, CUB, CNL, CPL, CHA, HPA, HPR, VPA, VPR, CUP, HVP, ED, EL, ECH, ICH, DCH, IL, DL, SU, SD, IND, RI, NEL

| Sequence | What it does |
|---|---|
| `CSI Ps A` | CUU: cursor up |
| `CSI Ps B` | CUD: cursor down |
| `CSI Ps C` | CUF: cursor forward |
| `CSI Ps D` | CUB: cursor back |
| `CSI Ps E` | CNL: cursor to the start of a following line |
| `CSI Ps F` | CPL: cursor to the start of a preceding line |
| `CSI Ps G` | CHA: cursor to a column |
| `` CSI Ps ` `` | HPA: cursor to a column, as CHA does |
| `CSI Ps a` | HPR: cursor forward, as CUF does |
| `CSI Ps d` | VPA: cursor to a row |
| `CSI Ps e` | VPR: cursor down, as CUD does |
| `CSI Ps ; Ps H` | CUP: cursor to a row and column |
| `CSI Ps ; Ps f` | HVP: cursor to a row and column, as CUP does |
| `CSI Ps J` | ED: erase in display |
| `CSI Ps K` | EL: erase in line |
| `CSI Ps X` | ECH: erase characters |
| `CSI Ps @` | ICH: insert blank characters |
| `CSI Ps P` | DCH: delete characters |
| `CSI Ps L` | IL: insert lines |
| `CSI Ps M` | DL: delete lines |
| `CSI Ps S` | SU: scroll up |
| `CSI Ps T` | SD: scroll down |
| `ESC D` | IND: index, down one row |
| `ESC M` | RI: reverse index, up one row |
| `ESC E` | NEL: next line |

## Parameters

- `Ps`: In CUU, CUD, CUF, CUB, CNL, CPL, HPR, VPR, ECH, ICH, DCH, IL, DL, SU and SD, how many rows, columns, characters or lines the function acts on. A count of 0 counts as 1. The default is 1.
- `Ps`: In ED and EL, which part to erase: 0 from the cursor to the end, 1 from the start to the cursor, and 2 all of it. ED 3 has a page of its own, ctl.ed3. The default is 0.
- `Ps`: In CHA and HPA, the column, and in VPA, the row, counted from 1. A value of 0 counts as 1. The default is 1.
- `Ps ; Ps`: In CUP and HVP, the row and then the column, both counted from 1. An omitted or 0 row or column is 1. The default is 1 ; 1, the top left corner.

## What vtium does

These functions move the cursor, erase text, and insert or delete characters and lines. vtium acts on each only when it has no private marker such as `?` and no intermediate byte, since those make a different function.

ED 0 erases from the cursor to the end of the screen, ED 1 from the start of the screen to the cursor, and ED 2 the whole screen. EL 0, 1 and 2 erase the same parts of the cursor's line. An erased cell is empty and takes the pen's background color.

IL, DL, SU, SD, IND, RI and NEL move lines only inside the scroll region that DECSTBM sets. When text scrolls up on the main screen, the line that leaves the region's top enters the scrollback. While left and right margins narrower than the screen are set, only the cells between them move, and nothing enters the scrollback. The 8-bit controls 0x84, 0x8D and 0x85 work as IND, RI and NEL.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** every full-screen program moves the cursor and edits the screen with these.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/); [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `cursor_movement_stops_at_the_screen_and_the_margins` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- `erase_in_display_line_and_characters` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- `insert_and_delete_characters_move_the_rest_of_the_line` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- `insert_and_delete_lines_move_the_region_only` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- `scrolling_moves_the_region_and_the_scrollback_takes_the_top` in [crates/vtium-term/tests/screen.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/screen.rs)
- [tools/conformance/vtium/cases/cursor-cha-vpa-vpr.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-cha-vpa-vpr.case)
- [tools/conformance/vtium/cases/cursor-cnl-cpl-clamp.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-cnl-cpl-clamp.case)
- [tools/conformance/vtium/cases/cursor-cnl-cpl.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-cnl-cpl.case)
- [tools/conformance/vtium/cases/cursor-cup-bounds.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-cup-bounds.case)
- [tools/conformance/vtium/cases/cursor-cuu-cud-cuf-cub.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/cursor-cuu-cud-cuf-cub.case)
- [tools/conformance/vtium/cases/edit-ich-dch.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/edit-ich-dch.case)
- [tools/conformance/vtium/cases/edit-il-dl.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/edit-il-dl.case)
- [tools/conformance/vtium/cases/erase-ech.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/erase-ech.case)
- [tools/conformance/vtium/cases/erase-ed.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/erase-ed.case)
- [tools/conformance/vtium/cases/erase-el.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/erase-el.case)
- [tools/conformance/vtium/cases/scroll-ind-nel-ri.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/scroll-ind-nel-ri.case)
- [tools/conformance/vtium/cases/scroll-large-counts.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/scroll-large-counts.case)
- [tools/conformance/vtium/cases/scroll-lf-ri-margins.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/scroll-lf-ri-margins.case)
- [tools/conformance/vtium/cases/scroll-su-sd.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/scroll-su-sd.case)

## Where to read more

To follow the work on this row, see issue [#11](https://github.com/vtium/vtium/issues/11).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
