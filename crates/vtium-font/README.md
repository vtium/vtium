# vtium-font

vtium-font is vtium's source of glyphs. It takes in font files, vitrified sets and built-in sets, and it puts out glyph bitmaps. The crate does these things:

- It reads and writes vitrified sets (ADR-1200 D2). A vitrified set is the glyph chunk format in which the glyph helper's glyphs reach the console.
- It loads PC Screen Fonts (PSF1 and PSF2) and raw console fonts with the `psf` feature. It loads gzipped fonts with the `gzip` feature, which brings `psf` with it. vtium-config enables `psf` and decodes gzip itself. The console enables neither feature, so the console parses no font file (ADR-1200 D6). The crate's own tests reach those fonts through a dev-dependency on the crate itself.
- It deflates the chunks it writes with the `deflate` feature. vtium-config enables that feature and the console does not, so the console carries an inflate and no deflate (ADR-1200 D19).
- It carries GNU Unifont and Unscii-8 built in as sets (the `unifont` and `unscii` features).
- It draws box drawing, block elements, Braille, sextants, octants and the rest of the block shapes of Symbols for Legacy Computing as sprites. A sprite is a shape that vtium-font computes to fill its cell.

A `Font` puts a face and the sprites together. For each sprite range, the `Font`'s `SpritePolicy` chooses between the face and the sprites. A `Font` draws any cluster exactly into its cells. The crate forbids unsafe code. Every glyph is a `Glyph`, which is a coverage mask or premultiplied RGBA. A glyph's bytes are the same on every host. The console draws no outline font through this crate's faces. Instead, vtium-config rasterizes outline and color fonts into vitrified sets, and this crate reads those sets (ADR-1200).

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, lints and public names. |
| `src/face.rs` | The `Face` trait that a glyph source implements, with the face's coverage of each sprite range. `Font`, which tries the sprites that its policy chooses, then the face, then U+25A1, then `?`, then the face's last resort. `Font` boxes each glyph to its cells. `Decorations`, which says where underline and strike go. |
| `src/policy.rs` | `SpritePolicy`, which gives a `Source` for each sprite range, with the default for a font that the operator names. `Coverage`, which says how much of each range a face holds. `Missing`, which says what a character that the face lacks draws as. |
| `src/glyph.rs` | `Glyph`, which is what every face and sprite draws. A glyph is a coverage mask or premultiplied RGBA, with the cells it spans. `CELL_SIDE_MAX` bounds its size. |
| `src/script.rs` | `Script` and `Variant`: superscript and subscript, SGR 73 and 74. The glyph's own box is drawn at two thirds of its size, and a 1-bit glyph stays 1-bit. The small glyph is placed against the top or the bottom of the text band (`Glyph::scripted`, ADR-1202). |
| `src/psf.rs` | `Psf`: the PSF1 and PSF2 loader, its unicode table, and the glyph drawing, behind the `psf` feature. The file carries the licence notice of the `kmscon` file that it was ported from. |
| `src/gzip.rs` | gzip (RFC 1952) framing and checks around `miniz_oxide`'s inflate, for gzipped PSF fonts. |
| `src/set/mod.rs` | The vitrified set's public face: `Set`, `SetBuilder` and `SetFace`. |
| `src/set/format.rs` | The set's layout: magic, version, section tags, and every record's fields and codes. `build.rs` includes it. |
| `src/set/write.rs` | The writer's core, which takes plain numbers in and puts reproducible bytes out. `build.rs` includes it. |
| `src/set/builder.rs` | `SetBuilder` and `StrikeBuilder`, which describe a set with vtium-font's types, for the tests and `vtium-config`. |
| `src/set/read.rs` | `Set`: the reader. The reader reads, checks and inflates each chunk the first time a glyph is drawn from it. It expands a 1-bit glyph as the glyph is drawn. |
| `src/set/read/parse.rs` | Each section's check against its bounds and against the other sections. |
| `src/set/builtin.rs` | The built-in sets, `unifont` and `unscii-8`, and the choice of one of them for a cell height and locale (ADR-1200 D7). |
| `src/set/face.rs` | `SetFace`: one cell size of a set, drawn as a `Face`, and the strike that a height picks (ADR-1200 D9). |
| `src/sha256.rs` | SHA-256 (FIPS 180-4), for the built-in sets' provenance and vtium-config's source pins. `build.rs` includes it. |
| `src/unifont.rs` | `Unifont`: the built-in unifont set, drawn at whole multiples of 8×16. |
| `src/hex.rs` | The missing glyph that `Missing::Hex` draws: the codepoint in 3×5 and 3×4 hex digits. |
| `src/sprite.rs` | The sprites, ported from `kmscon`, with the licence notices of `kmscon` and Ghostty. `Range`, the six ranges that the sprites fall in. |
| `src/sprite/legacy.rs` | The rest of the block shapes of Symbols for Legacy Computing as sprites, adapted from Ghostty's sprite font, with its notice. |
| `src/sprite/spans.rs` | Each sprite range's codepoints. A set's writer counts its coverage over them. `build.rs` includes it. |
| `src/bounds.rs` | Every bound, with its value, what happens past it, and its source. This file is the one source for the constants and for the table below. |
| `build.rs` | Packs `data/unifont.hex` and `data/unscii-8.hex` into the built-in sets, with the set writer's own code. |
| `data/` | The unifont and Unscii-8 glyphs and their licences. See its README. |
| `tests/` | The integration tests, fixtures and glyph digests. See its README. |
| `fuzz/` | The cargo-fuzz targets for the PSF loader and the set reader. They are a workspace of their own, so CI does not build them. See its README. |

`cargo test -p vtium-font` runs every test. The bounds table below is generated from `src/bounds.rs`. The test `readme_table_matches_bounds` fails when the table drifts from that file. `VTIUM_FONT_BLESS=1 cargo test -p vtium-font readme_table` rewrites the table.

## Behaviour

vtium draws the same pixels for the same font, size, style and character on every host. `tests/digests.rs` checks vtium's output against vtium's own digests of these:

- every sprite at 800 cell sizes;
- the box-drawing lines at 858 more sizes and thicknesses;
- every glyph of four PSF fixtures at three scales;
- every unifont glyph at two scales;
- every character of plane 0, drawn through `Font::glyph`.

`tests/sprite.rs` checks the sprites against a box model that is written apart from `src/sprite.rs`. It checks them at every cell size from 1×1 to 32×64.

vtium-font supersamples a sprite's curves and diagonals. It uses only IEEE 754 `f64` addition, subtraction, multiplication, division and comparison. Every host rounds those operations the same way, and Rust never fuses them into one operation. So a sprite is the same bytes on every host. The digests were taken on x86-64, and the tests check them on whatever host runs the tests.

### PSF fonts

The loader follows kbd's description of the format in Andries Brouwer's "The PSF font format". The loader settles the readings of that description below. Each reading has its test in `tests/psf.rs`.

- **A unicode table sequence does not end the glyph's entry.** A sequence starts with 0xFFFE in PSF1, and with 0xFE in PSF2. The entry runs to its terminator, 0xFFFF or 0xFF. vtium maps the entry's single codepoints and skips its sequences, as the Linux console does when it loads a table. So a sequence never moves the codepoints of a later glyph. Spec: "The PSF font format", `<unicodedescription> := <uc>*<seq>*<term>`, `<seq> := <ss><uc><uc>*`.
- **A PSF2 table is decoded as well-formed UTF-8.** A byte that is not part of a well-formed character maps nothing. 0xFE and 0xFF always act as marks. So a stray continuation byte, an overlong encoding, or a terminator after a lead byte never moves the codepoints of a later glyph. Spec: RFC 3629 and the Unicode Standard §3.9 (D92, Table 3-7). Well-formed UTF-8 never holds 0xFE or 0xFF.
- **A codepoint that the table lists for several glyphs maps to the last of them.** The Linux console maps it the same way, because the console overwrites as it loads a table (`con_insert_unipair` in drivers/tty/vt/consolemap.c).
- **Table entries past the font's last glyph are ignored**, however many there are. Spec: "The PSF font format", where the table holds one description per glyph.
- **The fallback `?` is looked up as a character.** A character that the font lacks draws U+25A1. If the font lacks U+25A1, the character draws `?`. If the font lacks `?` as well, the character draws glyph 0. vtium finds `?` through the font's table. In a font without a table, glyph 63 is `?`.
- **A raw (headerless) console font is read.** A raw font is a file with neither the PSF magic nor gzip's magic, whose size is 256 times a height of 1 to `RAW_HEIGHT_MAX` bytes. vtium reads such a file as 256 glyphs with no unicode table. Each glyph is 8 pixels across, with one byte a row. `setfont` and the kernel take the file the same way. vtium recognises a raw font by its size alone, as `setfont` does after its magic check. So a file of 256*N bytes and 8 KiB or less loads as a font, even when it is not one. kbd's 256-byte `README.Arabic` is such a file. vtium refuses a file of any other size as `PsfError::NotPsf`. Spec: setfont(8), "Raw font files are binary files of size 256*N bytes ... (0 < N <= 32)".
- **A gzip file with bytes after its last member is refused.** Spec: RFC 1952 §2.2, "A gzip file consists of a series of members".

### Sprites

