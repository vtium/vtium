# vtium-font fuzz targets

Two cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. Run each on nightly from this directory: `cargo +nightly fuzz run psf`, `cargo +nightly fuzz run set`.

| Path | What it is |
|---|---|
| `fuzz_targets/psf.rs` | Any bytes as a font file, plain or gzipped: no panic. A font the loader accepts has every glyph drawn at a scale the input picks, plain and bold, and the first 0x300 codepoints drawn through a `Font`, each exactly its cells' size. |
| `fuzz_targets/set.rs` | Any bytes as a vitrified set: no panic. A set the reader accepts has every chunk checked, and each of its cell sizes drawn at 1 and 2 times its height, the first 0x300 codepoints and the last, plain and bold, each glyph exactly its cells' size. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
