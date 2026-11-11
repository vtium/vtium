# vtium-font fuzz targets

Two cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. Run each on nightly from this directory: `cargo +nightly fuzz run psf`, `cargo +nightly fuzz run set`.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-font:psf` runs a target from a fresh corpus outside the repository and appends a row to `runs.tsv`, the record of how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/psf.rs` | Any bytes as a font file, plain or gzipped: no panic. A font the loader accepts has every glyph drawn at a scale the input picks, plain, bold, italic and bold italic, and the first 0x300 codepoints drawn through a `Font` bold and bold italic, each exactly its cells' size, and each sheared glyph's ink box inside its reach (ADR-1302 D4, D6). |
| `fuzz_targets/set.rs` | Any bytes as a vitrified set: no panic. A set the reader accepts has every chunk checked, and each of its cell sizes drawn at 1 and 2 times its height, the first 0x300 codepoints and the last, in each style, a 1-bit glyph sheared for italic where its strike is not italic, each glyph exactly its cells' size and each ink box inside its reach, and each glyph drawn as superscript and subscript at its own size and cells, with no ink box, a color glyph still premultiplied. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
