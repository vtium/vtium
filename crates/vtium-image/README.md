# vtium-image

Inline images: the one pipeline sixel and the kitty graphics protocol feed (ADR-1100). It holds each console's image store, the protocol tag and its rules, the handler trait the protocol front ends implement, the decode request and reply types the image helper answers, and its bounds. It forbids unsafe code, denies the arithmetic, cast and indexing lints `docs/architecture.md` lists, and uses no `HashMap`. It depends on `vtium-parse` alone and names a line by an opaque 64-bit key, so it never depends on `vtium-term`; vtium-term takes it behind its `images` feature, and the minspec build carries none of it.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/rules.rs` | The protocol tag, whose variants are private to the module, and `rules`, the one function that maps a tag to its rules (D3); the layers a frame draws in. |
| `src/image.rs` | The keys, the shared pixels, and the image, placement and refusal types the store takes. |
| `src/store.rs` | The store: two buffers, the stack at each depth (D4), the budget and the eviction order (D5), and storing and placing an image. |
| `src/store/events.rs` | What each screen event does to placements: cuts, scrolls, the scrollback's oldest line dropped, freed lines, ED 2, and the anchors a rewrap carries (D3, D8). |
| `src/store/frame.rs` | The frame's placement list, in draw order (D9). |
| `src/handler.rs` | The handler trait and the context vtium-term implements (D6). |
| `src/decode.rs` | The decode request and reply types, and the check of a reply's sizes against its request (D7). |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source, and the `auto` budget's rule. |

`cargo test -p vtium-image` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_IMAGE_BLESS=1 cargo test -p vtium-image readme_table` rewrites it.

## How the store works

A store has a main and an alternate buffer, each with its own images, kitty ids and placements, and one budget and one use clock over both. Images and placements are kept in ordered maps by key, each key counted up and never reused. A placement is anchored to the line holding its top row and a column; the terminal tells the store which line holds each screen row when an event needs it, the screen's rows and the newest scrollback rows above them, as many as the tallest placement reaches.

An image reserves its whole cost, its pixels at 4 bytes each and 512 bytes beside them, before it is stored; each placement of an image past its first costs 256 bytes more. A reservation that cannot fit once everything that may go has gone evicts nothing and is refused. Otherwise images go in the eviction order: images with no placement, transient ones first, then images placed only in the scrollback, each least recently used first, then images whose rules allow eviction while placed on the screen's own lines, oldest transmission first. A sixel placed on the screen's own lines is never evicted. The use clock moves only on stream events: storing an image and placing one.

The placements at one layer and z form a stack, kept as placements arrive: a later picture goes on top, except that a picture whose image has a kitty id slides in directly beneath the first placement from the bottom whose image has a higher id, so nothing already shown moves. A cut or a scroll that splits a placement keeps its pieces together at its place, in the order they were made: above, left, right and below for a cut, top to bottom for a scroll.

## Departures

- **The use clock is each store's own and moves only on stream events.** The C reference renewed every drawn image's use on each frame, on one clock for the whole process, so its eviction order depended on which frames were drawn (ADR-1100, D5). Test: `drawing_a_frame_moves_no_clock`.
- **Each console has its own budget.** The C reference had one budget for the whole process (ADR-1100, D5). The store holds its own limit, and the binary gives each console one.
- **At equal z, a sixel and a kitty picture draw in the order they arrived,** with a newly arrived kitty picture slid beneath the first higher kitty id. The C reference drew every sixel above every kitty picture at equal z (ADR-1100, D4). Tests: `kitty_two_a_sixel_then_kitty_one_draw_one_two_sixel`, `a_later_picture_goes_on_top_whatever_its_protocol`.
- **A kitty placement that replaces one with the same image and placement id is a new arrival** in its stack, where the C reference kept the replaced placement's place (ADR-1100, D4). Test: `a_replacement_is_a_new_arrival_in_its_stack`.
- **The charges of the pieces a screen event cuts are settled when the event ends.** The C reference settled them at the end of an input call, or before the next reservation, so when the budget refused them, which pieces went depended on how the input was chunked. Test: `pieces_the_budget_refuses_go_smallest_first`.
- **A placement's change marks the cells under its old and new boxes,** which vtium-term does from the regions the store reports, where the C reference left each renderer to damage placement rectangles itself (ADR-1001).

## Bounds

<!-- bounds:begin -->
| Bound | Value | What it bounds | Past it | Source |
|---|---|---|---|---|
| `IMAGES_PER_BUFFER` | 1024 | Images one buffer of a store holds, main or alternate | A new image evicts from that buffer alone in the eviction order, and is refused when nothing there may go | The C reference's `TSM_SCREEN_IMAGES_PER_STORE` |
| `PLACEMENTS_PER_BUFFER` | 4096 | Placements one buffer holds | A new placement evicts from that buffer alone, or is refused; a cut or a scroll keeps its largest pieces and drops the smallest, as if their cells were erased | The C reference's `TSM_SCREEN_PLACEMENTS_PER_STORE` |
| `IMAGE_SIDE_MAX` | 65535 | Pixels an image has a side, in every protocol | The image is refused (EINVAL); `max-graphic-size` sets the working limit below it | The C reference's `TSM_SCREEN_IMAGE_MAX` |
| `CELL_MAX` | 1024 | Pixels a cell has a side, the unit a placement measures its pixels in | The cell size is refused and the one before it stays | The C reference's `TSM_SCREEN_CELL_MAX` |
| `BOX_CELLS_MAX` | 10000 | Cells a placement's box has in each direction, columns and rows each | The placement is refused (EINVAL), and so is a picture scaled past that box | The C reference's `TSM_SCREEN_IMAGE_MAX_CELLS`, checked against the columns and the rows each |
| `IMAGE_OVERHEAD` | 512 | Bytes of the budget an image costs beside its pixels, which pay for its first placement | Counted in every reservation | The C reference's `TSM_SCREEN_IMAGE_OVERHEAD`; the maintainer, 2026-11-01 |
| `PLACEMENT_OVERHEAD` | 256 | Bytes of the budget each placement of an image past its first costs, a piece a cut makes included | A new placement whose charge the budget refuses is refused; a piece whose charge it refuses goes, smallest first | The C reference's `TSM_SCREEN_PLACEMENT_OVERHEAD` |
| `AUTO_FRAMES` | 4 | Frames of the head's displays in RGBA that `image-storage-limit = auto` allows each console | The sum is clamped to `AUTO_FLOOR` and `AUTO_CAP` | The maintainer, 2026-11-01: four frames, held as a named constant so it is easy to adjust |
| `AUTO_FLOOR` | 16777216 | The fewest bytes `auto` allows a console, 16 MiB | A smaller sum is raised to it | The maintainer, 2026-11-01, as the C reference's budget had it |
| `AUTO_CAP` | 201326592 | The most bytes `auto` allows a console, 192 MiB | A larger sum is lowered to it | The maintainer, 2026-11-01, as the C reference's budget had it |
<!-- bounds:end -->
