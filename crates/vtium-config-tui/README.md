# vtium-config-tui

This crate is the shell of vtium-config's TUI. It holds the App, the one navigation module, the update loop, the frame around the open tab and the File and Actions menus. Each tab is a crate of its own. The binary builds the six tabs and hands them to `run` as trait objects of vtium-config-widgets' `Tab` trait, in the top row's order. So the shell names no tab crate, and no tab crate names another (ADR-1206 D9). The shell takes the trait, the compiled interface tree and key table, the key decoder, the frame, the menus and the search row from vtium-config-widgets. It takes the shared state, the action, the commands and the one open document from vtium-config-core.

A key goes to the navigation module, which looks up the key's row for the node the person is on. The module itself handles the rows that are the same everywhere. These are Tab, Shift-Tab, the digits, the Alt letters, F10, `.`, Esc on a level, a question's answers and a menu's keys. It hands every other row's action to the open tab, or to the shell's own handler for Undo, Redo and Quit. Update applies the changes a tab returns to the shared state in one place, through the document's one change function, which records each as one undo step. It then hands the tab's requests to the navigation module, and returns its effects to the loop. The loop alone writes to the terminal.

The TUI runs in raw mode, through rustix's `make_raw`, which clears IXON and ISIG so that Ctrl+S and Ctrl+Z arrive as keys. It runs on the alternate screen, with mouse reporting on (modes 1000 and 1006) for every screen. Reporting goes off when the TUI leaves, and while it steps aside for the person's editor or sudo. `q`, File › Quit, Ctrl-C and the signals SIGTERM, SIGINT, SIGQUIT and SIGHUP quit at once, because the draft already holds every change. While the document holds changes that are not saved, the TUI prints one line on leaving that says how many wait.

The terminal's bytes are untrusted input, and no key, click or reply can panic the TUI. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's lints, its modules, the type `BoxTab`, and `run`, which runs the TUI on the terminal of standard input and output. |
| `src/app/` | The model: the navigation state, the tabs, the shared state and the screen's size (`mod.rs`), and the one update, `App::event`, with `route`, which says who acts on each row of the key table (`update.rs`). |
| `src/nav/` | The navigation module: the open tab, each tab's stack of levels, the question, the open menu, the search row and the sheets open over the tab (`mod.rs`), the kinds of node and the lookup of a key's row (`levels.rs`), and the open menu's keys (`menus.rs`). |
| `src/view.rs` | The one layout of the screen, which update reads where a click lands, and the view that draws the frame, the open tab, the search row, the message and an open menu from it. |
| `src/menus/` | The File and Actions menus' items, letters, shortcuts, submenus and check marks. An item whose action no card has built yet is greyed out with the reason "not built yet". |
| `src/port/` | The loop, `drive`, and the `Port` it runs against (`mod.rs`), the real terminal (`tty.rs`), and what wakes the loop (`wake.rs`). |
| `src/sheets/` | The sheets over a tab, as a stack the navigation state keeps: F1's help (`about.rs`), Ctrl+F's Find (`find.rs`), the Problems list (`problems.rs`), File › New… (`new.rs`) and Manage configurations… (`manage.rs`), with opening a configuration from File › Open › (`open.rs`). Each sheet answers a key with what it asks for, which update carries out. |
| `src/live/`, `src/save/`, `src/changes/`, `src/history/`, `src/editor/`, `src/revert/`, `src/player/` | Empty until the cards that fill them: the live console, Save, Show changes, History, Edit as text, Revert to saved, and the macro player. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `MIN_SIZE` in `src/app/mod.rs` | a screen of 80 columns by 25 rows | The TUI draws one line that says how large the screen must be, and `q` and Ctrl-C still quit |
| the read in `src/port/wake.rs` | 4096 bytes of the terminal's input in one read | The rest comes in the next read |

## Tests

`cargo nextest run -p vtium-config-tui` runs the crate's tests. `tests/app.rs` drives the App with two stand-in tabs from `tests/common/`. It runs over vtium-config-core's two fixtures, the screen map's machine and a large machine, at 100×30 and at 80×25. It has a test for each row of the key table that the shell owns. It also runs the loop on a pty of its own. There it checks that mouse reporting is on while the TUI runs, and off after `q` and while it steps aside. The screens are text snapshots under `tests/snapshots/`, each with a grid of the attributes every cell keeps without color; `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tui --test app` writes them again. `tests/sheets.rs` drives the sheets the same way, with stand-in tabs of its own that answer Find and F1, and finds one of the large machine's forty displays by its name. `tests/guard.rs` reads the manifests and the sources of the shell and of every `crates/vtium-config-tab-*` crate. It fails on a tab crate that names another tab or the shell, and on a shell that names a tab.

References:
- ADR-1206 D2: the model, the message, the one update and the view.
- ADR-1206 D3: the navigation module, the menus, the undo history and the search component's state.
- ADR-1206 D4: who may call whom, the frame and the guard test.
- ADR-1206 D9: the shell and each tab are crates, joined by traits.
- ADR-1403 D11: the frame, the menus, the Alt letters, the action bar, the mouse, quitting, undo and redo, F1, Find, the Problems list, New… and managing configurations.
- ADR-1513 D4: the TUI's crates.
