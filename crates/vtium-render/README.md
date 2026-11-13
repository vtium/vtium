# vtium-render

vtium-render is vtium's renderer. It takes in frames from `vtium-term`, and it puts out display pixels. This crate holds the CPU compositor (vtium/vtium#16), which ADR-1300 decides. The compositor has these parts: a drawn-glyph cache, decorations drawn as sprites, the cursor's shapes, the pointer, four orientations, damage handed to the display as clips, and the render timer. The crate forbids unsafe code and denies the determinism lints. It draws into anything that takes a run of pixels (`Target`). It never reads what it draws into, and it opens no device. The display backends are vtium/vtium#18. The GPU path's frame, and the `Gl` trait that the GPU path draws through, are behind the `gpu` feature (vtium/vtium#33, ADR-1303).

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/renderer.rs` | `Renderer`: the grid's place on the display, the cell jobs, the cursor and the pointer, and the CPU path's back end. `Config`, `DrawState`, `Drawn` and `Stats`. |
| `src/renderer/gpu.rs` | With `gpu`: `Renderer::draw_gpu`, the renderer's frame on the GPU path, and what the GPU path keeps of a renderer's state. |
| `src/arrow.rs` | `Arrow`, the pointer's image, and the target that blends the pointer over what is written beneath it. |
| `src/damage.rs` | What each buffer lacks, cell by cell, and the damage rectangles. |
| `src/compose.rs` | The row's jobs, composed into display lines through the row scratch. |
| `src/cache.rs` | The drawn-glyph cache. It has four pools by format and width, keyed by the glyph's text, width and variant. It holds an LRU on the drawn frame, and the `Profile` budgets. |
| `src/variant.rs` | The glyph variant that a cell's attributes ask for: bold, italic, and superscript or subscript. |
| `src/stored.rs` | A glyph as the cache keeps it. The cache turns the glyph once, keeps it at 1 bit when that is lossless, keeps it at the bitmap font's own size, and draws it a line at a time. |
| `src/deco.rs` | The underline, strike and overline sprites. |
| `src/trig.rs` | `sin` and `cos` from basic float operations, for the curly underline. |
| `src/blend.rs` | The per-pixel arithmetic, in whole numbers. |
| `src/mono.rs` | `Depth`. A 1-bit display's colors by cell, its coverage cut at half, and a color glyph's stand-in. |
| `src/target.rs` | `Target`, the write-only display buffer. `Canvas`, a buffer in memory. `Mapped`, a mapped dumb buffer. `Orientation` and `Rect`. |
| `src/notice.rs` | The console's own drawing over a display's frame (ADR-1401). `Notice` is vtium's message in a box. `Mark` is a head's focus mark. `Surface` is the display they draw on. `quote` is for text from a file. |
| `src/timer.rs` | `RenderTimer` and its `Report`. |
| `src/picture/mod.rs` | With `images`: where each of a frame's placements draws on the display, and the cells that a picture which moved or changed draws again. It also gives a picture's pixels a display line at a time. |
| `src/picture/scale.rs` | With `images`: scaling and turning a picture in integer arithmetic. |
| `src/picture/copies.rs` | With `images`: the cache of a display's kept pictures, its eviction, and `PictureCounts`. `PictureCounts` holds the CPU path's scaled copies and the GPU path's picture textures under one rule. |
| `src/gpu/mod.rs` | With `gpu`: the GPU path's public names (ADR-1303). |
| `src/gpu/gl.rs` | With `gpu`: the `Gl` trait that the GPU path draws through, its handles, `Limits`, `Quad`, `Draw`, `Batch` and `Error`. |
| `src/gpu/ledger.rs` | With `gpu`: `Ledger`, a context's live handles, and the checks that every `Gl` implementation makes before a call reaches GL. |
| `src/gpu/stand_in.rs` | With `gpu`: `StandIn`, the tests' GL. It records its calls, and it draws the clears and the cell program's quads into its upright frames. |
| `src/gpu/model.rs` | With `gpu`: the stand-in's model of the cell shader. The model reads what the GPU path hands the shader, as the shader does, and it blends with `src/blend.rs`. |
| `src/gpu/ink.rs` | With `gpu`: the ink grid. The ink grid is what the cell shader reads about the neighbours whose ink a cell receives, and about the display lines that receive it. |
| `src/gpu/box_lines.rs` | With `gpu`: where the box of a glyph whose ink passes its cells holds ink, cell by cell. The ink grid sets a cell's line bits from it. |
| `src/gpu/pipeline.rs` | With `gpu`: `Pipeline`, a display's frame through the five slots, what fills each slot, and present. |
| `src/gpu/compose.rs` | With `gpu`: the composition into the upright frame and its tiles, `Work`, and the renderer's back end that turns cell jobs into quads. |
| `src/gpu/glyphs.rs` | With `gpu`: the glyph pools as textures and their slots. The boxes of the glyphs whose ink passes their cells. The decoration sprites' texture, and the textures that they give back. |
| `src/gpu/pictures.rs` | With `gpu` and `images`: slot 1, the pictures' textures by GPU scaling or by the CPU copy, under the display's bound, and the scratch texture. |
| `src/gpu/tile.rs` | With `gpu`: `tiles`, an image as a grid of tiles within the largest tile. |
| `src/gpu/turn.rs` | With `gpu`: `Turn`, which maps a rectangle between the display's pixels and the upright frame's pixels. |
| `src/gpu/glsl/` | With `gpu`: the shaders. `quad.vert` is for the arrow. `cell.vert` is for the cells and present. `cell.frag` is for the cells and their text layers. `arrow.frag` draws the arrow. `picture.frag` is for a picture at another size. `under.frag` copies the pixels beneath a draw over a picture, and tints the selection. Present's fragment shader comes in two pieces, which the color and spatial steps join between. |
| `src/bounds.rs` | Every bound, with its value, what happens past it, and its citation. This file is the one source for the constants and for the table below. |
| `tests/` | The integration tests, and vtium's digests of whole cells and sprites. See its README. |

`cargo test -p vtium-render` runs every test. The bounds table below is generated from `src/bounds.rs`. The test `readme_table_matches_bounds` fails when the table drifts from that file. `VTIUM_RENDER_BLESS=1 cargo test -p vtium-render readme_table` rewrites the table. `cargo test -p vtium-render --test memory -- --nocapture` prints ADR-1300's memory table from filled caches. `cargo test --release -p vtium-render --test figures -- --nocapture` prints the render timer's figures at 1366×768.

## Behaviour

vtium draws every pixel with fixed arithmetic in whole numbers (`src/blend.rs`). So a frame draws to the same bytes on every machine, and tier 2's bitmap-font goldens (vtium/vtium#17) judge it exactly. `tests/digests.rs` and `src/deco.rs` check the renderer against digests of whole cells, and of every underline and strike sprite at 1,152 cell sizes (`tests/digests/README.md`).

ADR-1300 records how the cache and the composition are built. Its two profiles, its 1-bit and scale-1 storage (D2, D4), and its row scratch (D5) change memory and composition, and they change no pixel. Each run of a row's drawn cells is composed in the scratch. The run reaches the display a pixel line at a time, instead of in a call for each line of each cell. At 1366×768, a full frame's writes take 0.05 ms, where writing cell by cell took 0.48 ms (vtium/vtium#78).

The drawn-glyph cache never changes a pixel (ADR-1300 D3). Some glyphs would evict a glyph that the frame draws, or alone pass their pool's byte ceiling. The renderer draws such a glyph from the font without caching it. Test: `tests/cache.rs`, which draws every frame at a cache of 1 and of 7 glyphs, and compares it against a cache of none.

Glyphs that vtium generates arrive between frames (ADR-1200 D23). `Renderer::glyphs_changed` drops two kinds of glyph from the cache. The first is each glyph whose text holds an arrived character, a cluster's text included. The second is each glyph drawn as the font's missing glyph. The next frame scans only the rows that show one of those glyphs, by reading their lines' text. It draws those cells in each buffer at the buffer's age. Every other glyph stays cached.

For example, take an 80×25 screen of printable ASCII, redrawn whole after one character arrives. The redraw draws 1 glyph from the font, where emptying the cache drew 95. It takes a median of 120 µs, where emptying the cache took 186 µs, in a release build on the development machine. Tests: `tests/swap_in.rs` and `forgetting_a_character_drops_each_glyph_whose_text_holds_it`.

`draw` follows the frame contract with the displays (vtium/vtium#18). It takes the buffer that the display lends, and the buffer's age:

- 1 when the buffer holds the last frame drawn with damage;
- n when the buffer holds the frame n back;
- 0 when the buffer's contents are unknown.

Each cell carries the serial of the frame that last changed what the cell shows. What it shows includes its cursor (place, shape and color), its blink phase and its wide pairing. A buffer at age n gets the cells that changed in the last n frames. At age 0, or at an age past the renderer's history, the buffer gets the whole picture and the border. Every other buffer's history stays as it was.

Nothing assumes that the buffers take turns, and nothing counts them. So each of these gets exactly what it lacks: a swap that failed, a third buffer, and a buffer that the display lost. The frame is read since the checkpoint taken after the renderer's previous `draw`, whatever buffer that `draw` drew into. A buffer drawn whole lacks nothing, so a full redraw reaches each buffer once. A keystroke draws only the cell it wrote, and the cells that the cursor left and entered. Tests: `a_full_redraw_reaches_each_buffer_once`, `buffers_at_age_0_are_each_drawn_whole_once` and `a_keystroke_damages_only_the_cells_it_changes`.

A turned display draws the unturned picture, turned. So a wide character cut at the margin shows its left half in the wide cell's first column, in every orientation. The border around the grid is split in the display's own coordinates. An odd remainder rounds down in every orientation. So with 5 pixels to spare, a display turned upside down leaves 3 at the text's left, where an unturned one leaves 2. Tier 2's `edges` goldens show this. Tests: `every_orientation_draws_the_same_picture_turned`, `a_wide_glyph_cut_at_the_margin_shows_its_left_half` and `an_odd_border_rounds_down_in_display_coordinates`.

A wide character's leading half without its trailing half draws in its own cell, and so does a trailing half without its leading half. vtium-term's frame does not promise that the halves come paired, and this way no frame leaves a cell undrawn.

Dim text (SGR 2) draws its foreground halfway to its background. Each channel is the sum of the two channels halved. Dim mixes the colors that reverse video left, and the cursor draws over the result: a block swaps the mix with the background. An underline or strike in the foreground's color takes the mix. SGR 58's underline color is exact. Test: `tests/dim.rs`.

A block cursor's background takes the color that OSC 12 set, and so does an underline or bar cursor's line. The character under a block keeps the cell's background as its color. With no color set, a block draws its cell inverted. A selected cell draws in the colors that OSC 17 and 19 set, which the frame resolves (`vtium-term`'s README). A decoration color may name a palette index. `DecoColor::Indexed` resolves through each frame's palette, so a program's OSC 4 recolors the decorations. Tests: `a_colored_block_takes_its_color`, `the_selection_draws_in_its_colors` and `a_palette_decoration_color_follows_the_palette`.

Blinking text (SGR 5 and 6) draws in the text blink's off phase as `Config::blink` says. The binary sets `Config::blink` from `blink-text` and `blink-decorations`. There are three options:

- `Partial`, the default, draws the glyph halfway to its background with SGR 2's arithmetic. It does so over a glyph that SGR 2 already dimmed, too.
- `Full` draws the cell without its glyph.
- `Off` draws the cell steady, and a phase change redraws nothing.

With `decorations`, the underline and strike hide with the glyph under `Full`, and dim with it under `Partial`. This includes SGR 58's color and a configured color. Without `decorations`, the underline and strike stay as the on phase draws them. The cell under the cursor shows in either phase. The maintainer chose `partial` as the default on 2026-11-03, after the first acceptance pass on a VT, so that blinking text no longer disappears. ECMA-48 says only that SGR 5 makes text blink, and not how the off phase looks. The options themselves are the maintainer's decision on vtium/vtium#81. Tests: `tests/blink.rs`, `the_default_blink_dims_the_text_halfway` and `every_frame_matches_a_whole_draw_under_each_blink_option`.

Rapidly blinking text (SGR 6) has a phase of its own, `DrawState::rapid_hidden`, beside SGR 5's `blink_hidden`. The binary's rapid timer flips `rapid_hidden` every 200 ms (vtium/vtium#37). A cell with SGR 6 draws in the off phase when `rapid_hidden` is set, whether or not it carries SGR 5 too. A cell with SGR 5 alone draws in the off phase when `blink_hidden` is set. Both draw as `Config::blink` says. A change of either phase redraws only the cells that blink at that phase. Tests: `tests/rapid_blink.rs`.

A superscript or subscript cell (SGR 73 or 74) draws vtium-font's scaled variant of its glyph (vtium/vtium#37). The variant is the glyph at two thirds of its size, with its top on the cell's top or its bottom on the cell's bottom. Its underline in each style, its strike-through and its overline follow the small glyph. The renderer makes the decoration sprites in three sets: from the font's normal lines, and from its superscript and subscript lines (`Font::script_decorations`). Each cell draws its own lines from the set of its script. In unifont at 16 rows, a superscript's underline lies on row 10, under the raised glyph. A subscript's strike-through also lies on row 10, across the lowered glyph. An underline across normal and script cells steps at each edge between them. The background, reverse video, the selection and the cursors cover the whole cell, as in a normal cell. The underline cursor keeps a normal cell's rows, and the bar cursor keeps its full height. So the cursor has one shape and one height along a line. Tests: `tests/script_lines.rs`.

The glyph cache keys each glyph on its text, its width and its variant. The variant is bold, italic and the script. So a superscript glyph and a normal one never share an entry, on the CPU path or on the GPU path. Tests: `tests/script.rs`, and `each_script_is_a_glyph_of_its_own` in `src/cache.rs`.

The renderer draws the pointer only when `DrawState::pointer` is set. The owner sets it on a display without a cursor plane. Where the display drives a cursor plane, the plane shows the same arrow (`Renderer::arrow`) at the place that `Renderer::pointer_at` gives. Then moving the pointer involves no draw.

The arrow is a cell tall, kept between 8 and `POINTER_SIDE_MAX` pixels. It is white on a dark outline, with its hotspot at the tip. It is kept inside the grid, and it turns with the display (`src/arrow.rs`). No specification covers a pointer's shape. The arrow is the maintainer's decision on vtium/vtium#92, and it is the same in software as on the cursor plane.

`Config::pointer` can name a character (`pointer-char`). Then the pointer is that character's glyph from the font, one cell wide, white on the same outline. The glyph's top left sits on the corner of the cell under the pointer, wherever in the cell the pointer is. Two kinds of character keep the arrow. One is a character that the font lacks, which would draw as a fallback such as U+FFFD. The other is a blank character, whose glyph covers nothing. Over cells so tall that the outlined glyph passes `POINTER_SIDE_MAX`, the cursor plane refuses the glyph, and the renderer draws it. Tests: `a_glyph_pointer_sits_on_the_cell_under_the_pointer`, `a_glyph_pointer_the_font_cannot_show_keeps_the_arrow`.

The renderer blends the pointer over each run of pixels written beneath it, cells and border alike. So the cells show around the pointer, and nothing is read back. The pointer changes the cells it covers only when it appears, moves or hides. A cell under the pointer that changes is drawn alone, with the arrow over it. The pointer's damage is every cell it covers. Where the pointer reaches past the grid, its damage also holds its area clipped to the display, in which only the border is filled again. The history keeps the last 16 such areas, and a buffer older than those redraws the whole border. Tests: `the_pointer_is_an_arrow_over_the_cells`, `the_pointer_turns_with_the_display`, `the_pointer_leaves_nothing_behind_at_odd_cell_sizes` and `a_move_in_the_bottom_row_damages_only_its_cells_and_the_arrow`.

A frame that changes nothing writes nothing and returns no damage, whether or not the pointer shows. The owner does not swap such a frame, and it does not count as a frame. A change of grid size or of the default background changes every cell and the border.

A link (OSC 8) draws nothing of its own until the pointer is over it (ADR-1002, "Drawing"). The frame marks each cell of the link under the pointer `hovered`. Such a cell draws a dashed underline in place of its own underline style. The dashes take the color that the cell's own underline would take: SGR 58's color where set, otherwise the configured underline color or the foreground. When the hover moves, the frame marks the cells of the old and the new link changed. So the dashes come and go with the cells' own redraws. Tests: `tests/hover.rs`; tier 2's `link-hover-unifont` and `link-hover-unifont-1bit`. The overlay row that shows the link's address while Ctrl is held is not drawn yet. ADR-1002 leaves open what the rest of that row shows.

A concealed cell (SGR 8) draws its background alone. It draws no glyph, no underline, no strike and no overline, and its ink reaches no other cell. The cursor and the selection draw over it as over a blank cell. So a block cursor on a concealed password shows the cursor and no letter. ECMA-48 and ctlseqs say only that the text is hidden. vtium hides the text's lines too, since a line under a hidden word would show the word's length. Test: `tests/renditions.rs`.

An overline (SGR 53) draws along the cell's top rows, over the glyph. In a script cell, the overline lies on the top row of the small glyph's box. It is as thick as the underline, and as wide as the cell. It draws in the strike's color: the configured `strike-color`, else the foreground. Where a cell has both a strike and an overline, the two lines are one sprite, since the GPU path's quad keeps one slot for the lines over the glyph. Both lines are whole pixels, so the one sprite draws what the two lines draw one after the other. The GPU path keeps the sprites in one texture, a grid of cells, as the next paragraphs say. Blinking text's overline follows `blink-decorations`, as the strike does. ECMA-48 names the overline and gives no shape, so vtium chooses the cell's top rows. Tests: `tests/renditions.rs`, and `an_overline_names_its_sprite_in_the_strikes_slot` in `tests/gpu_compose.rs`.

The underline and strike are sprites made from the font's lines (`src/deco.rs`). No specification covers an underline's shape. vtium gives an underline at most a quarter of the cell's rows, because an underline drawn over the letters hides their lower rows. At 8 pixels, a double or a curl drawn whole would take three of the cell's eight rows (vtium/vtium#119).

The single, dotted and dashed lines are the font's line. A face with no metrics of its own puts that line in the cell's bottom rows, one row below the letters at 8 pixels. A curl lowers its wave to fit, down to 2 rows peak to peak. A double, or a curl that still does not fit, draws as the single line. A cell 12 rows tall or more has room for the double, with the lines that `for_height` gives. The cell also has room for the curl when it is at most half as wide as it is tall. A face with lines of its own meets the same quarter at the sizes that its own thickness sets.

A strike is one size of a bitmap font. A strike that vtium-font scales by a whole factor brings the lines of the cell it draws (vtium/vtium#124). When the strike's own lines were `for_height` of its height, the strike brings `for_height` of the scaled height. Otherwise the strike brings its own lines, scaled. The room is the drawn cell's room, so an 8-pixel strike drawn at 16 pixels draws a 16-pixel cell's double. Take a strike whose own lines scale with it, two rows thick from row 14. It draws its double as the 8-pixel single line, scaled. It draws its curl as a wave 2 rows peak to peak in rows 12 to 15, which the 8-pixel cell had no room for. Tests: `every_underline_at_8_pixels_lies_in_the_bottom_row`, `no_underline_takes_more_than_a_quarter_of_the_cell`, `a_double_or_curl_with_no_room_draws_as_the_single_line`, `a_curl_lowers_its_wave_to_fit_before_it_flattens` and `a_scaled_strike_takes_the_room_of_the_cell_it_draws`.

A dash runs from the cell's left edge for `round(5w/8)` pixels of a cell `w` wide, with a half rounded up. The dash is at least 1 pixel long. From 2 pixels wide, it is at most `w - 1`, so a gap remains. The rest of the cell is gap. At 8 pixels, that is `█████···`. One sprite serves a cell size, tiled across a wide cell. The maintainer chose the shape on 2026-10-28, so that each character's dash reads as that letter's own underline. Test: `a_dash_runs_from_the_left_edge_for_five_eighths_of_the_cell`.

`tests/digests/cells.tsv` holds the digests of the earlier shapes at the sizes that these two rules redraw. Those are the sizes where the double and the curl would pass a quarter of the cell, and every cell 2 or more pixels wide for the dash. At those sizes, `every_sprite_matches_its_digest_or_its_redrawn_shape` checks that the sprite differs from its digest, and that it is the single line, the lowered curl or the dash. `a_dashed_screen_matches_its_digest_but_for_its_dashes` checks the dashed screen of `tests/digests/screens.tsv` against its digest, with the earlier dash put back (`tests/digests/README.md`).

The console draws two things of its own over a display's frame, outside the terminal (ADR-1401 D5, D7):

- A `Notice` is vtium's message in a box across the top of the view. Its lines are wrapped to the box and kept clear of the mark. It draws in the colors that its owner gives: `Colors::NOTICE` for a problem, and `Colors::STATUS` for the on-screen display's line alone.
- A head's focus `Mark` sits in the grid cell that its owner gives it. The mark is a filled or hollow circle. With a font that lacks the circles, it is a filled or hollow square.

The console draws both with the built-in fonts. A `NoticeReport` may carry a `Drawing`, which the notice shows to the left of its text or above it. The notice shows the drawing only where the text still shows every line that it shows without the drawing. A build with images draws a `Sprite` in the drawing's place. The sprite is RGB samples matched to a six-entry palette, at a whole-number scale (ADR-1401 D7). The notice and the mark both turn with the display, and both take the 1-bit mapping and stand-ins that a cell takes. Neither touches the renderer's record of the cells.

When what the console drew over a frame changes, the owner gives the areas it covered to `Renderer::redraw_area`. `Renderer::redraw_area` marks the cells and the border under those areas changed on the next frame. So each buffer that the display lends draws them again, whatever the buffer's age (ADR-1401 D2). Tests: `src/notice.rs`, and `an_area_drawn_over_is_redrawn_in_every_buffer` in `tests/damage.rs`.

### Glyph ink past its cells

A glyph from an outline font may draw ink past its own cells. The ink may reach up to one cell to the left and to the right (ADR-1302), and up to one row above and below (ADR-1306). So an accent on a capital, an Arabic mark and an italic letter's lean draw whole. The font gives such a glyph an ink box, the rectangle that its ink covers. The cache stores the glyph at that box (`src/stored.rs`). The two bounds are the font crate's constants `REACH_CELLS` and `REACH_ROWS`. The renderer reads them through `Bound` in `src/reach.rs`, so widening a bound is a change of its constant. Ink past the bounds, or past the grid's edges, is cut.

Each cell remembers its reach in one byte. The reach says how many cells the ink of the cell's last glyph reaches to the left, to the right, up and down, in two bits each. To draw a cell, the renderer finds the neighbours whose reach covers the cell. It composes each pixel in this order (ADR-1306 D4):

1. the cell's background;
2. its underline;
3. the ink of the row above;
4. the ink of the row below;
5. the ink of the cells beside it, the farthest first;
6. its own glyph;
7. its strike-through and overline.

The received ink blends over the background, and it is never XORed. No background is ever drawn over another cell's ink, and the display's buffer is never read. A cell that no neighbour's ink reaches draws exactly as before. So a screen of glyphs that stay in their cells draws the same pixels. Every bitmap screen without italic is such a screen.

A cell whose ink changes damages the neighbours that its old reach or its new reach covers. So every buffer at every age gets those neighbours (ADR-1306 D5). The scan of a row can damage cells that it has already passed: the cells before it in the row, and the cells of the rows above. The renderer draws those cells once the row's scan is done. So a frame whose ink stays in its cells still scans and draws each row once.

The renderer reads no reach at all until the font has drawn a glyph whose ink passes its cells. So a screen in the built-in font, or in a PSF font without italic, does no reach work. The frame that draws the first such glyph draws the whole grid again, with every reach read. The GPU path reads the reaches as the CPU path does, from its own glyphs, so its stamps and its damage are the CPU path's.

Some cells hold their ink in their own ways:

- The letter that the visible cursor stands on keeps its ink above and below its row, in its colors without the cursor, in both phases of the cursor's blink. Its ink beside it stays in its cell.
- A blinking letter's ink stays in its cell. So does a letter's ink that its own background hides.
- Inside a block cursor's cell or a selected cell, every ink takes the mark's text color.
- A selected glyph's ink outside the selection takes its colors unselected, read from the frame without its selection.
- At 1 bit, ink that lands in another cell is the opposite of that cell's background.

Received ink belongs to the text layer of the cell it lands in. So a kitty picture below the text shows that ink, and a picture at z 0 or more, or a sixel, hides it (ADR-1306 D7). Tests: `tests/overhang.rs`, and `src/renderer/wider.rs`. `src/renderer/wider.rs` runs the composition and damage at a sideways bound of two cells, against a whole draw at every buffer age. On the GPU path, the tests are `tests/gpu_ink.rs` and `src/renderer/wider/gpu.rs`, which the GPU path's section below describes.

### 1-bit and gray displays

A 1-bit display draws in black and white (vtium/vtium#135). `Config::depth` can be `Depth::One`, which the binary sets from `display-depth` or from the levels that the display reports. Then every pixel that the renderer writes is `0x000000` or `0xffffff`.

Colors map to lit or unlit by cell, never by brightness. That is the operator's decision (2026-10-28). Every background is lit or unlit by one cut, and that is the operator's decision too. A background whose BT.601 luma is 128 or more is lit, and one under 128 is unlit. 128 is the cut at which the kernel lights a mono panel's pixel. The default background sets the screen, so a light theme draws dark text on a lit screen. A cell's own background takes the same cut, reverse video and the selection included. So a background on the default's side draws like the default, and one on the other side flips. A light theme's black on white draws dark on lit, as written.

A cell's foreground and decorations are its background's opposite, even where both fall on one side of the cut. They take the background's own state only where they equal the background. So a foreground equal to its background stays invisible, as on a color display.

What marks state is never lost to the cut, by the operator's decision. Reverse video (SGR 7) draws the opposite of what the cell draws unreversed. vtium draws the selection and a block cursor to mark state. They invert the cell's state again, whatever their colors, and their text is always its new background's opposite. So a selected or reversed cell always differs from the same cell plain. An underline or bar cursor is always its cell's ink. Colors compare by value. The border takes the screen's state. The pointer stays white in its black outline, which shows it on a lit screen.

Each pixel's coverage, a glyph's or a sprite's, is lit at 128 and above. That is the threshold that the kernel's `drm_fb_xrgb8888_to_mono` gives a DRM mono panel. So a frame drawn here passes that conversion unchanged. The mapping and the cut ask nothing of where a glyph came from. A vitrified set's strike, the built-in unifont, a PSF file, and a strike rasterised at run time on a larger machine (vtium/vtium#123) follow the same rules. That is the operator's decision (2026-10-28). Nothing in the rules is the CPU path's alone, so a hardware path draws a 1-bit display by the same rules.

A color glyph at 1 bit is the built-in unifont's bitmap of the same character, by the operator's decision. That holds where unifont draws the character, and where the glyph's cell, at a whole scale, is the font's cell. Elsewhere vtium draws the glyph's silhouette: its alpha cut at half, in the cell's foreground. The silhouette keeps the glyph's shape where the replacement glyph would lose it. Dithering a color glyph is a set's build option (vtium/vtium#136).

The curly underline at 1 bit is a wave of whole pixels. The wave is the line's thickness in each column, joined from column to column. Its 8-by-8 sampled coverage, cut at half, could leave gaps.

vtium-font draws 124 of its sprites with partial coverage: the rounded corners U+256D to U+2570, the diagonals U+2571 to U+2573, the shades, and the Legacy Computing wedges. At 1 bit their coverage is cut at half like any glyph's. Where the cut breaks a sprite, vtium-font's 1-bit shape of the sprite stands in (`vtium_font::sprite::one_bit`):

- The shades U+2591 to U+2593 are ordered 2×2 patterns, a quarter, half and three quarters lit. The cut drew the light shade empty and the others solid.
- U+1FB9A and U+1FBBD to U+1FBBF light every pixel they cover. The cut split their tips and thin diagonals.

A shade's pattern starts at its cell's top left. So cells of even sides join without a seam, and a cell of an odd side meets its neighbour a pixel out of phase. The hatches U+1FB98 and U+1FB99 fall into their separate stripes at 1 bit, as drawn. A sprite that a bitmap font draws is 1-bit already, and it stays as it is. Test: `tests/sprites_1bit.rs`, which checks that every sprite keeps its pieces at 1 bit from 8×8 to 16×32.

Where #135 left the rule open, vtium chooses:

- Dim text (SGR 2) draws as plain text, since its half-way color is not the background.
- `blink-text=partial` hides the glyph in the off phase as `full` does, since the half-way glyph would never blink.
- The pointer is drawn white with a black outline, as it is in color, with the outline cut to black.

Tests: `tests/mono.rs`, `src/mono.rs`'s, `a_1_bit_curl_is_a_joined_wave_of_whole_pixels` and vtium-font's `tests/shade.rs`.

A gray panel draws as a color display. `Depth::Gray(n)` records a gray panel's levels, detected or set. The frame is drawn as in color, with no mapping by cell and no cut to levels. The panel converts the frame to its levels itself. That is the operator's decision (2026-10-28), since each gray panel's conversion is its own. The one exception, by the operator's decision too, is a color glyph. A color glyph is the built-in unifont's bitmap of the same character, as at 1 bit, drawn in the cell's own colors. Where unifont has no such bitmap, the glyph draws in color. Rules for quantizing and for contrast wait for a real gray panel to judge them on. Test: `a_gray_display_draws_as_a_color_one`.

### Pictures

The `images` feature draws a frame's image placements (ADR-1100 D4, D5, D9; vtium/vtium#30). The binary turns the feature on with `vtium-term/images` and vtium-image. The minspec build leaves it out. Without the feature, the crate draws exactly what it drew before, and the minspec binary is the same size. The binary hands the renderer nothing new. A frame built with vtium-term's `Frame::with_placements` carries its placements, and `draw` reads them. `DRAWS_PLACEMENTS` says whether a build draws them. A head needs that answer before it offers sixel or kitty graphics (ADR-1100 D10). `Renderer::picture_counts` reports what the scaled-copy cache did.

- **The layers.** A cell that no picture's pixels reach is drawn exactly as without the feature. A cell that a picture reaches is composed in the row scratch from the bottom up, in ADR-1100 D4's order:
  1. the cell's background, with the layer below backgrounds (kitty z below -1,073,741,824) over it only where the frame cell's `default_bg` says that the background is the default;
  2. the layer below text (kitty z from -1,073,741,824 to -1);
  3. the cell's text layer, premultiplied, as a cell of several layers composes it (ADR-1300, "Carried forward");
  4. the layer above text (kitty z of 0 or more, and every sixel);
  5. last, vtium's mark.

  Within a layer, the pictures draw in the order that the frame hands them, which is the store's stack order. A picture's premultiplied pixels go over what lies beneath with the same arithmetic as the text layer, `src + div255(dst * (255 - a))`. An opaque picture over the whole cell hides what lies under it. So the cell's composition starts from the highest such picture.
- **The cursor and the pointer draw above every picture, and the selection tints it.** A block cursor covers its whole cell, so that cell is drawn as if no picture were there. A selected cell is composed with its pictures. Then the selection's color goes over the whole cell at `SELECTION_TINT`, 128 out of 255. So the cell shows both the selection and the picture under it (the maintainer, 2026-11-01). A cursor in a selected cell lies above the tint. A bar cursor's bar and an underline cursor's line are drawn again over the pictures. Under the bar, the cell's letter is not drawn, as without pictures. The pointer's arrow is blended as each line is written. vtium's notices are drawn over the buffer after the renderer. So the pointer and the notices both lie above every picture too. Test: `the_cursor_the_selection_and_the_pointer_draw_above_every_picture`.
- **A kitty Unicode placeholder cell draws no text.** A cell whose cluster starts with U+10EEEE marks where the picture of a virtual placement draws (ADR-1100 D13). So the renderer draws the cell's background and no glyph, diacritics included. It draws no underline, no strike-through, and no dashed underline of a hovered link. The cursor and the selection draw over the cell as over any cell. A cluster that holds U+10EEEE after another character is text. Without the feature, U+10EEEE is a private-use character like any other. Tests: `tests/placeholder.rs`.
- **Scaling and turning.** A placement's pixel values are in the cell that the placement was made in. A value `m` measured from the grid's origin lands at `floor(m * cell / made)` in the renderer's cell. So the pieces of one picture meet with no gap and no overlap at any cell size (ADR-1100 D8). A picture is cut at its box and at the grid.

  Scaling is an integer filter. Its weights are 16-bit fixed-point numbers that sum to exactly 65,536 on each axis. It is a box filter where a picture shrinks, and bilinear where it grows. A copy is made in two passes. First, each source row is filtered across once, into a bounded ring of rows. Then each output pixel sums its rows down, and is rounded once. The sums are whole numbers, so a picture scales to the same pixels on every machine. A picture scaled for one frame alone, a pixel at a time, gets the same bytes. A turned display draws the unturned picture turned, as it draws the grid.
- **The scaled-copy cache.** A picture drawn at its own size is read straight from the image's shared pixels. On a turned display, the read goes along each display line with a fixed step through the image. Every other picture is drawn from a copy, scaled and turned for the display. The renderer makes the copy when a cell under the picture is first drawn. It keeps the copy in a cache of the display's own, outside the image budget, so a copy never evicts a stored image (ADR-1100 D5).

  The cache's key is the whole picture. The key holds the image, the frame shown and its pixel revision, and the image's size. It also holds the placement's crop (the part of the image it shows), the size it is drawn at, and the turn. The frame's entry carries the crop, the drawn size, and the picture's offset in its box, for the whole placement. So the pieces that a cut leaves, and a picture scrolling off the top, all share one copy. A copy never reads past its crop.

  The cache holds at most `PICTURE_CACHE_SCREENS` screens of 4-byte pixels. A copy that needs room evicts the copy placed longest ago that no placement of the frame shows, with ties going by key. It evicts only when that makes the copy fit. A copy that still does not fit is scaled for the frame alone and not kept. That copy has the pixels that a kept copy gives, so the cache changes speed and never pixels (ADR-1300 D3). A picture moved up a row each frame is therefore scaled once. A turned screen scrolling twelve pictures evicts only what its first frame evicted. Tests: `a_scaled_placement_moved_up_a_row_each_frame_is_scaled_once`, `a_turned_screen_scrolling_twelve_pictures_evicts_only_what_its_first_turn_evicted`, `the_scaled_copies_never_pass_their_bound` and `a_picture_scaled_for_one_frame_draws_its_kept_copys_pixels`.
- **Animated pictures.** The entry of a kitty animation's placement carries the frame that the placement shows (ADR-1102 D3). The renderer draws that frame's pixels as it draws any picture's. The store fills in the image's current frame. That is the frame that `a=a` with `c` made current, or the root frame while no command has chosen another. The binary's animation clock will put the frame it chooses on the entry with `FramePlacement::show`. No clock does so yet, so today every animated placement draws its current frame.

  The image's key, the frame's number and the frame's pixel revision name the pixels. The store never reuses an image's key. It gives a frame a new pixel revision whenever a command changes what the frame shows. The renderer keys its copies, and its record of what it last drew, on all three. So a placement that moves to another frame, or whose frame a command changed, draws its cells again from the new pixels. It never draws them from another frame's copy.

  The cache keeps at most one copy of each frame of a picture at each size that the frame is drawn at. Sometimes the cache makes a copy of a frame at a newer pixel revision. Then it drops the copies of that frame's older revisions that no placement in the draw shows, since the store never shows them again. An animation whose copies all fit in the cache is scaled only in its first pass through its frames. An animation whose copies do not all fit is scaled again in every draw that shows it. Each draw scales a whole copy and evicts another, as a picture replaced in every draw is scaled. The animation's copies stay within `PICTURE_CACHE_SCREENS` screens.
- **One image store for each renderer.** The cache's keys and the damage history name images and placements by their `ImageKey` and `PlacementKey`. Each console's store numbers those keys from zero. So a renderer draws the frames of one store, and so of one console, for its whole life. The binary gives each console its own renderer. It gives a display a new renderer when the display starts showing another console. Frames of two stores through one renderer could take one picture's copy or history for another picture's.
- **Damage.** The renderer keeps each placement as it last drew the placement. A placement can arrive, leave or change. For each such placement, the renderer draws the cells of its old box and of its new box again, in every buffer at the buffer's age. So a picture that moves or changes redraws only the cells it covers. vtium-term also marks the cells under each change that the store makes (ADR-1001), so the two rules meet. The renderer's own rule also covers a placement list that changed while the frame's cells did not. Test: `a_picture_that_changes_or_moves_redraws_only_the_cells_it_covers`.
- **Malformed and huge placements.** A placement draws nothing when it has a zero size, a zero cell, a source rectangle outside its image, or pixels of the wrong count. A picture can be larger than the cache, such as one fitted to the store's largest box of 10,000 cells a side. Such a picture is scaled for each frame, and only in the cells that the grid shows.

- **What pictures cost.** These are machine figures, printed and not pinned. They come from a release build on the development machine on 2026-11-02. Other agents' builds kept the machine's load average at about 10 on 32 cores. At 1366×768 in 8×16 cells, a full redraw takes these medians:
  - 0.62 ms with no picture;
  - 0.59 ms under one full-screen picture at its own size;
  - 0.59 ms under one picture scaled to the screen from its kept copy;
  - 5.40 ms for text over one full-screen picture below text;
  - 0.61 ms with a ten-cell picture.

  The renderer copies cells side by side under one opaque picture, which nothing draws over, from the picture as a run. Each display line of the run is one copy. That is why a full-screen picture costs about what a screen of text does. A cell whose text lies over a picture composes each line from its layers, and that is the 5.40 ms. A frame where only a ten-cell picture moves a column draws its 12 old and new cells in 0.012 ms. Scaling the full-screen picture takes about 7 ms more, on the frame that makes its copy.

  A picture replaced every frame, as a video player sends it, pays that cost every frame. A full-screen picture at its own size takes 0.58 ms a frame at 1366×768, and 13.7 ms at 7680×2160 unturned. On a display turned a quarter, it takes 2.0 and 46.8 ms. A picture scaled to twice its size takes 6.7 ms and 136 ms, or 7.4 and 153 ms turned. The command is `cargo test --release -p vtium-render --features images --test picture_figures -- --ignored --nocapture`. For a picture replaced every frame, the command takes `--test picture_video` instead.

Tests: `tests/pictures.rs`, `tests/picture_frames.rs`, `tests/picture_goldens.rs`, and the tests of the scaler, the cache and the frame-alone scaling in `src/picture/`.

### The GPU path

The `bidi` feature draws right-to-left text (ADR-1004 D6, D7, D9). It draws from the order, the joining sides and the mirrored characters that vtium-term's frame gives each row. The minspec build leaves the feature out. Each letter of a joining script is looked up by its key. The key is the letter and its marks, with U+200D on each side that the letter joins (`joined::text`).

Take lam followed by alef, where the row draws alef on the left and lam beside it on the right. The pair draws as one ligature across the two cells, keyed by both letters (`joined::span`, `joined::pair_text`). The alef's cell draws the ligature as a wide character's first half does. With the visible cursor on either cell, each letter draws its own joined form instead, so the cursor marks one letter.

A cluster whose first character is mirrored draws the counterpart, with its marks. A bar cursor on a character that resolves right to left draws U+2595, RIGHT ONE EIGHTH BLOCK, on the right side of its cell. On a wide character, the bar takes the character's right half. The console asks the glyph helper for the same keys through `joined`. So what the console asks for and what the renderer looks up are the same text. Tests: `tests/bidi.rs`, run with `cargo test -p vtium-render --features bidi`.

The `gpu` feature adds the GPU path's frame. The feature is off by default. The minspec build and a default build compile none of it, and they link no GL. `cargo test -p vtium-render --features gpu` runs its tests.

vtium-render forbids unsafe code, so it never calls GL itself. It draws through the `Gl` trait, which speaks the renderer's terms. The trait offers these things:

- textures of a stated format and size, filled a rectangle at a time;
- the upright frame and an imported display buffer, as targets to draw into;
- programs compiled from vtium-render's own shader source, with named inputs;
- a batch of quads drawn through a program;
- a frame's end, which gives a fence that the render thread waits on before the display shows the buffer;
- the GPU's limits;
- a read of a rectangle back to memory, for tests.

The binary implements the trait with an adapter over vtium-platform's safe GL type. That type holds the unsafe calls to GL, EGL and GBM (ADR-1303 D7).

Every handle that the trait gives out is an owned value that names the context that made it. A handle is a texture, a target, a program or a fence. A handle cannot be copied, and freeing it consumes it. Each implementation keeps a `Ledger` of its context's live handles. The implementation passes every call through the ledger before the call reaches GL. This is the one list of what the ledger refuses:

- a handle another context made, or one that was freed, and a fence already waited on;
- a texture or upright frame past the GPU's largest texture or viewport, and a display buffer past its largest renderbuffer or viewport;
- an upload outside its texture, short of its bytes, or with rows a part of a pixel apart;
- a program that lists an input twice;
- a program whose shaders declare no uniform of the input's name and type, or declare it as an array;
- any program on a GPU without `highp` in fragment shaders;
- a draw that gives one of its program's inputs no value, a value twice, a value of the wrong kind, or a value for an input the program lacks;
- a draw that reads the texture it draws into;
- a read outside its target, or into a buffer of the wrong length;
- every call once the context is lost.

A real driver refuses more than the ledger can see, and only a real GL shows those refusals. Those refusals are a shader that does not compile and a program that does not link. They also include a uniform that the driver dropped because nothing uses it (its location is -1), and a texture or framebuffer that is incomplete. The stand-in compiles any text it is given. Part 2 of slice 0, which builds the adapter, proves that the shaders compile and link on llvmpipe. The ledger never gives a serial out twice. So a freed handle never names a newer object, even where GL reuses the freed object's name. The upright frame is a target that holds its own texture, and freeing the target frees the texture.

GL reports most of its errors late, through an error flag. The renderer reads that flag only at the boundaries that ADR-1303 D7 names, through `Gl::check`. It checks once after a batch of allocations and uploads, once after the composition's draws, and once at the end of a frame. It checks only when the batch made a call, so a frame that changes nothing makes no call at all. A check after every call would make a threaded driver wait for the GPU each time.

Each error carries its kind and the batch whose check found it, so that the head can sort the error as ADR-1303 D9 decides. The kinds are out of memory, a lost context, another GL error, a refused handle, a limit, a misuse, a failed compile, and a failed import.

A frame that drew ends with `Gl::submit` and `Gl::wait`. `Gl::submit` gives a fence after the frame's commands. `Gl::wait` waits until the GPU has passed the fence, and frees it. So the buffer goes back to the display finished. vtium-platform makes the fence an EGL fence sync. Where it can make none, it ends the frame with `glFinish` instead. vtium-platform fails a wait that passes `GPU_FENCE_WAIT_MS` as a lost context.

`StandIn` is the tests' GL. It keeps a ledger as the adapter does, so it refuses every call that the list above names. It records the calls that pass. It keeps what each texture is filled with. It draws the clears, and the quads of the cell program, into the upright frames that it made. It draws them through `src/gpu/model.rs`, a model of the cell shader. The model reads the quads, the textures and the uniforms as the shader reads them, and it does each blend with `src/blend.rs`. `StandIn` draws no other program's pixels. So present, the pictures, the arrow, and a text layer over a picture draw nothing there.

A test reads an upright frame with `StandIn::frame_pixels`. A test can give `StandIn` smaller limits, to drive a frame past the largest texture. A test can raise a GL error that the next check reports. A test can make the next texture, frame, import, compile or wait fail, with a kind that it chooses (`StandIn::fail_next`). It can also make one fail a number of calls later (`StandIn::fail_after`), as a driver would. Pixels are judged on a real GL, Mesa's llvmpipe, in tier 2, once the composition and present draw.

`Renderer::draw_gpu` draws a frame on the GPU path. The renderer decides what the frame draws through the same front end as `Renderer::draw`. The front end decides where the grid sits, which cells changed, and how each one looks, its cursor, blink and decorations included. It also decides which parts of the border are stale, and where the pointer is. Only the back end differs. The CPU path composes each row's cell jobs in the row scratch, and writes them into the display's buffer. The GPU path turns each job into one quad through the cell shader. It turns the border's stale parts, less the grid, into clears.

The GPU path composes into the upright frame, which keeps the last frame. So the renderer treats the upright frame as a buffer one frame old. It treats the upright frame as a buffer at age 0 when the pipeline composes it whole. The pipeline does that when the upright frame is new, after a failed frame, for a new border color, or when another renderer's glyphs were in the pipeline. A frame that the CPU path draws between two GPU frames moves the renderer's history on, while the upright frame stays. So the renderer takes a new identity, and its next GPU frame replaces the pipeline's glyphs and draws whole.

`draw_gpu` refuses a 1-bit display until present's depth reduction is built. Present then takes what the display's buffer lacks over the buffer's own age. `draw_gpu` takes the console's notices and focus mark as `Placed` pictures, which `Notice::upright` and `Mark::upright` give. Each is uploaded whole when it is new, or when it changed since the last frame. Otherwise it is drawn over what the frame drew beneath it. When the last frame drew a notice and this frame does not, the cells beneath the notice are drawn again, as `redraw_area` draws them on the CPU path. Tests: `tests/gpu_compose.rs`.

The cell shader (`src/gpu/glsl/cell.frag`) draws a cell with the CPU path's integer arithmetic, `src/blend.rs`. ADR-1300 D6 decides this, and the maintainer confirmed it on 2026-11-03. The rules are these:

- every value is a whole number in a `highp` float;
- `div255` has its shifts written as floors of divisions by 256;
- texels are read whole with `texelFetch`;
- there is no dithering;
- each channel `c` is written as `(c + 0.25) / 255`.

A quad carries the cell's foreground and the glyph's kind, and its background. It also carries the colors and sprites of its underline, and of the lines over its glyph: the strike, the overline, or both. Each kind sits in the top byte of a color. A cell of one layer is `mask` or `over_bg`. A cell of several layers composes its underline, glyph and strike premultiplied, and then goes over its background, as `cell_line` does.

A glyph's ink that reaches past its cells is drawn into the cells around it, as the CPU path draws it (ADR-1306 D9, ADR-1300 D6). A glyph whose ink passes its cells goes to a fifth pool, the boxes. Each slot of the boxes holds the largest box that the glyph's ink may have at the bound. That is a wide glyph's two cells and the bound's cells to each side, by its row and the bound's rows above and below. The boxes count against the masks' budgets. They lie in one R8 texture, so that the shader samples every neighbour's glyph from it.

When the texture is full, a texture with twice its rows takes its place. The new texture is filled again from the glyphs that the boxes keep for it, so a slot keeps its place. This goes on until the texture reaches `GLYPH_TEXTURE_SIDE`. After that, a glyph that the boxes have no room for is drawn cut at its cells, and its ink reaches no other cell. The glyphs then give a note for the log.

Each frame names, in the ink grid (`src/gpu/ink.rs`), the neighbours whose ink the frame's cells receive. For each neighbour, the ink grid holds these:

- the reach byte that the renderer keeps for the neighbour's cell once the frame is scanned;
- its span;
- its box;
- the color its ink takes in a cell that no mark covers.

Each cell of the grid also holds a bit for each display line through it. Once the frame is scanned, the renderer sets the bits of each cell that receives ink. A bit is set when a neighbour's ink reaches that display line anywhere in the cell's job. The renderer learns which lines a neighbour's ink reaches from that neighbour's box. When a glyph takes its box, the glyphs note which rows and columns of each cell of the box hold ink (`src/gpu/box_lines.rs`). So a frame only combines those notes. A cell whose bits the last frame set, and this frame does not set, has its bits cleared. So no job reads the bits of an earlier frame.

A display line runs across the job, the same in each of its cells. On a display turned a quarter, a display line runs down one column of the upright frame. A cell holds as many bits as the larger of its width and its height, 32 to a texel. The grid is an RGBA8 texture. Each cell has two texels for its entry, and one texel for each 32 of its line bits. A frame fills only the rows whose entries or bits changed. An entry that the last frame named and this one does not is emptied.

A cell that receives ink is flagged in its quad. The cell shader built with `#define INK` draws such cells, in batches of their own. The cell shader built without `#define INK` draws every other cell. So a screen whose ink stays in its cells costs what it did.

