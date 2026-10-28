//! Helpers the integration tests share: a seeded generator, byte streams
//! from a grammar of what programs write, and the screen as text.
#![allow(dead_code, reason = "each test crate uses its own part")]

use vtium_term::{Glyph, Line, Screen, Terminal, Width};

/// splitmix64: a seeded generator with no dependency.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// How many seeds a property test runs: `default`, or `VTIUM_TERM_SEEDS`
/// for a longer soak.
pub fn seeds(default: u64) -> u64 {
    std::env::var("VTIUM_TERM_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Text to print: ASCII, wide characters, combining marks, clusters,
/// VS15 and VS16, regional indicators, Hangul jamo and zero-width
/// characters, as UTF-8.
pub const TEXT: &[&str] = &[
    "a",
    "Z",
    " ",
    "~",
    "\u{E9}",
    "e\u{301}",
    "\u{301}",
    "\u{301}\u{302}\u{303}",
    "\u{4E2D}",
    "\u{6F22}\u{5B57}",
    "\u{1F600}",
    "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
    "\u{2764}",
    "\u{FE0F}",
    "\u{FE0E}",
    "\u{2764}\u{FE0F}",
    "\u{1F1FA}\u{1F1F8}",
    "\u{1F1FA}",
    "\u{1100}\u{1161}\u{11A8}",
    "\u{200B}",
    "\u{200D}",
    "\u{AD}",
    "\u{600}",
    "\u{915}\u{94D}\u{937}",
    "\u{FFFD}",
];

/// Controls and sequences the screen acts on, with parameters that reach
/// every edge: zero, one, the screen's size and past it.
pub const SEQUENCES: &[&str] = &[
    "\r",
    "\n",
    "\x0b",
    "\x0c",
    "\x08",
    "\t",
    "\x1a",
    "\x1bD",
    "\x1bE",
    "\x1bH",
    "\x1bM",
    "\x1b[A",
    "\x1b[3B",
    "\x1b[C",
    "\x1b[99D",
    "\x1b[2E",
    "\x1b[F",
    "\x1b[5G",
    "\x1b[`",
    "\x1b[3a",
    "\x1b[d",
    "\x1b[4d",
    "\x1b[2e",
    "\x1b[H",
    "\x1b[3;7H",
    "\x1b[99;99f",
    "\x1b[J",
    "\x1b[1J",
    "\x1b[2J",
    "\x1b[3J",
    "\x1b[K",
    "\x1b[1K",
    "\x1b[2K",
    "\x1b[?K",
    "\x1b[?2J",
    "\x1b[X",
    "\x1b[5X",
    "\x1b[@",
    "\x1b[3@",
    "\x1b[P",
    "\x1b[4P",
    "\x1b[L",
    "\x1b[2L",
    "\x1b[M",
    "\x1b[3M",
    "\x1b[S",
    "\x1b[2S",
    "\x1b[T",
    "\x1b[3T",
    "\x1b[I",
    "\x1b[2Z",
    "\x1b[b",
    "\x1b[3b",
    "\x1b[g",
    "\x1b[3g",
    "\x1b[r",
    "\x1b[2;5r",
    "\x1b[3;3r",
    "\x1b[0;99r",
    "\x1b[?5A",
    "\x1b[ A",
    "\x1b[1:2A",
    "\x1b[38:2::1:2:3m",
    "\x1b[1;31m",
    "\x1b[m",
    "\x1b[7m",
    "\x1b[44m",
    "\x1b[4h",
    "\x1b[4l",
    "\x1b[?6h",
    "\x1b[?6l",
    "\x1b[?7l",
    "\x1b[?7h",
    "\x1b[?5h",
    "\x1b[?5l",
    "\x1b[?25l",
    "\x1b[?47h",
    "\x1b[?1047l",
    "\x1b[?1049h",
    "\x1b[?1049l",
    "\x1b[?1048h",
    "\x1b[?1048l",
    "\x1b[?2027l",
    "\x1b[?2026h",
    "\x1b[?2031h",
    "\x1b[?2033h",
    "\x1b[?2048h",
    "\x1b[?996n",
    "\x1b[?998n",
    "\x1b7",
    "\x1b8",
    "\x1b[!p",
    "\x1bc",
    "\x1b[3 q",
    "\x1b[c",
    "\x1b[6n",
    "\x1b[?6n",
    "\x1b[?25$p",
    "\x1b[12$p",
    "\x1b]4;1;?\x07",
    "\x1b]10;?\x1b\\",
    "\x1b]4;1;#123456\x07",
    "\x1b]4;2;rgb:ff/00/00;1;?\x1b\\",
    "\x1b]4;3;Dark Slate Gray;3;?\x07",
    "\x1b]10;#202020;#101010\x07",
    "\x1b]12;#00ff00\x07",
    "\x1b]11;#f0f0f0\x07",
    "\x1b]17;#0000ff;?;#ff00ff\x07",
    "\x1b]21;foreground=#303030;3=?;cursor=;background\x1b\\",
    "\x1b]104;1\x07",
    "\x1b]104\x07",
    "\x1b]110\x07",
    "\x1b]112\x07",
    "\x1b]117\x07",
    "\x1bP$qm\x1b\\",
    "\x1bP+q544e\x1b\\",
    "\x1b[18t",
    "\x1b(0",
    "\x1b)0",
    "\x1b*0",
    "\x1b+0",
    "\x1b(B",
    "\x1b)<",
    "\x1b(K",
    "\x0e",
    "\x0f",
    "\x1bN",
    "\x1bO",
    "\x1bn",
    "\x1bo",
    "\x1b~",
    "\x1b}",
    "\x1b|",
    // SS2 and SS3 as C1 bytes in 8-bit input, after the C2 lead UTF-8 gives them.
    "\u{8E}",
    "\u{8F}",
    "\x1b[61\"p",
    "\x1b[62\"p",
    "\x1b[64\"p",
    "\x1b[1\"q",
    "\x1b[\"q",
    "\x1b[2;2;4;5$z",
    "\x1b[$z",
    "\x1b[3;1;99;99${",
    "\x1b[120;1;3;4;9$x",
    "\x1b[19968$x",
    "\x1b[1;1;3;5;1;2;3;1$v",
    "\x1b[2;3;99;99;1;1;1$v",
    "\x1b[2*x",
    "\x1b[*x",
    "\x1b[1;2;4;9;1;44$r",
    "\x1b[2;2;3;3;4:3;38:2::1:2:3$r",
    "\x1b[1;1;9;9;0;7$t",
];

/// A byte stream from the grammar: text and sequences in random order.
pub fn grammar_stream(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < len {
        let piece = if rng.below(3) == 0 {
            rng.pick(SEQUENCES)
        } else {
            rng.pick(TEXT)
        };
        out.extend_from_slice(piece.as_bytes());
    }
    out
}

/// Raw random bytes, weighted towards ESC, `[`, digits and UTF-8 lead
/// bytes so sequences and characters form often.
pub fn raw_stream(rng: &mut Rng, len: usize) -> Vec<u8> {
    const LIKELY: &[u8] =
        b"\x1b[;0123456789?ABCDEGHJKLMPSTXZ@bdegr\r\n\t\x08 a\xe2\xf0\xcc\x80\x9f";
    (0..len)
        .map(|_| {
            if rng.below(2) == 0 {
                *rng.pick(LIKELY)
            } else {
                rng.next() as u8
            }
        })
        .collect()
}

/// A terminal fed `bytes`.
pub fn term(cols: usize, rows: usize, bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(cols, rows, 1000);
    t.input(bytes);
    t
}

/// A line's text: each cell's codepoints, a space for an empty cell,
/// nothing for a wide character's tail, trailing empty cells dropped.
pub fn line_text(line: &Line) -> String {
    let mut cells: Vec<String> = Vec::new();
    for &cell in line.cells() {
        cells.push(match line.glyph(cell) {
            Glyph::Empty if cell.width() == Width::WideTail => String::new(),
            Glyph::Empty => "\0".into(),
            Glyph::Char(c) => c.into(),
            Glyph::Cluster(s) => s.into(),
        });
    }
    while cells.last().is_some_and(|c| c == "\0" || c.is_empty()) {
        cells.pop();
    }
    cells.concat().replace('\0', " ")
}

/// The screen's rows as text, top to bottom, cut to its width.
pub fn rows(screen: &Screen) -> Vec<String> {
    screen
        .lines()
        .iter()
        .map(|l| {
            let mut t = l.clone();
            t.resize(screen.cols());
            line_text(&t)
        })
        .collect()
}

/// The scrollback's lines as text, oldest first.
pub fn scrollback(screen: &Screen) -> Vec<String> {
    screen.scrollback().map(line_text).collect()
}
