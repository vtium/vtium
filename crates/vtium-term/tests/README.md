# vtium-term integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-term`. The unit tests of each type sit in its module under `src/`.

| Path | What it is |
|---|---|
| `memory.rs` | The resident size of the cell model on the minspec machine, 170 by 48 cells, main and alternate screens, with the default scrollback and each cap, for common lines and the worst a program can write, computed from the structures by capacity and pinned; ADR-1001's "Memory" quotes the figures. It also checks the worst case at the default fits the minspec peak. `-- --nocapture` prints them. |
| `common/mod.rs` | What the tests share: a seeded generator, byte streams from a grammar of what programs write and from raw random bytes, and the screen as text. `VTIUM_TERM_SEEDS` sets how many seeds the property tests run. |
| `screen.rs` | The screen driven through the terminal: each control function the screen carries (ADR-1000's rows `ctl.edit`, `ctl.tabs`, `ctl.decstbm`, `ctl.bce`, `ctl.ed3`), clusters and widths (`text.grapheme`, `text.vs-width`, `DEC-2027`), REP (`text.rep`), the selection (`clipboard.selection`), resizing, the alternate screen, the scrollback and its view. |
| `modes.rs` | The mode table driven through the byte stream: each supported mode set, reset and reported by DECRQM, with its effect; the ignored and refused rows at their fixed state; DECSTR and RIS. The sweep of every number with no row, the generated documents and the table's agreement with ADR-1000 are unit tests in `src/terminal/modes.rs`. |
| `palettes.rs`, `palettes.c.txt` | Each named palette against the C reference's tables, which `palettes.c.txt` holds verbatim from libtsm at the conformance pin; the cube and the grey ramp every palette shares; the custom palette. |
| `sgr.rs` | SGR: each attribute set and cleared, the palette and extended colours in both forms, the underline styles and colour, and the forms vtium ignores. |
| `charsets.rs` | The character sets: SCS into G0 to G3, SO and SI, the locking and single shifts, DEC special and supplemental graphics, GR in 8-bit input, a decoded UTF-8 character printing as itself (vtium/vtium#13), and DECSC, DECSTR and RIS around them. |
| `replies.rs` | The replies and the control functions around them: DA1 to DA3, DSR, XTVERSION, XTGETTCAP at its bounds, DECRQSS, the colour queries, XTWINOPS, DECSCUSR, DECSC and DECRC, DECKPAM, DECSCL and the C1 announcers, and the reports, control strings and OSC strings vtium ignores or refuses. The reply rule's test, with hostile text in every field, is a unit test in `src/terminal/reply.rs`. |
| `colours.rs` | The colours a program sets, queries and resets: OSC 4 and 104, OSC 10 to 19 and 110 to 119, kitty's OSC 21, the colour specifications they read, the cells they mark changed, DECSTR, RIS and the host's palette, and the replies' bound, past which a list's sets still apply. |
| `clipboard.rs` | OSC 52: the selection a program sets, its targets, strict base64, the cap and one byte over it, the query that gets no reply, truncated and cancelled strings, and a set cut at every offset. |
| `input.rs` | What keys and pastes send the program: the legacy encoding with Ctrl's control bytes, modifyOtherKeys, and pastes filtered as xterm filters them. |
| `departures.rs` | Each departure from the C reference the crate's README lists, with the case that tells the two apart. |
| `damage.rs` | The property test of change tracking (aaronsb/libtsm#48): after any change, through the byte stream or the screen's own calls, every cell that draws differently is marked changed. |
| `determinism.rs` | Chunking invariance (docs/architecture.md, "Determinism"): a stream from the grammar or of raw bytes, fed whole, cut at random points and fed a byte at a time, gives identical screens, palettes and replies, and a synchronized update's pause changes no state; a failure prints its seed. |
| `flood.rs` | The cluster arena's bound on every path that writes, moves or erases cells: a flood of long clusters through each, the bytes the cells name summed and checked against the line's account and its budget after every step. |
