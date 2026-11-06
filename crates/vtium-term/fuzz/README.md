# vtium-term fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. Run it on nightly from this directory: `cargo +nightly fuzz run screen`.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-term:screen` runs a target from a fresh corpus outside the repository and appends a row to `runs.tsv`, the record of how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/screen.rs` | Any input, parsed by `vtium-parse` into the screen: no panic, and the input cut where its third byte says gives the screen, the replies and the drawn frame it gives whole. Its first two bytes pick the screen's size. Drawing the frame runs each paragraph through the bidirectional algorithm, and the target maps a pointer back to its logical cell on every row (ADR-1004 D8, D10). |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. It builds vtium-term with the `bidi` feature, so the target reaches the paragraph properties, the modes and controls that set them, and the frame's order with the guarded call into unicode-bidi (ADR-1004 D13, D14). |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
