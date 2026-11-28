# vtium-config-widgets

This crate holds the toolkit that vtium-config's TUI draws with. The TUI is the full-screen program that `vtium config` opens in a terminal, and ratatui lays out its screens. This crate turns ratatui's cells into the bytes that the TUI's loop writes, and measures text by the console's width rule. It names the sixteen colors the TUI draws in, and provides the one list that every screen picks items from. It also decodes the keys, mouse reports and replies that the terminal sends.

vtium-config depends on this crate and re-exports the five modules its TUI draws with in its `tui` module, so `vtium_config::tui::canvas` and the rest keep their paths. The crate depends on no other part of vtium-config. It needs only ratatui-core, vtium-term for the console's width rule, and vtium-font for the superscript and subscript that a style carries. So the toolkit builds and is tested without the configuration tool. Code that paints a picture of the console from rows of styled text can use it without the TUI.

The terminal's replies are untrusted input. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's lints and its modules. |
| `src/backend.rs` | The Backend through which ratatui draws. It writes each cell as ECMA-48 cursor positions, SGR and text into a buffer of bytes, and follows the terminal's cursor by the console's width rule, so a character that ratatui and the console measure differently never shifts the rest of its row. |
| `src/canvas.rs` | A row of styled text, writing text on a row at a column, and fitting a name, a sentence or a list of words to the room a screen has for it. At 80×25 a long name is cut in the middle with `…`. |
| `src/color.rs` | The console's sixteen colors, named by SGR so the user's palette applies, without blue or bright blue for text. The styles a span carries, and `NO_COLOR`, under which the TUI draws no color at all. |
| `src/listbox.rs` | The shared list: the one component the TUI uses wherever it shows a set of items or asks the person to pick one. It holds the highlighted item, reads the keys that move the highlight, pick an item or close the list, and lays the items out as rows. |
| `src/input.rs` | The decoder of what the terminal sends: keys as a terminal of the VT100 line sends them, mouse reports, and the replies to the TUI's requests. A sequence split across reads waits for its end, and a lone ESC is told apart from the start of a reply. |
| `src/tab/` | The seams between the rebuilt TUI's shell and its six tabs. `Tab` is the trait every tab implements: its name, its Alt letter, its node of the interface tree, its draw and its update are required, and the key line, the action bar's buttons, the items it adds to the Actions menu, F1's full text, Find's items and its search have defaults. A tab that offers the `?` search implements `Searchable`, and a line with choices implements `Choices` for the list box. The trait names the shared state, the action and the commands as type parameters, because the crate that defines them sits above this one. |
| `src/contract/` | The interface tree and the key table beside ADR-1403, compiled in. A node's rows come in order of priority, each with its key in the action bar's form, such as `Ctrl+S`, and a node names the fields its `?` search matches. `text.rs` is generated: see below. |
| `contract/` | The rows and nodes the rebuilt TUI adds to ADR-1403's two tables, which wait here until the binary switches to it: `d11-keys.tsv` and `d11-tree.tsv` add rows and nodes or replace them by id, `d11-gone.tsv` takes rows and nodes out, and `d11-search.tsv` is the tree's `search` column. |
| `src/roles.rs` | The color roles. Every colored element of the rebuilt TUI is drawn through a named role, such as `menu-highlight` or `problems`, and one table gives each role today's look in the console's sixteen colors. Without color a role keeps only its bold, underline and reverse video. |
| `src/keys/`, `src/search/`, `src/list_box/`, `src/frame/`, `src/menu/` | The rebuilt TUI's shared components: the decoder of keys, the search row and its matcher, the list box, the frame of every screen with its top row and action bar, and the menus. Each holds only the line that names it until its card fills it. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `STRING_MAX` in `src/input.rs` | 4096 bytes of a DCS, OSC or APC string | The decoder drops the string |
| `NAME_MAX` in `src/listbox.rs` | 32 cells of a list item's name column | The item's detail goes on the rows below its name |
| A node's rows in `src/contract/text.rs` | 65,535 rows in the key table, each named by a `u16` index | The generator refuses to write the tables |

`cargo test -p vtium-config-widgets` runs the crate's tests. vtium-config's TUI tests and scenarios drive the old toolkit further, through the TUI.

The test `the_compiled_tables_match_the_tsvs` in `tests/contract.rs` reads ADR-1403's two tables and the four files under `contract/`, and fails when `src/contract/text.rs` differs from what they give. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-widgets --test contract` writes `text.rs` again.

References:
- ADR-1206 D1: the TUI draws through ratatui's core over vtium-config's own Backend, and the console's width rule decides where the cursor goes.
- ADR-1206 D2: how the decoder tells the Esc key from the start of a reply.
- ADR-1403 D7: the sixteen colors, the shared list, and how a long name is cut at the smallest screen.
- ADR-1513 D1: the split that made this crate, which moved the code without changing it.
- ADR-1513 D4: the rebuilt TUI's traits, compiled tables and shared components live in this crate.
- ADR-1206 D3: the compiled tables, the action bar's buttons from them, and the search component.
- ADR-1206 D4: the color roles and their table.
- ADR-1206 D8 and D9: the list box, and the `Tab`, `Searchable` and choices traits with their defaults.
- ADR-1403 D11, "How the tables carry this section": why the new rows wait in an overlay until the binary switches.
