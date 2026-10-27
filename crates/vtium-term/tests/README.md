# vtium-term integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-term`. The unit tests of each type sit in its module under `src/`.

| Path | What it is |
|---|---|
| `memory.rs` | The resident size of the cell model on the minspec machine, 170 by 48 cells, main and alternate screens, with the default scrollback and each cap, for common lines and the worst a program can write, computed from the structures by capacity and pinned; ADR-1001's "Memory" quotes the figures. It also checks the worst case at the default fits the minspec peak. `-- --nocapture` prints them. |
| `common/mod.rs` | What the tests share: a seeded generator, byte streams from a grammar of what programs write and from raw random bytes, and the screen as text. `VTIUM_TERM_SEEDS` sets how many seeds the property tests run. |
| `screen.rs` | The screen driven through the terminal: each control function the screen carries (ADR-1000's rows `ctl.edit`, `ctl.tabs`, `ctl.decstbm`, `ctl.bce`, `ctl.ed3`), clusters and widths (`text.grapheme`, `text.vs-width`, `DEC-2027`), REP (`text.rep`), the selection (`clipboard.selection`), resizing, the alternate screen, the scrollback and its view. |
| `departures.rs` | Each departure from the C reference the crate's README lists, with the case that tells the two apart. |
| `damage.rs` | The property test of change tracking (aaronsb/libtsm#48): after any change, through the byte stream or the screen's own calls, every cell that draws differently is marked changed. |
| `determinism.rs` | Chunking invariance (docs/architecture.md, "Determinism"): a stream from the grammar or of raw bytes, fed whole, cut at random points and fed a byte at a time, gives identical screens and replies; a failure prints its seed. |
| `flood.rs` | The cluster arena's bound on every path that writes, moves or erases cells: a flood of long clusters through each, the bytes the cells name summed and checked against the line's account and its budget after every step. |
