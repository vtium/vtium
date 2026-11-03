# vtium-image fuzz targets

cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. Run them on nightly from this directory: `cargo +nightly fuzz run sixel` or `cargo +nightly fuzz run kitty`. Each target feeds its handler through a vtium-term terminal built with images, so the decoders the handler runs are fuzzed through the path a program's bytes take.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-image:sixel` runs a target from a fresh corpus outside the repository and appends a row to `runs.tsv`, the record of how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/sixel.rs` | Any input, as the data of a DCS `q` string the sixel handler takes, after the sixel modes, a scroll region, the cursor's place and the string's parameters the input's second byte chooses: no panic, and the stream cut where the input's first byte says gives the screen, its pictures' pixels and the replies it gives whole. |
| `fuzz_targets/kitty.rs` | Any input, split at each `|` into the data of APC `G` strings the kitty graphics handler takes, so chunked uploads, puts and deletes arrive in order; the input's second byte gives the terminal an image helper that answers each request at once from the request alone. No panic, and the stream cut where the first byte says gives the screen and the replies it gives whole. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
