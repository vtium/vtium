# vtium-parse integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-parse`. Every test cites the C reference line, libtsm test or specification its expected actions come from; a test of a departure says so and names the specification.

| Path | What it is |
|---|---|
| `common/mod.rs` | A consumer that records every action and checks, at each call, the bounds and ranges the parser promises. |
| `state_machine.rs` | Each state's transitions, from the C reference's state machine and libtsm's test_vte cases, and the departures from them. |
| `bounds.rs` | Every bound at its edge, and what happens past it. |
| `xtgettcap.rs` | XTGETTCAP collection, from libtsm's `test_vte_xtgettcap_*` cases. |
| `input.rs` | UTF-8, 8-bit and 7-bit input, mode changes mid-input, and pausing. |
| `random.rs` | Seeded random and adversarial streams: no panic, every bound holds, and a stream cut anywhere, fed a byte at a time or paused gives the actions it gives whole. The crate's chunking-invariance gate (docs/architecture.md, "Determinism"). |