The composition compiles every program that it draws with when the pipeline is set up. These are both cell shaders, the arrow's shader, and the under shader. In a build with `images`, they also include the picture shader without and with `#define OVER`, and the cell shader with `#define OVER` and `#define INK` for the text layers over pictures. So no frame compiles a program. That holds for the frame in which the first glyph whose ink passes its cells takes its box. It also holds for the first frame with the pointer, and the first frame with a picture. Mesa's llvmpipe still turns a program into machine code the first time a draw uses it. So on llvmpipe with an empty shader cache, that frame waits for the ink program's machine code. Mesa's shader cache keeps the machine code for later runs.

The shader takes the bound as the compile-time constants `REACH_SIDE` and `REACH_ROWS`. A pixel on a display line whose bit is clear receives no ink, and the shader draws it as in a cell that receives none. For each pixel on a line whose bit is set, the shader walks the neighbours in ADR-1306 D4's order, in loops that those constants bound:

1. the rows above, each from the column before the cell to the column after it;
2. the rows below, in the same way;
3. the cells beside it, farthest first, and at one distance, the cell before it first.

Some neighbours have a reach byte that covers the cell, and a block that covers the pixel. Each such neighbour lends its box's coverage there, tinted with its color. Where a block cursor or the selection marks the cell, the coverage is tinted with the cell's foreground instead (ADR-1306 D6). The received ink is blended premultiplied into one layer, as `receive` does. That layer goes over the underline and under the cell's own glyph, as `received_over` does. It is never XORed. A display line whose bit is set is composed as a cell of several layers, as `cell_line` composes it. Its rounding differs from that of a cell of one mask where the mask's coverage is partial. A cell's text layer over pictures carries the ink that it receives (ADR-1306 D7).

