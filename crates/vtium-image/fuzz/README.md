# vtium-image fuzz targets

cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. Run them on nightly from this directory: `cargo +nightly fuzz run sixel` or `cargo +nightly fuzz run kitty`. Each target feeds its handler through a vtium-term terminal built with images, so the decoders the handler runs are fuzzed through the path a program's bytes take.

| Path | What it is |
|---|---|
| `fuzz_targets/sixel.rs` | Any input, as the data of a DCS `q` string the sixel handler takes: no panic, and the string cut where the input's first byte says gives the screen and the replies it gives whole. |
| `fuzz_targets/kitty.rs` | Any input, split at each `|` into the data of APC `G` strings the kitty graphics handler takes, so chunked uploads, puts and deletes arrive in order; the input's second byte gives the terminal an image helper that answers each request at once from the request alone. No panic, and the stream cut where the first byte says gives the screen and the replies it gives whole. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
