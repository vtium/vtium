//! Any input: no panic, the recorder's bound checks hold at every call,
//! and the input cut where its first two bytes say, and paused, gives the
//! actions it gives whole.
#![no_main]

#[path = "../../tests/common/mod.rs"]
mod common;

use common::{Act, Recorder};
use libfuzzer_sys::fuzz_target;
use vtium_parse::{InputMode, Parser};

fn run(mode: InputMode, pieces: &[&[u8]], pause: bool) -> Vec<Act> {
    let mut parser = Parser::new();
    let mut rec = Recorder {
        mode,
        follow_decscl: true,
        ..Recorder::new()
    };
    if pause {
        rec.pause_on = Some(|a| format!("{a:?}").len() % 2 == 0);
    }
    for piece in pieces {
        rec.feed(&mut parser, piece);
    }
    rec.acts
}

fuzz_target!(|data: &[u8]| {
    let [sel, cut, input @ ..] = data else {
        return;
    };
    let mode = match sel % 3 {
        0 => InputMode::Utf8,
        1 => InputMode::EightBit,
        _ => InputMode::SevenBit,
    };
    let whole = run(mode, &[input], false);
    let cut = usize::from(*cut).min(input.len());
    let (a, b) = input.split_at(cut);
    assert_eq!(run(mode, &[a, b], false), whole);
    assert_eq!(run(mode, &[input], true), whole);
});
