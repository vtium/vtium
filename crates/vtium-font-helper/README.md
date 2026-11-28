# vtium-font-helper

This crate holds the font helper, the program that generates vtium's glyphs at run time (ADR-1200, ADR-1301 D2). The helper reads a frit (a font set file) and finds and reads the frit's sources. It parses each bitmap font and rasterises and shapes each outline font. It sends the glyphs at the cell the console asks for, in the glyph chunk format that vtium-font writes. It also draws the console's pointer pictures from an SVG file or a cursor theme, and describes a font file for vtium-config's font list.

The helper runs as `vtium-config serve`. vtium-config depends on this crate and re-exports each of its modules at its own root, so `vtium_config::serve` and the rest keep their paths. vtium-config's binary starts the helper and the parser's child. The console links none of this crate. It starts the helper as a program and reads only the glyphs it answers with. The crate depends on no other part of vtium-config, so the helper's code builds and is tested without the configuration tool and its TUI.

Fonts, frits and the files they name are untrusted input. Every decoder lives here, and each reads no further than its bound ([Bounds](#bounds)). The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`. The helper runs under its own sandbox, `src/serve/sandbox.rs`, and parses each font in a child process of its own, `src/child.rs`. Each dependency is pure Rust, and `Cargo.toml` says why each was chosen. vtium-config's README describes the frit format, the sources, the outline fonts, the parser and the helper's requests and frames as the tool uses them.

| Path | What it is |
|---|---|
| `src/frit.rs` | Frits in vtium.conf's line syntax, read strictly. |
| `src/sources.rs` | Each source, found and read: kbd names as `setfont` finds them, files, vconsole.conf and the kernel command line, the kernel's VT font behind the `Consoles` seam, and the built-ins. The `Gate` that lets the console's helper read only the active fonts. |
| `src/child.rs` | The parser's child process, `--parse`: its limits and the request it reads. Its parent's side lives in `tests/common/parser.rs`. `preview`, `fonts` and the TUI reach fonts through the helper, `serve`, under its own sandbox. |
| `src/describe.rs` | What a font file is, as the font helper reads it in its sandbox: each face's family, style and version, bitmap or outline, monospaced or not, the scripts its character map covers, its groups, its bitmap strikes and its coverage. Also the bounded frame that carries the description, and its decoder, which replaces control characters in names. |
| `src/decode.rs` | The decoders that the helper and the parser run: compressions told apart by their magic, gzip, PSF through vtium-font's reader, the font of a CPI file that `cp-size=` takes, and kbd unimaps. |
| `src/bdf.rs` | vtium-config's own BDF reader. It gives a `bdf:` source's glyphs at the font's own cell. |
| `src/hex.rs` | vtium-config's own reader of GNU Unifont's `.hex` format. It gives a `hex:` source's glyphs at their 8×16 cell. |
| `src/cover.rs`, `src/cover.rs.sha256` | The named `cover=` sets as range tables, and their SHA-256. `tools/cover-tables` generates them, and nobody edits them by hand. |
| `src/outline.rs` | Outline fonts: the request, and the bounded answer that the parent checks. Also the rasteriser that the parser runs, where swash places each glyph in its cell as R8 or premultiplied RGBA. |
| `src/outline/strike.rs` | Color bitmap strikes: the `sbix`, `CBLC` and `CBDT` tables read with read-fonts, and each glyph's PNG image decoded with the png crate, every count, offset and image size checked first. |
| `src/shaping/` | Clusters (vtium/vtium#144). Each cluster is judged by what it holds, shaped with HarfRust, and drawn whole by swash. `data.rs` holds the Unicode tables that `gen-shaping/` generates, and nobody edits it by hand. |
| `src/xcursor.rs` | The reader of a desktop cursor theme's files, the Xcursor format, which the helper runs for the console's pointer (ADR-1309 D6). It keeps the first frame of each size, at most 64 pixels a side, within a budget of pixels. |
| `src/pointer_svg.rs` | The drawing of an SVG pointer file's symbols with resvg, a cell tall, which the helper runs for the console's pointer (ADR-1309 D4). |
| `src/made.rs` | How the glyph helper made each superscript and subscript glyph, which the font preview's caption names (ADR-1202 D14). Also the frame that carries that record. |
| `src/resample.rs` | Scaling a color bitmap down to its cells, by the method chosen when the tool is built. |
| `src/glyphs.rs` | The bounded glyph form that the parser answers with, and its check. |
| `src/font.rs` | A bitmap source's glyphs at its own cell, from a parsed font, a BDF font or a built-in one. |
| `src/serve.rs` | `serve`: the helper that the console starts to generate a frit's glyphs at run time. It holds the helper's loop: it reads each request, answers it, keeps filling a cell's glyphs between requests, and exits once it is idle. |
| `src/serve/wire.rs` | The helper's wire format: the requests and their bounds, the codes their fields carry, the frames the helper answers with, and the active fonts the console names first. |
| `src/serve/engine.rs`, `src/serve/engine/` | The engine that draws a frit's glyphs. `engine.rs` reads the frit's sources once for the helper's life, `metrics.rs` gives the frit's own cell, its sizes, baselines and lines, and `draw.rs` draws the glyphs. |
| `src/serve/bitmap.rs` | The placement of a bitmap font's glyphs in a text band's cells: scaled, styled, and made smaller for a superscript or a subscript. |
| `src/serve/pointer.rs` | The pointer pictures that the helper draws for a pointer ask. |
| `src/serve/sandbox.rs` | The helper's sandbox: its limits, the descriptors it closes at start, its refusal to run as root, its landlock domain and its seccomp filters. |
| `src/onscreen.rs` | Text that comes from a source vtium does not control, such as a file's path or a reason the helper gave, made safe to print to a terminal: each control character, and each character that turns text around or hides it, becomes U+FFFD. |
| `src/needs.rs` | The helper's match of each font it finds against what the frit says the source needs: `check=`, `version=` and `sha256=`. |
| `src/lib.rs` | The crate's modules and its lints. |
| `src/scratch.rs` | A unit test's own temporary directory, removed when the test ends. vtium-config keeps a copy for its own unit tests. |
| `tests/engine.rs` | The helper's engine on each source form, decoder and shipped frit, with fonts, a kbd tree and font directories generated in the test. |
| `tests/parser.rs`, `tests/common/parser.rs` | The parser's child as its parent runs it, through the parent's side in `tests/common/parser.rs`, of which vtium-config keeps a copy. A child fails its source when it dies, fails, answers badly, overruns its deadline or floods stderr. The child holds an empty environment and only its pipes. A child that leaves a process holding its stdout is reaped as it exits, and that process is ended with it. |
| `tests/accents.rs` | Accented capitals in DejaVu Sans Mono keep their whole accents at 16- and 32-pixel cells, and a precomposed capital and its decomposed sequence draw alike. |
| `tests/blocks.rs` | Block elements and box lines from an outline font fill their character cell exactly, whatever the font's vertical metrics. |
| `tests/color_strikes.rs` | Damaged and unusual color bitmap strikes, none of which hangs the helper, panics in it or makes it allocate past its memory limit. |
| `tests/glyph_bound.rs` | Fonts that scale a glyph to tens of thousands of pixels, whose glyph the helper draws as empty. |
| `tests/script.rs`, `tests/script_sizes.rs` | Superscript and subscript asks: the outline, color and bitmap glyphs made small, and the glyphs taken from a smaller size of a bitmap source. |
| `tests/style.rs` | Style asks: bold, italic and bold italic from the frit's own faces, or synthesised from the regular glyph. |
| `tests/separators.rs` | Powerline separators from a merged symbol font meet the segment beside them at every text height. |
| `tests/pointer_file.rs` | vtium's shipped pointer file, `crates/vtium/install/pointers.svg`, against the pointer file reference, with each of its symbols drawn. |
| `tests/indic_sizes.rs` | A measurement, ignored by default, of how the helper draws the Indic font set's clusters at 32 pixels with the Noto fonts in tier 2's font cache. For each cluster, it prints the ink as the helper draws it, beside the cluster drawn with each of the helper's rules added in turn, so the rule that changes a glyph shows. Run it with `cargo test -p vtium-font-helper --test indic_sizes -- --ignored --nocapture`. |
| `tests/doubled_timing.rs` | A measurement, ignored by default, of how long the engine takes to draw a screen of text at each double size. |
| `tests/fixtures/` | The font subsets that this crate's tests read, with their licences. The README says how each subset was made. vtium-config's `tests/fixtures/` keeps copies of three of them for its own tests. |
| `tests/harfbuzz/` | A copy of vtium-config's `tests/harfbuzz/`: HarfBuzz's shaping tests of syllabic scripts on subsets of Noto fonts, copied from HarfRust, which judge the shaper in `src/shaping/harfbuzz.rs`. Its README says where they come from, and `differs.tsv` lists the 9 cases that the helper cannot draw as HarfBuzz does, with each reason. |
| `fuzz/` | The fuzz targets, in a workspace of their own (`fuzz/README.md`). `make fuzz FUZZ_TARGETS=vtium-font-helper:TARGET` runs one. `shape` drives clusters through the shaper and the rasteriser. `bdf` drives any bytes through the BDF reader, and `hex` through the `.hex` reader. `serve` drives any bytes through the glyph helper's request parser. `names` drives a font's bytes through the name-table reader that finds a font by its family and the reader of a face's version, `frit` drives any bytes through the frit reader, and `describe` through the description of a font file. `xcursor` and `pointer_svg` drive a cursor file and an SVG pointer file through their readers. |

## Bounds

A frit and every source file are opened once, never blocking on a FIFO. Each is refused unless its descriptor is a regular file, and read no further than its bound.

| Bound | Value | Past it |
|---|---|---|
| `FRIT_BYTES_MAX` | 64 KiB | The frit is refused |
| `FRIT_LINE_MAX` | 4096 bytes | The frit is refused, naming the line |
| `FRIT_SOURCES_MAX` | 16 | The frit is refused, naming the line |
| `UNIMAP_ENTRIES_MAX` | 65535, kbd's count | The unimap is refused |
| A font file, a unimap | vtium-font's `PSF_BYTES_MAX` | The source is refused before it is parsed |
| `COMPRESSED_BYTES_MAX` | vtium-font's `GZIP_BYTES_MAX`, 1 MiB, in every compression | The source is refused before it is decoded |
| `ZSTD_WINDOW_MAX` | vtium-font's `PSF_BYTES_MAX`, 16 MiB, the window a zstd frame declares. `ruzstd` holds up to one declared window of decoded bytes, and a block, before it hands any out, so this bound is what limits a frame's buffer, beside `PSF_BYTES_MAX` on the output. `zstd font` and `zstd -19` through a pipe stay inside it; a font piped through `zstd --ultra -20` or above, or `zstd --long`, declares more | The source is refused as the frame's header is read, with a warning that names its window and says the font needs recompressing with a window of 16 MiB or less |
| A zstd block | the smaller of its frame's window and 128 KiB decoded, RFC 8878's `Block_Maximum_Size`, which `ruzstd` enforces | The source is refused as the block decodes |
| vconsole.conf, the kernel command line | 64 KiB | The source is refused |
| `GLYPHS_BYTES_MAX` | the largest PSF's glyphs and a map of every codepoint | The parser's answer is refused |
| `PARSER_MEMORY`, `PARSER_SECONDS` | 512 MiB, 30 s | The kernel ends the parser; the source fails |
| `PARSER_WALL_SECONDS` | 60 s | The parent kills the parser; the source fails |
| `FAMILY_ENTRIES_MAX` | 65536 | A `font:` scan of a directory past it fails the source |
| `FAMILY_FILES_MAX` | 4096 outline font files opened in one `font:` search | The search fails, naming the cap and the family |
| `FACES_MAX`, `NAME_BYTES_MAX` | 256 faces of a collection, 1 MiB of a `name` table | A face past them is not read for its family |
| `FONT_DEPTH_MAX` | 8 | A font directory's subdirectories deeper than it are not searched |
| `OUTLINE_BYTES_MAX` | 64 MiB | An outline font file is refused before it is parsed |
| `OUTLINE_ANSWER_BYTES_MAX` | 128 MiB, a strike of GB 2312's ideographs at 48 pixels and room | The parser's answer is refused |
| `FRIT_SIZE_MIN`, `FRIT_SIZE_MAX` | 6 and 96 points, one `size=` | The frit is refused, naming the line |
| `FRIT_CELL_MIN`, `FRIT_CELL_MAX` | 6 and 96 pixels a side | The frit is refused, naming the line |
| `OUTLINE_CELLS_MAX` | 3 cells an outline request names | The request is refused |
| `BDF_LINE_MAX` | 4096 bytes | The BDF font is refused, naming the line |
| `BDF_GLYPHS_MAX` | 65535 glyphs, as a PSF's: the `CHARS` line, and the glyphs it counts. A glyph of `ENCODING -1` is read and dropped, so only the decompressed file's size bounds how many there are, and none is held | The BDF font is refused |
| `BDF_BITMAP_MAX` | 192 pixels, a glyph's side or offset | The BDF font is refused, naming the line |
| A `.hex` file | `PSF_BYTES_MAX`, read uncompressed; a code point of 1 to 6 hexadecimal digits and a glyph of 32 or 64 | The source is refused; a malformed line refuses the font, naming the line |
| A BDF font's cell | 1 to `FRIT_CELL_MAX` a side | The BDF font is refused |
| A BDF font's glyphs | `PSF_BYTES_MAX` in all, each counted as placed in its one or two cells | The BDF font is refused, naming the bound |
| A CPI file | 8 fonts of 512 glyphs, 32 wide | It is no CPI file |
| A glyph | a set's 16 KiB chunk | Left out: a color glyph two cells wide past about 64 pixels |
| `GLYPHS_MAX`, `MAP_MAX` | 65536 glyphs, a font's 16-bit glyph ids, and no more than the map names; 0x110000 codepoints | The parser's answer is refused |
| `RANGES_MAX` | 4096 `cover=` ranges a request names | The request is refused |
| `NAME_MAX`, `LICENSE_MAX` | 256 bytes of a family or format, 1024 of a licence | A request or answer is refused; a licence is cut there |
| A console font | 32 pixels wide, 64 high, 512 glyphs | The source is refused |


`cargo test -p vtium-font-helper` runs the crate's unit tests and the integration tests that call its modules directly. vtium-config's integration tests drive the helper further, through its binary.

References:
- ADR-1200: why vtium generates its glyphs at run time, in a sandboxed helper, from a frit.
- ADR-1301 D2: the glyph chunk format the helper answers in.
- ADR-1309: the pointer pictures the helper draws.
- ADR-1513 D1 and D4: the split that made this crate, which moved the code without changing it.
