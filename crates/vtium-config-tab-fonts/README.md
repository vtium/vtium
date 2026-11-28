# vtium-config-tab-fonts

This crate holds the Fonts tab of vtium-config's TUI, the full-screen program that `vtium config` opens in a terminal. The tab lists the fonts found on this system, grouped by the folder each was found in. It also lists the fonts built into vtium, and the fonts a font set of the configuration names that no file is installed for.

Each font's row shows its kind and one mark, the same words every screen of vtium-config gives a font: `● active`, `○ not active`, `○ turned off on purpose`, `○ not active · fails trust`, `not usable yet` or `✗ missing`. The words carry the meaning, so the marks read the same without color. The highlighted font's facts sit beside the list: its file, folder, kind, coverage, sizes, state, trust, where its activation comes from and the looks that use it. The picture of the console sits under the list.

A filter line keeps every font, or one kind, group or script. The shell's `?` search filters the list by a font's family and its file's name. Space activates or deactivates one font as one change, and asks first when a look uses a font it would deactivate. Some fonts the configuration uses may be installed and trusted but not active. Then the list's first item and the action bar's `[A] Activate missing (3)` activate them all as one change. Fonts turned off on purpose stay off, and the message names them apart. Enter opens a font's full detail, where Enter on a font of your own starts its install as a system font. `t` returns the font's path for the shell's live trial, or says why the console would refuse it.

The tab implements vtium-config-widgets' `Tab` trait over vtium-config-core's shared state, action and commands. It implements `Searchable` for the shell's `?` search, and keeps the matches the shell hands it. It draws only into the viewer the frame gives it and reads the shared state only. A change is a command that the shell's one change function makes and records as one step, which Undo takes back. The tab depends on no other tab's crate and not on the shell.

The tab works out every font's state once for each change of the document or of the fonts found. It reads the states from the shared state's walk of the fonts in use and its batch. A key that moves, filters or searches reads no folder, so the list of thousands of fonts filters as you type on a small machine too. The tab reads keys, clicks, font files' names and what the font helper says of them. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The tab's state, its nodes of the interface tree, its draw, its action bar and its labels. Its F1 text, its items for Find, and the search it is handed. The font a system install acts on, and what the picture shows. |
| `src/act.rs` | What each action does: the moves, Enter, Space, Activate missing, `t` and `p`. The list box of the filters, the detail's keys, a click, and Find's request to highlight a font. |
| `src/states.rs` | Every font the tab lists with its state, its group, its users and the fields the search matches, built once for each change. |
| `src/list.rs` | The list: the filter line, the configuration's own row, the groups and each font's row. |
| `src/facts.rs` | The highlighted row's facts beside the list, which F1 opens uncut. |
| `src/detail.rs` | A font's full detail, each fact wrapped under its label. |
| `src/own.rs` | A font of your own: which font the install acts on, and the commands that start it. |
| `src/trial.rs` | The path a live trial sends, or why the console would refuse it. |
| `src/layout.rs` | Where the list, the picture, the rule and the facts go in the viewer. |
| `src/words.rs` | What the tab says on screen. |
| `tests/screens.rs` | Each screen at 100×30 and 80×25, on the screen map's machine and on a large one, with and without color. The snapshots are under `tests/snapshots/`. |
| `tests/drive.rs` | The tab driven through its `Tab` methods: every row of its nodes, Space, Activate missing, the bar's labels, `t`, `p` and the preview. The filters, the shell's search, the detail, the clicks, F1, Find, and no folder read on a key. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| The facts' width | 36 hundredths of the viewer, from 26 to 40 columns | A fact is cut with `…`, and F1 shows it whole |
| The list's kind column | Shown on a list of 56 columns and more | The row gives the kind's cells to the font's name |
| The picture's height | Two fifths of the viewer, at most 13 rows, on a viewer of 16 rows and more | The list takes the rows |
| A font's users named in its facts | 3, then a count of the rest | The detail names every user |

`cargo test -p vtium-config-tab-fonts` runs the crate's tests. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-fonts --test screens` writes the screens' snapshots again.

References:
- ADR-1206 D9: each tab is a crate of its own that implements the `Tab` trait and the opt-in traits of the behaviours it offers.
- ADR-1206 D2 and D4: a tab's update returns commands, and its view draws from the shared state without changing it.
- ADR-1206 D10: the screens at 100×30 and 80×25, on a large machine too, and without color.
- ADR-1403 D11, "The tabs' layouts": the facts beside the list, and the picture under it.
- ADR-1403 D11, "Fonts the configuration uses but has not activated": the marks, Activate missing and its count.
- ADR-1205 D4: the five states, and fonts turned off on purpose.
- ADR-1513 D4: the rebuilt TUI's tabs live in crates of their own.