The renderer draws a cell a second time in a frame once the row below has sent it ink (ADR-1306 D5). The GPU path draws each batch of quads whole. So the cell's second drawing replaces its first in every list: its cell quad, its text layer and mark, its part of each picture, and its tint. Each cell is drawn once, as the CPU path's drawing in order leaves it. A frame that draws no cell twice does no more work for this.

The tests of the ink grid are these:

- `tests/gpu_ink.rs` compares the stand-in's upright frame with the CPU path's whole draw. It makes this comparison at each turn of the display. The cases are an accent over the row above, a descender meeting an accent, and the letter under a block cursor in both blink phases. They also include a cell that the frame draws twice, and a typing script. It also checks the order of a text layer's ink and the pictures. It checks that a cell under a picture drawn twice draws each layer once. It checks that no frame compiles a program, including the first reaching glyph's frame, the pointer's frame and the first pictures' frame.
- The unit tests in `src/gpu/ink.rs` check the line bits set for a known box and layout, across a job and down a turned one. They also check where the bits lie in the texture.
- `src/renderer/wider/gpu.rs` does the same as `tests/gpu_ink.rs` at a sideways bound of two cells and at two rows. Its glyph budgets are large enough that neither path evicts the glyphs that the tests put in.

Both of those draw through the stand-in's model of the shader. So the shader itself is judged by the `ink` tests in vtium's `src/gpu.rs`, on llvmpipe. Those tests feed bytes to a terminal, and draw each frame on the GPU path into a buffer one frame old. They read the display back, and compare it pixel for pixel with the CPU path's whole drawing. They cover a descender meeting an accent, a block cursor through three blink phases, the selection, and reverse video. They also cover ink reaching a picture's cell at four layers, a cell drawn twice with and without a picture, and a typing script. Each runs at the display's four turns. One more test runs a typing script at a bound of two cells. For that test, `Renderer::set_reach_bound_for_test` sets the bound. `Renderer::keep_glyph_for_test` and `Renderer::put_glyph_for_test` give the two paths glyphs whose ink reaches two cells. No font draws such glyphs while the font crate's bound is one cell. Tier 2's `@gpu` cases draw tier 2's screens on llvmpipe too. But the shader defects that these tests catch moved no tier 2 result. Those defects are received ink drawn in the wrong order, over the cell's own glyph, without a mark's color, or along the wrong lines of a turned display.

