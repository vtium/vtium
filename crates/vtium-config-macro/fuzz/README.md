# vtium-config-macro fuzz target

A cargo-fuzz target in its own workspace, so the main workspace and CI never build it or pull `libfuzzer-sys`. A macro may come over ssh from another machine, and vtium-config parses it before the first step plays, so the parser runs under fuzzing (ADR-1208 D7). The target depends on this crate. Run it on nightly from this directory: `cargo +nightly fuzz run parse corpus/parse seeds/parse`. Your own corpus directory comes first and the seeds after it.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-config-macro:parse` runs the target from a fresh corpus outside the repository, with the seeds copied in. It appends a row to `runs.tsv`, which records how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/parse.rs` | Any input as a macro, read whole and line by line. The target checks that nothing panics. A macro that parses writes canonical text that parses back into the same macro and is its own canonical text. A macro that fails names a line the input has, in one line with no control character, and reading it line by line stops at the same line with the same message. It reaches the size bounds, the byte-order mark, UTF-8, the control characters, the comment rule, every step and its errors, the modifiers and the keys, the counts, the quotes, the labels' spacing, the pauses, the names, `Set Live`'s place, and the writer of every step. |
| `seeds/parse/` | The two example macros of ADR-1208 and a macro with every step. |
| `runs.tsv` | One row for each run of `make fuzz`. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
