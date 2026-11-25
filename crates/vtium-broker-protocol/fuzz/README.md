# vtium fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. The broker runs as root and reads every message the drawer sends, so the reader of those messages runs under fuzzing (ADR-1400 D4). The reader is `src/broker/protocol.rs`, a module of the vtium binary, and the target includes that file. Run the target on nightly from this directory: `cargo +nightly fuzz run protocol corpus/protocol seeds/protocol`. Your own corpus directory comes first and the seeds after it.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium:protocol` runs the target from a fresh corpus outside the repository, with the seeds copied in. It appends a row to `runs.tsv`, which records how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/protocol.rs` | Any input as a request from the drawer and as an order from the broker: no panic, and every message either reader accepts encodes back to the same bytes. The answer that the broker gives a refused request is an order the drawer accepts. |
| `seeds/protocol/` | Whole messages to start from, one of each layout: ready, opening a device, opening a session, starting an image helper, switching VT, a power key, a device's descriptor, a refused pty, a session killed by a signal, and stop. The unit test `the_fuzz_seeds_are_accepted_messages` checks that each one is accepted. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
