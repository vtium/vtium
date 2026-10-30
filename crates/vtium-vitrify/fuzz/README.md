# vtium-vitrify fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. A cluster the helper shapes is text a program wrote to the terminal (ADR-1301 D1), so this drives clusters through HarfRust and swash as the helper does. Run it on nightly from this directory: `cargo +nightly fuzz run shape`.

| Path | What it is |
|---|---|
| `fuzz_targets/shape.rs` | Any input as clusters for the test font of `tests/common/ttf.rs`, or as the font itself asked for a fixed set of clusters, rasterised with each cluster shaped: no panic. Its first byte picks which. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
