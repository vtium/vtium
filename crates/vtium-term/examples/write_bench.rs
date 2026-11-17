//! vtium-term's standing write benchmarks: the CPU time the terminal takes
//! to interpret output that programs write, with no renderer. ADR-1009
//! records the figures it gave for the line's tables.
//!
//! Each benchmark builds its input once, then feeds it to a fresh terminal
//! of 170 by 48 cells, the minspec grid, with the default 1,000 lines of
//! scrollback, and times the whole feed. It repeats that and prints the
//! median, the fastest and the slowest repetition, as milliseconds a frame
//! where the input is frames, and as milliseconds a MiB and MiB a second
//! for every benchmark.
//!
//! - `redraw`: 60 full-screen frames, each cell a new 24-bit foreground and
//!   background color and an upper half block, as a video player that
//!   draws with half blocks writes them.
//! - `tui`: 60 full-screen frames in the 16 colors, a run of ten cells in
//!   each color, as a full-screen program redraws.
//! - `scroll`: 4 MiB of 160-character lines, each with a few color
//!   changes, as a compiler or `ls --color` writes them, scrolled through
//!   the scrollback.
//! - `clusters`: 2 MiB of lines of combining marks, emoji with skin tones
//!   and ZWJ sequences, flags, and CJK, scrolled through the scrollback.
//! - `cat`: 8 MiB of plain ASCII text in lines of up to 79 characters,
//!   ended by CR LF, as `cat` of a large file reaches the terminal.
//!
//! Run it with `make bench`, or `cargo run --release -p vtium-term
//! --example write_bench -- [NAME ...] [--reps N]`. With no name it runs
//! every benchmark, 15 repetitions each.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::print_stdout,
    reason = "a benchmark program prints its figures and stops on a bad argument"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "a benchmark measures wall time; the terminal it drives reads no clock"
)]

use std::fmt::Write as _;
use std::time::Instant;

use vtium_term::Terminal;

const COLS: usize = 170;
const ROWS: usize = 48;
const SCROLLBACK: usize = 1_000;
const MIB: f64 = 1024.0 * 1024.0;

/// One benchmark's input: the bytes, split where a program's writes would
/// split them, and how many frames they hold, 0 for a stream.
struct Input {
    name: &'static str,
    chunks: Vec<Vec<u8>>,
    frames: usize,
}

/// A small deterministic generator, so every build sees the same bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn redraw() -> Input {
    let mut chunks = Vec::new();
    for f in 0..60 {
        let mut s = String::from("\x1b[H");
        for y in 0..ROWS {
            write!(s, "\x1b[{};1H", y + 1).unwrap();
            for x in 0..COLS {
                let r = (x * 3 + f * 7) % 256;
                let g = (y * 5 + f * 3) % 256;
                let b = (x + y + f * 11) % 256;
                write!(s, "\x1b[38;2;{r};{g};{b};48;2;{g};{b};{r}m\u{2580}").unwrap();
            }
        }
        s.push_str("\x1b[m");
        chunks.push(s.into_bytes());
    }
    Input {
        name: "redraw",
        chunks,
        frames: 60,
    }
}

fn tui() -> Input {
    let mut chunks = Vec::new();
    for f in 0..60 {
        let mut s = String::from("\x1b[H");
        for y in 0..ROWS {
            write!(s, "\x1b[{};1H", y + 1).unwrap();
            for x in 0..COLS {
                if x % 10 == 0 {
                    let c = (x / 10 + y + f) % 16;
                    let sgr = if c < 8 { 30 + c } else { 82 + c };
                    write!(s, "\x1b[{sgr}m").unwrap();
                }
                s.push(char::from(b'a' + ((x + f) % 26) as u8));
            }
        }
        s.push_str("\x1b[m");
        chunks.push(s.into_bytes());
    }
    Input {
        name: "tui",
        chunks,
        frames: 60,
    }
}