The glyphs live in ADR-1300's four pools, masks and color glyphs one or two cells wide, under the same budgets as the CPU path's. The glyph cache is generic over what it holds. The CPU path keeps a glyph's pixels in it, and the GPU path keeps a slot in a pool's textures. So both share one key, one LRU and one budget check. A slot is the glyph's cells. A pool lays its slots out as a grid, masks in R8 and color glyphs in RGBA8. Its first texture holds 64 slots, and each later texture holds twice the one before, up to `GLYPH_TEXTURE_SIDE` pixels a side. A slot is filled once, from the glyph as the CPU path stores it, read a line at a time at the size it is drawn. So its pixels are the CPU path's.

Some glyphs find no room in the budget, because they would evict a glyph that this frame draws. Such a glyph draws from a slot held for the frame, and the slot is given back at the frame's end (ADR-1300 D3). The next frame asks the budget again. So a pool that refused the glyph takes it once the frame no longer draws all that the pool holds.

The decoration sprites are one R8 texture. Each kind of line in each script's set is a cell of that texture. The cells are laid out as a grid whose larger side is as small as the sprites allow, so that the texture stays near square. For example, an 8×16 cell's 24 sprites lie six across and four down, in 48×64 pixels, where a row of them would be 192×16. A cell's quad names each line's sprite by its kind and its cell's script. The cell shader takes the sprites across from the `cell` input's third value. A GPU whose largest texture is too small for the grid fails to make the glyphs, with an error that names the sprites. OpenGL ES 3.0 promises a largest texture of at least 2048 pixels. So only a cell hundreds of pixels wide or tall reaches that limit. Tests: `the_sprites_lie_in_a_grid_that_fits_a_largest_texture_a_row_of_them_passes`, `sprites_past_the_largest_texture_fail_with_an_error_that_names_them`, `every_decoration_draws_on_the_gpu_path_as_on_the_cpu_path`.

