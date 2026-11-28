# vtium-config-tab-flow

This crate holds vtium-config's Flow tab. The tab draws the open configuration as one page of paths, each joining displays and inputs to a terminal, the way vtium joins them when it runs. The page has three sections. CONFIGURATION shows the paths vtium.conf holds, with the inputs that roam and the path to Nowhere. NEW PATH holds the path the person builds, and PARTS lists the displays, terminals, inputs and looks to build it from. The page scrolls as a whole, and the arrow keys move the highlight from box to box.

The tab implements the `Tab` trait of vtium-config-widgets over the shared state, the action and the commands of vtium-config-core, and the shell holds it as a trait object. It draws only into the area of the screen the frame gives it, and it reads the shared state without changing it. Every change it asks for is a command that the shell applies. The tab depends on no other tab's crate and not on the shell's.

Besides the page, the tab opens three screens over it: a part's detail, with its label and the choices that narrow what it matches; a display or an input of a path, read-only; and the list of the inputs that are neither keyboards nor pointers. A terminal's card, NEW PATH's editing, the part makers and labels are the editing that `src/edit/` holds.

The page draws in the console's sixteen colors through the color roles, and under `NO_COLOR` or on a monochrome console each state of a box keeps its words or its glyph: `in no path`, `not connected`, `✓` for a picked part, a double border for a template, a dashed border in NEW PATH, and a heavy border for the highlighted box.

The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`. A vtium.conf and the devices present are untrusted input.

| Path | What it is |
|---|---|
| `src/lib.rs` | The tab: its state, the level open over the page, and its `Tab` methods, which turn each action into the commands it returns. The model of the open document is kept until the document or the devices change. |
| `src/paths.rs` | The configuration read as paths, with the roaming inputs and the path to Nowhere; NEW PATH's path, which Space fills; and each part's title in a reader's words. |
| `src/parts.rs` | The parts PARTS offers, made from the devices present, the draft's rules and labels and the special parts, and merged again when the devices change; and a part's detail. |
| `src/inputs.rs` | The inputs sorted into keyboards, pointers and the other inputs; which terminals hold a device; and the list of the other inputs. |
| `src/info.rs` | A display or an input of a path, read-only: what matches it and where it goes. |
| `src/page/` | The page: `layout.rs` lays the document out as boxes, lines and text, `draw.rs` draws a window on it, `colors.rs` names the role of each element, and `mod.rs` holds the highlight, the scroll and the moves. |
| `src/words.rs` | The words the tab shows, each fact in one line and each key in brackets. |
| `src/edit/` | The editing, which a later card fills. It holds only the line that names it. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| A box's natural width in `src/page/layout.rs` | 30 characters of text | The box's text is cut with `…` |
| The rows of the other inputs' list | the viewer's rows less three | The list ends with a line that counts the inputs it leaves out |
| A message | one line at 80 columns | `words.rs`'s test fails; a longer text goes behind F1 |

`cargo nextest run -p vtium-config-tab-flow` runs the crate's tests. They draw each screen at 100×30 and 80×25 on the screen map's machine, on the large machine and on the mock-up catalog's six example pages, and press every key of the tab's nodes on both machines. The text of each screen is kept under `tests/snapshots/`, and `VTIUM_CONFIG_BLESS=1` writes the snapshots again.

References:
- ADR-1206 D9: each tab is a crate that implements the `Tab` trait, and the shell holds the tabs as trait objects.
- ADR-1206 D2 and D4: a tab's update returns commands, its view draws from the model, and update and view share one layout function.
- ADR-1206 D10: every screen at 100×30 and 80×25, readable on a large machine and without color.
- ADR-1403 D7: the Flow view, its three sections, its boxes, lines, colors and keys.
- ADR-1403 D11, "The frame" and "The keys that change": the page in the viewer, and `r`, `n`, `v` and `o`, which open the File menu's items.
- ADR-1513 D4: the rebuilt TUI's tabs are crates of their own.
