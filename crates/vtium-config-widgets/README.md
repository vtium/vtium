# vtium-config-widgets

This crate holds the toolkit that vtium-config's TUI draws with. The TUI is the full-screen program that `vtium config` opens in a terminal, and ratatui lays out its screens. This crate turns ratatui's cells into the bytes that the TUI's loop writes, and measures text by the console's width rule. It names the sixteen colors the TUI draws in, and provides the one list that every screen picks items from. It also decodes the keys, mouse reports and replies that the terminal sends.

vtium-config depends on this crate and re-exports each of its modules in its `tui` module, so `vtium_config::tui::canvas` and the rest keep their paths. The crate depends on no other part of vtium-config. It needs only ratatui-core, vtium-term for the console's width rule, and vtium-font for the superscript and subscript that a style carries. So the toolkit builds and is tested without the configuration tool. Code that paints a picture of the console from rows of styled text can use it without the TUI.

The terminal's replies are untrusted input. The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's lints and its five modules. |
| `src/backend.rs` | The Backend through which ratatui draws. It writes each cell as ECMA-48 cursor positions, SGR and text into a buffer of bytes, and follows the terminal's cursor by the console's width rule, so a character that ratatui and the console measure differently never shifts the rest of its row. |
| `src/canvas.rs` | A row of styled text, writing text on a row at a column, and fitting a name, a sentence or a list of words to the room a screen has for it. At 80×25 a long name is cut in the middle with `…`. |
| `src/color.rs` | The console's sixteen colors, named by SGR so the user's palette applies, without blue or bright blue for text. The styles a span carries, and `NO_COLOR`, under which the TUI draws no color at all. |
| `src/listbox.rs` | The shared list: the one component the TUI uses wherever it shows a set of items or asks the person to pick one. It holds the highlighted item, reads the keys that move the highlight, pick an item or close the list, and lays the items out as rows. |
| `src/input.rs` | The decoder of what the terminal sends: keys as a terminal of the VT100 line sends them, mouse reports, and the replies to the TUI's requests. A sequence split across reads waits for its end, and a lone ESC is told apart from the start of a reply. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `STRING_MAX` in `src/input.rs` | 4096 bytes of a DCS, OSC or APC string | The decoder drops the string |
| `NAME_MAX` in `src/listbox.rs` | 32 cells of a list item's name column | The item's detail goes on the rows below its name |

`cargo test -p vtium-config-widgets` runs the crate's unit tests. vtium-config's TUI tests and scenarios drive the toolkit further, through the TUI.

References:
- ADR-1206 D1: the TUI draws through ratatui's core over vtium-config's own Backend, and the console's width rule decides where the cursor goes.
- ADR-1206 D2: how the decoder tells the Esc key from the start of a reply.
- ADR-1403 D7: the sixteen colors, the shared list, and how a long name is cut at the smallest screen.
- ADR-1513 D1: the split that made this crate, which moved the code without changing it.