A new renderer, made for a new font or cell size, replaces the pipeline's glyphs and draws whole. Tests: `a_pool_its_budget_refuses_draws_from_a_held_slot_and_takes_the_glyph_once_the_budget_allows`, `a_pool_past_its_first_texture_takes_another_twice_its_size`, `a_new_renderer_on_the_pipeline_replaces_the_glyphs_and_draws_whole`.

The pointer's arrow is drawn upright (`Renderer::upright_arrow`, `Arrow::upright`) as one texture. That texture is uploaded when the arrow's image changes. The arrow is drawn over the pixels that the frame drew beneath it, and over no others. It is drawn as quads cut to those pixels a line at a time, with alike lines joined. This is needed because the upright frame keeps earlier frames' arrows where it drew nothing. In each tile, the pixels beneath those quads are first copied out into the copy that the pictures' paragraph below describes. The arrow's shader (`glsl/arrow.frag`) goes over them with the CPU path's `over_bg`, in whole numbers. It writes each channel as `(c + 0.25) / 255`. So the arrow's outline, black at 220 of 255, lands on the CPU path's byte on any driver. The GPU's own blending is not used. Tests: `the_arrow_is_one_texture_drawn_over_a_copy_of_what_the_frame_drew_beneath_it`, `the_arrow_covers_each_drawn_pixel_under_it_once`, and in the binary's adapter, `the_pointers_arrow_on_llvmpipe_is_the_cpu_paths_byte_for_byte`. That last test judges the arrow's pixels on llvmpipe, since no tier 2 screen draws the arrow.

