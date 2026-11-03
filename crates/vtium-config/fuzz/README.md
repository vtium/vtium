# vtium-config fuzz targets

Two cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. A cluster the helper shapes is text a program wrote to the terminal (ADR-1301 D1), so `shape` drives clusters through HarfRust and swash as the helper does. A BDF font is a file anyone can name in a frit, so `bdf` drives any bytes through vtium-config's own BDF reader (ADR-1200 D14). Run each on nightly from this directory: `cargo +nightly fuzz run shape`, `cargo +nightly fuzz run bdf`.

| Path | What it is |
|---|---|
| `fuzz_targets/shape.rs` | Any input as clusters for the test font of `tests/common/ttf.rs`, or as the font itself asked for a fixed set of clusters, rasterised with each cluster shaped: no panic. Its first byte picks which. |
| `fuzz_targets/bdf.rs` | Any input as a BDF font, plain or compressed: no panic, and each glyph the reader keeps is exactly its cells' rows. A seed corpus of a few small BDF files, such as the head of a real one cut before its `ENDFONT`, finds the reader's paths faster than an empty one. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
