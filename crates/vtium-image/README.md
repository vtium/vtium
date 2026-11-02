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
| `src/store/edit.rs` | `Edit`, the one way a handler changes the store: the store's own operations for a handler, with both buffers' screen rows bound, and none of the screen events (D6). |
| `src/store/kitty.rs` | Empty for now. The kitty graphics handler's work fills it with the store operations only kitty makes: its delete selectors and the lookup of a placement by its id. |
| `src/handler.rs` | The handler trait and the context vtium-term implements: the store to read and to edit, the cursor, the margins, the index and the scroll, the sixel modes, the reply sink, the switches and the limits (D6). |
| `src/protocol/sixel.rs` | The sixel handler: a DCS `q` string streamed into the decoder, the picture stored with what it reserved, placed at the cursor or at the screen's top left by mode 80, the cursor after it by mode 8452, and the color registers shared or private by mode 1070 (How sixel works). |
| `src/protocol/kitty.rs` | The kitty graphics handler. For now it reads its strings, counts the bytes as input work and changes nothing. |
| `src/decode.rs` | The decode request and reply types, and the check of a reply's sizes against its request (D7). |
| `src/decode/sixel.rs` | The sixel decoder: a string's data taken as it arrives and drawn into premultiplied pixels, never held whole, with the color registers, HLS and RGB, repeats, raster attributes and the work it does (D7). |
| `src/decode/base64.rs`, `src/decode/raw.rs` | Empty for now. The kitty graphics handler's work fills them with the base64 decoder and the raw RGB and RGBA conversion (D7). |
| `fuzz/` | The handlers' cargo-fuzz targets, in their own workspace ([fuzz/README.md](fuzz/README.md)). |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source, and the `auto` budget's rule. |

