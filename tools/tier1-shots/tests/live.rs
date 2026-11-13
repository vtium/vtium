//! The live path against the picture path: for a case, the screens the
//! live path writes, read back through a terminal, draw the pictures the
//! picture path draws, block by block; and `tier1-play`, run in a pty of
//! the test's own, writes those screens in order and waits for a key
//! after each.
#![cfg(feature = "draw")]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use vtium_conformance::runner::Corpus;
use vtium_conformance::view::View;
use vtium_platform::event_loop::Loop;
use vtium_platform::pty::{Consumer, Erase, Pty, Replies, Size};
use vtium_tier1_shots::shoot::{self, Screen};
use vtium_tier1_shots::{CONFORMANCE, root};

/// The case `name` of the set `set`, replayed once, every block's screen
/// taken from the same terminal by both paths.
fn screens(set: &str, name: &str) -> Vec<Screen> {
    let corpus = Corpus::open(&root().join(CONFORMANCE).join(set)).unwrap();
    let case = corpus.cases.iter().find(|c| c.name == name).unwrap();
    let input = corpus.input(case, View::Screen).unwrap();
    let (out, screens) = shoot::replay(&input, None);
    out.unwrap();
    assert_eq!(
        screens.len(),
        input.cuts.len() + 1,
        "a screen for each block"
    );
    screens.into_values().collect()
}

// iTerm2 pictures over text, sixels between margins, kitty relative
// placements, a picture taller than the screen, pictures drawn at other
// sizes, a sixel made in cells of 6x13 and drawn in cells of 8x16, SGR's attributes, clusters written under each width method,
// and a wrapped right-to-left paragraph: each block the live path writes
// reads back as the picture
#[test]
fn each_screen_the_live_path_writes_reads_back_as_the_picture_of_its_block() {
    for (set, name) in [
        ("vtium", "iterm2-place"),
        ("vtium", "sixel-margins-cursor"),
        ("vtium", "kit-relative"),
        ("vtium", "kit-g14-tall-cursor"),
        ("corpus", "kit-place-size"),
        ("corpus", "six-cursor-xterm"),
        ("corpus", "sgr-basic"),
        ("corpus", "width-methods"),
        ("vtium", "bidi-08-wrap"),
    ] {
        for (k, screen) in screens(set, name).iter().enumerate() {
            let picture = screen.picture.as_ref().unwrap();
            let live = screen.live_picture().unwrap();
            assert!(
                *picture == live,
                "{set}/{name}, screen {}: the live path draws another picture",
                k + 1
            );
        }
    }
}

// the live play of a case stops at every checkpoint, prog-vim's among
// them, and writes each block's screen in the replay's order: the screens
// that, read back through a terminal, draw each block's picture
#[test]
fn the_live_play_writes_every_blocks_screen_in_order_and_waits_after_each() {
    for (set, name) in [("corpus", "prog-vim"), ("vtium", "iterm2-place")] {
        let corpus = Corpus::open(&root().join(CONFORMANCE).join(set)).unwrap();
        let case = corpus.cases.iter().find(|c| c.name == name).unwrap();
        let screens = screens(set, name);
        let mut out = Vec::new();
        let mut waits = 0;
        let done = vtium_tier1_shots::play::case(&corpus, case, set, &mut out, &mut || {
            waits += 1;
            vtium_conformance::scenario::play::Step::Next
        })
        .unwrap();
        assert!(done);
        assert_eq!(waits, screens.len(), "{name}: a wait after each screen");
        let mut at = 0;
        for (k, screen) in screens.iter().enumerate() {
            // the screen's picture is what its bytes read back as
            assert!(screen.picture.as_ref().ok() == screen.live_picture().ok().as_ref());
            at = find(&out, &screen.live, at)
                .unwrap_or_else(|| panic!("{name}: screen {} is not where it follows", k + 1))
                + screen.live.len();
        }
    }
}

/// What the pty gives back: every byte the player wrote, and whether it
/// has exited.
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    exited: bool,
}

impl Consumer for Output {
    fn output(&mut self, output: &[u8], _: &mut Replies<'_>) -> usize {
        self.bytes.extend_from_slice(output);
        output.len()
    }

    fn exited(&mut self, _: std::process::ExitStatus) {
        self.exited = true;
    }
}

struct Host {
    pty: Option<Pty>,
    output: Output,
}

fn parts(h: &mut Host) -> Option<(&mut Pty, &mut Output)> {
    let output = &mut h.output;
    h.pty.as_mut().map(|p| (p, output))
}

/// The loop's turns a wait may take, each of at most a tenth of a second:
/// the bound only makes a stuck player fail the test.
const TURNS: usize = 600;

/// Turns the loop until `done` holds of the output, and fails when the
/// turns run out first.
fn until(lp: &mut Loop<Host>, h: &mut Host, what: &str, done: impl Fn(&Output) -> bool) {
    for _ in 0..TURNS {
        if done(&h.output) {
            return;
        }
        lp.dispatch(Some(Duration::from_millis(100)), h).unwrap();
    }
    panic!(
        "the player never {what}:\n{}",
        String::from_utf8_lossy(&h.output.bytes)
    );
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

/// Where `needle` first is in `hay` at or after `from`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// `bytes` as the pty's output processing writes them: each line feed
/// after a carriage return (ONLCR).
fn onlcr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

// tier1-play writes screen 1, waits, writes screen 2 after a key, waits,
// and stops at q before screen 3: the bytes are the live path's for each
// block, the ones the test above reads back as the pictures
#[test]
fn tier1_play_writes_each_screen_in_a_pty_and_waits_for_a_key() {
    const HINT: &[u8] = b"(any key: next screen, q: stop)";
    let screens = screens("vtium", "iterm2-place");
    let bin = Path::new(env!("CARGO_BIN_EXE_tier1-play"));
    let mut command = Command::new(bin);
    command.args(["case", "iterm2-place"]);
    let size = Size {
        cols: 80,
        rows: 40,
        cell_width: 8,
        cell_height: 16,
    };
    let mut lp: Loop<Host> = Loop::new().unwrap();
    let mut pty = Pty::spawn(command, size, Erase::Delete).unwrap();
    pty.insert(&lp.handle(), parts).unwrap();
    let mut h = Host {
        pty: Some(pty),
        output: Output::default(),
    };
    until(&mut lp, &mut h, "showed screen 1", |o| {
        count(&o.bytes, HINT) == 1
    });
    let first = onlcr(&screens[0].live);
    until(&mut lp, &mut h, "wrote screen 1 whole", |o| {
        find(&o.bytes, &first, 0).is_some()
    });
    assert_eq!(count(&h.output.bytes, b"screen 2 of 3"), 0, "it waits");
    h.pty.as_mut().unwrap().write(b" ");
    until(&mut lp, &mut h, "showed screen 2", |o| {
        count(&o.bytes, HINT) == 2
    });
    let second = onlcr(&screens[1].live);
    until(&mut lp, &mut h, "wrote screen 2 whole", |o| {
        find(&o.bytes, &second, 0).is_some()
    });
    let at = find(&h.output.bytes, &first, 0).unwrap();
    assert!(
        find(&h.output.bytes, &second, at + first.len()).is_some(),
        "screen 2 follows screen 1"
    );
    h.pty.as_mut().unwrap().write(b"q");
    until(&mut lp, &mut h, "exited after q", |o| o.exited);
    assert_eq!(count(&h.output.bytes, b"screen 3 of 3"), 0, "q stops it");
}
