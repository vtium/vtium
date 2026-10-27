# vtium-term integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-term`. The unit tests of each type sit in its module under `src/`.

| Path | What it is |
|---|---|
| `memory.rs` | The resident size of the cell model on the minspec machine, 170 by 48 cells, main and alternate screens, with the default scrollback and each cap, for common lines and the worst a program can write, computed from the structures by capacity and pinned; ADR-1001's "Memory" quotes the figures. It also checks the worst case at the default fits the minspec peak. `-- --nocapture` prints them. |
