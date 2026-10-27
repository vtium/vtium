# vtium-render integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-render`. Every test cites the C reference line, issue or record its expectation comes from.

| Path | What it is |
|---|---|
| `common/mod.rs` | Screens written through `vtium-term`'s lines, the frames a display reads from them, the reference screens, the digest, and `WriteOnly`, a target that keeps what it is given. |
| `reference.rs` | Whole screens against digests of what the C reference's code drew (`reference/README.md`). |
| `cache.rs` | ADR-1300's invariance: frames drawn at every cache size and storage give the same bytes, past a full cache included. |
| `damage.rs` | What each frame redraws and reports, under the displays' ages: one, two and three buffers, swaps that fail and buffers lost, each frame against a whole draw in every orientation; age 0, an age past the history, a full redraw, a keystroke, the damage merge. |
| `orientation.rs` | The four orientations against each other, the wide glyph cut at the margin, and the row scratch's bands. |
| `cursor.rs` | The block, underline and bar cursors, OSC 12's colour, and the pointer. |
| `write_only.rs` | A buffer of garbage and one of zeros hold the same frames; every write lies inside the display. |
| `memory.rs` | ADR-1300's memory table, from caches filled to their budgets. |
| `figures.rs` | The render timer's figures at 1366x768, printed. |
| `reference/` | The C reference's digests, the screens they draw and the program that takes them. |