The glyph cache, on both paths, remembers up to `FAILED_GLYPHS` glyphs that the font drew nothing for. It does not ask the font for them again until `glyphs_changed` or `forget_glyphs` names any character. The font draws nothing for a glyph only after its whole fallback chain drew nothing: U+25A1, `?` and its last resort. So any arrival may give the font a glyph, and the table then empties. vtium's font draws nothing only for a glyph it cannot draw, never for want of memory, so the table keeps no expiry. A glyph refused for room is not remembered. Tests: `a_glyph_the_font_draws_nothing_for_is_asked_for_once_until_forgotten`, `a_glyph_the_font_draws_nothing_for_is_not_asked_for_every_frame`.

A character that no font holds draws the font's missing glyph, which comes from the same chain. So the missing glyph's drawing can change when a set arrives, even a set that does not hold the character itself. That happens, for example, when the first set brings its own U+25A1. The font says when it draws its missing glyph (`Drawn::missing`, which vtium-font's README describes). Each cached glyph on both paths records that answer. On any arrival, through `glyphs_changed` or `forget_glyphs`, the cache drops each glyph that it recorded as the missing glyph.

The renderer keeps the characters of each text that it drew as the missing glyph, or drew nothing for, since the last arrival. The next frame draws again each cell that shows one of those characters, whether or not a cell shows an arrived character. When more than `MISSING_CHARS` characters drew that way, the renderer keeps none of them, and the next arrival draws every cell again. `Renderer::redraws_missing` says whether that frame has such cells to draw. The console then asks for the frame at once. Tests: `tests/missing_glyph.rs`, and in vtium, `a_set_that_brings_u25a1_redraws_a_missing_character_at_once` in `tests/boot.rs` and `a_later_u25a1_redraws_the_lone_base_of_a_held_cluster` in `src/deferred.rs`.

The renderer makes its underline and strike sprites from the font's lines (`Face::decorations`). It takes the lines again when they change. An outline font's renderer is made before the glyph helper has measured the font's lines. So the font first gives lines from the cell's height, and gives the measured ones later. `glyphs_changed` and `forget_glyphs` each compare the font's lines with the ones that the sprites were made from. When they differ, the renderer makes the sprites again, and its next frame draws every cell and the border.

The glyph cache keeps its glyphs through that change. The font draws the box-drawing sprites in a weight that follows the cell's height (`vtium_font::sprite::light_line`), not the font's lines. `tests/box_lines.rs` checks that a 20×38 cell's light, heavy and double lines are 2, 4 and 2+2 pixels after the lines change. On the GPU path, the renderer takes a new identity. So the pipeline makes its glyphs and the sprites' texture anew, and composes whole. Lines that did not change change nothing. Tests: `tests/lines.rs`.

The upright frame is held in tiles that `tiles` gives (ADR-1303 D2, D3). `tiles` is one function. Given an image's size, the largest tile and an overlap, it gives each tile's held part, and the part that the tile owns, up to the middle of each overlap. The upright frame's largest tile is the smaller of the largest texture and the largest viewport. So a display that fits is one tile, which is the common case, and one tile and many tiles take the same path. The overlap is twice the spatial step's reach, so that each pixel a tile owns has the reach of neighbours on each side in that tile. With no spatial step the overlap is 0, and the tiles meet edge to edge. The composition clears and draws each rectangle into every tile it meets, with the tile's offset taken off its quads. Present draws from each tile the part of the buffer that the tile owns. Tests: `src/gpu/tile.rs`, `a_display_past_the_largest_texture_is_held_in_tiles`, `a_display_past_the_largest_texture_draws_each_cell_into_each_tile_it_meets`.

A display's frame passes through the `Pipeline`'s five slots in a fixed order (ADR-1303 D1):

1. pictures at another size;
2. composition into the upright frame;
3. color;
4. spatial effects;
5. present into the display's buffer.

A slot with nothing to do holds a no-op. The no-op makes no GL call, joins no shader source, and changes no pixel. The color and spatial slots hold only their no-ops until the frame shader's record defines their steps. A step that is switched on joins its GLSL into present's fragment shader, between its head and its `main`, when the pipeline is set up. So an empty slot adds no pass and no instruction. `Spatial::reach` gives how far the spatial step reads, which sets the overlap of the upright frame's tiles. `Spatial::presents_whole` says whether every present draws the whole buffer.

The pipeline's set-up makes the upright frame, in RGBA8 in its tiles. The upright frame is the display's size, with its sides swapped for a quarter turn. The set-up compiles the cell shader, the arrow's shader and present's shader, and it checks once. Each frame's `Work` names the rectangles cleared to the border's color, the cells, the arrow and the notices, and the rectangles that changed. The composition draws the whole upright frame when the frame is new or the border's color changes. The CPU path likewise draws whole when the background changes. GL's errors are read at three points: after the frame's uploads (glyph slots and the arrow's texture), after the composition's draws and uploads of notices, and at the frame's end.

