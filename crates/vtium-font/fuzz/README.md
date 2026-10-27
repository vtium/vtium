# vtium-font fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. Run it on nightly from this directory: `cargo +nightly fuzz run psf`.

| Path | What it is |
|---|---|
| `fuzz_targets/psf.rs` | Any bytes as a font file, plain or gzipped: no panic. A font the loader accepts has every glyph drawn at a scale the input picks, plain and bold, and the first 0x300 codepoints drawn through a `Font`, each exactly its cells' size. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
