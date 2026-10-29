# vtium-font

vtium's glyph sources: font files, vitrified sets and built-in sets in, glyph bitmaps out. It reads and writes vitrified sets (ADR-1200 D2), the glyph chunk format the helper's glyphs reach the console in; loads PC Screen Fonts (PSF1 and PSF2) with the `psf` feature, and gzipped ones with the `gzip` feature, which brings `psf`: vtium-vitrify enables both, and the console neither, so it parses no font file (ADR-1200 D6), while the crate's own tests take them through a dev-dependency on itself; carries GNU Unifont and Unscii-8 built in as sets (the `unifont` and `unscii` features); and draws box drawing, block elements, Braille, sextants, octants and the rest of Symbols for Legacy Computing's block shapes as sprites that fill their cell. A `Font` puts a face and the sprites together, choosing between them for each sprite range by its `SpritePolicy`, and draws any cluster exactly into its cells. It forbids unsafe code. Every glyph is a `Glyph`, a coverage mask or premultiplied RGBA, and its bytes are the same on every host. Outline fonts (vtium/vtium#23) come later as another `Face`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, lints and public names. |
| `src/face.rs` | The `Face` trait a glyph source implements, with its coverage of each sprite range, and `Font`: the sprites its policy chooses, then the face, then U+25A1, `?` and the face's last resort, each glyph boxed to its cells. `Decorations`: where underline and strike go. |
| `src/policy.rs` | `SpritePolicy`, a `Source` for each sprite range, with the default for a font the operator names; `Coverage`, how much of each range a face holds; `Missing`, what a character the face lacks draws as. |
| `src/glyph.rs` | `Glyph`, what every face and sprite draws: a coverage mask or premultiplied RGBA, with the cells it spans, bounded by `CELL_SIDE_MAX`. |
| `src/psf.rs` | `Psf`: the PSF1 and PSF2 loader, its unicode table, and the glyph drawing, with the C reference's notice; the `psf` feature. |
| `src/gzip.rs` | gzip (RFC 1952) framing and checks around `miniz_oxide`'s inflate, for gzipped PSF fonts. |
| `src/set/mod.rs` | The vitrified set's public face: `Set`, `SetBuilder` and `SetFace`. |
| `src/set/format.rs` | The set's layout: magic, version, section tags, every record's fields and codes. `build.rs` includes it. |
| `src/set/write.rs` | The writer's core, plain numbers in and reproducible bytes out. `build.rs` includes it. |
| `src/set/builder.rs` | `SetBuilder` and `StrikeBuilder`: a set described with vtium-font's types, for the tests and `vtium-vitrify`. |
| `src/set/read.rs` | `Set`: the reader, each chunk read, checked and inflated the first time a glyph is drawn from it, a 1-bit glyph expanded as it is drawn. |
| `src/set/read/parse.rs` | Each section's check against its bounds and the other sections. |
| `src/set/builtin.rs` | The built-in sets, `unifont` and `unscii-8`, and the one ADR-1200 D7 draws for a cell height and locale. |
| `src/set/face.rs` | `SetFace`: one cell size of a set drawn as a `Face`, and the strike a height picks (ADR-1200 D9). |
| `src/sha256.rs` | SHA-256 (FIPS 180-4), for the built-in sets' provenance and vtium-vitrify's source pins. `build.rs` includes it. |
| `src/unifont.rs` | `Unifont`: the built-in unifont set, drawn at whole multiples of 8x16. |
| `src/hex.rs` | The missing glyph `Missing::Hex` draws: the codepoint in 3x5 and 3x4 hex digits. |
| `src/sprite.rs` | The sprites, ported from the C reference's `font_box.c` with its and Ghostty's notices, and `Range`, the six ranges they fall in. |
| `src/sprite/legacy.rs` | The Symbols for Legacy Computing sprites the C reference left to the font, adapted from Ghostty's sprite font with its notice. |
| `src/sprite/spans.rs` | Each sprite range's codepoints, which a set's writer counts its coverage over. `build.rs` includes it. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source: the one source for the constants and for the table below. |
| `build.rs` | Packs `data/unifont.hex` and `data/unscii-8.hex` into the built-in sets with the set writer's own code. |
| `data/` | The unifont and Unscii-8 glyphs and their licences: see its README. |
| `tests/` | The integration tests, fixtures and C reference digests: see its README. |
| `fuzz/` | The cargo-fuzz targets for the PSF loader and the set reader, their own workspace so CI does not build them: see its README. |

`cargo test -p vtium-font` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_FONT_BLESS=1 cargo test -p vtium-font readme_table` rewrites it.

## Behaviour

The C reference is the floor: its `font_psf.c`, `font_unifont.c`, `font_box.c` and `font.c`. vtium draws the same pixels for the same font, size, style and character, and `tests/reference.rs` holds it to digests of the glyphs the C reference drew, over every sprite at 800 cell sizes, the box-drawing lines at 858 more sizes and thicknesses, every glyph of four PSF fixtures at three scales, every unifont glyph at two, and every character of plane 0 drawn through `kmscon_font_render`. The sprites also match the C reference's box model (`tests/box_model.h`) at every cell size from 1x1 to 32x64.

The PSF format follows kbd's description in Andries Brouwer's "The PSF font format". Where the C reference departed from it, vtium follows the format; each departure is below, with a test seen red against the C behaviour.

A sprite's curves and diagonals are supersampled with IEEE 754 `f64` addition, subtraction, multiplication, division and comparison only, which every host rounds the same way and Rust never fuses into one operation, so a sprite is the same bytes on every host. The digests were taken on x86-64 and are checked on whatever host runs the tests.

## Departures

- **A unicode table sequence does not end the glyph's entry.** In PSF1 0xFFFE, and in PSF2 0xFE, starts a sequence; the entry runs to its terminator, 0xFFFF or 0xFF. vtium maps the entry's single codepoints and skips its sequences, as the Linux console loads a table. The C reference took the start mark for a terminator (font_psf.c:445, 473), so each sequence shifted every later glyph's codepoints by one (aaronsb/kmscon#231). Spec: "The PSF font format", `<unicodedescription> := <uc>*<seq>*<term>`, `<seq> := <ss><uc><uc>*`.
- **A PSF2 table is decoded as well-formed UTF-8.** A byte that is not part of a well-formed character maps nothing, and 0xFE and 0xFF always act as marks. The C reference decoded loosely (font_psf.c:459-493): a stray continuation byte began a four-byte character, an overlong encoding mapped a codepoint, and a terminator that followed a lead byte was taken into the character, so every later glyph shifted. Spec: RFC 3629 and the Unicode Standard §3.9 (D92, Table 3-7), whose well-formed UTF-8 never holds 0xFE or 0xFF.
- **A codepoint the table lists for several glyphs maps to the last.** The Linux console overwrites as it loads a table (`con_insert_unipair` in drivers/tty/vt/consolemap.c). The C reference's binary search over a sorted table found any one of them (font_psf.c:456, 492, 496-510), depending on the C library's sort.
- **Table entries past the font's last glyph are ignored.** The C reference counted glyphs in 16 bits (font_psf.c:440, 467), so after 65536 terminators it mapped later entries onto the first glyphs. Spec: "The PSF font format", the table holds one description per glyph.
- **The fallback `?` is looked up as a character.** A character the font lacks draws U+25A1, else `?`, else glyph 0. The C reference drew glyph 63 for `?` whatever the font's table mapped (font_psf.c:360-362); in a font without a table, glyph 63 is still `?`.
- **A PSF file past 16 MiB is refused whatever follows its glyphs.** The C reference counted only the bytes it read, so it took a PSF1 font without a table however long the file (`PSF_BYTES_MAX` below).
- **A gzip file with bytes after its last member is refused.** zlib's `gzread`, which the C reference used, ignored them. Spec: RFC 1952 §2.2, "A gzip file consists of a series of members".
- **A font the operator names draws its own glyphs for each sprite range it covers whole.** `SpritePolicy::default_for` gives a range `Font` when the face holds every character of it and `Sprites` when it misses any, so no range mixes the two; a partly covered range draws sprites and warns. The built-in unifont keeps `Sprites` for every range, though it covers all six, so it looks as the C reference's did and the goldens hold. The C reference always drew the sprites. Source: the operator's decision on vtium/vtium#85.
- **A cluster on a sprite character the face lacks draws the sprite.** When the policy draws a cluster's first character as a sprite and the face lacks that character, the cluster draws the character's sprite and drops its marks, as the C reference's bitmap faces dropped them. The C reference took every cluster of more than one codepoint to the font, which drew U+25A1 for a character it lacked (font.c:345-366). So a PSF font without box drawing drew U+2500 + U+0301 as U+25A1, and vtium draws the line. A face that holds the character draws the cluster, as before. Source: the operator's decision on vtium/vtium#85, which draws the sprites for a range the font lacks.

## Beyond the C reference

- **The built-in unifont is a vitrified set, inflated as it is used.** The binary carries the 3.7 MB of glyphs as a set (ADR-1200 D7) whose 16 KiB chunks are each deflated (1.28 MB), and each chunk is inflated the first time a glyph is drawn from it, so text in a few scripts holds a few chunks. The C reference inflated the whole font into memory when it loaded (font_unifont.c:203-245).
- **The rest of Symbols for Legacy Computing's block shapes are sprites** (vtium/vtium#27, aaronsb/kmscon#92). The C reference drew them from the font, between its ascent and descent. vtium draws them to fill the cell, at every cell size, in the shapes the Unicode 18.0 code charts show:
  - U+1CEA0-U+1CEAF, the one quarter blocks: the Symbols for Legacy Computing Supplement chart, <https://www.unicode.org/charts/PDF/U1CC00.pdf>;
  - U+1FB3C-U+1FBAF, U+1FBBD-U+1FBBF and U+1FBCE-U+1FBEF, the smooth mosaics, triangles, eighth, third and quarter blocks (U+1FB82-U+1FB85 and U+1FBE4-U+1FBE7 among them), shades, fills, light diagonals and circles: the Symbols for Legacy Computing chart, <https://www.unicode.org/charts/PDF/U1FB00.pdf>.

  The block's pictographs stay with the face: the pointer, check mark, running man, arrows, folders and squared dot (U+1FBB0-U+1FBBC), the saltire, pointing index, question mark, stick figures, chevrons, cross mark and bracket (U+1FBC0-U+1FBCD), and the segmented digits and alarm bell (U+1FBF0-U+1FBFA). No cell-grid geometry defines them, so they draw from the face, and unifont carries them. `tests/legacy.rs` holds every sprite to a model built from its Unicode name, at every cell size from 1x1 to 32x64 and in larger cells, and to literal masks. `tests/reference.rs` leaves these characters out of the C reference's page digests. A tier 2 screen of the ranges waits for the tier 2 goldens (vtium/vtium#17).
- **Each sprite range can be drawn from the font instead** (vtium/vtium#85). The C reference always drew its sprites. A `SpritePolicy` gives each `Range` (box drawing, block elements, Braille, sextants, octants and Legacy Computing) a `Source`:
  - `Sprites`: the sprite, whatever the face holds, as the C reference drew.
  - `Font`: the face's glyph, and the font's `Missing` glyph for a character the face lacks.
  - `Auto`: the face's glyph, and the sprite for a character the face lacks.

  `Font::coverage` counts the characters of each range the face holds, 874 in all, through `Face::coverage`, which asks `Face::has_glyph` of each; a face that reads a character map can override it. `Coverage::partial` names the ranges a face covers only partly, for the operator's warning. `Font::new` takes the face's own `Face::sprite_policy`, and `Font::with_policy` sets another; the default is listed under Departures. `tests/policy.rs` holds each source to stand-in faces that cover a range fully, partly and not at all.
- **A character the face lacks can draw its codepoint** (vtium/vtium#85). `Missing::Replacement`, the configuration's `missing = box` and the default, draws U+25A1, `?` or the face's last resort, as the C reference did. `Missing::Hex`, `missing = hex`, draws the codepoint in hex digits, in the layout and digit shapes the operator approved on vtium/vtium#85. Every pair of digits differs by at least two pixels, so a program can read the codepoint back:
  - a narrow cell is the text color with the digits cut out, two rows of two 3x5 digits up to U+FFFF and three rows of two 3x4 digits past it, b, c and d in lower case;
  - a wide cell is a box outline with 3x5 digits inside, two rows of two or of three.

  The design is 8x16 narrow and 16x16 wide, scaled by the largest whole factor the cell holds and centred, a narrow cell's margin in the text color; a cell smaller than the design draws U+25A1. `Font::glyph_unscaled` hands it over at its drawn size, as a sprite. `tests/missing.rs` holds it to literal masks.
- **A cell is bounded.** A font scales by whole factors only as far as keeps its cell inside `CELL_SIDE_MAX`, and a sprite or glyph past it is not drawn (`CELL_SIDE_MAX` below). The C reference scaled to any height it was asked for.

## Vitrified sets

A vitrified set (ADR-1200 D2) is one buffer, little-endian throughout, written to no file: the `VTGLASS\0` magic, a major and minor version, a section table, then `META` (the provenance as `key=value` lines), `STRK` (each strike's style, pixel format, cell, baseline, underline and strike lines, glyph count; sorted by cell, style and pixel format, so a cell holds a 1-bit, an R8 and an RGBA strike of each style), `CMAP` (each strike's runs of codepoints, a cell's strikes of one style mapping codepoints apart), `POLY` (the sprite policy and each strike's coverage of each sprite range) and `GLYF` (an index of chunks, each with its offset, stored and decoded lengths and raw-or-deflated flag, then the chunks). `src/set/format.rs` lays out every field. Format 2.0 is what this build writes and reads, the helper and the console changing together, so it carries no hash; a set of another major version is refused with `SetError::UnknownMajor`, and a later minor version's sections and record fields are skipped.

- **Writing.** `SetBuilder` takes a name, provenance lines, a sprite policy and `StrikeBuilder`s, each a style at one cell size in one pixel format (1-bit, R8 or premultiplied RGBA), its glyphs and the codepoints that draw them. The bytes depend only on these: strikes and runs are sorted, glyphs are numbered in the order the sorted map first reaches them, a glyph no codepoint maps is dropped, and chunks are deflated at one fixed level, so the same inputs give the same bytes on every host (`tests/set.rs` pins a set's SHA-256). Chunks hold at most `SET_CHUNK_BYTES` and never split a glyph; a strike's glyphs start a new chunk. By default a strike's chunks are deflated for R8 and RGBA and stored raw for 1-bit, as a frit's `compress=yes` asks (ADR-1200 D13), and `StrikeBuilder::compress(false)` stores them raw.
- **Reading.** `Set::from_bytes` checks a whole set in memory, as the console checks each of the helper's frames, and `Set::check_chunks` decodes every chunk without keeping it. Every count and size is bounded, and every table is checked against the others before a glyph is drawn: runs sorted and apart, glyph counts, `POLY`'s coverage against `CMAP`, and the chunks against each strike's glyph bytes. The map, a run and the glyph span it may bring in, takes 36 bytes a run, at most 9 MiB at `SET_RUNS_MAX`, and `CMAP`'s bytes at most 16,777,352 while they are checked. A chunk is inflated to exactly its decoded length the first time a glyph is drawn from it, then kept as stored. A 1-bit glyph is expanded to a mask of 0 and 255 as it is drawn (ADR-1200 D13), as `Psf` expands its glyphs, so a 1-bit chunk costs its own 16 KiB at most. A chunk that fails draws its glyphs as missing ones, and `Set::fault` names it.
- **Drawing.** `SetFace` draws one cell size of a set: the tallest strike whose whole multiple is the height asked for, else the tallest under it scaled by the largest whole factor that fits, else the smallest at its own size (ADR-1200 D9). A style draws from its own strikes, whichever of them maps the codepoint, in its own pixel format: a 1-bit glyph expanded to an 8-bit mask, an R8 glyph as its mask, an RGBA glyph as premultiplied color, one or two cells wide. A codepoint it lacks, or a style with no strike, draws from the regular strikes, a mask widened for bold as vtium-font's bitmap faces widen it, a pixel set when it or its left neighbour in the same cell is; a color glyph is drawn as it is.

The built-in sets (ADR-1200 D7) are `unifont`, an 8x16 strike compiled in with the `unifont` feature, and `unscii-8`, an 8x8 strike compiled in with the `unscii` feature, 15,486 B as a set. `set::builtin` gives either by name, and `set::builtin_name` the one D7 draws: `unscii-8` for cells under 16 pixels high and `unifont` from 16, and `unifont` in a zh, ja or ko locale, since Unscii-8 holds no CJK. Both draw the sprites for every sprite range.

Choices the format leaves to this crate, each settled here:

- A deflated chunk is stored deflated only when that makes it smaller; otherwise raw, with its flag clear.
- A multi-line `META` value, such as the frit, is one line of its key for each of its lines; `name` and `format` are written first by the builder.
- `POLY` holds each strike's coverage, and the reader refuses a `POLY` whose coverage is not its strike's map's.
- A strike drawn at a whole factor keeps the lines the bitmap faces draw: a strike whose decorations are `Decorations::for_height` of its height, as every bitmap source gives, takes `for_height` of the scaled height. A strike with lines of its own scales them with its glyphs.
- A set face's last resort, drawn when the set holds neither U+25A1 nor `?`, is glyph 0 of the first regular strike at its cell that holds a glyph: the glyph of the lowest codepoint that strike maps. A cell's coverage of each sprite range is its regular strikes' counts summed.

## Glyph storage

**What a draw returns.** Every draw allocates a new `Glyph`: its width, height, the cells it spans (1 or 2) and its pixels row by row, in one of two formats.

| Format | Bytes a pixel | Holds | A narrow 8x16 cell | Two cells at `CELL_SIDE_MAX` (4096x2048) |
|---|---|---|---|---|
| `Format::Mask` | 1 | Coverage, 0 to 255; the renderer blends it in the cell's colors | 128 B | 8 MiB |
| `Format::Rgba` | 4 | Red, green, blue and alpha, the colors premultiplied by alpha | 512 B | 32 MiB |

PSF, unifont and the sprites draw only masks. RGBA is for color faces (vtium/vtium#23, #24).

**What each source keeps.**

| Source | Kept in memory | Cost at minspec |
|---|---|---|
| `Psf` | The glyphs' 1-bit rows as one slice, count x height x ceil(width / 8) bytes, and, when the font has a unicode table, a codepoint table sorted for binary search, 8 bytes an entry. The file's other bytes are dropped after parsing. | kbd's `default8x16`: 4 KiB of glyphs and 282 entries (2.2 KiB). Terminus 16x32: 32 KiB and 627 entries (5 KiB). The largest font the bounds admit keeps up to 16 MiB of glyphs and 8.5 MiB of table, and parsing it holds the file's bytes as well, up to 16 MiB more. |
| `Unifont` | Compiled into the binary, read-only: the built-in set, 1,344,063 B: the 3.7 MB of 1-bit glyphs deflated in 227 chunks of at most 16 KiB (1.28 MB), its map of 3465 runs at 16 bytes each (54 KiB) and its chunk index (4.4 KiB). In memory: the set's runs, glyph spans and chunk index, read once (about 145 KiB), and each chunk a glyph has been drawn from, inflated once on its first draw and kept 1-bit for the life of the process, shared by every `Unifont`. A chunk is never dropped and never inflated twice. | ASCII and Latin-1 are chunk 0 and U+25A1, drawn for a missing character, is chunk 14, so plain text holds 32,752 B. Text with Latin Extended, Greek and Cyrillic adds chunk 1, and punctuation, arrows and maths chunks 12 and 13. The CJK ideographs U+4E00-U+9FFF span 42 chunks (688,128 B). All 227 hold 3,718,608 B (3.55 MiB), under the 3.58 MiB that the C reference's unifont held inside the minspec peak, now 16 MiB, inflated whole from the moment it loaded. |
| `SetFace` | The set's runs, glyph spans and chunk index, and each chunk drawn from, decoded and kept as stored, at most `SET_CHUNK_BYTES` each. | |
| Sprites | Nothing: each is computed at the size asked for. | |

Scale does not change what a source keeps, only what a draw returns: a font scaled to a cell of 2048 still keeps its 1-bit rows, and each draw returns a glyph of up to 8 MiB as a mask.

**The drawn-glyph cache is vtium-render's** (vtium/vtium#16) and is not decided here. A draw costs an allocation, a bit-to-byte expansion for a 1-bit glyph, and for a set an inflate only the first time a chunk is used, so the cache saves the allocation and never repeats an inflate.

## Notes for vtium-render

- Ask `Font::glyph` for every cell's glyph: it draws the sprites, the face and the fallbacks, and boxes the result to exactly the cells asked for, marked with that cell count. Check its `format()`: a mask is blended in the cell's colors, RGBA is composited as it is. It returns `None` only for an empty cluster or a face that can draw nothing at all.
- A sprite's thickness is the face's underline thickness (`Font::decorations`), as in the C reference, so lines and underlines match.
- Bold is drawn into the glyph; italic is not, as in the C reference's bitmap fonts. Overhang is vtium/vtium#26.
- A glyph's bytes depend only on the font, the size, the style and the character, so a cache may key on those. A `Font`'s policy is fixed when it is built, so a new policy is a new `Font`.
- The configuration (vtium/vtium#21) builds a font's policy from its keys: the one value for all six ranges as `SpritePolicy::all`, else the face's default, then each range's own key with `SpritePolicy::with`. It warns for each range `Font::coverage().partial()` names.
- `Font::glyph_unscaled` hands a bitmap face's glyph over at scale 1 with its scale, for a cache that stores it small and scales it while drawing (ADR-1300 D2). Drawn with each pixel a square `scale` across, it is `Font::glyph`'s glyph exactly. A sprite comes at its drawn size, as does a glyph whose box to its cells centres it in an odd number of spare pixels, which does not scale.

## Licences

The Unscii-8 glyphs in `data/unscii-8.hex` are Unscii 2.1 by Viznut, in the public domain (`data/unscii.txt`). The unifont glyphs in `data/unifont.hex` are GNU Unifont 17.0.03, dual-licensed under the SIL Open Font License 1.1 and the GNU GPL 2+ with the font embedding exception; vtium takes them under the OFL, whose text is `data/OFL-1.1.txt`. The test fixtures and their licences are listed in `tests/fixtures/README.md`. `miniz_oxide` (MIT, Zlib or Apache-2.0) inflates a set's chunks, the built-in sets' among them, and gzip fonts, in every build, and deflates a set's chunks when one is written.

## Bounds

A font file and a set are untrusted input: their sizes are bounded, and a malformed one is refused with a `PsfError` or a `SetError`, never a panic. Each refusal case has its own test in `tests/psf.rs` and `tests/set.rs`, and the fuzz targets feed the loader and the set reader any bytes.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Source |
|---|---|---|---|---|
| `PSF_GLYPHS_MAX` | 65535 | Glyphs a PSF font holds | The font is refused | `PSF_MAX_GLYPHS`, font_psf.c:62-64, 74, 200 (aaronsb/kmscon#229). The C reference kept 0xffff free as its table's no-entry index; vtium keeps the value, and its table holds glyph indices in 16 bits |
| `PSF_WIDTH_MAX` | 32 | A PSF glyph's width in pixels | The font is refused | `PSF_MAX_WIDTH`, font_psf.c:65, 75, 200: a row is read into 32 bits |
| `PSF_HEIGHT_MAX` | 256 | A PSF glyph's height in pixels | The font is refused | `PSF_MAX_HEIGHT`, font_psf.c:66-67, 76, 200: PSF1 holds the height in a byte |
| `PSF_BYTES_MAX` | 16777216 | Bytes a PSF file holds: the whole file, decompressed when gzipped | The font is refused, whatever the bytes past the glyphs hold | `PSF_MAX_BYTES`, font_psf.c:68-69, 77, 207-208, 420-428; the C reference counted only the bytes it read, so it took a PSF1 font without a table whatever followed its glyphs |
| `PSF_ENTRIES_MAX` | 1114112 | Codepoints a PSF unicode table maps, before duplicates are merged | The font is refused | `PSF_MAX_ENTRIES`, font_psf.c:70-72, 78, 383-385: one entry for each Unicode codepoint |
| `GZIP_BYTES_MAX` | 1048576 | Bytes a gzipped PSF file holds before it is decompressed. kbd's largest gzipped console font is 6 KiB | The font is refused before anything is decompressed | vtium's own: the inflater builds a Huffman table for each deflate block, about a microsecond, and a block can be 10 bits, so a 15 MiB file of empty blocks took 14 s to refuse. At 1 MiB the worst file takes about 1 s on the test machine. The C reference read through zlib, whose fixed tables are prebuilt |
| `CELL_SIDE_MAX` | 2048 | A cell's width or height in pixels. A glyph is at most two cells wide | A font scales by the largest whole factor that keeps its cell inside; a sprite or glyph asked for past it is not drawn | vtium's own: the C reference scaled a bitmap font to any height it was asked for (font_psf.c:247-249, font_unifont.c:229-231). A cell of 2048 is taller than a 7680x2160 display's one row |
| `SET_BYTES_MAX` | 1073741824 | Bytes a vitrified set holds | The set is refused | vtium's own, from ADR-1200's measured sets: Noto Sans Mono CJK unlimited at 48 pixels is about 100 MB a strike (D1), a full GBK set at 48 pixels 49.8 MB (D13), and a set holds at most three sizes (D11) |
| `SET_SECTIONS_MAX` | 16 | Sections a set's header lists | The set is refused | vtium's own: format 2.0 writes five (ADR-1200 D2), and a minor version adds sections a reader skips |
| `SET_META_BYTES_MAX` | 1048576 | Bytes of a set's `META` provenance | The set is refused | vtium's own: the frit verbatim and a line or three for each source (ADR-1200 D2) |
| `SET_STRIKES_MAX` | 32 | Strikes a set holds | The set is refused | vtium's own: a strike for each of four styles at up to three sizes (ADR-1200 D1, D11), and color strikes beside them |
| `SET_GLYPHS_MAX` | 1114112 | Glyphs one strike holds | The set is refused | One for each Unicode codepoint: a set maps single codepoints only (ADR-1200 D2, `CMAP`) |
| `SET_RUNS_MAX` | 262144 | Runs of codepoints a set's `CMAP` holds, over every strike | The set is refused | vtium's own: the built-in unifont is 3,465 runs, and Noto Sans Mono CJK unlimited, about 44,000 glyphs (ADR-1200 D1), at most 44,000 a strike. A run and its glyph span take 36 bytes in memory, so the map holds at most 9 MiB, and its `CMAP` at most 16,777,352 bytes while it is read |
| `SET_CHUNKS_MAX` | 262144 | Glyph chunks a set's `GLYF` index lists | The set is refused | vtium's own: `SET_BYTES_MAX` of raw chunks is 65,536 of them, and deflated ones are 1.5-4.7 times smaller (ADR-1200 D13) |
| `SET_CHUNK_BYTES` | 16384 | Bytes a glyph chunk holds, as stored and decoded. No glyph is split across two, so a glyph is at most this | The set is refused; the writer starts a new chunk | ADR-1200 D2: "chunks of at most 16 KiB, no glyph split across two", the built-in unifont's chunk size |
| `SET_RECORD_BYTES_MAX` | 64 | Bytes a record of a set's tables declares | The set is refused | vtium's own: format 2.0's largest record is 48 bytes, and a minor version adds fields at a record's end (ADR-1200 D2); it bounds what a table's counts can make the reader hold |
<!-- end of bounds table -->
