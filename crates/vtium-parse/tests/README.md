# vtium-parse integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-parse`. A test cites the specification its expected actions come from, or the rule in the crate's README that it checks, where no specification covers the case.

| Path | What it is |
|---|---|
| `common/mod.rs` | A consumer that records every action and checks, at each call, the bounds and ranges the parser promises. |
| `state_machine.rs` | Each state's transitions, from vt100.net's DEC parser, and the rules the crate's README gives where vtium chooses between readings of a specification. |
| `bounds.rs` | Every bound at its edge, and what happens past it. |
| `xtgettcap.rs` | XTGETTCAP collection: how the parser takes a request's payload, ends it and bounds it. |
| `input.rs` | UTF-8, 8-bit and 7-bit input, mode changes mid-input, and pausing. |
| `random.rs` | Seeded random and adversarial streams: no panic, every bound holds, and a stream cut anywhere, fed a byte at a time or paused gives the actions and the end state it gives whole, with controls that the cuts fall inside UTF-8 characters, headers and STs. The crate's chunking-invariance gate (docs/architecture.md, "Determinism"). |
