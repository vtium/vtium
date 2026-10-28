# vtium-vitrify

The program that builds vitrified font sets for vtium (ADR-1200), its own binary outside the minspec build. It reads a frit, finds and reads the frit's sources as the invoking user, parses each bitmap font and rasterises each outline font in an unprivileged child process, lays the glyphs into a set with vtium-font's writer, and writes the set into the root-owned store under its lock, or into a portable file with `create --out`. The console reads only the sets this program writes and its own built-ins; every decoder lives here. The crate forbids unsafe code; the console font ioctls it needs live in vtium-platform's `src/vt/font.rs`.

| Path | What it is |
|---|---|
| `src/main.rs` | The binary: the command line, or the parser when run as `vtium-vitrify --parse`. |
| `src/cli.rs` | The commands, their options and exit codes, `list`'s TSV and JSON. |
| `src/frit.rs` | Frits in kmscon.conf's line syntax, read strictly. |
| `src/sources.rs` | Each source found and read: kbd names as `setfont` finds them, files, vconsole.conf and the kernel command line, the kernel's VT font behind the `Consoles` seam, the built-ins. |
| `src/child.rs` | The parser's child process: how it is started, its limits, the request it reads and the answer the parent checks. |
| `src/decode.rs` | The decoders the parser runs: compressions told by their magic, gzip, PSF through vtium-font's reader, and kbd unimaps. |
| `src/outline.rs` | Outline fonts: the request and the bounded answer the parent checks, and the rasteriser the parser runs, swash placing each glyph in its cell as R8 or premultiplied RGBA. |
| `src/resample.rs` | Scaling a color bitmap down to its cells, by the method chosen when the tool is built. |
| `src/glyphs.rs` | The bounded glyph form the parser answers with, and its check. |
| `src/build.rs` | The set from its sources: the cell, the fallbacks fitted into it, the sprite policy, the provenance and the build key. |
| `src/store.rs` | Writing the store: the lock, files synced and renamed, the manifest, unreferenced sets removed. |
| `src/serve.rs` | `serve`: the helper the console starts to generate a frit's glyphs at run time, its request, its framed answers and its idle exit. |
| `src/switch.rs` | `set`: OSC 50 and its query on the controlling terminal, the reply read in raw mode within its wait. |
| `tests/cli.rs` | The commands run as the invoking user against a store in a test directory, with fonts, a kbd tree and frits generated in the test. |
| `tests/serve.rs` | `serve` run as the console runs it, under its own sandbox. |
| `tests/switch.rs` | `set` on a real pty in raw mode, answered by vtium-term's terminal, and its refusals before the terminal is touched. |
| `tests/outline.rs`, `tests/common/ttf.rs` | Outline rasterising on a TrueType font with COLR color glyphs written in the test. |
| `frits/` | The frits vtium ships: `minspec-16` and `minspec-8`, minspec's curated sets, and `proof`, vtium/vtium#23's desktop set. |

`cargo test -p vtium-vitrify` runs every test. The store tests need a directory whose ancestors only root or the invoking user may write, as the store's check requires: Cargo's temporary directory under the workspace, else `XDG_RUNTIME_DIR`, or `VTIUM_FONT_STORE_TEST_DIR`.

## Commands

| Command | Runs as | Does |
|---|---|---|
| `create NAME\|FRIT...`, `create --all` | root | Builds each frit into the store. A set whose build key equals its manifest row is `current`; `--force` rebuilds it. |
| `create --kbd NAME`, `--file PATH`, `--vconsole`, `--console VT` | root | Writes the one-source frit, named for the font (`console` for `--console`), refusing to overwrite a different frit of that name, and builds it. |
| `create --out FILE FRIT` | any user | Builds a portable set into FILE. |
| `install FILE...` | root | Checks each set whole, every chunk's hash and its version included, and adds it to the store. |
| `list [--json]` | anyone | The store's sets and the built-ins, as TSV with a header or a JSON array. |
| `verify [NAME...]` | anyone | The console's check of each set, every chunk read. |
| `remove NAME...` | root | Drops each set's row and file; its frit stays. |
| `set NAME` | the session's user | Switches the running console to a set `list` gives (ADR-1200 D5): writes `OSC 50 ; NAME ST` and the query `OSC 50 ; ? ST` to `/dev/tty`, reads the reply in raw mode for at most a second, and puts the terminal's mode back, dropping input left unread first; SIGTERM, SIGHUP, SIGINT and SIGQUIT are held while the terminal is raw and act once its mode is back. It prints `set NAME` when the reply names NAME, else `failed`, the name, and the set the console kept or that no reply came, which the console's `allow-font-ops` off or a terminal that is not vtium gives. |