Present then turns into the display's buffer what that buffer lacks. That is the rectangles composed over the buffer's age. At age 0, or at an age past `PRESENT_HISTORY` frames, present draws the whole buffer, whether or not this frame composed anything. The CPU path likewise draws the whole picture into a buffer at age 0. When what the buffer lacks includes the whole frame, present draws one whole rectangle. A frame that composes nothing into a buffer that lacks nothing makes no call, and does not count for the ages that follow. Present takes its size from the buffer, and the pipeline refuses a buffer of another size than the display's. Present reads each pixel of the upright frame by integer coordinates, with no filtering (`texelFetch`). A frame that fails leaves the upright frame and the buffers unknown. So the next frame composes and presents whole (ADR-1303 D3, D9).

A turned display draws on the GPU path as on the CPU path (ADR-1303 D3, D6). The front end works in the display's own pixels on both paths. So the renderer's record of the cells, the border and the pointer is the same, whichever path drew the last frame. `pointer_at` gives the cursor plane's place on either path. The GPU path's back end maps each rectangle that the front end gives into the upright frame through `Turn`. Those rectangles are a cell, its glyph's place, a piece of the border, and the arrow's place. `Turn` maps a pixel as the CPU path turns it. For example, a display turned right draws the upright pixel `x`, `y` at `width - 1 - y`, `x`. So the grid sits in the upright frame where the CPU path puts it on the display, mapped back. An odd margin's spare pixel lands where the CPU path leaves it, once present turns the frame.

The arrow and the notices are drawn upright. The arrow is drawn at its turned place, mapped back. Each notice is drawn at its place in the text's view. A notice that ends hands the renderer its area, mapped onto the display. Present draws one quad for each tile and rectangle. The quad's destination is the display's rectangle. Its source is the tile's texels under the rectangle's upright top left, with the turn in its first parameter. Present's fragment shader turns each display pixel's offset within the quad into the upright offset by integer arithmetic. It reads that texel with `texelFetch`. The frame's damage is the rectangles that present drew, in the display's pixels.

Tests: `tests/gpu_turn.rs` and `src/gpu/turn.rs`. `tests/gpu_turn.rs` reads present's quads as present's shader does. It compares where the grid, the pointer, the border and a notice land with the CPU path's drawing of the same screen. It does so at every turn, with 5 pixels to spare on each axis.

A turn keeps the GPU path's glyphs, the arrow's texture and every program (ADR-1303 D3, D9). `Renderer::turned` gives the same renderer, turned. On the CPU path, that is a new renderer, whose cache holds turned glyphs. On the GPU path, it keeps the same identity. So the pipeline keeps the glyphs it holds for the renderer, with the budgets that their pools were made with. The decoration sprites that the GPU path uploads are always upright. `turned` first takes the font's lines, as `glyphs_changed` does. So lines that changed before a turn reach the GPU path's sprites. The renderer's next GPU frame turns the pipeline (`Pipeline::turn`), which composes the upright frame whole and presents the whole buffer.

A half turn makes no GL call beyond that frame's, and neither does any turn of a square display. A quarter turn of a display that is not square swaps the upright frame's sides. So it makes the frame's tiles anew at the new shape, in one batch of allocations that is checked before the old tiles are freed. It compiles nothing. When a turn's allocation or check fails, the turn frees what it made. It leaves the pipeline as it was, with its orientation and its frame's shape together. `Pipeline::turned` sets up a pipeline for a display that is already turned. Tests: `a_turn_keeps_every_texture_and_compiles_nothing`, `a_quarter_turn_of_a_display_that_is_not_square_makes_only_the_upright_frame_anew`, `a_turn_whose_check_fails_leaves_the_pipeline_as_it_was`, `a_turn_whose_allocation_fails_leaves_the_pipeline_as_it_was`, and `the_shaders_texel_is_the_one_the_tests_read`. That last test keeps the tests' Rust copy of present's `texel` in step with the shader's.

Slot 1 draws pictures on the GPU path, in a build with `images` (ADR-1303 D2). The renderer hands the pipeline every picture of the frame, in the frame's order, and where the frame draws each one. The composition draws a cell that pictures reach in ADR-1100 D4's layers, as the CPU path composes it:

1. the cell's background;
2. each picture over it, from slot 1's textures;
3. the cell's text layer, premultiplied over the pictures beneath it, before the first picture above the text;
4. the selection's tint;
5. vtium's mark.

The pictures beneath the last opaque picture over the whole cell are not drawn there. A block cursor's cell draws no picture, as on the CPU path.

Two steps fill slot 1. `Pipeline::set_picture_step` chooses between them, and vtium.conf never does. GPU scaling is the default. It uploads each picture's own pixels once, as the crop that its placement shows. The picture shader (`glsl/picture.frag`) then scales them with the CPU path's filter, in whole numbers. It uses the same 16-bit weights, summed in two 32-bit parts and rounded once. So its pixels are the CPU path's, byte for byte, on any GL with OpenGL ES 3.0's integers.

In two corner cases, the copy step takes the place of GPU scaling. ADR-1303 D2 says a picture is never drawn from the CPU copy for its size. The maintainer agreed to these exceptions on 2026-11-04, as extreme cases:

- A picture drawn wider or taller than 32,767 pixels, or from more than 65,535 source pixels across or down. That is past what the shader's 32-bit integers hold.
- A picture whose tiles cannot overlap by twice the filter's reach, because that overlap is at least the largest texture. For example, an image 10,000 pixels wide, shrunk into one 8-pixel cell, on a GPU whose largest texture is 2,048 pixels. 2,048 is the least that OpenGL ES 3.0 allows.

Only a picture drawn wider or taller than any display, or shrunk by a factor larger than about half the largest texture's side in pixels, meets either case. The copy step uploads the CPU path's scaled copy at the drawn size, read one texel a pixel.

A picture past the largest texture is held in tiles that overlap by twice the filter's reach. Each drawn pixel is drawn from the tile that owns the source pixel under the drawn pixel's centre. `Pipeline::limit_picture_textures` holds the tiles smaller for a test. The textures count against the display's bound, `PICTURE_CACHE_SCREENS` screens. They count through the cache that the CPU path's copies use (`src/picture/copies.rs`). That cache evicts as it does for the CPU path, and it hands back what it lets go, which slot 1 frees in GL. Where the bound has no room for a picture's own pixels, the picture is drawn from its CPU copy, kept while the bound has room for the copy. Where the copy has no room either, the part that the frame draws is scaled on the CPU into the display's one scratch texture, for that frame alone.

An opaque picture replaces what lies beneath it. Four draws need the pixels beneath them: a picture that is not opaque, a cell's text layer over the pictures beneath it, vtium's mark, and the selection's tint. No draw reads its own target. So each such draw first copies the pixels beneath its quads out of the tile into the copy. It then draws with the shader built with `OVER` (or the tint), which reads those pixels from the copy. The shader goes over them with the CPU path's arithmetic: `over_px` with `div255` for the picture, the text layer and the mark, and `mask` for the tint. Each shader writes every channel as `(c + 0.25) / 255`. So the GPU stores the CPU path's byte, whether the GPU rounds or truncates.

The GPU's own blending is not used there. Its conversion of the blended float to a byte belongs to the driver. On GitHub's runner, llvmpipe (Mesa 25.2.8) came one level below `over_px` at 72 of 195,072 sampled pixels. Mesa 26.2.6's llvmpipe matched every one.

The copy is one target for each display, kept from frame to frame. In a build without `images`, the arrow uses the copy too. A frame may draw over more pixels in one tile than the copy holds, in width or in height. That frame makes the copy anew, at the larger of each side. A frame that draws over fewer pixels makes nothing. The copy is given back once no draw has read it for `GPU_IDLE_FRAMES` frames, and at a release. `Pipeline::picture_counts` gives the textures' counts, as `Renderer::picture_counts` gives the counts of the CPU path's copies.

Tests: `tests/gpu_pictures.rs`. Among them is `draws_over_a_copy_read_their_own_tiles_pixels_across_the_seams`, where a picture that is not opaque, its text layers and the arrow cross the seams of six tiles. Another is `the_copy_of_the_pixels_beneath_grows_only_when_a_frame_needs_more`. Then `the_copy_of_the_pixels_beneath_lasts_until_no_frame_reads_it_for_the_idle_frames`. Last, tier 2's `pictures-*@gpu` and `pictures-*@gpu-copy` cases measure the GPU path's pictures as byte-identical to the CPU path's on llvmpipe, by GPU scaling and by the copy step.

The GPU path gives back textures that it no longer uses. It counts frames, and never wall time (the issue on GPU texture garbage collection). A glyph pool hands out its lowest free slot, so its glyphs gather in its first textures. The pool gives back its last texture once no glyph has held a slot there for `GPU_IDLE_FRAMES` frames. A picture's texture that no placement showed for as many frames is given back too. So is the copy of the pixels beneath, when no draw read it.

The pools and the sprites never hold more than `GLYPH_TEXTURE_CEILING` bytes of textures for a display. Past that ceiling, a pool takes no new texture. A glyph that the pool has no free slot for is drawn blank, until the budget's evictions free a slot. vtium's CPU path differs here, because it draws every glyph however full its cache is. The rule also differs from ADR-1303 D9's rule that vtium's own budgets change no pixel. The maintainer agreed to this on 2026-11-04, since only cells far larger than any font's cells reach the ceiling. `Pipeline::take_notes` gives the head a line for its log when a glyph is drawn blank, at most once in `GPU_IDLE_FRAMES` frames. It holds at most one such line until the head takes it.

