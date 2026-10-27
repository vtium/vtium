# vtium-font

vtium's glyph sources: font files and built-in fonts in, glyph bitmaps out. It loads PC Screen Fonts (PSF1 and PSF2, gzipped ones with the `gzip` feature), carries GNU Unifont built in (the `unifont` feature), and draws box drawing, block elements, Braille, sextants, octants and the rest of Symbols for Legacy Computing's block shapes as sprites that fill their cell. A `Font` puts a face and the sprites together and draws any cluster exactly into its cells. It forbids unsafe code. Every glyph is a `Glyph`, a coverage mask or premultiplied RGBA, and its bytes are the same on every host. Outline fonts (vtium/vtium#23) come later as another `Face`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, lints and public names. |
| `src/face.rs` | The `Face` trait a glyph source implements, and `Font`: sprites first, then the face, then U+25A1, `?` and the face's last resort, each glyph boxed to its cells. `Decorations`: where underline and strike go. |
| `src/glyph.rs` | `Glyph`, what every face and sprite draws: a coverage mask or premultiplied RGBA, with the cells it spans, bounded by `CELL_SIDE_MAX`. |
| `src/psf.rs` | `Psf`: the PSF1 and PSF2 loader, its unicode table, and the glyph drawing, with the C reference's notice. |
| `src/gzip.rs` | gzip (RFC 1952) framing and checks around `miniz_oxide`'s inflate, for gzipped PSF fonts. |
| `src/unifont.rs` | `Unifont`: the built-in font, inflated a 16 KiB chunk at a time as glyphs are drawn from it. |
| `src/sprite.rs` | The sprites, ported from the C reference's `font_box.c` with its and Ghostty's notices. |
| `src/sprite/legacy.rs` | The Symbols for Legacy Computing sprites the C reference left to the font, adapted from Ghostty's sprite font with its notice. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source: the one source for the constants and for the table below. |
| `build.rs` | Packs `data/unifont.hex` into the chunks and index `src/unifont.rs` includes. |
| `data/` | The unifont glyphs and their licence: see its README. |
| `tests/` | The integration tests, fixtures and C reference digests: see its README. |
| `fuzz/` | The cargo-fuzz target for the PSF loader, its own workspace so CI does not build it: see its README. |

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

## Beyond the C reference

- **The built-in unifont is inflated as it is used.** The binary carries the 3.7 MB of glyphs deflated in 16 KiB chunks (1.3 MB), and each chunk is inflated the first time a glyph is drawn from it, so text in a few scripts holds a few chunks. The C reference inflated the whole font into memory when it loaded (font_unifont.c:203-245).
- **The rest of Symbols for Legacy Computing's block shapes are sprites** (vtium/vtium#27, aaronsb/kmscon#92). The C reference drew them from the font, between its ascent and descent. vtium draws them to fill the cell, at every cell size, in the shapes the Unicode 18.0 code charts show:
  - U+1CEA0-U+1CEAF, the one quarter blocks: the Symbols for Legacy Computing Supplement chart, <https://www.unicode.org/charts/PDF/U1CC00.pdf>;
  - U+1FB3C-U+1FBAF, U+1FBBD-U+1FBBF and U+1FBCE-U+1FBEF, the smooth mosaics, triangles, eighth, third and quarter blocks (U+1FB82-U+1FB85 and U+1FBE4-U+1FBE7 among them), shades, fills, light diagonals and circles: the Symbols for Legacy Computing chart, <https://www.unicode.org/charts/PDF/U1FB00.pdf>.

  The block's pictographs stay with the face: the pointer, check mark, running man, arrows, folders and squared dot (U+1FBB0-U+1FBBC), the saltire, pointing index, question mark, stick figures, chevrons, cross mark and bracket (U+1FBC0-U+1FBCD), and the segmented digits and alarm bell (U+1FBF0-U+1FBFA). Unifont draws them. `tests/legacy.rs` holds every sprite to a model built from its Unicode name, at every cell size from 1x1 to 32x64 and in larger cells, and to literal masks. `tests/reference.rs` leaves these characters out of the C reference's page digests. A tier 2 screen of the ranges waits for the tier 2 goldens (vtium/vtium#17).
- **A cell is bounded.** A font scales by whole factors only as far as keeps its cell inside `CELL_SIDE_MAX`, and a sprite or glyph past it is not drawn (`CELL_SIDE_MAX` below). The C reference scaled to any height it was asked for.

## Glyph storage

**What a draw returns.** Every draw allocates a new `Glyph`: its width, height, the cells it spans (1 or 2) and its pixels row by row, in one of two formats.

| Format | Bytes a pixel | Holds | A narrow 8x16 cell | Two cells at `CELL_SIDE_MAX` (4096x2048) |
|---|---|---|---|---|
| `Format::Mask` | 1 | Coverage, 0 to 255; the renderer blends it in the cell's colours | 128 B | 8 MiB |
| `Format::Rgba` | 4 | Red, green, blue and alpha, the colours premultiplied by alpha | 512 B | 32 MiB |

PSF, unifont and the sprites draw only masks. RGBA is for colour faces (vtium/vtium#23, #24).

**What each source keeps.**

| Source | Kept in memory | Cost at minspec |
|---|---|---|
| `Psf` | The glyphs' 1-bit rows as one slice, count x height x ceil(width / 8) bytes, and, when the font has a unicode table, a codepoint table sorted for binary search, 8 bytes an entry. The file's other bytes are dropped after parsing. | kbd's `default8x16`: 4 KiB of glyphs and 282 entries (2.2 KiB). Terminus 16x32: 32 KiB and 627 entries (5 KiB). The largest font the bounds admit keeps up to 16 MiB of glyphs and 8.5 MiB of table, and parsing it holds the file's bytes as well, up to 16 MiB more. |
| `Unifont` | Compiled into the binary, read-only: the 3.7 MB of glyphs deflated in 227 chunks of at most 16 KiB (1.28 MB), a block index of 3465 runs (54 KiB) and a chunk index (1.8 KiB). In memory: each chunk a glyph has been drawn from, inflated once on its first draw and kept for the life of the process, shared by every `Unifont`. A chunk is never dropped and never inflated twice. | ASCII and Latin-1 are chunk 0 and U+25A1, drawn for a missing character, is chunk 14, so plain text holds 16 to 32 KiB. Text with Latin Extended, Greek and Cyrillic adds chunk 1, and punctuation, arrows and maths chunks 12 and 13. The CJK ideographs U+4E00-U+9FFF span 42 chunks (about 670 KiB). All 227 hold 3,718,608 B (3.55 MiB), under the 3.58 MiB that the C reference's measured 15.7 MiB peak already held for its unifont, inflated whole from the moment it loaded. |
| Sprites | Nothing: each is computed at the size asked for. | |

Scale does not change what a source keeps, only what a draw returns: a font scaled to a cell of 2048 still keeps its 1-bit rows, and each draw returns a glyph of up to 8 MiB as a mask.

**The drawn-glyph cache is vtium-render's** (vtium/vtium#16) and is not decided here. A draw costs an allocation and a bit-to-byte expansion, and for unifont an inflate only the first time a chunk is used, so the cache saves the expansion and the allocation and never repeats an inflate.

## Notes for vtium-render

- Ask `Font::glyph` for every cell's glyph: it draws the sprites, the face and the fallbacks, and boxes the result to exactly the cells asked for, marked with that cell count. Check its `format()`: a mask is blended in the cell's colours, RGBA is composited as it is. It returns `None` only for an empty cluster or a face that can draw nothing at all.
- A sprite's thickness is the face's underline thickness (`Font::decorations`), as in the C reference, so lines and underlines match.
- Bold is drawn into the glyph; italic is not, as in the C reference's bitmap fonts. Overhang is vtium/vtium#26.
- A glyph's bytes depend only on the font, the size, the style and the character, so a cache may key on those.
- `Font::glyph_unscaled` hands a bitmap face's glyph over at scale 1 with its scale, for a cache that stores it small and scales it while drawing (ADR-1300 D2). Drawn with each pixel a square `scale` across, it is `Font::glyph`'s glyph exactly. A sprite comes at its drawn size, as does a glyph whose box to its cells centres it in an odd number of spare pixels, which does not scale.

## Licences

The unifont glyphs in `data/unifont.hex` are GNU Unifont 17.0.03, dual-licensed under the SIL Open Font License 1.1 and the GNU GPL 2+ with the font embedding exception; vtium takes them under the OFL, whose text is `data/OFL-1.1.txt`. The test fixtures and their licences are listed in `tests/fixtures/README.md`. `miniz_oxide` (MIT, Zlib or Apache-2.0) inflates gzip fonts and the unifont chunks, and deflates the chunks at build time.

## Bounds

A font file is untrusted input: its sizes are bounded, and a malformed one is refused with a `PsfError`, never a panic. Each refusal case has its own test in `tests/psf.rs`, and the fuzz target feeds the loader any bytes.

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
<!-- end of bounds table -->
