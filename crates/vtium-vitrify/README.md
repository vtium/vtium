# vtium-vitrify

The program that builds vitrified font sets for vtium (ADR-1200), its own binary outside the minspec build. It reads a recipe, finds and reads the recipe's sources as the invoking user, parses each in an unprivileged child process, lays the glyphs into a set with vtium-font's writer, and writes the set into the root-owned store under its lock, or into a portable file with `create --out`. The console reads only the sets this program writes and its own built-ins; every decoder lives here. The crate forbids unsafe code; the console font ioctls it needs live in vtium-platform's `src/vt/font.rs`.

| Path | What it is |
|---|---|
| `src/main.rs` | The binary: the command line, or the parser when run as `vtium-vitrify --parse`. |
| `src/cli.rs` | The commands, their options and exit codes, `list`'s TSV and JSON. |
| `src/recipe.rs` | Recipes in kmscon.conf's line syntax, read strictly. |
| `src/sources.rs` | Each source found and read: kbd names as `setfont` finds them, files, vconsole.conf and the kernel command line, the kernel's VT font behind the `Consoles` seam, the built-ins. |
| `src/child.rs` | The parser's child process: how it is started, its limits, the request it reads and the answer the parent checks. |
| `src/decode.rs` | The decoders the parser runs: compressions told by their magic, gzip, PSF through vtium-font's reader, and kbd unimaps. |
| `src/glyphs.rs` | The bounded glyph form the parser answers with, and its check. |
| `src/build.rs` | The set from its sources: the cell, the fallbacks fitted into it, the sprite policy, the provenance and the build key. |
| `src/store.rs` | Writing the store: the lock, files synced and renamed, the manifest, unreferenced sets removed. |
| `tests/cli.rs` | The commands run as the invoking user against a store in a test directory, with fonts, a kbd tree and recipes generated in the test. |

`cargo test -p vtium-vitrify` runs every test. The store tests need a directory whose ancestors only root or the invoking user may write, as the store's check requires: Cargo's temporary directory under the workspace, else `XDG_RUNTIME_DIR`, or `VTIUM_FONT_STORE_TEST_DIR`.

## Commands

| Command | Runs as | Does |
|---|---|---|
| `create NAME\|RECIPE...`, `create --all` | root | Builds each recipe into the store. A set whose build key equals its manifest row is `current`; `--force` rebuilds it. |
| `create --kbd NAME`, `--file PATH`, `--vconsole`, `--console VT` | root | Writes the one-source recipe, named for the font (`console` for `--console`), refusing to overwrite a different recipe of that name, and builds it. |
| `create --out FILE RECIPE` | any user | Builds a portable set into FILE. |
| `install FILE...` | root | Checks each set whole, every chunk's hash and its version included, and adds it to the store. |
| `list [--json]` | anyone | The store's sets and the built-ins, as TSV with a header or a JSON array. |
| `verify [NAME...]` | anyone | The console's check of each set, every chunk read. |
| `remove NAME...` | root | Drops each set's row and file; its recipe stays. |

Each build or install prints one line a set, tab-separated: `built`, `current`, `installed`, `removed`, `verified` or `failed`, the name, and for a failure the reason. The exit status is 0 when every set succeeded or was current, 1 when any failed, and 2 on a usage error. A command that writes the system's store refuses without root, naming `sudo`, and exits 1.

The store is `/var/lib/vtium/fonts` and the recipes are in `/etc/vtium/fonts`, each changed at build time with `VTIUM_FONT_STORE` and `VTIUM_FONT_RECIPES`. `--store DIR` names a store the invoking user owns in place of the system's, for tests and for staging; the console reads only the system's. `--recipes DIR`, `--kbd-dir DIR` (repeatable), `--vconsole-conf FILE` and `--kernel-cmdline FILE` replace the other paths the host gives.

## Recipes

