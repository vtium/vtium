//! Tests of the terminal's one-step write of a run of printable ASCII
//! against the pauses that stop an input call. The parser hands the
//! terminal a whole run in one call, so the terminal must stop inside the
//! run at the byte where it would have stopped for each byte alone. Only a
//! print that cuts a picture can pause the terminal, through the image
//! work bound. These tests check both halves of that claim.

use vtium_parse::{Chunk, Consumer, InputMode, Intermediates, Params, StringEnd};

use super::{State, Terminal};

/// Passes every call to the terminal's state and counts the calls that
/// leave the terminal paused when it was not paused before.
struct Watch<'a> {
    state: &'a mut State,
    prints: usize,
    print_pauses: usize,
    other_pauses: usize,
}

impl Watch<'_> {
    fn other(&mut self, act: impl FnOnce(&mut State)) {
        let before = self.state.paused();
        act(self.state);
        if !before && self.state.paused() {
            self.other_pauses = self.other_pauses.saturating_add(1);
        }
    }
}

impl Consumer for Watch<'_> {
    fn print(&mut self, c: char) {
        let before = self.state.paused();
        self.state.print(c);
        self.prints = self.prints.saturating_add(1);
        if !before && self.state.paused() {
            self.print_pauses = self.print_pauses.saturating_add(1);
        }
    }
    fn execute(&mut self, control: u8) {
        self.other(|s| s.execute(control));
    }
    fn esc_dispatch(&mut self, i: Intermediates, f: u8) {
        self.other(|s| s.esc_dispatch(i, f));
    }
    fn csi_dispatch(&mut self, p: &Params, i: Intermediates, f: u8) {
        self.other(|s| s.csi_dispatch(p, i, f));
    }
    fn osc_end(&mut self, data: &[u8], truncated: bool, end: StringEnd) {
        self.other(|s| s.osc_end(data, truncated, end));
    }
    fn dcs_hook(&mut self, p: &Params, i: Intermediates, f: u8) {
        self.other(|s| s.dcs_hook(p, i, f));
    }
    fn dcs_put(&mut self, data: Chunk<'_>) {
        self.other(|s| s.dcs_put(data));
    }
    fn dcs_put_run(&mut self, run: &[u8]) -> usize {
        let mut taken = 0;
        self.other(|s| taken = s.dcs_put_run(run));
        taken
    }
    fn dcs_unhook(&mut self, end: StringEnd) {
        self.other(|s| s.dcs_unhook(end));
    }
    fn apc_start(&mut self) {
        self.other(State::apc_start);
    }
    fn apc_put(&mut self, data: Chunk<'_>) {
        self.other(|s| s.apc_put(data));
    }
    fn apc_put_run(&mut self, run: &[u8]) -> usize {
        let mut taken = 0;
        self.other(|s| taken = s.apc_put_run(run));
        taken
    }
    fn apc_end(&mut self, end: StringEnd) {
        self.other(|s| s.apc_end(end));
    }
    fn xtgettcap(&mut self, payload: Option<&[u8]>) {
        self.other(|s| s.xtgettcap(payload));
    }
    fn input_mode(&self) -> InputMode {
        self.state.input_mode()
    }
    fn paused(&self) -> bool {
        self.state.paused()
    }
}

/// Sequences that pause the terminal while the host allows them, and
/// others that change what printing does.
const PIECES: &[&[u8]] = &[
    b"\x1b[?2026h",
    b"\x1b[?2026l",
    b"\x1b]50;fixed\x07",
    b"\x1b]setBackground\x07",
    b"\x1b[8;5;20t",
    b"\x1b[4h",
    b"\x1b[4l",
    b"\x1b(0",
    b"\x1b(B",
    b"\x1bN",
    b"\x1b#6",
    b"\x1b]66;s=2;A\x07",
    b"\x1b[1;31m",
    b"\r\n",
    b"\x1b[H",
    "\u{4E2D}\u{301}".as_bytes(),
];

/// A small splitmix64 generator, so the streams are the same every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let n = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from((z ^ (z >> 31)).checked_rem(n).unwrap_or(0)).unwrap_or(0)
    }
}

