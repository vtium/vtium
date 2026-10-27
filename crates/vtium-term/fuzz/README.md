# vtium-term fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. Run it on nightly from this directory: `cargo +nightly fuzz run screen`.

| Path | What it is |
|---|---|
| `fuzz_targets/screen.rs` | Any input, parsed by `vtium-parse` into the screen: no panic, and the input cut where its third byte says gives the screen and the replies it gives whole. Its first two bytes pick the screen's size. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