`Pipeline::release` frees the glyph and picture textures, the arrow's texture, the scratch texture, the copy of the pixels beneath, and the upright frame. It keeps the programs and the context. It makes the next frame draw whole, after making the upright frame again at the display's size. The head calls `Pipeline::release` when its VT is released (ADR-1303 D9). Tests: `a_picture_texture_no_frame_shows_is_given_back_after_the_idle_frames`, `a_glyph_pools_emptied_texture_is_given_back_after_the_idle_frames`, `past_the_ceiling_a_pool_takes_no_texture_and_reuses_a_slot_given_back` and `a_release_frees_the_glyph_and_picture_textures_and_the_next_frame_draws_whole`.

The quads of every program reach the vertex shader as instances of a strip of four vertices. Each quad's destination is at attribute location 0, and its source and parameters are at 1 and 2. The corner comes from `gl_VertexID`. vtium-render draws only into textures and imported buffers, where GL's row 0 is the first row in memory. vtium-render calls that row the top, as a display buffer does. Tests: `tests/gpu_gl.rs`, `tests/gpu_pipeline.rs`, `tests/gpu_compose.rs` and the ledger's own tests in `src/gpu/ledger.rs`.

## Not yet

- **The GPU path draws no 1-bit display yet.** `Renderer::draw_gpu` refuses a 1-bit display. The depth reduction for a 1-bit display (ADR-1301) is not yet in present. The cell shader draws no color glyph as its silhouette, which a 1-bit display needs. A glyph slot past the GPU's largest texture fails the frame with a limit error. That is possible only for a cell past 2048 pixels, on a GPU at OpenGL ES 3.0's least largest texture. The GPU path's pixels are judged on a real GL in tier 2 once vtium-platform's GL lands.
- **The ink grid holds one ink color for each glyph that sends ink.** On a 1-bit display, the CPU path draws ink that lands in another cell as the opposite of that cell's background (ADR-1302 D8). So the color depends on the cell that receives the ink. When the GPU path draws 1-bit displays, the ink grid or the shader needs that color for each receiving cell.
- **A glyph cut because the boxes were full stays cut while it is cached.** Take a glyph whose ink passes its cells, drawn when the boxes' texture has reached `GLYPH_TEXTURE_SIDE` and has no free slot. That glyph is drawn cut at its cells, and cached that way. It takes a box only after the cache evicts it and the font draws it again.
- **The letter under a bar cursor keeps its ink above and below.** Under a bar cursor the letter is not drawn, as ADR-1302 D8 has it. But its ink in the rows above and below still is drawn, so that no neighbour changes with the cursor's blink (ADR-1306 D6).
- **An underline cursor in a cell a picture covers is not crossed by received ink.** There vtium draws the cursor's line above the pictures (ADR-1100 D4), and so above the cell's received ink. In every other cell, an accent from the row below crosses the line (ADR-1306 D6).
- **The reach byte is pinned in `src/damage.rs`.** ADR-1306 D4 names `tests/memory.rs`. The reach is crate-private, so the unit test `a_cells_history_keeps_its_reach_in_its_old_size` pins a cell's history at 16 bytes, the size it had before the reach.
- **Ink beside a right-to-left letter is ordered left to right.** ADR-1302 D7 orders the ink that a cell receives from the cells beside it by the cell's direction. The order is right to left when the cell's character's Bidi_Class is R or AL. vtium-term keeps its Bidi_Class table to itself, so the renderer orders every cell left to right. The order shows only where two neighbours' ink covers the same pixel of a right-to-left letter's cell.
- **`Frame::palette()` is owed.** The frame does not expose its palette, so the owner passes the default foreground and background in `DrawState`. The frame types that the renderers read are vtium/vtium#10's.
- **A full frame takes about 0.5 ms.** At 1366×768, a full redraw with a warm cache measures 0.49 ms, in a release build and in the minspec build alike, on one pinned core (`tests/figures.rs`). ADR-1300 measured 0.33 ms for its draw kernel alone, over the same screen in cached memory. That kernel, the 1-bit glyph loop and its writes, takes 0.18 ms of the release figure. The rest lies outside the kernel:
  - reading the frame a row at a time takes about 0.035 ms;
  - each cell's history takes 0.04 ms;
  - building the cells' jobs takes 0.12 ms;
  - the glyph cache's lookups take 0.04 ms;
  - setting up each cell's composition takes 0.05 ms;
  - collecting the damage takes 0.03 ms.

  The 0.035 ms is derived. Reading a cell at a time took 0.07 ms. Reading a row at a time dropped the whole frame by 0.037 ms. A 170×48 frame read alone, outside the renderer, took 0.053 ms a cell at a time and 0.027 ms a row at a time.

  The minspec build builds this crate at `opt-level = 3` (the operator's decision on vtium/vtium#78). At the profile's `opt-level = "z"`, a full redraw measured 0.93 ms, with the composition and the writes taking twice as long. The binary is 6,232 B larger for `opt-level = 3`.

  `tests/figures.rs` measures each figure twice: with unifont, and with unifont whose vowels carry an accent above their row (ADR-1306 D4). Unifont's ink stays in its cells, so with unifont the renderer reads no reach. A full redraw then costs what it did before glyph ink reached past its cells: 0.579 ms, the median of 15 runs on one pinned core, before and after. With the accents, about a third of the letters send ink into the row above. A full redraw in which every letter changes then measures 2.6 ms. Each cell that an accent lands in is composed through the layered path. It is composed a second time once the row below it is scanned. Reaching ADR-1300's figure, or the operator revising it, is vtium/vtium#78's work.

## Bounds

The frame comes from a program, the font from a file, and the cell size from the configuration. The font's `CELL_SIDE_MAX` bounds every glyph. `vtium-term`'s `CLUSTER_MAX` bounds a cluster. The cache's budgets bound the cache, and the row scratch's size bounds the row scratch.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `POOL_BYTES_PERFORMANCE` | 67108864 | Bytes of glyph pixels each of the four glyph pools holds in the performance profile; minspec's pools hold two screens of their format | Older glyphs are evicted; a glyph that would evict one the frame draws, or that alone passes it, is drawn uncached | ADR-1300 D3 and D4: it bounds the glyph cache's memory, and a full cache changes no pixel |
| `ROW_SCRATCH_MAX` | 1048576 | Bytes of the row scratch, where each run of a row's drawn cells is composed | The run is composed in bands of pixel lines that fit | ADR-1300 D5: one 16-pixel row across 1366 pixels is 87,424 bytes |
| `RENDER_TIMER_SAMPLES` | 1024 | Frames whose time the render timer keeps in an interval, for the median and the 95th percentile | Later frames count in the mean and the maximum only | It keeps the timer's memory fixed: 1,024 frames is about 17 seconds at 60 frames a second |
| `POINTER_SIDE_MAX` | 64 | Pixels on the longest side of the pointer's arrow, which is a cell tall | The arrow stays this tall over taller cells | 64 by 64 is the cursor size DRM reports for `DRM_CAP_CURSOR_WIDTH` and `DRM_CAP_CURSOR_HEIGHT` when a driver sets none, so the arrow fits a cursor plane |
| `PICTURE_CACHE_SCREENS` | 2 | Screens of the display, in 4-byte pixels, that its scaled and turned copies of pictures hold, in a build with `images` | The copy no placement of the frame shows that was placed longest ago is evicted; a copy that still does not fit is scaled for each frame it draws and not kept | ADR-1100 D5: it bounds the memory a display's copies take, whatever pictures a program sends |
| `PRESENT_HISTORY` | 3 | Frames whose composed rectangles the GPU path's present keeps, to draw what a buffer of that age lacks, in a build with `gpu` | A buffer older than the history is presented whole | `display-buffers`, at most 3 buffers a display (ADR-1401 D2); ADR-1303 D6 |
| `FAILED_GLYPHS` | 1024 | Glyphs the font drew nothing for that the glyph cache remembers, so the font is not asked for them again every frame | A glyph that lands on a remembered one's place takes it over, and the one it replaced is asked for again the next time it is drawn | It bounds the table's memory however many characters a screen shows that no font holds; the table keeps no expiry, since `Renderer::glyphs_changed` and `forget_glyphs` forget the glyphs whose characters arrive |
| `MISSING_CHARS` | 1024 | Characters the renderer keeps of the texts it drew as the font's missing glyph since the last arrival, so that the next arrival draws those cells again | The renderer keeps none, and the next arrival draws every cell again | The maintainer's choice: past 1024 different characters, one full redraw at the next arrival costs less than keeping track of each of them, and a console rarely shows that many different characters as the missing glyph. It bounds the list's memory at 4 KiB, where a screen with no arrivals would otherwise collect characters for as long as it runs |
| `GLYPH_TEXTURE_SIDE` | 2048 | Pixels on the longer side of each texture a GPU glyph pool lays its slots out in, at most the GPU's largest texture, in a build with `gpu` | The pool takes another texture | 2048 is the least `GL_MAX_TEXTURE_SIZE` OpenGL ES 3.0 allows, so every GPU takes the pools' textures; ADR-1300 D6 |
| `GLYPH_TEXTURE_CEILING` | 536870912 | Bytes of the textures one display's GPU glyph pools and decoration sprites hold together, in a build with `gpu` | A pool takes no new texture: a glyph it has no free slot for is drawn blank, and the pool's slots are reused as its budget evicts glyphs | It bounds the GPU memory a display's glyphs take at twice the 256 MiB the four pools' budgets hold together, which leaves room for the slots a frame holds past its budgets; the issue on GPU texture garbage collection; ADR-1303 D9 |
| `GPU_IDLE_FRAMES` | 600 | Frames a GPU texture may go unused before the GPU path gives it back: a glyph pool's last texture with no glyph in it, a picture's texture no placement showed, and the copy of the pixels beneath that no draw read, in a build with `gpu` | The texture is freed, and made again if a later frame needs it; frames are counted, never wall time, so a display that draws nothing frees nothing | 600 frames is ten seconds at 60 frames a second; ADR-1303 D9 |
<!-- end of bounds table -->