One `key=value` a line, blanks around key and value dropped, `#` to the line's end a comment, as kmscon.conf reads; unlike kmscon.conf, an unknown key, a malformed line, a value with a control character or a key this build does not build yet refuses the whole recipe, naming the file and line. This slice reads `name=`, `source=` (`kbd:`, `file:`, `vconsole`, `console:VT`, `builtin:`), `unimap=`, `cover=` (`ascii`, `latin-1`, `U+XXXX` and `U+XXXX-U+YYYY`), `style=regular`, `compress=` and `license=`.

The first source sets the cell. Each later one fills only the codepoints the earlier ones lack, scaled by the largest whole factor at which its cell fits the strike's, on the strike's bottom row and centred across; one whose cell does not fit at factor 1 fills nothing. Bitmap sources give one 1-bit strike, stored raw under either `compress=`, as ADR-1200 D13 decides. The set's sprite policy is `SpritePolicy::default_for` its coverage, the policy the console gave a PSF font it read itself.

The build key is the SHA-256 of the format version, the recipe's bytes, and for each source in order the SHA-256 of the font's bytes as read, of its unimap and of kbd's `def.uni` when read. A built-in source's hash is its name's: its glyphs change only with the tool.

## Sources

- **kbd names** resolve as `setfont` resolves them (kbd's `src/libkbdfile/kbdfile.c`, `src/libkfont/context.c`): in `consolefonts/` of each data directory in turn (`/usr/share/kbd`, `/usr/share`, `/usr/lib/kbd`, or `--kbd-dir`), the name followed by the first of `""`, `.psfu`, `.psf`, `.cp` and `.fnt` for which a file exists, plain before `.gz`, `.bz2`, `.xz` and `.zst`; a name with a `/` looks in the subdirectory its first component names, and an absolute path is taken as given, then with each suffix. Unimaps resolve the same way under `unimaps/` with `""`, `.uni` and `.sfm`.
- **A font's map** is the recipe's `unimap=` when given, else the font's own table, else kbd's `def.uni`; a font with none is refused, naming `unimap=`.
- **vconsole** reads `FONT=` and `FONT_UNIMAP=` from vconsole.conf, with the kernel command line's `vconsole.font=` and `vconsole.font_unimap=` over them, as systemd-vconsole-setup reads them (`src/vconsole/vconsole-setup.c`). With no `FONT=`, systemd ran no `setfont` and the kernel kept its font, so the source is refused and names `console:VT`. `FONT_MAP=` maps the kernel's 8-bit translation and sets no glyph, so it is not read.
- **console:VT** opens `/dev/tty<VT>` read-only as root, checks with `KDGETMODE` that it is in text mode, reads its font with `KD_FONT_OP_GET_TALL`, or `KD_FONT_OP_GET` on a kernel without it, and its map with `GIO_UNIMAP`, and hands the parser a PSF2 of them.
- **Compression** is told by its magic, as kbd tells it, and decoded with its stream's own check value verified: gzip, bzip2, xz and zstd, each refused past `COMPRESSED_BYTES_MAX` before it is decoded and past `PSF_BYTES_MAX` once decoded.

## Decoders

Each is pure Rust, compiles no C and has no build script, and only this binary links it; `cargo tree -p vtium -e normal` lists none of them and the minspec binary's size does not move. The operator approved them on 2026-10-28.

| Format | Crate | Licence | Why this one |
|---|---|---|---|
| gzip | `miniz_oxide` 0.8.9 | MIT, Zlib or Apache-2.0 | Already in every vtium build for the sets' deflated chunks; vitrify frames gzip around it. |
| bzip2 | `bzip2` 0.6.1, on `libbz2-rs-sys` 0.2.5 | MIT or Apache-2.0; `libbz2-rs-sys` under libbzip2's BSD-style licence | Trifecta Tech Foundation's Rust port of libbzip2, the default backend of the most used bzip2 crate, maintained (0.2.5, May 2026), `build = false`; its one C file is a header no build compiles. `bzip2-rs`, decoder only, has had no release since 2021. |
| xz | `lzma-rust2` 0.24.0, features `std` and `xz` | Apache-2.0 | Maintained (October 2026), verifies the stream's check, and with no default features adds only `sha2` and its RustCrypto crates (MIT or Apache-2.0). `lzma-rs` has had no release since January 2023; `xz2` and `liblzma` compile liblzma's C. |
| zstd | `ruzstd` 0.9.1, default features | MIT | The pure-Rust zstd decoder, maintained (October 2026); `hash` adds `twox-hash` (MIT) to verify each frame's checksum. `zstd` binds the C library. It needs Rust 1.87, so this crate's `rust-version` is 1.87 while the console's stay at the workspace's 1.85. |

The tests' xz fixtures take `lzma-rust2`'s encoder as a dev-dependency; bzip2 and zstd fixtures use the encoders the decoding crates carry.

## The parser

`create` and `create --out` read each source as the invoking user and run `vtium-vitrify --parse` for each font: an empty environment, `/` as its directory, its three pipes and no other descriptor. Run as root, the parent starts it as `nobody` from `/etc/passwd`, its supplementary groups dropped, and refuses to build when there is no such account. The parser sets no-new-privileges, an address space of 512 MiB, 30 s of CPU, no file writes, no processes and no core, refuses to run as root, reads its request, parses, and answers with the glyph form, which the parent checks field by field against the PSF bounds before it trusts it. A parser that fails, faults, passes a limit or answers malformed fails that source; `create` reports the set `failed` and goes on.

## Departures

- **A relative kbd name is never looked for in the working directory.** `setfont` tried the name as given first, so a file in the directory it ran from shadowed the data directories. `create` runs unattended, as root, from wherever a package hook runs it. Spec: ADR-1200 D1, "resolved as `setfont` resolves it, in the kbd data directories the build names".
- **A named font with no table and no `unimap=` takes `def.uni`.** `setfont` loaded `def.uni` only for its default font, and left the kernel's map in place for a named one; a set has no kernel map to inherit. Spec: ADR-1200, "Where vtium departs".
- **A unimap entry for a glyph past the font maps nothing.** kbd handed it to the kernel, which kept it.

## What this slice leaves for later

- BDF, CPI and raw fonts, `bdf:` and `outline:` sources, `size=`, `cp-size=`, styles other than regular, the named `cover=` sets and `glyph-source=`: each is refused by name.
- `create --default`, the locale's sets (ADR-1200 D11).
- The parser's account is `nobody`; ADR-1400's pre-login account takes its place once vtium/vtium#31 names it.
- A root run is unverified on the maintainer's machine: the drop to `nobody`, `nobody` executing the binary from its install path, and the real `console:VT` ioctls. Tier 3's guest is where they run.

## Bounds

| Bound | Value | Past it |
|---|---|---|
| `RECIPE_BYTES_MAX` | 64 KiB | The recipe is refused |
| `RECIPE_LINE_MAX` | 4096 bytes | The recipe is refused, naming the line |
| `RECIPE_SOURCES_MAX` | 16 | The recipe is refused, naming the line |
| `UNIMAP_ENTRIES_MAX` | 65535, kbd's count | The unimap is refused |
| A font file, a unimap | vtium-font's `PSF_BYTES_MAX` | The source is refused before it is parsed |
| `COMPRESSED_BYTES_MAX` | vtium-font's `GZIP_BYTES_MAX`, 1 MiB, in every compression | The source is refused before it is decoded |
| vconsole.conf, the kernel command line | 64 KiB | The source is refused |
| `GLYPHS_BYTES_MAX` | the largest PSF's glyphs and a map of every codepoint | The parser's answer is refused |
| `PARSER_MEMORY`, `PARSER_SECONDS` | 512 MiB, 30 s | The kernel ends the parser; the source fails |
| A console font | 32 pixels wide, 64 high, 512 glyphs | The source is refused |
