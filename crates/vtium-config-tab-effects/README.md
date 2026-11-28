# vtium-config-tab-effects

This crate holds the Effects tab of vtium-config's TUI, the full-screen program that `vtium config` opens in a terminal. The tab lists the effects a look can use, in three groups of one list. The vtlooks come first: No filter, then the presets the package ships, then the administrator's vtlooks. A vtlook is a file of frame filters, which change the colors or the look of the picture before the display shows it. The color maps (`.cube` tables that a vtlook's `cube` filter reads) and the shaders (the files of spatial filters) follow, each with its active mark and its trust mark.

Beside the list sits the highlighted vtlook's filter chain, read left to right in the order vtium runs it: `frame → spatial: none → turn → tint → contrast → screen`. Under the chain, one line for each filter gives its settings, and the frame of the picture of the sample screen through the vtlook follows. On a color map or a shader, the same place shows its file, its state and the looks that use it. vtium builds the color filters, and the spatial layer and its shaders are planned for later slices of ADR-1304. A filter vtium does not build yet is marked planned in words, with the slice that builds it, so the mark shows where no color does.

Enter on the filter line opens the list of groups in the list box under the line, and Enter on an item opens its detail, which changes nothing. Space activates or deactivates a color map or a shader. When a look uses the resource, Space asks first and names the look. The shell's `?` search filters the list by the items' names, and F1 opens the highlighted item's facts in full, where the pane beside the list cut a line.

The tab implements vtium-config-widgets' `Tab` trait over vtium-config-core's shared state, action and commands, and `Searchable` for the `?` search. The shell holds that search and hands the tab a copy after each key, through `Tab::filtered`. It draws only into the viewer the frame gives it and reads the shared state only. An activation is one command, which the shell's one change function makes, writes into the draft and records as one step that Undo takes back. The tab depends on no other tab's crate and not on the shell, so the binary hands it to the shell as a trait object. The shell asks `Tab::preview` for the preview's font, effect and place, and draws the picture inside the frame the tab leaves blank. The font is that of the look of the console's terminal, which the shared state's machine names.

The tab reads keys, clicks, and the names and errors of the administrator's files. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The tab's state, its node of the interface tree, its panes, its draw, its preview, its F1 text, its items for Find, and the names the search matches. |
| `src/act.rs` | What each action does: the moves through the list and a kept filter, Enter, Space and its question, the list box of groups, the detail's scrolling, and Find's request. |
| `src/list.rs` | The items of the groups shown, each item's name and marks, whether a resource is active once the draft is counted, the looks that use it, and the list at the left. |
| `src/chain.rs` | What sits beside the list: a vtlook's chain and its filters' lines, or a color map's or a shader's facts. |
| `src/detail.rs` | The detail of a vtlook, a color map or a shader, its long lines wrapped, and its scrolling under the title. |
| `src/words.rs` | What the tab says on screen. |
| `tests/screens.rs` | Each screen at 100×30 and 80×25, on the screen map's machine and on a large one, with and without color. The snapshots are under `tests/snapshots/`. |
| `tests/drive.rs` | The tab driven through its `Tab` methods and the shell's search. It covers its nodes' rows, 200 vtlooks scrolled and searched, the groups, the detail, Space, clicks, F1, Find and the preview. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `LIST_CELLS` in `src/lib.rs` | 32 columns for the list, or half of a narrower viewer | The pane beside the list takes the rest |
| `NAME_CELLS` in `src/list.rs` | 12 cells for an item's name in the list | The name is cut with `…`, and the pane beside the list gives it whole |
| `PICTURE_ROWS` in `src/lib.rs` | 12 rows for the picture's frame, its borders included | The frame takes the rows left under the chain; under 4 rows it is not drawn |

`cargo test -p vtium-config-tab-effects` runs the crate's tests. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-effects --test screens` writes the screens' snapshots again.

References:
- ADR-1206 D9: each tab is a crate of its own that implements the `Tab` trait and the opt-in traits of the behaviours it offers.
- ADR-1206 D2 and D4: a tab's update returns commands, and its view draws from the shared state without changing it.
- ADR-1206 D10: the screens at 100×30 and 80×25, on a large machine too, and without color.
- ADR-1403 D11: the Effects tab's chain beside the list, the list box, F1's text for what is highlighted, and the search on `?`.
- ADR-1304 D13: the slices that build the spatial layer and the filters the tab marks planned.
- ADR-1513 D4: the rebuilt TUI's tabs live in crates of their own.