Each build or install prints one line a set, tab-separated: `built`, `current`, `installed`, `removed`, `verified` or `failed`, the name, and for a failure the reason. A set built, or current, with sources left out as missing reports `built-partial` or `current-partial`, the name, and `left out: ` with those sources as the frit spelled them, comma-separated, so a package hook sees the set is not the frit's whole. The exit status is 0 when every set succeeded or was current, 1 when any failed, and 2 on a usage error. A command that writes the system's store refuses without root, naming `sudo`, and exits 1.

The store is `/var/lib/vtium/fonts` and the frits are in `/etc/vtium/fonts`, each changed at build time with `VTIUM_FONT_STORE` and `VTIUM_FONT_FRITS`. `--store DIR` names a store the invoking user owns in place of the system's, for tests and for staging; the console reads only the system's. `--frits DIR`, `--kbd-dir DIR` (repeatable), `--font-dir DIR` (repeatable), `--vconsole-conf FILE` and `--kernel-cmdline FILE` replace the other paths the host gives.

When stdin and stdout are both terminals, the tool opens with a banner of its name in glass (`src/banner.rs`, vtium/vtium#112). `--no-banner` or a non-empty `NO_COLOR` turns it off, and it never precedes `--json` or `--version`, never shows in a Chinese, Japanese or Korean locale, and never reaches a pipe.

## Frits

One `key=value` a line, blanks around key and value dropped, `#` to the line's end a comment, as kmscon.conf reads; unlike kmscon.conf, an unknown key, a malformed line, a value with a control character or a key this build does not build yet refuses the whole frit, naming the file and line. This slice reads `name=`, `source=` (`font:`, `kbd:`, `file:`, `outline:`, `vconsole`, `console:VT`, `builtin:`), `package=`, `sha256=`, `unimap=`, `cover=` (`ascii`, `latin-1`, `U+XXXX` and `U+XXXX-U+YYYY`), `style=regular`, `size=`, `cell=`, `compress=` and `license=`. `cell=WIDTHxHEIGHT`, each side 6 to 96, states the cell the console draws the frit in and the helper generates for (#145); the console reads it as text before its first frame.

A frit is meant to travel, so a source names a font each machine finds for itself. The `font:` form below is provisional: its syntax is the operator's to choose, and may be renamed before the first release (vtium/vtium#121).

```ini
source=font:Terminus 16      # a family, matched on the machine that builds the set
style=regular
package=terminus-font        # optional: the package that installs it
sha256=<64 hex digits>       # optional: the font file's SHA-256
```

`font:FAMILY` matches as fontconfig matches a family, with case and blanks ignored. kbd console fonts match it first, by name: the font `setfont` would find by that name, else the first, in each data directory's sorted `consolefonts/`, whose name less its suffixes is the family. A PSF file outside kbd's directories carries no family in its header, so only `file:` names it. With no kbd font of that name, an installed outline font matches: a `.ttf`, `.otf`, `.ttc` or `.otc` file whose name less its extension is the family, or the family and `Regular`, with case, blanks, `-` and `_` ignored, so `font:JetBrains Mono` finds `JetBrainsMono-Regular.ttf`; the plain name first, then the font directories in order (`/usr/share/fonts`, `/usr/local/share/fonts`, and for a user who is not root `$XDG_DATA_HOME/fonts`, else `~/.local/share/fonts`, or each `--font-dir`), then by path. The parser checks that the file's name table gives that family, typographic or legacy, and in a collection takes the first face of it. `outline:PATH` names an outline file by its absolute path and, like `file:`, warns that the frit is not portable. BDF families are matched once BDF is read. `package=` and `sha256=` follow a `font:`, `kbd:`, `file:` or `outline:` source.

`size=` names a cell height, 6 to 96 pixels, at most three times. An outline font that comes first sets the cell at each `size=`, one strike each, and fails the set without one; a bitmap font that comes first sets its own cell, and a `size=` other than its height fails the set. A later outline font is drawn at the cells the first source set, within its `cover=`.

A source missing on the building machine, or whose bytes are not its `sha256=`, is left out: `create` warns on stderr with the frit's file and line, the reason and the package, builds the set from the sources found, and names the one left out in the set's provenance as `source-missing=` and on its stdout line as `built-partial`. The manifest row's source is the first source found, as the set's provenance and `install` give it. The build key counts it as missing, so the set rebuilds when it appears. With every source missing, the set fails. A source that is there and cannot be read fails the set.

`file:` and an absolute `unimap=` are allowed, and `create` and `create --out` warn that they name a path on this machine, so the frit is not portable.

The first source sets the cell. Each later one fills only the codepoints the earlier ones lack. A bitmap font is scaled by the largest whole factor at which its cell fits the strike's, centred across, its baseline on the strike's where it gives one (a built-in set does; a PSF font does not, and sits on the cell's bottom row), kept inside the cell; one whose cell does not fit at factor 1 fills nothing. Each cell holds a strike a pixel format: bitmap fonts give 1-bit glyphs, stored raw under either `compress=`, outline fonts R8 coverage and color glyphs premultiplied RGBA, both deflated under `compress=yes`, as ADR-1200 D13 decides; the strikes' codepoints are apart, and all share the first source's baseline. The provenance's `color-scale=` names the method color bitmaps were scaled with, and the build key holds it. The set's sprite policy is `SpritePolicy::default_for` the first source's coverage at the first cell, the policy the console gave a PSF font it read itself: the sprites win over a later source's glyphs, so a range draws from the set's glyphs only where its first font covers it whole (the operator's decision, 2026-10-28), until `glyph-source=` lets a frit say otherwise. The provenance names each outline source's family (`source-family=`) and the licence its name table describes (`source-license=`); `license=` is the set's.

## Outline fonts

The parser rasterises each outline font with swash at every cell the set is built at, and places each glyph in its cell as the C reference's FreeType backend did (`kmscon/src/font/font_freetype.c:130-189,400-470` at `b1f7a7c5`): the font is sized so its ascent and descent span the cell's height less half a pixel, as a FreeType cell request for that height did; the baseline sits the ascent, rounded up, below the top; and the font that sets the cell sets its width from `W`'s advance or its ink, whichever is wider, then `M`'s or `0`'s. A glyph sits on the baseline at its own bearing, moved left by half of what passes the cell's right edge, and is cut at the cell's edges. Outlines are hinted.

A glyph advancing more than one and a half cells is two cells wide: CJK, emoji, and the wide icons of a Nerd Font that is not a Mono build. A color glyph, a COLR glyph's layers or a CBDT or sbix bitmap, is drawn unhinted, centred across its cells and kept inside the cell's height. COLR layers are laid over each other in the parser with the renderer's rounding, premultiplied, and a COLR glyph larger than its cells is drawn again at the size that fits them.

A color bitmap is taken from the font's largest strike holding it, its colors premultiplied by its alpha, and scaled by vitrify itself to the cell's size, or as far under as fits its cells: Lanczos then a light sharpen (an unsharp mask of radius 0.8 at 120 per cent), the operator's choice on vtium/vtium#23 from a comparison of six ways (`vtium-review-images/23/emoji-scaling.png`). Box, plain Lanczos, box with alpha snapped at half and nearest stay in `src/resample.rs`, one chosen when the tool is built with `VTIUM_VITRIFY_COLOR_SCALE=lanczos-sharpen|lanczos|box|box-snap|nearest`, the first the default. It is a build-time setting rather than Cargo features because the methods exclude each other and features add up across a workspace; an unknown value fails the build. Weights, sizes and placement are computed in f64 with IEEE basic operations alone and rounded to 14-bit integers, and pixels are integers, with no `sin`, `exp` or `powf` from the platform, so a set is the same bytes on every host.

The answer the parent checks gives, for each cell asked for, the cell, the baseline, every glyph's width, format and pixels and the map; every codepoint is one the request covers, every RGBA pixel premultiplied, and no glyph larger than a set's chunk.

The build key is the SHA-256 of the set format's version, this tool's version, the frit's bytes, and for each source in order the SHA-256 of the font's bytes as read, of its unimap and of kbd's `def.uni` when read, or a mark that the source was missing. A built-in source is hashed by its glyphs as read (`Font::digest`), since vtium-font gives the built-in sets' bytes to no other crate, so a tool whose built-in changed rebuilds the sets that take it.

## Sources

- **kbd names** resolve as `setfont` resolves them (kbd's `src/libkbdfile/kbdfile.c`, `src/libkfont/context.c`): in `consolefonts/` of each data directory in turn (`/usr/share/kbd`, `/usr/share`, `/usr/lib/kbd`, or `--kbd-dir`), the name followed by the first of `""`, `.psfu`, `.psf`, `.cp` and `.fnt` for which a file exists, plain before `.gz`, `.bz2`, `.xz` and `.zst`; a name with a `/` looks in the subdirectory its first component names, and an absolute path is taken as given, then with each suffix. Unimaps resolve the same way under `unimaps/` with `""`, `.uni` and `.sfm`. A font or unimap found by name is resolved through its links and read only if it stays under the `consolefonts/` or `unimaps/` of a data directory; a link out of them fails the set, and nothing is read from or hashed outside. A `font:` family's scan fails past `FAMILY_ENTRIES_MAX` entries in one directory.
- **A font's map** is the frit's `unimap=` when given, else the font's own table, else kbd's `def.uni`; a font with none is refused, naming `unimap=`.
- **vconsole** reads `FONT=` and `FONT_UNIMAP=` from vconsole.conf, with the kernel command line's `vconsole.font=` and `vconsole.font_unimap=` over them, as systemd-vconsole-setup reads them (`src/vconsole/vconsole-setup.c`). With no `FONT=`, systemd ran no `setfont` and the kernel kept its font, so the source is refused and names `console:VT`. `FONT_MAP=` maps the kernel's 8-bit translation and sets no glyph, so it is not read.
- **console:VT** opens `/dev/tty<VT>` read-only as root, checks with `KDGETMODE` that it is in text mode, reads its font with `KD_FONT_OP_GET_TALL`, or `KD_FONT_OP_GET` on a kernel without it, and its map with `GIO_UNIMAP`, and hands the parser a PSF2 of them.
- **Compression** is told by its magic, as kbd tells it, and decoded with its stream's own check value verified: gzip, bzip2, xz and zstd, each refused past `COMPRESSED_BYTES_MAX` before it is decoded and past `PSF_BYTES_MAX` once decoded.

## Decoders

Each is pure Rust, compiles no C and has no build script, and only this binary links it; `cargo tree -p vtium -e normal` lists none of them, so they add nothing to the minspec binary. The operator approved them on 2026-10-28.

| Format | Crate | Licence | Why this one |
|---|---|---|---|
| gzip | `miniz_oxide` 0.8.9 | MIT, Zlib or Apache-2.0 | Already in every vtium build for the sets' deflated chunks; vitrify frames gzip around it. |
| bzip2 | `bzip2` 0.6.1, on `libbz2-rs-sys` 0.2.5 | MIT or Apache-2.0; `libbz2-rs-sys` under libbzip2's BSD-style licence | Trifecta Tech Foundation's Rust port of libbzip2, the default backend of the most used bzip2 crate, maintained (0.2.5, May 2026), `build = false`; its one C file is a header no build compiles. `bzip2-rs`, decoder only, has had no release since 2021. |
| xz | `lzma-rust2` 0.24.0, features `std` and `xz` | Apache-2.0 | Maintained (October 2026), verifies the stream's check, and with no default features adds only `sha2` and its RustCrypto crates (MIT or Apache-2.0). `lzma-rs` has had no release since January 2023; `xz2` and `liblzma` compile liblzma's C. |
| zstd | `ruzstd` 0.8.2, default features | MIT | The pure-Rust zstd decoder (November 2025), `build = false`; `hash` adds `twox-hash` (MIT) to compute each frame's checksum, which vitrify compares with the frame's own, as `ruzstd` leaves that to its caller. 0.8.2 builds on the workspace's Rust 1.85; 0.9 needs 1.87. `zstd` binds the C library. |
| TrueType, OpenType, COLR, CBDT | `swash` 0.2.10, default features | MIT or Apache-2.0; under it `skrifa` 0.44, `read-fonts` 0.41 and `font-types` 0.12 (MIT or Apache-2.0, Rust 1.85), `zeno` 0.3 and `yazi` 0.2 (Apache-2.0 or MIT), and `bytemuck` 1.25 (Zlib, MIT or Apache-2.0) | ADR-1200 D14's decided rasteriser: the only pure-Rust one with hinting, embedded bitmaps and color, `build = false`. ab_glyph has no hinting, bitmaps or color; fontdue holds every glyph at load. |

The tests' xz fixtures take `lzma-rust2`'s encoder as a dev-dependency; bzip2 and zstd fixtures use the encoders the decoding crates carry.

## The parser

`create` and `create --out` read each source as the invoking user and run `vtium-vitrify --parse` for each font: an empty environment, `/` as its directory, its three pipes and no other descriptor. Run as root, the parent starts it as `nobody` from `/etc/passwd`, its supplementary groups dropped, and refuses to build when there is no such account. The parent kills the parser through a pidfd past 60 s by the wall clock, reads its stderr on a thread of its own, and so never hangs holding the store's lock. The parser sets no-new-privileges, an address space of 512 MiB, 30 s of CPU, no file writes, no processes and no core, refuses to run as root, reads its request, parses, and answers with the glyph form, which the parent checks field by field against the PSF bounds before it trusts it. A parser that fails, faults, passes a limit or answers malformed fails that source; `create` reports the set `failed` and goes on.

## The helper

`vtium-vitrify serve` is the helper the console starts on its first need for a frit's glyphs (ADR-1301 D2). It sets no-new-privileges, starts the thread that reads its requests, then sets the parser's limits with 300 s of CPU over its life, and refuses to run as root. Each request carries the frit's name and text, so a frit the console makes on the fly needs no file; the helper reads the frit's fonts itself, as the account the console started it as, parses bitmap fonts and rasterises outline ones in its own process, and draws at the cell each request names, the frit's, never another (#145): an outline font drawn at that cell, a bitmap font scaled by the largest whole factor that fits and placed on the cell's bottom row. A frit's `console:` source takes the kernel's font the console read at start and handed over. It takes three requests: an ask, answered whole ahead of anything else with the glyphs of the codepoints it names; a fill, which draws a cell's coverage while nothing is asked, the codepoints it names first, then printable Latin and box drawing, then the rest, 1,024 codepoints a batch, skipping what the console holds and what it sent, a waiting request taken before each batch; and a stop, which ends a cell's fill. Glyphs go as vitrified sets of one strike each. A source that fails as it draws is left out for the helper's life, and the next source in the frit's order gives its glyphs; a fill that never draws a glyph fails, naming each source whose cell does not fit or that failed. Each request is bounded before anything is allocated for it: the frit's name at 64 bytes and its text at `FRIT_BYTES_MAX`, the cell inside `CELL_SIDE_MAX`, the display's capabilities reserved and empty, 32,000 codepoints, each a scalar value, 65,536 held ranges, and the kernel's font inside the kernel's own bounds. A frame's payload is at most 16 MiB, a batch past it cut in two. It exits after `HELPER_IDLE_MS`, 60,000 ms, with no request and no fill, or when the console closes its end and its work is done.

## Departures

- **A relative kbd name is never looked for in the working directory.** `setfont` tried the name as given first, so a file in the directory it ran from shadowed the data directories. `create` runs unattended, as root, from wherever a package hook runs it. Spec: ADR-1200 D1, "resolved as `setfont` resolves it, in the kbd data directories the build names".
- **A named font with no table and no `unimap=` takes `def.uni`.** `setfont` loaded `def.uni` only for its default font, and left the kernel's map in place for a named one; a set has no kernel map to inherit. Spec: ADR-1200, "Where vtium departs".
- **A unimap entry for a glyph past the font maps nothing.** kbd handed it to the kernel, which kept it.
- **An outline font's cell is as tall as `size=`.** The C reference's FreeType backend made its cell the font's line height (`kmscon/src/font/font_freetype.c:189`), its ascent, descent and line gap scaled and rounded, which can pass the height asked for; a set's cell is the height its frit names, so the frit's sizes are the sizes drawn. Spec: ADR-1200 D1, "`size=` A cell height for outline sources".
- **A glyph is one or two cells wide by its own advance.** The C reference drew each glyph at the width the terminal gave its cell; a set is built before any terminal asks, so a glyph advancing past one and a half cells is stored two cells wide, and `Font` fits it to the width the terminal gives (ADR-1300).

## What this slice leaves for later

- BDF, CPI and raw fonts, `bdf:` sources, `cp-size=`, styles other than regular (an outline font's bold and italic faces, ADR-1000's `text.outline`), the named `cover=` sets and `glyph-source=`: each is refused by name.
- `create` with an outline font first and no `size=`: ADR-1200 D11 picks the size from the display; until then the frit names its sizes.
- ADR-1300's subpixel strikes, and shaping across cells (ligatures, emoji sequences such as flags and families): a set maps single codepoints.
- A collection matched by family needs a file named for that family; a collection of several families is named with `outline:`.
- COLR version 1 glyphs, such as `Noto-COLRv1.ttf`'s, are not drawn in color: swash reads version 0. A COLR layer in the text's color takes swash's mid grey, since a set cannot know the text's color.
- `create --default`, the locale's sets (ADR-1200 D11).
- The parser's account is `nobody`; ADR-1400's pre-login account takes its place once vtium/vtium#31 names it.
- A root run is unverified on the maintainer's machine: the drop to `nobody`, `nobody` executing the binary from its install path, and the real `console:VT` ioctls. Tier 3's guest is where they run.

## Bounds

A frit, a set given to `install` and every source file are opened once, never blocking on a FIFO, refused unless that descriptor is a regular file, and read no further than their bound.

| Bound | Value | Past it |
|---|---|---|
| `FRIT_BYTES_MAX` | 64 KiB | The frit is refused |
| `FRIT_LINE_MAX` | 4096 bytes | The frit is refused, naming the line |
| `FRIT_SOURCES_MAX` | 16 | The frit is refused, naming the line |
| `UNIMAP_ENTRIES_MAX` | 65535, kbd's count | The unimap is refused |
| A font file, a unimap | vtium-font's `PSF_BYTES_MAX` | The source is refused before it is parsed |
| `COMPRESSED_BYTES_MAX` | vtium-font's `GZIP_BYTES_MAX`, 1 MiB, in every compression | The source is refused before it is decoded |
| vconsole.conf, the kernel command line | 64 KiB | The source is refused |
| `GLYPHS_BYTES_MAX` | the largest PSF's glyphs and a map of every codepoint | The parser's answer is refused |
| `PARSER_MEMORY`, `PARSER_SECONDS` | 512 MiB, 30 s | The kernel ends the parser; the source fails |
| `PARSER_WALL_SECONDS` | 60 s | The parent kills the parser; the source fails |
| `FAMILY_ENTRIES_MAX` | 65536 | A `font:` scan of a directory past it fails the set |
| `FONT_DEPTH_MAX` | 8 | A font directory's subdirectories deeper than it are not searched |
| `OUTLINE_BYTES_MAX` | 64 MiB | An outline font file is refused before it is parsed |
| `OUTLINE_ANSWER_BYTES_MAX` | 128 MiB, a strike of GB 2312's ideographs at 48 pixels and room | The parser's answer is refused |
| `FRIT_SIZE_MIN`, `FRIT_SIZE_MAX`, `OUTLINE_CELLS_MAX` | 6 and 96 pixels, 3 sizes | The frit is refused, naming the line |
| A glyph | a set's 16 KiB chunk | Left out: a color glyph two cells wide past about 64 pixels |
| `GLYPHS_MAX`, `MAP_MAX` | 65536 glyphs, a font's 16-bit glyph ids, and no more than the map names; 0x110000 codepoints | The parser's answer is refused |
| `RANGES_MAX` | 4096 `cover=` ranges a request names | The request is refused |
| `NAME_MAX`, `LICENSE_MAX` | 256 bytes of a family or format, 1024 of a licence | A request or answer is refused; a licence is cut there |
| A console font | 32 pixels wide, 64 high, 512 glyphs | The source is refused |
