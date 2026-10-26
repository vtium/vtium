# vtium-parse fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. Run it on nightly from this directory: `cargo +nightly fuzz run parse`.

| Path | What it is |
|---|---|
| `fuzz_targets/parse.rs` | Any input: no panic, the integration tests' recorder checks every bound at every call, and the input split where its first two bytes say, and paused, gives the actions it gives whole. Its first byte also picks the input mode. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