fn host_operations_on(t: &mut Terminal) {
    t.set_sync_enabled(true);
    t.state.host.font_ops = true;
    t.state.host.display_ops = true;
    t.state.host.window_ops = true;
}

// A print never pauses the terminal while no picture can be cut, so a run
// of text never needs to stop halfway. The sequences that pause it do, in
// the same streams, so the watch sees pauses where they happen.
#[test]
fn a_print_never_pauses_the_terminal() {
    let (mut prints, mut other_pauses) = (0, 0);
    for seed in 0..300 {
        let mut rng = Rng(seed);
        let mut t = Terminal::new(1 + rng.below(20), 1 + rng.below(6), 10);
        host_operations_on(&mut t);
        let mut stream = Vec::new();
        while stream.len() < 600 {
            if rng.below(2) == 0 {
                let len = 1 + rng.below(50);
                stream.extend((0..len).map(|_| {
                    let b = u8::try_from(rng.below(0x5f)).unwrap_or(0);
                    b.saturating_add(0x20)
                }));
            } else {
                let piece = PIECES.get(rng.below(PIECES.len()));
                stream.extend_from_slice(piece.copied().unwrap_or_default());
            }
        }
        let mut watch = Watch {
            state: &mut t.state,
            prints: 0,
            print_pauses: 0,
            other_pauses: 0,
        };
        let mut rest: &[u8] = &stream;
        while !rest.is_empty() {
            // As `Terminal::input` starts each call, with the host having
            // taken each request.
            watch.state.pause = false;
            watch.state.out.clear();
            watch.state.font_request = None;
            watch.state.display_request = None;
            watch.state.grid_request = None;
            let taken = t.parser.advance(&mut watch, rest);
            assert!(taken > 0, "seed {seed}: the parser took nothing");
            rest = rest.get(taken..).unwrap_or_default();
        }
        assert_eq!(
            watch.print_pauses, 0,
            "seed {seed}: a print paused the terminal"
        );
        prints += watch.prints;
        other_pauses += watch.other_pauses;
    }
    assert!(prints > 10_000, "only {prints} prints");
    assert!(
        other_pauses > 100,
        "only {other_pauses} pauses after a sequence"
    );
}

// Text written over a sixel picture counts image work, and the work bound
// can fall inside a run. The terminal then stops after the byte that
// passed the bound, as it would if each byte came alone, and takes the
// rest of the run in the next call.
#[cfg(feature = "images")]
#[test]
fn text_over_a_picture_stops_at_the_work_bound_inside_a_run() {
    use crate::bounds::INPUT_WORK_MAX;
    use crate::images::{Sixel, Switches};
    use crate::terminal::ImageString;

    let picture = b"\x1bPq#1;2;100;0;0#1~~~~~~~~~~~~-~~~~~~~~~~~~\x1b\\\x1b[H";
    let run = b"abcdefghij";
    let stopped = |whole: bool| {
        let mut t = Terminal::new(20, 4, 10);
        t.screen_mut().set_image_limit(1 << 20);
        assert!(t.screen_mut().set_image_cell(4, 6).is_ok(), "a cell");
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        assert_eq!(t.input(picture), picture.len());
        assert!(t.screen().images().has_cuttable(), "no picture to cut");
        t.state.screen.take_image_work();
        // Each cut counts at least one visit, so the fourth byte passes
        // the bound at the latest.
        t.state.screen.add_image_work(INPUT_WORK_MAX - 3);
        let taken = if whole {
            t.parser.advance(&mut t.state, run)
        } else {
            let mut n = 0;
            while n < run.len() && !t.state.paused() {
                let byte = run.get(n..=n).unwrap_or_default();
                n = n.saturating_add(t.parser.advance(&mut t.state, byte));
            }
            n
        };
        (taken, t.screen().clone())
    };
    let (whole, whole_screen) = stopped(true);
    let (each, each_screen) = stopped(false);
    assert!(
        whole > 0 && whole < run.len(),
        "the run stopped after {whole} bytes"
    );
    assert_eq!(whole, each);
    assert!(whole_screen == each_screen);
}
