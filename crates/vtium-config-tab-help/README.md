# vtium-config-tab-help

This crate holds the Help tab of vtium-config's TUI, the full-screen program that `vtium config` opens in a terminal. The tab lists its topics at the left and shows the highlighted topic at the right. The topics are the six tabs, then Menus and Save, Mouse, Commands and About.

A tab's topic gives every key of that tab's screen and what each key does. It reads them from the TUI's key table, which vtium-config-widgets compiles in, so the help cannot say what the keys do not do. The keys that work on every tab close each tab's topic as a short grid, and Menus and Save gives each of them in full. Mouse gives what each click does, and says that Shift and a drag select text to copy while vtium-config has the mouse. Commands lists the command line's forms as `vtium-config --help` prints them, and About gives what `vtium-config --help about` prints.

Up and Down move between the topics, and Home and End go to the first and the last. PgUp and PgDn scroll a topic that is longer than the pane, and Left and Right switch to About and back. `?` opens the shell's search row. The tab keeps the lines that match under their topic, and lists only the topics that have one.

F1 on a tab's screen lists that tab's topic, laid out for F1's sheet by the Help tab.

The tab implements vtium-config-widgets' `Tab` trait over vtium-config-core's shared state, action and commands, and its `Searchable` trait. The shell holds it as a trait object and reaches it only through those traits. The tab changes nothing in the configuration, so its update returns no command.

The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | `HelpTab`, the tab's state, and its `Tab` and `Searchable` implementations. They hold the keys that move between and through the topics, and a click on a topic. They take the shell's search, give F1's list of a tab's keys, and give Find one item for each topic. |
| `src/topics.rs` | The topics and what each one says, as entries that do not depend on the screen's width: what a tab is, its keys from the key table, the grid of the keys that work everywhere, the clicks, the commands and the About text. It also gives the lines the search matches. |
| `src/page.rs` | How the tab is drawn: the column of topics, the rule, and the topic's page laid out for the pane's width, with which lines show when the page is longer than the pane. `Layout` gives the place of each topic, which a click is read against, and `step` scrolls by a window and keeps an entry whole where it can. |
| `tests/` | The topics read against ADR-1403's tables and the overlay, the tab driven through its `Tab` methods, and its screens drawn with the frame at 100×30 and 80×25 on the screen map's machine and a large one, kept as snapshots under `tests/snapshots/`. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `KEYS_MAX` in `src/page.rs` | 16 cells of a key column | The key takes a line of its own, and what it does goes on the lines below |
| `SYNOPSIS_MAX` in `src/page.rs` | 24 cells of a command's column | The command takes the lines it needs, and what it does goes below |
| A row of the logo in `src/page.rs` | the pane's width | The row is left out, as it is where the TUI draws no color |

`cargo test -p vtium-config-tab-help` runs the crate's tests. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-help --test screens` writes the snapshots again.

References:
- ADR-1403 D11, "The tabs' layouts": the Help tab's topics, and the keys that move between and through them.
- ADR-1403 D11, "F1, help for what is highlighted": F1's list of every key of a screen is the Help tab's topic for that tab.
- ADR-1206 D9: the `Tab` and `Searchable` traits, and the shell that holds each tab as a trait object.
- ADR-1206 D10: every screen designed for 100×30, usable at 80×25, and readable without color.
- ADR-1513 D4: each tab of the rebuilt TUI is a crate of its own.