`cargo test -p vtium-image` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_IMAGE_BLESS=1 cargo test -p vtium-image readme_table` rewrites it.

## How the store works

A store has a main and an alternate buffer, each with its own images, kitty ids and placements, and one budget and one use clock over both. Images and placements are kept in ordered maps by key, each key counted up and never reused. A placement is anchored to the line holding its top row and a column; the terminal tells the store which line holds each screen row when an event needs it, the screen's rows and the newest scrollback rows above them, as many as the tallest placement reaches.

An image reserves its whole cost, its pixels at 4 bytes each and 512 bytes beside them, before it is stored; each placement of an image past its first costs 256 bytes more. A reservation that cannot fit once everything that may go has gone evicts nothing and is refused. Otherwise images go in the eviction order: images with no placement, transient ones first, then images placed only in the scrollback, each least recently used first, then images whose rules allow eviction while placed on the screen's own lines, oldest transmission first. A sixel placed on the screen's own lines is never evicted. The use clock moves only on stream events: storing an image, placing one, and a command that names an existing image, such as a kitty put or query by id or number. A handler reserves an image's bytes before it allocates them, as its payload arrives, and a reservation for a kitty id already in the buffer holds that image so nothing evicts it before its replacement is stored.

The placements at one layer and z form a stack, kept as placements arrive: a later picture goes on top, except that a picture whose image has a kitty id slides in directly beneath the first placement from the bottom whose image has a higher id, so nothing already shown moves. A cut or a scroll that splits a placement keeps its pieces together at its place, in the order they were made: above, left, right and below for a cut, top to bottom for a scroll.

## How sixel works

The sixel handler takes a DCS `q` string while sixel is offered. Its data goes to the decoder as the parser hands it over, so a picture costs its pixels and never the text that drew it: a 7680 by 2160 frame from `mpv --vo=sixel` is up to 29 MB of text and 65 MB of pixels. The decoder draws each dot in its register's color as the dot arrives, so a register defined again later changes only what is drawn after. It reserves the picture's pixels of the console's budget before it allocates them: raster attributes reserve the whole picture before any sixel, so a picture that cannot fit is refused before it is decoded, and a picture without them reserves as it grows, each side by at least a quarter, holding both buffers while the rows move across. A string that ends with its terminator stores the picture, with that reservation and the 512 bytes beside it, and places it; a cancelled string, or one that ends while sixel is not offered, gives everything back.

With mode 80 (DECSDM) reset, the picture's top left is the cursor's cell, a pending wrap counting as the last column. When the picture would pass the bottom margin, the scroll region scrolls up first, as far as it can, so a picture that fits the region moves whole; then the cursor goes down a row fewer than the picture's rows, as that many line feeds do. The cursor ends on the row of the picture's last pixel row, at its left column, or with mode 8452 in the column after its right edge, at most the last column. With mode 80 set, the picture sits at the screen's top left, cut to the screen, and the cursor stays where it is. With mode 1070 set, as `private-color-registers` starts it, each picture starts from the default registers, the VT340's sixteen colors and black; reset, the pictures share one set, which RIS and a change of the register count return to the defaults. The three modes reset only on RIS. While sixel is not offered, DECRQM reports each one's fixed state, and SM and RM still store it.

The decoder counts every byte it reads and every pixel it writes, allocates or copies as input work (D5), so a short string that draws many pixels, such as `!10000~`, stops the input call once the call's work passes its bound. vtium-term checks the bound after each run of string data the parser hands over, so a run the host read in one piece is decoded whole before the call stops.

## Departures

- **An opaque sixel placed again over its own placement keeps its image.** The image being placed is held through the cut that removes its older placements, and a held image is never freed, so the new placement shows it and the charge that placement first took is given back. The C reference's `screen_image_place` freed the image in that cut and then placed it, a use after free. Test: `an_opaque_sixel_placed_twice_over_itself_keeps_its_image`.

- **The use clock is each store's own and moves only on stream events.** The C reference renewed every drawn image's use on each frame, on one clock for the whole process, so its eviction order depended on which frames were drawn (ADR-1100, D5). Test: `drawing_a_frame_moves_no_clock`.
- **Each console has its own budget.** The C reference had one budget for the whole process (ADR-1100, D5). The store holds its own limit, and the binary gives each console one.
- **At equal z, a sixel and a kitty picture draw in the order they arrived,** with a newly arrived kitty picture slid beneath the first higher kitty id. The C reference drew every sixel above every kitty picture at equal z (ADR-1100, D4). Tests: `kitty_two_a_sixel_then_kitty_one_draw_one_two_sixel`, `a_later_picture_goes_on_top_whatever_its_protocol`.
- **A kitty placement that replaces one with the same image and placement id is a new arrival** in its stack, where the C reference kept the replaced placement's place (ADR-1100, D4). Test: `a_replacement_is_a_new_arrival_in_its_stack`.
- **The charges of the pieces a screen event cuts are settled when the event ends.** The C reference settled them at the end of an input call, or before the next reservation, so when the budget refused them, which pieces went depended on how the input was chunked. Tests: `pieces_the_budget_refuses_go_smallest_first`, and vtium-term's `the_charges_of_cut_pieces_settle_per_event_however_the_input_is_cut`.
- **A placement's change marks the cells under its old and new boxes,** which vtium-term does from the regions the store reports, where the C reference left each renderer to damage placement rectangles itself (ADR-1001).
- **A sixel dot is one square pixel,** whatever pixel aspect the string's first parameter or raster attributes' Pan;Pad ask for, as the C reference, xterm, WezTerm and mlterm draw it; DEC's VT340 manual draws a dot twice as tall as it is wide unless the picture says otherwise (ADR-1100, D3), so this is a departure from the specification. Test: `raster_attributes_size_the_picture_and_reserve_it_first`.
- **HLS colors are computed exactly, in integers.** The C reference computed them in floating point, so where a channel's exact value lies halfway between two 8-bit levels, 2,747 of the 3.7 million HLS colors, its decimal fractions rounded the half down in some colors and up in others; vtium rounds each half up. No golden draws one of those colors. Test: `hls_in_integers_is_the_c_references_but_at_exact_halves`.

## Readings of the record

These follow ADR-1100 where its literal text leaves room, and each is the C reference's flow.

- **ED 2 keeps the part of a sixel that lies in the scrollback.** ED 2 removes every placement whose rules say the clear screen code removes it, kitty's, with a row on the screen's own lines. A sixel loses the cells the erase clears, as D3 says text and erases cut it, so a sixel reaching up into the scrollback keeps its part there. D8 says ED 2 removes every placement on the screen's own lines; vtium reads that as their parts on those lines.
- **An opaque sixel cuts the older sixels under its whole box.** D2 gives a new picture of the same tag the cells it covers fully; until a picture over a picture (ADR-1000's `image.overlap`) records which cells each picture covers, an opaque sixel cuts every cell of its box and a sixel with any clear pixel cuts nothing.
- **The shared color registers start again from the defaults when the register count in force at a picture's start differs from the count they were made with.** The handler sees the count XTSMGRAPHICS sets only through the count in force, so a count set and set back before the next picture keeps the registers, where the C reference dropped them at each change. RIS, and the host changing `num-color-registers` or `private-color-registers`, drop them as the C reference did; the host switching sixel off, or changing `max-graphic-size`, cancels a picture in progress and keeps them, where the C reference dropped them there too.

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