/// Lines from `line` until they hold `bytes`, in writes of about 4 KiB, as
/// a program's output reaches the terminal through a pty.
fn stream(name: &'static str, bytes: usize, mut line: impl FnMut(&mut String)) -> Input {
    let mut all = String::new();
    while all.len() < bytes {
        line(&mut all);
    }
    let all = all.into_bytes();
    let chunks = all.chunks(4096).map(<[u8]>::to_vec).collect();
    Input {
        name,
        chunks,
        frames: 0,
    }
}

fn scroll() -> Input {
    let mut rng = Rng(0x5eed);
    stream("scroll", 4 << 20, |s| {
        for i in 0..160 {
            if i % 20 == 0 {
                write!(s, "\x1b[{}m", 31 + rng.below(7)).unwrap();
            }
            s.push(char::from(b'!' + rng.below(94) as u8));
        }
        s.push_str("\x1b[m\r\n");
    })
}

fn clusters() -> Input {
    const PIECES: [&str; 8] = [
        "e\u{301}\u{302}",
        "a\u{308}",
        "\u{1F44D}\u{1F3FD}",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1EB}\u{1F1F7}",
        "\u{6F22}\u{5B57}",
        "\u{2764}\u{FE0F}",
        "word ",
    ];
    let mut rng = Rng(0xc105);
    stream("clusters", 2 << 20, |s| {
        let mut cells = 0;
        while cells < 150 {
            let p = PIECES[rng.below(PIECES.len())];
            s.push_str(p);
            cells += 5;
        }
        s.push_str("\r\n");
    })
}

fn cat() -> Input {
    const WORDS: [&str; 12] = [
        "the", "terminal", "writes", "each", "line", "of", "text", "to", "its", "screen", "and",
        "scrolls",
    ];
    let mut rng = Rng(0xca7);
    stream("cat", 8 << 20, |s| {
        let mut len = 0;
        let width = 20 + rng.below(60);
        while len < width {
            let w = WORDS[rng.below(WORDS.len())];
            s.push_str(w);
            s.push(' ');
            len += w.len() + 1;
        }
        s.push_str("\r\n");
    })
}

/// A benchmark's name and the function that builds its input.
type Bench = (&'static str, fn() -> Input);

/// Feeds every chunk to a fresh terminal and returns the seconds it took.
fn run(input: &Input) -> f64 {
    let mut t = Terminal::new(COLS, ROWS, SCROLLBACK);
    let start = Instant::now();
    for chunk in &input.chunks {
        let mut at = 0;
        while at < chunk.len() {
            at += t.input(&chunk[at..]);
            let _ = t.take_replies();
        }
    }
    let secs = start.elapsed().as_secs_f64();
    std::hint::black_box(&t);
    secs
}

fn main() {
    let mut names = Vec::new();
    let mut reps = 15;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--reps" {
            reps = args.next().and_then(|n| n.parse().ok()).expect("--reps N");
        } else {
            names.push(a);
        }
    }
    let all: [Bench; 5] = [
        ("redraw", redraw),
        ("tui", tui),
        ("scroll", scroll),
        ("clusters", clusters),
        ("cat", cat),
    ];
    println!(
        "benchmark  MiB    reps  ms/frame (median, min-max)  ms/MiB (median, min-max)  MiB/s median"
    );
    for (name, make) in all {
        if !names.is_empty() && !names.iter().any(|n| n == name) {
            continue;
        }
        let input = make();
        let bytes: usize = input.chunks.iter().map(Vec::len).sum();
        let mib = bytes as f64 / MIB;
        let mut times: Vec<f64> = (0..reps).map(|_| run(&input)).collect();
        times.sort_by(f64::total_cmp);
        let (lo, mid, hi) = (times[0], times[times.len() / 2], times[times.len() - 1]);
        let frame = if input.frames > 0 {
            let f = input.frames as f64 / 1e3;
            format!("{:7.3} ({:.3}-{:.3})", mid / f, lo / f, hi / f)
        } else {
            "-".to_string()
        };
        println!(
            "{:<10} {:5.2}  {:4}  {:<27} {:7.2} ({:.2}-{:.2})       {:7.1}",
            input.name,
            mib,
            reps,
            frame,
            mid * 1e3 / mib,
            lo * 1e3 / mib,
            hi * 1e3 / mib,
            mib / mid
        );
    }
}
