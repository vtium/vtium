# vtium-config-tab-looks

This crate holds the Looks tab of vtium-config's TUI, the full-screen program that `vtium config` opens in a terminal. A look puts together what the console draws with: a font set and its size, a color scheme, a vtlook that holds the frame's filters, and a pointer theme.

The tab's list holds the open configuration's looks, then the font sets, the color schemes and the pointer themes, each group under its heading. Beside the list sits the highlighted look's card, which sums up its lines and its fonts above the preview of how the console draws it. On a font set, a color scheme or a pointer theme, the same place shows the facts that its detail lists. Enter opens a look's card, and Enter on a card line opens that line's choices in the list box under it. Left and Right step a line to the choice before or after it. A look whose fonts are not all active says so in its own row, and its card offers `[A] Activate` for the fonts that can be activated. `?` filters the list by name.

The tab implements vtium-config-widgets' `Tab` trait over vtium-config-core's shared state, action and commands. It also implements `Searchable` for the shell's `?` search, which hands the tab its matches, and the choices trait for each card line's list box. It draws only into the viewer the frame gives it and reads the shared state only. Each change is one command. The shell's one change function makes it, writes the draft, and records it as one step that Undo takes back. The tab depends on no other tab's crate and not on the shell.

vtium-config-core lists the fonts each look uses and judges each one against the batch, the list of active resources. `src/missing.rs` hands that walk the batch the shared state holds. It walks again only after the document, the fonts found or the batch change. No key that moves the highlight reads a folder.

The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The tab's state, its nodes of the interface tree, its preview, its F1 text, its items for Find, and every item's name for the shell's search. |
| `src/act.rs` | What each action does on the list and among a kept filter's matches, on a card and its list boxes, and on a detail, with the clicks on each. |
| `src/list.rs` | The looks as vtium.conf holds them, the list's items in their four groups, and the pane at the left. |
| `src/card.rs` | A look's lines, what each shows, each line's choices through the choices trait, and the change a pick, a step or a new name makes. |
| `src/sizes.rs` | The sizes a look's text font supplies: the usual sizes for a font that draws at any size, and the sizes that draw a bitmap font's strikes. |
| `src/resources.rs` | The details of a font set, a color scheme and a pointer theme, a resource's mark, and Space on a color scheme file or a pointer theme. |
| `src/missing.rs` | The fonts each look uses with their states, a card's fonts by role, what a look still misses, and Activate for one look. |
| `src/view.rs` | The screens, where each part went for clicks, F1's text and Find's items. |
| `src/draw.rs` | Where the list, the rule, the card, the preview's box and the line under it go, and the helpers that write text. |
| `src/words.rs` | What the tab says on screen. |
| `src/forms/` | The forms the second half of this tab adds. |
| `tests/screens.rs` | Each screen at 100×30 and 80×25, on the screen map's machine and on a large one, with and without color. The snapshots are under `tests/snapshots/`. |
| `tests/drive.rs` | The tab driven through its `Tab` methods: every row of its nodes, each line's list box, a bitmap font's sizes, Activate, the keys, the search, F1, Find and the clicks. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `LIST_WIDTH` in `src/draw.rs` | 34 columns at 100 columns and more, else a third of the viewer and at least 24 | A name is cut with `…` |
| `CARD_ROWS` in `src/draw.rs` | 10 rows for the card or the facts beside the list | The preview starts below them, and a viewer too short for its box shows none |
| `USUAL` in `src/sizes.rs` | 15 sizes from 6 to 32 points for a font that draws at any size | A size the look sets outside them is listed as the look's own |
| `DPI` in `src/sizes.rs` | 96 dots per inch where vtium.conf sets no `display-dpi` | A bitmap font's sizes are worked out at that density |

`cargo test -p vtium-config-tab-looks` runs the crate's tests. `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-looks --test screens` writes the screens' snapshots again.

References:
- ADR-1206 D9: each tab is a crate of its own that implements the `Tab` trait and the opt-in traits of the behaviours it offers.
- ADR-1206 D2 and D4: a tab's update returns commands, and its view draws from the shared state without changing it.
- ADR-1206 D8: the list box under a line with choices, its choices given through the choices trait.
- ADR-1206 D10: the screens at 100×30 and 80×25, on a large machine too, and without color.
- ADR-1403 D11: the Looks tab's card beside the list, the list box, F1, the search on `?`, and the fonts a configuration uses but has not activated.
- ADR-1205 D4, D6 and D7: how a look is assembled from active resources, and the sizes its fonts supply.
- ADR-1513 D4: the rebuilt TUI's tabs live in crates of their own.