- **Box drawing, block elements, Braille, the sextants, the octants and the rest of Symbols for Legacy Computing's block shapes are sprites** (vtium/vtium#27). vtium-font draws them to fill the cell at every cell size. The Legacy Computing shapes follow the Unicode 18.0 code charts:
  - U+1CEA0-U+1CEAF, the one quarter blocks: the Symbols for Legacy Computing Supplement chart, <https://www.unicode.org/charts/PDF/U1CC00.pdf>.
  - U+1FB3C-U+1FB92, U+1FB94-U+1FBAF, U+1FBBD-U+1FBBF and U+1FBCE-U+1FBEF: the Symbols for Legacy Computing chart, <https://www.unicode.org/charts/PDF/U1FB00.pdf>. These are the smooth mosaics, triangles, eighth, third and quarter blocks (U+1FB82-U+1FB85 and U+1FBE4-U+1FBE7 among them), shades, fills, light diagonals and circles. U+1FB93 is unassigned in Unicode, so it has no sprite (`src/sprite/spans.rs`, and `UNASSIGNED` in `tests/legacy.rs`).

  The block's pictographs stay with the face. They are these:

  - the pointer, check mark, running man, arrows, folders and squared dot (U+1FBB0-U+1FBBC);
  - the saltire, pointing index, question mark, stick figures, chevrons, cross mark and bracket (U+1FBC0-U+1FBCD);
  - the segmented digits and alarm bell (U+1FBF0-U+1FBFA).

  No cell-grid geometry defines the pictographs, so they draw from the face, and unifont carries them. `tests/legacy.rs` checks every Legacy Computing sprite against a model built from its Unicode name. It checks them at every cell size from 1×1 to 32×64 and in larger cells, and it also checks them against literal masks. The page digests of `tests/digests.rs` leave these characters out. A tier 2 screen of the ranges waits for the tier 2 goldens (vtium/vtium#17).
- **Each sprite range can be drawn from the font instead** (vtium/vtium#85). A `SpritePolicy` gives each `Range` a `Source`. The ranges are box drawing, block elements, Braille, sextants, octants and Legacy Computing. The sources are these:
  - `Sprites`: the sprite, whatever the face holds.
  - `Font`: the face's glyph, and the font's `Missing` glyph for a character the face lacks.
  - `Auto`: the face's glyph, and the sprite for a character the face lacks.

  `Font::coverage` counts the characters of each range that the face holds. The six ranges hold 874 characters in all. `Font::coverage` counts them through `Face::coverage`, which asks `Face::has_glyph` about each character. A face that reads a character map can override `Face::coverage`. `Coverage::partial` names the ranges that a face covers only partly, so that the operator can be warned. `Font::new` takes the face's own `Face::sprite_policy`, and `Font::with_policy` sets another policy. `tests/policy.rs` checks each source against stand-in faces that cover a range fully, partly and not at all.
- **A font the operator names draws its own glyphs for each sprite range it covers whole.** `SpritePolicy::default_for` gives a range `Font` when the face holds every character of the range. It gives the range `Sprites` when the face misses any character of it, so no range mixes the two. A range that the face covers only partly draws sprites, and vtium warns about it. The built-in unifont keeps `Sprites` for every range, although unifont covers all six. The sprites line up exactly at every cell size, and the goldens show them. Source: the operator's decision on vtium/vtium#85.
- **A sprite's light line follows the cell, not the font's underline.** `sprite::light_line` gives a cell `h` rows tall a light line `h / 16` pixels wide. The line is at least 1 pixel wide and at most a quarter of the cell. That is the thickness that a face with no line metrics gives its underline. Heavy lines are twice that width. Double lines are two light strokes, a light stroke apart. So a 20×38 cell draws 2-pixel light lines and 4-pixel heavy ones in every font. An outline font states its own underline in its `post` table. DejaVu Sans Mono's own underline is 1 pixel at a 38-row cell. The font's measured underline still sets where vtium draws an underline, and how thick. Source: the maintainer's decision on vtium/vtium#310, that line weight grows with the cell.
- **A cluster on a sprite character the face lacks draws the sprite.** Sometimes the policy draws a cluster's first character as a sprite, and the face lacks that character. Then the cluster draws the character's sprite and drops its marks. So a PSF font without box drawing draws U+2500 + U+0301 as the line, and not as U+25A1. A face that holds the character draws the cluster. Source: the operator's decision on vtium/vtium#85, which draws the sprites for a range that the font lacks.

### Superscript and subscript

`Font::drawn_variant` draws a glyph in a `Variant`, which is its style and its `Script` (vtium/vtium#37). SGR 73 asks for superscript, SGR 74 asks for subscript, and SGR 75 returns to the baseline. The codes come from mintty. mintty's wiki lists them in the table of text attributes on its Tips page (<https://github.com/mintty/mintty/wiki/Tips#text-attributes-and-rendering>). mintty's changelog adds them in 3.0.1 (<https://github.com/mintty/mintty/wiki/Changelog>). mintty leaves the glyph's size and place to the terminal, so vtium chooses them (ADR-1202):

- **The size.** A glyph's own box is the rows from its font's ascent to its descent. For a bitmap font, that is the whole height of its strike. The small glyph is two thirds of the own box's width and height, rounded to whole pixels. An 8×16 unifont glyph becomes 5×11.
- **The place.** The small glyph is centred across its cells, with an odd spare column on its right. A superscript's top lies on the top row of the text band, and a subscript's bottom lies on the bottom row of the text band. The text band is the cell less the rows that line spacing adds above and below it.
- **A smaller size of the same font comes first.** A bitmap face can be drawn at a whole multiple of its size, as on a high-DPI display. Such a face also draws its own strike at each smaller multiple (`Face::strike`, `Face::draw_multiple`). The small glyph is the face's own glyph at the largest smaller multiple whose height is from one half to three quarters of the normal glyph's height (`script::smaller`). vtium places that glyph whole against the text band, with each of the font's pixels a square block (`Glyph::placed`). So a face at 2x takes itself at 1x, and a face at 3x takes itself at 2x. Faces at 4x and 5x take themselves at 3x, and a face at 6x takes itself at 4x. Bold and italic come with the smaller strike. Bold widens by one of the font's pixels, and italic shears by whole rows of the font's pixels. A face drawn at its own size has no smaller multiple in the range. Its glyph is made small as the next item says.
- **A 1-bit glyph stays 1-bit.** A face says what kind of source its glyph comes from (`Face::origin`). Some glyphs come from a 1-bit source: a bitmap font, a set's 1-bit strike, the built-in fonts, and the glyph that shows a missing character's codepoint. For such a glyph, each pixel of the normal glyph goes to the small pixel that holds the pixel's centre. A small pixel is lit when any pixel that goes to it is lit. Every stroke stays whole and one pixel wide, and a 1-bit display shows the small glyph exactly. Take unifont's 94 printable ASCII characters and é, É, Å, ², α and β. Over those characters, this rule keeps the strokes and holes of all but 9 glyphs. In those 9, a gap one pixel high closes, as in the dot of i. A test checks that count and those 9 characters.
- **Any other glyph is averaged.** This covers a color glyph, and a gray glyph from the glyph helper. Each small pixel is the average of the pixels it covers, weighted by how much of each it covers. A color glyph stays premultiplied.
- **Sprites are drawn small.** A character that the policy draws as a sprite is drawn to fill a smaller box. The box is two thirds of the text band's height and two thirds of the cells' width. The sprite uses the light line of that height, so its edges stay on whole pixels.
- **Bold and italic are added to the small glyph.** Some faces draw bold by widening a bitmap glyph and italic by shearing it. Such a face draws the glyph upright, and the small glyph takes the style. Bold widens each row by a pixel. Italic shears the small glyph by whole pixels about its own baseline. The shear follows ADR-1302 D4's rule for a font of the small glyph's height.
- **Ink stays in its row.** Ink above and below the row is cut. Ink beside the cells, such as an italic glyph's lean, stays in the glyph's ink box, within one cell of its cells.
- **The face's own glyph.** A script glyph comes from the face that draws the cell's normal glyph. vtium never draws it from another font, so a font set's bitmap font is never replaced by unifont.
- **The lines follow the small glyph.** A superscript or subscript cell has an underline, a strike-through and an overline. These are the lines of the small glyph's own size, placed where its own box lies (`Font::script_decorations`, ADR-1202 D8). A face makes one set of lines for superscript and one for subscript. The face's small glyphs may come from a smaller multiple of its strike. Then the lines are that strike's own lines at that multiple, moved down with the strike. Otherwise, each row of a normal line goes to the small row that holds the row's centre, as the glyph's rows go. A line keeps at least one row. A set whose strike for that script reports its own lines gives those lines, as the glyph helper's sets do. The overline lies on the top row of the small glyph's own box, as thick as the underline. In unifont at 16 rows, a superscript's underline lies on row 10, under the raised glyph. A subscript's strike-through also lies on row 10, across the lowered glyph. `src/script/lines.rs` holds the rule. `tests/script_lines.rs` checks unifont's sets at 1x, 2x and 3x against ADR-1202's table.
- `Font::glyph_unscaled_variant` hands over a glyph from a smaller multiple j at the face's own size, with scale j. It does the same for a normal bitmap glyph at its scale. This works because the glyph's cells, the line spacing and the glyph's place in the cells are each whole blocks of j. `Font::glyph_unscaled_variant` hands every other small glyph over at its drawn size, since two thirds is no whole number of a bitmap glyph's pixels.

`src/script.rs` holds the size, the place and the 1-bit scale. The glyph helper uses the same functions (`Glyph::scripted`, `script::size`, `script::place`). The tests in `src/script.rs` and in `tests/script.rs` check them against literal rows and columns.

### A character the face lacks

A character that the face lacks draws as the font's `Missing` glyph (vtium/vtium#85). `Missing::Replacement` is the default, and the configuration names it `missing = box`. It draws U+25A1, `?` or the face's last resort. `Missing::Hex`, which the configuration names `missing = hex`, draws the codepoint in hex digits. Its layout and digit shapes are the ones the operator approved on vtium/vtium#85. Every pair of digits differs by at least two pixels, so a program can read the codepoint back.

- A narrow cell is the text color with the digits cut out. It holds two rows of two 3×5 digits up to U+FFFF, and three rows of two 3×4 digits past it. The digits b, c and d are in lower case.
- A wide cell is a box outline with 3×5 digits inside, in two rows of two digits or of three.

The design is 8×16 for a narrow cell and 16×16 for a wide one. vtium scales the design by the largest whole factor that the cell holds, and centres it. A narrow cell's margin is in the text color. A cell smaller than the design draws U+25A1. `Font::glyph_unscaled` hands the hex glyph over at its drawn size, as it does a sprite. `tests/missing.rs` checks the hex glyph against literal masks.

`Font::drawn` says whether a glyph is the font's missing glyph (`Drawn::missing`). A glyph is the missing glyph when the face lacks the cluster's first character. It is also the missing glyph when the face says it has that character but draws nothing for the cluster. A set that holds a character only as the first of a cluster does this. A cache records the flag with each glyph, so that the cache can drop the missing glyph's drawings when the face gains glyphs (vtium/vtium#320). `tests/missing.rs` checks both cases.

## Vitrified sets

A vitrified set (ADR-1200 D2) is one buffer, little-endian throughout. It is never written to a file. The buffer holds the `VTGLASS\0` magic, a major and minor version, and a section table. These sections follow:

- `META`: the provenance, as `key=value` lines.
- `STRK`: each strike's style, pixel format, script, cell, baseline, underline, strike and overline lines, and glyph count. The strikes are sorted by cell, script, style and pixel format. So a cell holds a 1-bit, an R8 and an RGBA strike of each style in each script.
- `CMAP`: each strike's runs of codepoints. The strikes of one style in a cell map codepoints that do not overlap.
- `POLY`: the sprite policy, and each strike's coverage of each sprite range.
- `GLYF`: an index of chunks, then the chunks. Each entry of the index holds the chunk's offset, its stored and decoded lengths, and a flag that says whether the chunk is raw or deflated.
- `CLUS`, when the set maps a cluster.
- `INKX`, when a glyph's ink passes its cells.

`src/set/format.rs` lays out every field.

This build writes format 2.3, and it reads formats 2.0 to 2.3. The glyph helper and the console change together, so the format carries no hash. vtium refuses a set of another major version with `SetError::UnknownMajor`. The reader skips the sections and record fields of a later minor version.

Format 2.1 added the ink box's top and height to each `INKX` record (ADR-1306 D3).

Format 2.2 gives each strike a script, in the 16 bits after its pixel format. Formats 2.0 and 2.1 reserved those 16 bits as 0 (ADR-1202 D11). A strike's script is 0 for normal glyphs, 1 for superscript glyphs and 2 for subscript glyphs. The glyph helper makes superscript and subscript glyphs small, and places them in the whole cell. The reader draws a glyph from a superscript or subscript strike only for a cell in that script (`SetFace::draw_variant`), never for a normal cell. So such a strike adds no coverage. This build reads the strikes of a 2.0 or 2.1 set as normal ones, since the field is 0 there. A 2.0 or 2.1 reader refuses a strike whose reserved field is not 0. So such a reader reads a 2.2 set of normal strikes as its own. It refuses whole a 2.2 set that holds a script strike, instead of drawing a small glyph for a normal cell (`tests/script_set.rs`).

Format 2.3 gives each strike its overline's row, in the 16 bits after its strike-through's thickness. Formats 2.0 to 2.2 reserved those 16 bits as 0 (ADR-1202 D8). A normal strike's overline lies on row 0. A superscript or subscript strike's overline lies on the top row of its small glyph's own box. This build reads an older set's overlines on row 0, where every overline lay before 2.3. An older reader refuses a strike whose reserved field is not 0. So an older reader reads a 2.3 set whose overlines all lie on row 0 as its own. It refuses whole a 2.3 set that holds a script strike with its overline lower down.

- **Writing.** `SetBuilder` takes a name, provenance lines, a sprite policy and `StrikeBuilder`s. Each `StrikeBuilder` is one style at one cell size in one pixel format (1-bit, R8 or premultiplied RGBA), with its glyphs and the codepoints that draw them. The bytes depend only on these inputs:
  - The writer sorts the strikes and the runs.
  - It numbers the glyphs in the order in which the sorted map first reaches them.
  - It drops a glyph that no codepoint maps.
  - It deflates the chunks at one fixed level.

  So the same inputs give the same bytes on every host built with the same features. The console's writer stores raw a `compress=yes` strike that vtium-config's writer deflates. `tests/set.rs` pins a set's SHA-256.

  A chunk holds at most `SET_CHUNK_BYTES`, and a chunk never splits a glyph. A strike's glyphs start a new chunk. A glyph can be larger than `SET_CHUNK_BYTES`. For example, each glyph of an 8×16 bitmap font drawn at 33 times its size is 17,424 bytes. Such a glyph goes in a chunk of its own, which holds exactly the glyph's bytes and is never deflated. This holds in `GLYF` and in `CLUS` alike (ADR-1302 D6). A set with no such glyph is written byte for byte as before. A test pins this by the SHA-256 of the built-in sets and of sets that the test writes.

  By default the writer deflates a strike's chunks for R8 and RGBA, and stores them raw for 1-bit. That is what `compress=yes` asks for in a frit, which is a font set file (ADR-1200 D13). `StrikeBuilder::compress(false)` stores the chunks raw. `build.rs` deflates the built-in sets in every build.
- **Reading.** `Set::from_bytes` checks a whole set in memory, as the console checks each of the glyph helper's frames. `Set::check_chunks` decodes every chunk without keeping it. Every count and size is bounded. The reader checks every table against the others before a glyph is drawn. It checks that the runs are sorted and do not overlap. It checks the glyph counts, `POLY`'s coverage against `CMAP`, and the chunks against each strike's glyph bytes.

  A chunk of `GLYF` or `CLUS` can be larger than `SET_CHUNK_BYTES`. The reader takes such a chunk only when it is raw and holds exactly one glyph, of exactly that glyph's bytes. So its decoded bytes are bytes the set holds, and the strike's cell bounds its size. Any other chunk past the bound makes the reader refuse the set. An `INKX` chunk is always held to `SET_CHUNK_BYTES`.

  The map takes 36 bytes a run, for the run and the glyph span that the run may bring in. At `SET_RUNS_MAX` the map takes at most 9 MiB. `CMAP`'s bytes take at most 16,777,352 while the reader checks them. The reader inflates a chunk to exactly its decoded length the first time a glyph is drawn from the chunk. The reader then keeps the chunk as stored. A 1-bit glyph is expanded to a mask of 0 and 255 as it is drawn (ADR-1200 D13), as `Psf` expands its glyphs. So a 1-bit chunk costs at most its own 16 KiB, or the bytes of the one larger glyph that it holds. A chunk that fails draws its glyphs as missing ones, and `Set::fault` names the chunk.
- **Clusters.** `CLUS` maps clusters to glyphs that are drawn for them whole (vtium/vtium#144). A cluster is a short sequence of scalar values, such as an emoji ZWJ sequence, a flag, or a letter with its marks. Each cluster belongs to a strike. The cluster's glyph takes that strike's cell and pixel format, and is one or two cells wide. A cluster's key is 1 to `CLUSTER_SCALARS_MAX` scalar values. A strike's clusters are sorted by key, and no key appears twice.

  The section holds its records, the keys end to end, a chunk table like `GLYF`'s, and its own chunks. So a reader that does not know the tag skips the section whole, and the section needed no new version. `tests/clusters.rs` reads a set with the section renamed to a tag that the reader does not know. `StrikeBuilder::cluster` maps a cluster to one of the strike's glyphs, and that glyph may also draw codepoints. A set with no cluster is written without `CLUS`, byte for byte as before.

  The reader checks every record against its strike and its chunk before a glyph is drawn. It holds 20 bytes for each cluster and its key, at most 5.25 MiB at `SET_CLUSTERS_MAX`. `SetFace::cluster` looks a cluster up by its exact scalar values, with a binary search in each strike's clusters. It chooses a strike the same way as for a codepoint. A cluster that it does not map is `None`, and then the console's lookup chain decides what draws. `Set::clusters` lists the clusters that each strike maps, and the console checks that list against what it asked for.
- **Ink boxes.** `INKX` holds the ink box of each glyph whose ink passes its cells (ADR-1200 D2, ADR-1302 D6, ADR-1306 D3). Italic, symbol and accented glyphs often have such ink. A box is a rectangle of pixels in the strike's pixel format:
  - Its columns run from its left, across its width. Its left is the offset of its first column from the left edge of the glyph's first cell, and it is 0 or negative.
  - Its rows run from its top, down its height. Its top is the offset of its first row from the top of the glyph's row, and it is 0 or negative.

  The box spans the glyph's cells and the ink past them. It reaches at most `REACH_CELLS` cells past the cells on each side, and `REACH_ROWS` rows above and below them. That limit is the glyph's reach (ADR-1302 D1, ADR-1306 D1). The two constants, one cell and one row, live in `src/reach.rs`, and every check of a box reads them.

  Each record names a strike. It also names either a glyph of `GLYF` by its number, or a cluster of `CLUS` by its record's number. So a glyph that draws codepoints and clusters has a record for each. The box's bytes are stored once, in the section's own chunks, with its rows one after another. A record's offset names the box's row at the top of the glyph's row.

  A glyph keeps its pixels cut to its cells in `GLYF` or `CLUS`, as before. So a reader that does not know `INKX` skips the section and draws every glyph cut to its cells. A set with no box is written without `INKX`.

  Format 2.0's records had no top and height. The reader reads such a record as a box of the cell's height. A reader of format 2.0 reads the first six fields of a 2.1 record. Those fields give the box's rows within the glyph's own row. For a box that passes its cells to the side, a 2.0 reader draws the glyph's ink cut at its row (`tests/ink.rs`). A box that passes its cells only above or below looks to a 2.0 reader like a box of the cells alone. A 2.0 reader refuses such a box, so it refuses the whole set. The console and the glyph helper always come from one build (ADR-1200 D15). So no 2.0 reader meets a 2.1 set there, and a set that the console refuses draws in the built-in font.

  Only R8 and 1-bit strikes hold boxes, since a color glyph fits its cells (ADR-1302 D5). `StrikeBuilder::ink` gives a glyph its box. The writer refuses four kinds of box. They are a box in a color strike, a box past the reach, a box that does not pass the glyph's cells, and a box larger than `SET_CHUNK_BYTES`. The reader refuses each of these before a glyph is drawn. So the console's reader of the glyph helper's answers holds every box to its reach (ADR-1200 D15). A glyph whose box would be larger than a chunk carries no box, and is drawn cut to its cells.

  `SetFace` draws a glyph's box beside its pixels, at the same whole factor, with bold widened inside the box, as `Glyph::ink`. A `Glyph` that `Font` boxes to another width loses its box, because the box was placed for the glyph's own cells. The reader holds 32 bytes for each box, at most 8 MiB at `SET_INKS_MAX`.
- **Drawing.** `SetFace` draws one cell size of a set. For the height asked for, it picks one strike (ADR-1200 D9):
  - the tallest strike that reaches that height at a whole multiple;
  - if there is none, the tallest strike under that height, scaled by the largest whole factor that fits;
  - if there is none, the smallest strike, at its own size.

  A style draws from its own strikes, from whichever of them maps the codepoint, in that strike's pixel format. A 1-bit glyph is expanded to an 8-bit mask. An R8 glyph is drawn as its mask. An RGBA glyph is drawn as premultiplied color. Each glyph is one or two cells wide.

  A codepoint that the style lacks, or a style with no strike, draws from the regular strikes. For bold, the mask is widened as vtium-font's bitmap faces widen it: a pixel is set when the pixel or its left neighbour in the same cell is set. For italic, a 1-bit glyph is sheared as vtium-font's bitmap faces shear it. A color glyph is drawn as it is.

The built-in sets (ADR-1200 D7) are `unifont` and `unscii-8`. `unifont` is an 8×16 strike, compiled in with the `unifont` feature. `unscii-8` is an 8×8 strike, compiled in with the `unscii` feature, and it is 15,486 B as a set. `set::builtin` gives either set by name. `set::builtin_name` gives the one that ADR-1200 D7 chooses: `unscii-8` for cells under 16 pixels high, and `unifont` from 16. It also gives `unifont` in a zh, ja or ko locale, since Unscii-8 holds no CJK. Both sets draw the sprites for every sprite range.

The format leaves these choices to this crate, and the crate settles each one here:

- A deflated chunk is stored deflated only when that makes it smaller. Otherwise it is stored raw, with its flag clear.
- A multi-line `META` value, such as the frit, is one line of its key for each of its lines. The builder writes `name` and `format` first.
- `POLY` holds each strike's coverage, and the reader refuses a `POLY` whose coverage is not its strike's map's.
- A strike drawn at a whole factor keeps the lines that the bitmap faces draw. Every bitmap source gives a strike whose decorations are `Decorations::for_height` of its height. Such a strike takes `for_height` of the scaled height. A strike with lines of its own scales them with its glyphs.
- A set face's last resort is drawn when the set holds neither U+25A1 nor `?`. The last resort is glyph 0 of the first regular strike at its cell that holds a glyph. That is the glyph of the lowest codepoint that the strike maps. A cell's coverage of each sprite range is the sum of its regular strikes' counts.

## Glyph storage

**What a draw returns.** Every draw allocates a new `Glyph`. A `Glyph` holds its width, its height, the cells it spans (1 or 2), and its pixels row by row, in one of two formats.

| Format | Bytes a pixel | Holds | A narrow 8x16 cell | Two cells at `CELL_SIDE_MAX` (4096x2048) |
|---|---|---|---|---|
| `Format::Mask` | 1 | Coverage, 0 to 255; the renderer blends it in the cell's colors | 128 B | 8 MiB |
| `Format::Rgba` | 4 | Red, green, blue and alpha, the colors premultiplied by alpha | 512 B | 32 MiB |

PSF, unifont and the sprites draw only masks. RGBA is for color faces (vtium/vtium#23, #24).

**What each source keeps.**

| Source | Kept in memory | Cost at minspec |
|---|---|---|
| `Psf` | The glyphs' 1-bit rows as one slice, count x height x ceil(width / 8) bytes. When the font has a unicode table, a codepoint table sorted for binary search, at 8 bytes an entry. The parser drops the file's other bytes after parsing. | kbd's `default8x16`: 4 KiB of glyphs and 282 entries (2.2 KiB). Terminus 16x32: 32 KiB and 627 entries (5 KiB). The largest font the bounds admit keeps up to 16 MiB of glyphs and 8.5 MiB of table. Parsing it holds the file's bytes as well, up to 16 MiB more. The console parses no font file (ADR-1200 D6), so this cost falls on vtium-config's helper. |
| `Unifont` | Compiled into the binary, read-only. The built-in set is 1,344,063 B. It holds the 3.7 MB of 1-bit glyphs, deflated in 227 chunks of at most 16 KiB (1.28 MB). It also holds its map of 3465 runs at 16 bytes each (54 KiB) and its chunk index (4.4 KiB). In memory, the set keeps its runs, glyph spans and chunk index, read once (about 145 KiB). It also keeps each chunk that a glyph has been drawn from. Such a chunk is inflated once on its first draw, kept 1-bit for the life of the process, and shared by every `Unifont`. A chunk is never dropped and never inflated twice. | ASCII and Latin-1 are chunk 0, and U+25A1, drawn for a missing character, is chunk 14. So plain text holds 32,752 B. Text with Latin Extended, Greek and Cyrillic adds chunk 1. Punctuation, arrows and maths add chunks 12 and 13. The CJK ideographs U+4E00-U+9FFF span 42 chunks (688,128 B). All 227 chunks hold 3,718,608 B (3.55 MiB), inside the minspec build's memory peak of 16 MiB. |
| `SetFace` | The set's runs, glyph spans and chunk index. Each chunk that a glyph is drawn from, decoded and kept as stored, at most `SET_CHUNK_BYTES` each, or one glyph's bytes for a glyph larger than that. | |
| Sprites | Nothing: each is computed at the size asked for. | |

Scale does not change what a source keeps, only what a draw returns. For example, a font scaled to a cell of 2048 still keeps its 1-bit rows, and each draw returns a glyph of up to 8 MiB as a mask.

**The drawn-glyph cache is vtium-render's** (vtium/vtium#16), and this crate does not decide it. A draw costs an allocation, and a bit-to-byte expansion for a 1-bit glyph. For a set, a draw also costs an inflate, but only the first time a chunk is used. So the cache saves the allocation, and it never repeats an inflate.

## Line spacing

`spacing` holds what the console and the glyph helper share for added line spacing (ADR-1302 D13).

`Band::of` gives the rows that a percentage of the text's height adds. It rounds to the nearest pixel, with a half rounding up. It splits the rows evenly, with the odd pixel above. It cuts the rows so that the cell stays inside `CELL_SIDE_MAX`.

`Fit::default_for` gives a character's fit when the set in the character's frit names none:

- Box drawing, block elements, Braille, Powerline (U+E0A0 to U+E0D7) and the two Legacy Computing blocks are `fill`.
- Hebrew, Arabic, Syriac, Thaana, N'Ko and any combining mark are `room`.
- The rest is `natural`.

`place` puts a glyph drawn at the text band's height into the taller cell. A `fill` mask stretches. A bitmap's mask stretches by repeating its top and bottom rows, and an outline's mask stretches by scaling vertically. Everything else gets blank rows above and below, every color glyph included.

A glyph's ink box goes with the glyph. A stretched glyph's box is cut to the text band's rows and stretched with the glyph. A padded glyph may have ink above or below the text band. That ink now lies in the rows that the band adds, and so in the glyph's own cell, so the ink joins the glyph's own pixels. The box keeps only what still passes the taller cell (ADR-1306 D8). A face reports the rows that its cell adds through `Face::band`, which reports none by default.

## Notes for vtium-render

- Ask `Font::glyph` for every cell's glyph. It draws the sprites, the face and the fallbacks. It boxes the result to exactly the cells asked for, and marks the result with that cell count. Check the glyph's `format()`: blend a mask in the cell's colors, and composite RGBA as it is. `Font::glyph` returns `None` only for an empty cluster, or for a face that can draw nothing at all.
- A glyph narrower than its cells is centred in them, with an odd spare pixel on its right. A glyph wider than its cells is squeezed sideways into them, so no ink is cut (ADR-1302 D14). For example, GNU Unifont draws Devanagari letters 16 pixels wide, and the terminal gives each of them one 8-pixel cell. Where the glyph is a whole number of times as wide as its cells, each column of a mask takes the strongest pixel of the columns it covers, so a 1-bit glyph stays 1-bit. A face drawn at a whole scale merges its font's own pixels, so `Font::glyph_unscaled` still hands over the same glyph at scale 1. Where the ratio is not a whole number, and for a color glyph, each pixel takes the coverage of the part of the row it covers.
- A sprite's light line is `sprite::light_line` of the cell's height, a sixteenth of it. So box lines grow with the cell, whatever the font. See [Sprites](#sprites), above.
- Bold and italic are drawn into the glyph. A bitmap face widens bold by a pixel. It shears italic by 0.2 about the font's baseline, one pixel row at a time, at the font's own size before it is scaled (`italic`, ADR-1302 D4). The rows move right in bands of five, each band one pixel further than the band below it.
  - A font 16 pixels tall or taller at its own size keeps still the three rows above the baseline and every row below it. So a letter's last two rows of ink and the row above them move together, and no row moves left.
  - A font under 16 pixels tall moves each row by 0.2 times the height of the row's middle above the baseline, rounded to the nearest pixel. So its rows more than two below the baseline move 1 pixel left.
  - In an 8×16 console font whose baseline lies under row 11, rows 0 to 3 move 2 pixels and rows 4 to 8 move 1 pixel. Rows 9 to 15 stay.

  A sheared glyph's pixels are its ink cut to its cells. The ink that passes the cells goes to the glyph's ink box (`Glyph::ink`, ADR-1302 D6). A renderer that draws no box leaves that ink out. For bold italic, the widening comes first, and the shear moves the widened rows. A set with no bold-italic strike draws bold italic from its regular strike, widened and sheared, never from its bold strike (the maintainer, 2026-11-04).

  The 1-bit rows of a sheared glyph's box can be larger than a set's chunk, 16 KiB. Such a glyph carries no box and is cut to its cells, as the glyph helper leaves such a box out of a set. A PSF font states no baseline. So `Psf::baseline` takes the row under the lowest ink of the font's `H`, or the glyph's bottom edge for a font with no `H`. A set's 1-bit strike is sheared about the baseline that the strike states: row 14 for the built-in unifont and row 7 for Unscii-8. An R8 or RGBA strike drawn for italic stays upright. An outline's slant is the glyph helper's work, drawn before the glyph is rasterised, and color glyphs never lean. Overhang is vtium/vtium#26.
- A glyph's bytes depend only on the font, the size, the style and the character, so a cache may key on those four. A `Font`'s policy is fixed when the `Font` is built, so a new policy needs a new `Font`.
- The configuration (vtium/vtium#21) builds a font's policy from its keys. When one value is set for all six ranges, the configuration uses `SpritePolicy::all`, and otherwise the face's default. Then it applies each range's own key with `SpritePolicy::with`. It warns for each range that `Font::coverage().partial()` names.
- `Font::glyph_unscaled` hands a bitmap face's glyph over at scale 1, together with its scale. This suits a cache that stores the glyph small and scales it while drawing (ADR-1300 D2). Drawn with each pixel a square `scale` across, the glyph is exactly `Font::glyph`'s glyph. A sprite comes at its drawn size. So does a glyph that its box to its cells centres in an odd number of spare pixels, because such a glyph does not scale.

## Licences

The Unscii-8 glyphs in `data/unscii-8.hex` are Unscii 2.1 by Viznut, in the public domain (`data/unscii.txt`). The unifont glyphs in `data/unifont.hex` are GNU Unifont 17.0.03. They are dual-licensed under the SIL Open Font License 1.1 and the GNU GPL 2+ with the font embedding exception. vtium takes them under the OFL, whose text is `data/OFL-1.1.txt`. `tests/fixtures/README.md` lists the test fixtures and their licences. `miniz_oxide` (MIT, Zlib or Apache-2.0) inflates a set's chunks in every build, the built-in sets' chunks among them. It also inflates gzip fonts. It deflates a set's chunks when a set is written with the `deflate` feature.

## Bounds

A font file and a set are untrusted input. Their sizes are bounded. vtium-font refuses a malformed one with a `PsfError` or a `SetError`, never with a panic. Each refusal case has its own test in `tests/psf.rs` and `tests/set.rs`. The fuzz targets feed the loader and the set reader any bytes.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Source |
|---|---|---|---|---|
| `PSF_GLYPHS_MAX` | 65535 | Glyphs a PSF font holds | The font is refused | It bounds the glyphs a crafted header can make vtium read, and keeps every glyph index inside the unicode table's 16-bit field. Indices run from 0 to 0xfffe, so 0xffff is never a glyph |
| `PSF_WIDTH_MAX` | 32 | A PSF glyph's width in pixels | The font is refused | A glyph's row is read into 32 bits |
| `PSF_HEIGHT_MAX` | 256 | A PSF glyph's height in pixels | The font is refused | PSF1 holds the height in a byte, and PSF2 is held to the same bound |
| `RAW_HEIGHT_MAX` | 32 | A raw font glyph's height in pixels: a raw font is 256 glyphs of N bytes, one byte a row, 8 pixels across | The file is refused as a font | setfont(8): "Raw font files are binary files of size 256*N bytes ... (0 < N <= 32)" |
| `PSF_BYTES_MAX` | 16777216 | Bytes a PSF file holds: the whole file, decompressed when gzipped | The font is refused, whatever the bytes past the glyphs hold | It bounds the bytes a parse holds in memory. The whole file counts, so a PSF1 font without a table cannot carry any number of bytes after its glyphs |
| `PSF_ENTRIES_MAX` | 1114112 | Codepoints a PSF unicode table maps, before duplicates are merged | The font is refused | One entry for each Unicode codepoint |
| `GZIP_BYTES_MAX` | 1048576 | Bytes a gzipped PSF file holds before it is decompressed. kbd's largest gzipped console font is 6 KiB | The font is refused before anything is decompressed | vtium's own: the inflater builds a Huffman table for each deflate block, about a microsecond, and a block can be 10 bits, so a 15 MiB file of empty blocks took 14 s to refuse. At 1 MiB the worst file takes about 1 s on the test machine |
| `CELL_SIDE_MAX` | 2048 | A cell's width or height in pixels. A glyph is at most two cells wide | A font scales by the largest whole factor that keeps its cell inside; a sprite or glyph asked for past it is not drawn | vtium's own: it bounds a drawn glyph at 32 MiB, two RGBA cells of 2048 by 2048. A cell of 2048 is taller than a 7680x2160 display's one row |
| `SET_BYTES_MAX` | 1073741824 | Bytes a vitrified set holds | The set is refused | vtium's own, from ADR-1200's measured sets: Noto Sans Mono CJK unlimited at 48 pixels is about 100 MB a strike (D1), a full GBK set at 48 pixels 49.8 MB (D13), and a set holds at most three sizes (D11) |
| `SET_SECTIONS_MAX` | 16 | Sections a set's header lists | The set is refused | vtium's own: format 2.3 writes at most seven (ADR-1200 D2), and a minor version adds sections a reader skips |
| `SET_META_BYTES_MAX` | 1048576 | Bytes of a set's `META` provenance | The set is refused | vtium's own: the frit verbatim and a line or three for each source (ADR-1200 D2) |
| `SET_STRIKES_MAX` | 32 | Strikes a set holds | The set is refused | vtium's own: a strike for each of four styles at up to three sizes (ADR-1200 D1, D11), and color strikes beside them |
| `SET_GLYPHS_MAX` | 1114112 | Glyphs one strike holds | The set is refused | One for each Unicode codepoint: a set maps single codepoints only (ADR-1200 D2, `CMAP`) |
| `SET_RUNS_MAX` | 262144 | Runs of codepoints a set's `CMAP` holds, over every strike | The set is refused | vtium's own: the built-in unifont is 3,465 runs, and Noto Sans Mono CJK unlimited, about 44,000 glyphs (ADR-1200 D1), at most 44,000 a strike. A run and its glyph span take 36 bytes in memory, so the map holds at most 9 MiB, and its `CMAP` at most 16,777,352 bytes while it is read |
| `SET_CHUNKS_MAX` | 262144 | Glyph chunks a set's `GLYF` index lists | The set is refused | vtium's own: `SET_BYTES_MAX` of raw chunks is 65,536 of them, and deflated ones are 1.5-4.7 times smaller (ADR-1200 D13) |
| `SET_CHUNK_BYTES` | 16384 | Bytes a glyph chunk holds, as stored and decoded. No glyph is split across two. A glyph larger than this, in `GLYF` or `CLUS`, takes a raw chunk of its own, exactly its bytes, so its cell bounds it instead | The set is refused; the writer starts a new chunk | ADR-1200 D2: "chunks of at most 16 KiB, no glyph split across two", the built-in unifont's chunk size. ADR-1302 D6: a glyph whose own cells pass the bound is carried, not left out (the maintainer, 2026-11-01) |
| `SET_RECORD_BYTES_MAX` | 64 | Bytes a record of a set's tables declares | The set is refused | vtium's own: format 2.3's largest record is 48 bytes, and a minor version adds fields at a record's end (ADR-1200 D2); it bounds what a table's counts can make the reader hold |
| `SET_CLUSTERS_MAX` | 65536 | Clusters a set's `CLUS` maps, over every strike | The set is refused | vtium's own: Unicode 18.0's RGI emoji sequences other than single emoji are 2,563, or 3,536 with each keyed again without its U+FE0F; canonical decompositions 1,349 keys; a screen of 320 by 100 cells shows at most 32,000 clusters. A cluster takes 20 bytes in memory and its key at most 64, so the map holds at most 5.25 MiB |
| `SET_INKS_MAX` | 262144 | Ink boxes a set's `INKX` holds, over every strike: one for each glyph and each cluster whose ink passes its cells | The set is refused | vtium's own, as `SET_CHUNKS_MAX` is: a box is a glyph's, and a strike's glyphs that reach past their cells are its italic and symbol glyphs. A box's record takes 32 bytes in memory, so the boxes hold at most 8 MiB |
| `CLUSTER_SCALARS_MAX` | 16 | Scalar values in a cluster's key | The set is refused; a longer cluster is looked up as none | vtium's own: Unicode 18.0's longest RGI emoji sequence, a kiss with two skin tones, is 10 |
<!-- end of bounds table -->
