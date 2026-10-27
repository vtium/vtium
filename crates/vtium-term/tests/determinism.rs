//! Chunking invariance (docs/architecture.md, "Determinism"): the state
//! after a byte stream does not depend on how the stream was split. Each
//! stream is fed whole, cut at random points, and a byte at a time, and the
//! screens (the cells, the scrollback, the cursor, the modes the screen
//! holds, the damage), the palettes (the colours OSC 4, 10 to 19 and 21
//! set) and the reply bytes must be identical.
//!
//! The streams come from a grammar of what programs write
//! (`common::grammar_stream`) and from raw random bytes
//! (`common::raw_stream`), so the cuts fall inside UTF-8 characters, inside
//! sequences and between a base and its combining marks. A failure prints
//! its seed, its kind and its cuts.

mod common;

use common::{Rng, grammar_stream, raw_stream};
use vtium_term::Terminal;

const COLS: usize = 13;
const ROWS: usize = 5;

fn fresh() -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 20);
    t.screen_mut().set_scrollback_max(20);
    t
}

/// Feeds `stream` in the pieces `cuts` makes, every byte taken.
fn fed(stream: &[u8], cuts: &[usize]) -> Terminal {
    let mut t = fresh();
    let mut from = 0;
    for &cut in cuts.iter().chain([&stream.len()]) {
        let piece = &stream[from..cut];
        assert_eq!(t.input(piece), piece.len(), "the terminal paused");
        from = cut;
    }
    t
}

/// The state chunking must not change: the screen, the palette and the
/// replies.
fn same(a: &Terminal, b: &Terminal) -> bool {
    a.screen() == b.screen() && a.palette() == b.palette() && a.replies() == b.replies()
}

fn check(seed: u64, kind: &str, stream: &[u8], rng: &mut Rng) {
    let whole = fed(stream, &[]);
    let mut cuts: Vec<usize> = (0..1 + rng.below(8))
        .map(|_| rng.below(stream.len() + 1))
        .collect();
    cuts.sort_unstable();
    let cut = fed(stream, &cuts);
    assert!(
        same(&whole, &cut),
        "seed {seed}, {kind}: cut at {cuts:?} the state differs from the stream fed whole"
    );
    let bytewise: Vec<usize> = (1..stream.len()).collect();
    let each = fed(stream, &bytewise);
    assert!(
        same(&whole, &each),
        "seed {seed}, {kind}: fed a byte at a time the state differs from the stream fed whole"
    );
}

// docs/architecture.md, "Determinism", chunking invariance.
#[test]
fn a_stream_cut_anywhere_gives_the_state_it_gives_whole() {
    for seed in 0..common::seeds(1000) {
        let mut rng = Rng(seed);
        let len = 1 + rng.below(400);
        let grammar = grammar_stream(&mut rng, len);
        check(seed, "grammar", &grammar, &mut rng);
        let raw = raw_stream(&mut rng, len);
        check(seed, "raw", &raw, &mut rng);
    }
}

// A cut between a base and the marks that join it, and inside the UTF-8 of
// each, keeps the cluster whole.
#[test]
fn a_cut_inside_a_cluster_keeps_it_whole() {
    let stream = "e\u{301}\u{302}\u{2764}\u{FE0F}\u{1F468}\u{200D}\u{1F469}x".as_bytes();
    let whole = fed(stream, &[]);
    for cut in 1..stream.len() {
        assert!(
            whole.screen() == fed(stream, &[cut]).screen(),
            "cut at {cut}"
        );
    }
}

// docs/architecture.md, "Determinism": a synchronized update's end pauses
// the input call, which changes when work is done, never what state
// results. With the host presenting by mode 2026, each piece is fed until
// the terminal has taken all of it.
#[test]
fn a_synchronized_updates_pause_changes_no_state() {
    let fed_through = |stream: &[u8], cuts: &[usize]| {
        let mut t = fresh();
        t.set_sync_enabled(true);
        let mut from = 0;
        for &cut in cuts.iter().chain([&stream.len()]) {
            let mut piece = &stream[from..cut];
            while !piece.is_empty() {
                piece = &piece[t.input(piece)..];
            }
            from = cut;
        }
        t
    };
    for seed in 0..common::seeds(300) {
        let mut rng = Rng(seed);
        let mut stream = Vec::new();
        for _ in 0..1 + rng.below(6) {
            stream.extend_from_slice(b"\x1b[?2026h");
            let len = 1 + rng.below(80);
            stream.extend(grammar_stream(&mut rng, len));
            stream.extend_from_slice(b"\x1b[?2026l");
        }
        let whole = fed_through(&stream, &[]);
        assert!(whole.sync_frames() > 0, "seed {seed}: no hold ended");
        let bytewise: Vec<usize> = (1..stream.len()).collect();
        let each = fed_through(&stream, &bytewise);
        assert!(
            same(&whole, &each) && whole.sync_frames() == each.sync_frames(),
            "seed {seed}: fed a byte at a time the state differs from the stream fed whole"
        );
    }
}
