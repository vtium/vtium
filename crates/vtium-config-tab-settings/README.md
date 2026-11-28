# vtium-config-tab-settings

This crate holds the Settings tab of vtium-config's TUI, the full-screen program that `vtium config` opens in a terminal. The tab shows the keys that vtium.conf sets at its top level, the ones that apply to the whole console rather than to one look or one terminal. The keys stand in the sections and groups of vtium-settings' key table, the order that the example vtium.conf and docs/config.md also follow.

The sections sit at the left and the highlighted section's keys at the right. Each key shows its value and a word that says whether it is changed or the default. So moving between sections shows their keys without opening them. Enter moves into the keys, and Enter on a key opens its allowed values in the list box under its line, or lets the person type its value. The highlighted key's first sentence sits along the viewer's last row, and F1 opens its whole help, its default and its value now. `?` filters the keys by their names and their descriptions, so a setting is found by what it does as well as by its name.

The tab implements vtium-config-widgets' `Tab` trait over vtium-config-core's shared state, action and commands, and `Searchable` for the `?` search. It draws only into the viewer the frame gives it and reads the shared state only. A change to a value is one command, which the shell's one change function makes, writes into the draft and records as one step that Undo takes back. The tab depends on no other tab's crate and not on the shell, so the binary hands it to the shell as a trait object.

The tab reads keys, clicks and the configuration's text. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The tab's state, its node of the interface tree, its draw, its F1 text, its items for Find, and its search over every key. |
| `src/act.rs` | What each action does: the moves through the sections and the keys, Enter, the list box's keys and clicks, the keys of a typed value and of the search row, and Find's request to highlight a key. |
| `src/sections.rs` | The sections and their keys as one flat list, built once from the key table, a key's value and its mark, and the left pane. |
| `src/keys.rs` | The right pane, the foot's line for a key, the whole help F1 opens, and Find's item for a key. |
| `src/value.rs` | The allowed values the list box offers, what typing starts from and what a typed value means, and the one change that sets a key. |
| `src/layout.rs` | Where the two panes, the rule between them and the foot go in the viewer. |
| `src/words.rs` | What the tab says on screen. |
| `tests/screens.rs` | Each screen at 100×30 and 80×25, on the screen map's machine and on a large one, with and without color. The snapshots are under `tests/snapshots/`. |
| `tests/drive.rs` | The tab driven through its `Tab` methods: every key reachable and settable, the list box, typing, the search, F1, Find, the clicks, and every row of the tab's nodes. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| The left pane's width | 33 columns at 100 columns and more, a third of a narrower viewer, and at least 20 | A section's name is cut with `…` |
| `NAME_MAX` in `src/keys.rs` | 27 cells for a key's name and the blanks after it | The name is cut with `…` |
| `VALUE_MAX` in `src/keys.rs` | 24 cells for a value | The value is cut with `…`, and F1 shows it whole |

`cargo test -p vtium-config-tab-settings` runs the crate's tests. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-settings --test screens` writes the screens' snapshots again, and `--test drive` writes F1's text for `font-name`.

References:
- ADR-1206 D9: each tab is a crate of its own that implements the `Tab` trait and the opt-in traits of the behaviours it offers.
- ADR-1206 D2 and D4: a tab's update returns commands, and its view draws from the shared state without changing it.
- ADR-1206 D10: the screens at 100×30 and 80×25, on a large machine too, and without color.
- ADR-1403 D11: the Settings tab's two panes, the list box, F1's help for what is highlighted, and the search on `?`.
- ADR-1513 D4: the rebuilt TUI's tabs live in crates of their own.
