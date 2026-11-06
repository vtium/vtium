# vtium-settings fuzz targets

Two cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. vtium.conf is text anyone with root can write and any user can hand `vtium check-config`, so their readers are the crate's input surface. `reading` drives any bytes through the section reader of vtium.conf, read whole and as a flow (ADR-1401 D7, ADR-1403 D3), and `split` drives any bytes through the split at the first section heading and the top level's line editor (ADR-1403 D8). Run each on nightly from this directory: `cargo +nightly fuzz run reading`, `cargo +nightly fuzz run split`.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-settings:reading` runs a target from a fresh corpus outside the repository and appends a row to `runs.tsv`, the record of how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/reading.rs` | Any input as vtium.conf, read whole and as a flow: no panic, every rule that survives sends its devices to a head that survives, and the section reader names no error on a line that holds one of vtium's own top-level keys. It reaches the line checks, the policy line, every section's reader with the key table's values and bounds, the rules and their match tokens, and what each error drops. |
| `fuzz_targets/split.rs` | Any input as vtium.conf: no panic, the top level and the sections are the whole file in order, the top level holds no heading and numbers its lines up to the first heading, and setting a key with the line editor leaves the sections byte for byte while vtium takes the new value. It reaches the split, the one comment rule and the line editor. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
