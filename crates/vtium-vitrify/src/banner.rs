//! The banner (vtium/vtium#112): at an interactive terminal, vtium-vitrify
//! opens with its name in glass, panes of green and cyan glass behind
//! letters that fall from ice white to deep teal, with a shine crossing
//! them once.
//!
//! It shows only when stdin and stdout are both terminals, and never with
//! [`OFF`], `--json`, `--version`, a non-empty `NO_COLOR`, a dpkg
//! maintainer script's environment, or a `TERM` that is unset, `dumb` or a
//! monochrome VT. It follows the color depth the environment reports
//! ([`Depth`]), and takes the narrow form below [`Form::Wide`]'s width. It
//! is static: no motion, and nothing waits on it.
//!
//! The art is a picture of pixels at twice the cell's resolution each
//! way, painted from the data below and drawn two colors a cell.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{IsTerminal, Write};

/// The flag that turns the banner off.
pub const OFF: &str = "--no-banner";

/// Arguments whose output a script reads; the banner never precedes it.
const MACHINE: [&str; 2] = ["--json", "--version"];

/// The colors the terminal reports it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// `COLORTERM=truecolor` or `24bit`, or a `TERM` ending `-direct`.
    TrueColor,
    /// A `TERM` naming `256color`.
    Indexed256,
    /// `TERM=linux`, and any other color terminal: the 16 ANSI colors,
    /// dithered, and drawn with the half blocks and shades the Linux
    /// console's built-in font holds, `▀ ▄ ▌ ▐ █ ░ ▒ ▓`. The other depths
    /// draw with the quadrants too, U+2580 to U+259F.
    Ansi16,
}

/// The banner's size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Wide,
    Narrow,
}

impl Form {
    /// The fewest columns the form is drawn at; no line it draws is wider.
    pub const fn columns(self) -> u16 {
        match self {
            Form::Wide => 64,
            Form::Narrow => 40,
        }
    }
}

/// How the banner is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub depth: Depth,
    pub form: Form,
}

/// What the process's standard streams are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminal {
    pub stdin: bool,
    pub stdout: bool,
    /// The terminal's width, when it reports one.
    pub columns: Option<u16>,
}

/// Whether and how to draw the banner for the command line `args`, at
/// `terminal`, with the environment `env`; `None` keeps it hidden.
pub fn plan(
    args: &[OsString],
    terminal: Terminal,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Option<Plan> {
    if !(terminal.stdin && terminal.stdout) {
        return None;
    }
    let quiet = |a: &OsString| *a == *OFF || MACHINE.iter().any(|m| *a == **m);
    if args.iter().take_while(|a| *a != "--").any(quiet) {
        return None;
    }
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    if set("NO_COLOR").is_some() || set("DPKG_MAINTSCRIPT_NAME").is_some() {
        return None;
    }
    let term = set("TERM")?.to_string_lossy().into_owned();
    let monochrome_vt = term
        .strip_prefix("vt")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    if term == "dumb" || monochrome_vt {
        return None;
    }
    let linux = term == "linux" || term.starts_with("linux-");
    let truecolor = set("COLORTERM").is_some_and(|c| c == "truecolor" || c == "24bit");
    let depth = if linux {
        Depth::Ansi16
    } else if truecolor || term.ends_with("-direct") {
        Depth::TrueColor
    } else if term.contains("256color") {
        Depth::Indexed256
    } else {
        Depth::Ansi16
    };
    let columns = terminal
        .columns
        .or_else(|| set("COLUMNS")?.to_str()?.parse().ok())
        .unwrap_or(80);
    let form = [Form::Wide, Form::Narrow]
        .into_iter()
        .find(|f| columns >= f.columns())?;
    Some(Plan { depth, form })
}

/// Draws the banner on `out` when [`plan`] says to, reading the process's
/// streams and environment.
pub fn show(args: &[OsString], out: &mut dyn Write) {
    let stdout = std::io::stdout();
    let terminal = Terminal {
        stdin: std::io::stdin().is_terminal(),
        stdout: stdout.is_terminal(),
        columns: rustix::termios::tcgetwinsize(&stdout)
            .ok()
            .map(|w| w.ws_col)
            .filter(|&c| c > 0),
    };
    if let Some(plan) = plan(args, terminal, &|name| std::env::var_os(name)) {
        let _ = out.write_all(render(plan).as_bytes());
        let _ = out.flush();
    }
}

/// The banner `plan` draws: a blank line, the kicker, the art, the tagline
/// with the version, and a blank line.
pub fn render(plan: Plan) -> String {
    let scene = match plan.form {
        Form::Wide => &WIDE,
        Form::Narrow => &NARROW,
    };
    let pixels = paint(scene);
    let mut text = String::from("\n");
    line(&mut text, plan.depth, KICKER, scene.kicker);
    for row in 0..scene.height / 2 {
        text.push_str(INDENT);
        let cells: Vec<Cell> = (0..scene.width / 2)
            .map(|col| cell(&pixels, scene.width, col, row, plan))
            .collect();
        draw_row(&mut text, &cells, plan.depth);
        text.push('\n');
    }
    let tagline = format!("{}  {}", scene.tagline, env!("CARGO_PKG_VERSION"));
    line(&mut text, plan.depth, TAGLINE, &tagline);
    text.push('\n');
    text
}

const INDENT: &str = "  ";

/// A color in sRGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

const fn hex(v: u32) -> Rgb {
    Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// The squared distance from `a` to `b`, green weighted most and red
/// least, as the eye sees them.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).unsigned_abs().pow(2);
    2 * d(a.0, b.0) + 4 * d(a.1, b.1) + 3 * d(a.2, b.2)
}

/// A text line's color, with its fixed ANSI color for [`Depth::Ansi16`].
struct Ink(Rgb, u8);

const KICKER: Ink = Ink(hex(0x8fd8d0), 6);
const TAGLINE: Ink = Ink(hex(0x6f8a88), 8);

fn line(text: &mut String, depth: Depth, ink: Ink, words: &str) {
    let code = match depth {
        Depth::TrueColor => Code::Rgb(ink.0),
        Depth::Indexed256 => nearest_256(ink.0).1,
        Depth::Ansi16 => Code::Ansi(ink.1),
    };
    let _ = writeln!(text, "{INDENT}\x1b[{}m{words}\x1b[0m", code.sgr(Layer::Fg));
}

// The scenes.

/// A pixel's color in the 16 colors: `a`, with `b` in `quarters` of each
/// 2x2 block of pixels, laid out by an ordered dither so a half mix draws
/// as a checkerboard within a cell. `b` is bright only over a dark `a`: a
/// cell's background takes only the eight dark colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ansi {
    a: u8,
    b: u8,
    quarters: u8,
}

const fn solid(n: u8) -> Ansi {
    Ansi {
        a: n,
        b: n,
        quarters: 0,
    }
}

const fn half(a: u8, b: u8) -> Ansi {
    Ansi { a, b, quarters: 2 }
}

const fn quarter(a: u8, b: u8) -> Ansi {
    Ansi { a, b, quarters: 1 }
}

/// A painted pixel: its color, and its color in the 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pixel {
    rgb: Rgb,
    ansi: Ansi,
}

/// A pane of glass behind the word: a quadrilateral from `top` to
/// `bottom`, its left and right edges running from `top_x` to `bottom_x`.
struct Pane {
    top: i32,
    bottom: i32,
    top_x: (i32, i32),
    bottom_x: (i32, i32),
    tint: Rgb,
    /// The pane in the 16 colors, and its lit edge.
    ansi: Ansi,
    edge: u8,
}

impl Pane {
    /// The pane's left and right edge on row `y`, when it spans the row.
    fn span(&self, y: i32) -> Option<(i32, i32)> {
        if y < self.top || y > self.bottom {
            return None;
        }
        let rows = (self.bottom - self.top).max(1);
        let along = |a: i32, b: i32| a + ((b - a) * (y - self.top)) / rows;
        Some((
            along(self.top_x.0, self.bottom_x.0),
            along(self.top_x.1, self.bottom_x.1),
        ))
    }
}

struct Scene {
    /// The art's size in pixels, two a cell each way.
    width: usize,
    height: usize,
    /// The letters, `#` for glass, set `gap` pixels apart from `origin`.
    word: &'static [&'static [&'static str]],
    origin: (usize, usize),
    gap: usize,
    panes: &'static [Pane],
    /// The shine: the pixels whose `x + 2 * y` falls in `band` or `glint`,
    /// a diagonal at 45 degrees in a cell twice as tall as it is wide.
    band: (i32, i32),
    glint: (i32, i32),
    kicker: &'static str,
    tagline: &'static str,
}

/// The letters' gradient, top to bottom: ice white, pale cyan, cyan
/// glass, deep teal; and in the 16 colors, white, bright cyan, and bright
/// cyan over cyan.
const GLASS: [(f32, Rgb); 4] = [
    (0.0, hex(0xf2feff)),
    (0.3, hex(0xb2eeea)),
    (0.65, hex(0x4cc2bc)),
    (1.0, hex(0x137a75)),
];
const GLASS_16: [(f32, Ansi); 3] = [(0.2, solid(15)), (0.7, solid(14)), (1.0, half(6, 14))];
const WHITE: Rgb = hex(0xffffff);
const SHADOW: Rgb = hex(0x0a3e3b);
const LIT: Rgb = hex(0x86ecc4);
const DEEP: Rgb = hex(0x04201f);

const GREEN: Rgb = hex(0x18582c);
const TEAL: Rgb = hex(0x10495a);
const PALE: Rgb = hex(0x2f8063);

const WIDE: Scene = Scene {
    width: 120,
    height: 14,
    word: &[
        &[
            "####......####",
            "####......####",
            ".####....####.",
            ".####....####.",
            "..####..####..",
            "..####..####..",
            "...########...",
            "...########...",
            "....######....",
            "....######....",
        ],
        &[
            "########", "########", "..####..", "..####..", "..####..", "..####..", "..####..",
            "..####..", "########", "########",
        ],
        &[
            "############",
            "############",
            "....####....",
            "....####....",
            "....####....",
            "....####....",
            "....####....",
            "....####....",
            "....####....",
            "....####....",
        ],
        &[
            "##########..",
            "############",
            "####....####",
            "####....####",
            "############",
            "##########..",
            "####..####..",
            "####..####..",
            "####....####",
            "####....####",
        ],
        &[
            "########", "########", "..####..", "..####..", "..####..", "..####..", "..####..",
            "..####..", "########", "########",
        ],
        &[
            "############",
            "############",
            "####........",
            "####........",
            "##########..",
            "##########..",
            "####........",
            "####........",
            "####........",
            "####........",
        ],
        &[
            "####......####",
            "####......####",
            ".####....####.",
            ".####....####.",
            "..##########..",
            "..##########..",
            ".....####.....",
            ".....####.....",
            ".....####.....",
            ".....####.....",
        ],
    ],
    origin: (14, 2),
    gap: 2,
    panes: &[
        Pane {
            top: 0,
            bottom: 13,
            top_x: (8, 64),
            bottom_x: (0, 56),
            tint: GREEN,
            ansi: quarter(0, 2),
            edge: 10,
        },
        Pane {
            top: 1,
            bottom: 13,
            top_x: (54, 112),
            bottom_x: (46, 104),
            tint: TEAL,
            ansi: quarter(0, 6),
            edge: 14,
        },
        Pane {
            top: 3,
            bottom: 11,
            top_x: (104, 119),
            bottom_x: (98, 113),
            tint: PALE,
            ansi: quarter(2, 10),
            edge: 10,
        },
    ],
    band: (62, 67),
    glint: (71, 72),
    kicker: "v  t  i  u  m",
    tagline: "vitrified font sets for vtium",
};

const NARROW: Scene = Scene {
    width: 72,
    height: 10,
    word: &[
        &[
            "##....##", "##....##", ".##..##.", ".##..##.", "..####..", "..####..",
        ],
        &["######", "..##..", "..##..", "..##..", "..##..", "######"],
        &["######", "..##..", "..##..", "..##..", "..##..", "..##.."],
        &["####..", "##..##", "##..##", "####..", "##..##", "##..##"],
        &["######", "..##..", "..##..", "..##..", "..##..", "######"],
        &["######", "##....", "####..", "##....", "##....", "##...."],
        &["##..##", "##..##", ".####.", ".####.", "..##..", "..##.."],
    ],
    origin: (8, 2),
    gap: 2,
    panes: &[
        Pane {
            top: 0,
            bottom: 9,
            top_x: (6, 38),
            bottom_x: (0, 32),
            tint: GREEN,
            ansi: quarter(0, 2),
            edge: 10,
        },
        Pane {
            top: 1,
            bottom: 9,
            top_x: (34, 68),
            bottom_x: (28, 62),
            tint: TEAL,
            ansi: quarter(0, 6),
            edge: 14,
        },
        Pane {
            top: 3,
            bottom: 8,
            top_x: (64, 71),
            bottom_x: (60, 67),
            tint: PALE,
            ansi: quarter(2, 10),
            edge: 10,
        },
    ],
    band: (44, 47),
    glint: (51, 51),
    kicker: "v t i u m",
    tagline: "font sets for vtium",
};

/// The word's glass: each pixel's height down the letters, 0 at the top
/// and 1 at the foot, row by row.
fn word(scene: &Scene) -> Vec<Option<f32>> {
    let (w, h) = (scene.width, scene.height);
    let mut glass = vec![None; w * h];
    let rows = scene.word.iter().map(|l| l.len()).max().unwrap_or(1);
    let mut left = scene.origin.0;
    for letter in scene.word {
        for (dy, bits) in letter.iter().enumerate() {
            for (dx, bit) in bits.bytes().enumerate() {
                let (x, y) = (left + dx, scene.origin.1 + dy);
                if bit == b'#' && x < w && y < h {
                    if let Some(p) = glass.get_mut(y * w + x) {
                        *p = Some(dy as f32 / (rows - 1).max(1) as f32);
                    }
                }
            }
        }
        left += letter.first().map_or(0, |r| r.len()) + scene.gap;
    }
    glass
}

/// The scene's pixels, row by row; `None` is the terminal's background.
fn paint(scene: &Scene) -> Vec<Option<Pixel>> {
    let w = scene.width as i32;
    let glass = word(scene);
    let height = |x: i32, y: i32| -> Option<f32> {
        if x < 0 || y < 0 || x >= w {
            return None;
        }
        glass.get((y * w + x) as usize).copied().flatten()
    };
    let mut pixels = Vec::with_capacity(glass.len());
    for y in 0..scene.height as i32 {
        for x in 0..w {
            let d = x + 2 * y;
            let band = (scene.band.0..=scene.band.1).contains(&d);
            let glint = (scene.glint.0..=scene.glint.1).contains(&d);
            let behind = behind(scene, x, y);
            let pixel = match height(x, y) {
                Some(t) => {
                    let mut rgb = gradient(t);
                    let mut ansi = GLASS_16
                        .iter()
                        .find(|(at, _)| t <= *at)
                        .map_or(half(6, 14), |s| s.1);
                    if let Some(b) = behind {
                        rgb = mix(rgb, b.rgb, 0.12);
                    }
                    // Light from the upper left: the glass's upper and
                    // left faces catch it, its lower and right ones fall
                    // into shadow.
                    if height(x, y - 1).is_none() {
                        rgb = mix(rgb, WHITE, 0.6);
                        ansi = solid(15);
                    } else if height(x - 1, y).is_none() {
                        rgb = mix(rgb, WHITE, 0.3);
                    } else if height(x, y + 1).is_none() || height(x + 1, y).is_none() {
                        rgb = mix(rgb, SHADOW, 0.35);
                    }
                    if band || glint {
                        rgb = mix(rgb, WHITE, if band { 0.75 } else { 0.5 });
                        ansi = solid(15);
                    }
                    Some(Pixel { rgb, ansi })
                }
                None => behind.map(|mut p| {
                    if band || glint {
                        p.rgb = mix(p.rgb, LIT, if band { 0.45 } else { 0.25 });
                        p.ansi = quarter(p.ansi.a, 15);
                    }
                    p
                }),
            };
            pixels.push(pixel);
        }
    }
    pixels
}

/// The panes at (`x`, `y`): one pane's tint, lit along its upper and left
/// edges; where two cross, less light comes through, but the upper one's
/// edge still catches it.
fn behind(scene: &Scene, x: i32, y: i32) -> Option<Pixel> {
    let mut over = scene.panes.iter().filter_map(|p| {
        p.span(y)
            .filter(|s| (s.0..=s.1).contains(&x))
            .map(|s| (p, s))
    });
    let (a, span) = over.next()?;
    let lit = |p: &Pane, left: i32| y == p.top || x == left;
    Some(match over.next() {
        None if lit(a, span.0) => Pixel {
            rgb: mix(a.tint, LIT, 0.7),
            ansi: solid(a.edge),
        },
        None => Pixel {
            rgb: a.tint,
            ansi: a.ansi,
        },
        Some((b, bspan)) if lit(b, bspan.0) => Pixel {
            rgb: mix(mix(a.tint, b.tint, 0.5), LIT, 0.55),
            ansi: solid(b.edge),
        },
        Some((b, _)) => Pixel {
            rgb: mix(mix(a.tint, b.tint, 0.5), DEEP, 0.3),
            ansi: quarter(0, 8),
        },
    })
}

fn gradient(t: f32) -> Rgb {
    let mut prev = GLASS[0];
    for stop in GLASS {
        if t <= stop.0 {
            let span = (stop.0 - prev.0).max(f32::EPSILON);
            return mix(prev.1, stop.1, (t - prev.0) / span);
        }
        prev = stop;
    }
    prev.1
}

// Color depth.

/// A color as the terminal is told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Code {
    Rgb(Rgb),
    Indexed(u8),
    /// One of the 16 ANSI colors.
    Ansi(u8),
}

enum Layer {
    Fg,
    Bg,
}

impl Code {
    fn sgr(self, layer: Layer) -> String {
        let fg = matches!(layer, Layer::Fg);
        match self {
            Code::Rgb(Rgb(r, g, b)) => format!("{};2;{r};{g};{b}", if fg { 38 } else { 48 }),
            Code::Indexed(n) => format!("{};5;{n}", if fg { 38 } else { 48 }),
            Code::Ansi(n) => {
                let base = if fg { 30 } else { 40 };
                if n < 8 {
                    format!("{}", base + n)
                } else {
                    format!("{}", base + 60 + (n - 8))
                }
            }
        }
    }

    fn bright(self) -> bool {
        matches!(self, Code::Ansi(n) if n >= 8)
    }
}

/// A pixel as the depth draws it: the color shown, and how it is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shade {
    rgb: Rgb,
    code: Code,
}

/// The xterm 256-color cube's channel levels.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The nearest of the 256 colors' cube to `c`. The gray ramp is left
/// out: tinted glass falls to gray in it.
fn nearest_256(c: Rgb) -> (Rgb, Code) {
    let mut best = (u32::MAX, Rgb(0, 0, 0), 16);
    for (i, r) in (0u8..).zip(CUBE) {
        for (j, g) in (0u8..).zip(CUBE) {
            for (k, b) in (0u8..).zip(CUBE) {
                let e = distance(c, Rgb(r, g, b));
                if e < best.0 {
                    best = (e, Rgb(r, g, b), 16 + 36 * i + 6 * j + k);
                }
            }
        }
    }
    (best.1, Code::Indexed(best.2))
}

/// The 16 colors as the VGA palette the Linux console starts with draws
/// them, for choosing between two in a cell.
const VGA: [Rgb; 16] = [
    hex(0x000000),
    hex(0xaa0000),
    hex(0x00aa00),
    hex(0xaa5500),
    hex(0x0000aa),
    hex(0xaa00aa),
    hex(0x00aaaa),
    hex(0xaaaaaa),
    hex(0x555555),
    hex(0xff5555),
    hex(0x55ff55),
    hex(0xffff55),
    hex(0x5555ff),
    hex(0xff55ff),
    hex(0x55ffff),
    hex(0xffffff),
];

fn shade(p: Pixel, x: usize, y: usize, depth: Depth) -> Shade {
    match depth {
        Depth::TrueColor => Shade {
            rgb: p.rgb,
            code: Code::Rgb(p.rgb),
        },
        Depth::Indexed256 => {
            let (rgb, code) = nearest_256(p.rgb);
            Shade { rgb, code }
        }
        Depth::Ansi16 => {
            const ORDER: [[u8; 2]; 2] = [[0, 2], [3, 1]];
            let rank = ORDER
                .get(y % 2)
                .and_then(|r| r.get(x % 2))
                .copied()
                .unwrap_or(0);
            let n = if rank < p.ansi.quarters {
                p.ansi.b
            } else {
                p.ansi.a
            };
            Shade {
                rgb: VGA.get(usize::from(n)).copied().unwrap_or(WHITE),
                code: Code::Ansi(n),
            }
        }
    }
}

// Cells.

/// One cell: which of its quadrants take the foreground (upper left 1,
/// upper right 2, lower left 4, lower right 8), and the two colors;
/// `None` is the terminal's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    mask: u8,
    fg: Option<Code>,
    bg: Option<Code>,
}

const EMPTY: Cell = Cell {
    mask: 0,
    fg: None,
    bg: None,
};

/// Cell (`col`, `row`) of `pixels`: its four pixels in the depth's colors,
/// cut to the two that lose the least.
fn cell(pixels: &[Option<Pixel>], width: usize, col: usize, row: usize, plan: Plan) -> Cell {
    let quad: [Option<Shade>; 4] = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| {
        let (x, y) = (2 * col + dx, 2 * row + dy);
        pixels
            .get(y * width + x)
            .copied()
            .flatten()
            .map(|c| shade(c, x, y, plan.depth))
    });
    let cost = |p: Option<Shade>, q: Option<Shade>| match (p, q) {
        (None, None) => 0,
        (Some(p), Some(q)) => distance(p.rgb, q.rgb),
        _ => 9 * 255 * 255 + 1,
    };
    let mut distinct: Vec<Option<Shade>> = Vec::with_capacity(4);
    for p in quad {
        if !distinct.contains(&p) {
            distinct.push(p);
        }
    }
    let mut pair = (quad[0], quad[0]);
    let mut least = u32::MAX;
    for (i, a) in distinct.iter().enumerate() {
        for b in distinct.iter().skip(i) {
            let e: u32 = quad.iter().map(|&p| cost(p, *a).min(cost(p, *b))).sum();
            if e < least {
                least = e;
                pair = (*a, *b);
            }
        }
    }
    let (mut a, mut b) = pair;
    // The terminal's background, and the Linux console's eight dark
    // backgrounds, only ever sit behind.
    if a.is_none() || (b.is_some_and(|s| s.code.bright()) && a.is_some_and(|s| !s.code.bright())) {
        std::mem::swap(&mut a, &mut b);
    }
    let fits = |p: Option<Shade>| cost(p, a) <= cost(p, b);
    let mut mask = quad
        .iter()
        .enumerate()
        .filter(|(_, p)| fits(**p))
        .fold(0u8, |m, (i, _)| m | 1 << i);
    if b.is_some_and(|s| s.code.bright()) {
        // Both bright: the cell takes the one most of it shows.
        if mask.count_ones() < 2 {
            a = b;
        }
        mask = 15;
    }
    match a {
        None => EMPTY,
        Some(fg) if mask == 15 => Cell {
            mask,
            fg: Some(fg.code),
            bg: None,
        },
        Some(fg) => Cell {
            mask,
            fg: Some(fg.code),
            bg: b.map(|s| s.code),
        },
    }
}

/// The quadrant each mask draws.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

/// The glyph `mask` draws at `depth`: a quadrant, or in the 16 colors, a
/// half block, or a shade for a dither.
fn glyph(mask: u8, depth: Depth) -> char {
    match depth {
        Depth::TrueColor | Depth::Indexed256 => {
            QUADRANTS.get(usize::from(mask)).copied().unwrap_or(' ')
        }
        Depth::Ansi16 => match mask {
            0 => ' ',
            3 => '▀',
            12 => '▄',
            5 => '▌',
            10 => '▐',
            15 => '█',
            6 | 9 => '▒',
            m if m.count_ones() == 1 => '░',
            _ => '▓',
        },
    }
}

fn draw_row(text: &mut String, cells: &[Cell], depth: Depth) {
    let used = cells.iter().rposition(|c| *c != EMPTY).map_or(0, |i| i + 1);
    let (mut fg, mut bg): (Option<Code>, Option<Code>) = (None, None);
    for c in cells.iter().take(used) {
        let mut sgr = Vec::new();
        if c.mask != 0 && c.fg != fg {
            if let Some(code) = c.fg {
                sgr.push(code.sgr(Layer::Fg));
            }
            fg = c.fg;
        }
        if c.bg != bg {
            sgr.push(c.bg.map_or_else(|| "49".to_owned(), |code| code.sgr(Layer::Bg)));
            bg = c.bg;
        }
        if !sgr.is_empty() {
            let _ = write!(text, "\x1b[{}m", sgr.join(";"));
        }
        text.push(glyph(c.mask, depth));
    }
    if fg.is_some() || bg.is_some() {
        text.push_str("\x1b[0m");
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    const TTY: Terminal = Terminal {
        stdin: true,
        stdout: true,
        columns: Some(80),
    };

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn plan_for(words: &[&str], terminal: Terminal, vars: &[(&str, &str)]) -> Option<Plan> {
        plan(&args(words), terminal, &env(vars))
    }

    /// `text` with its control sequences taken out.
    fn visible(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Every SGR parameter list `text` sends.
    fn sgrs(text: &str) -> Vec<String> {
        text.split("\x1b[")
            .skip(1)
            .filter_map(|s| s.split_once('m').map(|(p, _)| p.to_owned()))
            .collect()
    }

    fn every_plan() -> Vec<Plan> {
        let mut plans = Vec::new();
        for depth in [Depth::TrueColor, Depth::Indexed256, Depth::Ansi16] {
            for form in [Form::Wide, Form::Narrow] {
                plans.push(Plan { depth, form });
            }
        }
        plans
    }

    #[test]
    fn shows_at_an_interactive_terminal() {
        assert_eq!(
            plan_for(&["list"], TTY, &[("TERM", "xterm-256color")]),
            Some(Plan {
                depth: Depth::Indexed256,
                form: Form::Wide,
            })
        );
    }

    #[test]
    fn hidden_unless_stdin_and_stdout_are_terminals() {
        let vars = [("TERM", "xterm-256color")];
        for (stdin, stdout) in [(true, false), (false, true), (false, false)] {
            let t = Terminal {
                stdin,
                stdout,
                columns: Some(80),
            };
            assert_eq!(plan_for(&["list"], t, &vars), None, "{stdin} {stdout}");
        }
    }

    #[test]
    fn hidden_by_the_flag_and_machine_readable_output() {
        let vars = [("TERM", "xterm-256color")];
        for words in [
            &["--no-banner", "list"][..],
            &["list", "--json"],
            &["--version"],
            &["create", "--no-banner", "x"],
        ] {
            assert_eq!(plan_for(words, TTY, &vars), None, "{words:?}");
        }
        // After `--` they are words, not options.
        assert!(plan_for(&["remove", "--", "--json"], TTY, &vars).is_some());
    }

    #[test]
    fn hidden_under_no_color_when_it_is_set_and_not_empty() {
        let term = ("TERM", "xterm-256color");
        assert_eq!(plan_for(&[], TTY, &[term, ("NO_COLOR", "1")]), None);
        assert_eq!(plan_for(&[], TTY, &[term, ("NO_COLOR", "anything")]), None);
        // no-color.org: an empty NO_COLOR is as if unset.
        assert!(plan_for(&[], TTY, &[term, ("NO_COLOR", "")]).is_some());
    }

    #[test]
    fn hidden_in_a_dpkg_maintainer_script() {
        let vars = [
            ("TERM", "xterm-256color"),
            ("DPKG_MAINTSCRIPT_NAME", "postinst"),
        ];
        assert_eq!(plan_for(&["create", "--all"], TTY, &vars), None);
    }

    #[test]
    fn hidden_without_a_color_terminal() {
        for term in ["", "dumb", "vt100", "vt220"] {
            assert_eq!(plan_for(&[], TTY, &[("TERM", term)]), None, "{term:?}");
        }
        assert_eq!(plan_for(&[], TTY, &[]), None);
    }

    #[test]
    fn the_depth_follows_colorterm_and_term() {
        let depth = |vars: &[(&str, &str)]| plan_for(&[], TTY, vars).map(|p| p.depth);
        for (vars, want) in [
            (
                &[("TERM", "xterm"), ("COLORTERM", "truecolor")][..],
                Depth::TrueColor,
            ),
            (
                &[("TERM", "xterm-256color"), ("COLORTERM", "24bit")],
                Depth::TrueColor,
            ),
            // vtium's own environment.
            (
                &[("TERM", "kmscon"), ("COLORTERM", "truecolor")],
                Depth::TrueColor,
            ),
            (&[("TERM", "xterm-direct")], Depth::TrueColor),
            (
                &[("TERM", "screen-256color"), ("COLORTERM", "yes")],
                Depth::Indexed256,
            ),
            (&[("TERM", "xterm")], Depth::Ansi16),
            (&[("TERM", "kmscon")], Depth::Ansi16),
            // The Linux console draws 16 colors and its font has no
            // quadrants, whatever COLORTERM a session carried in.
            (&[("TERM", "linux")], Depth::Ansi16),
            (
                &[("TERM", "linux"), ("COLORTERM", "truecolor")],
                Depth::Ansi16,
            ),
            (
                &[("TERM", "linux-16color"), ("COLORTERM", "truecolor")],
                Depth::Ansi16,
            ),
        ] {
            assert_eq!(depth(vars), Some(want), "{vars:?}");
        }
    }

    #[test]
    fn the_narrow_form_below_the_wide_ones_width() {
        let vars = [("TERM", "xterm-256color")];
        let form = |columns: Option<u16>, vars: &[(&str, &str)]| {
            plan_for(&[], Terminal { columns, ..TTY }, vars).map(|p| p.form)
        };
        let wide = Form::Wide.columns();
        let narrow = Form::Narrow.columns();
        assert_eq!(form(Some(wide), &vars), Some(Form::Wide));
        assert_eq!(form(Some(wide - 1), &vars), Some(Form::Narrow));
        assert_eq!(form(Some(narrow), &vars), Some(Form::Narrow));
        assert_eq!(form(Some(narrow - 1), &vars), None);
        // Unreported, the width is COLUMNS's, and else 80.
        assert_eq!(form(None, &vars), Some(Form::Wide));
        assert_eq!(
            form(None, &[vars[0], ("COLUMNS", "40")]),
            Some(Form::Narrow)
        );
        assert_eq!(form(None, &[vars[0], ("COLUMNS", "39")]), None);
        assert_eq!(
            form(Some(80), &[vars[0], ("COLUMNS", "39")]),
            Some(Form::Wide)
        );
    }

    #[test]
    fn no_line_is_wider_than_its_form() {
        for plan in every_plan() {
            let text = render(plan);
            let limit = usize::from(plan.form.columns());
            for line in text.lines() {
                let width = visible(line).chars().count();
                assert!(width <= limit, "{plan:?}: {width} > {limit}: {line:?}");
            }
        }
    }

    #[test]
    fn it_has_the_kicker_the_art_and_the_tagline() {
        for plan in every_plan() {
            let text = render(plan);
            let lines: Vec<String> = text.lines().map(visible).collect();
            let scene = match plan.form {
                Form::Wide => &WIDE,
                Form::Narrow => &NARROW,
            };
            let art = scene.height / 2;
            assert_eq!(lines.len(), art + 4, "{plan:?}");
            assert_eq!(lines[0], "");
            assert_eq!(lines[1].trim(), scene.kicker);
            let tagline = &lines[2 + art];
            assert!(
                tagline.starts_with(&format!("{INDENT}{}", scene.tagline)),
                "{tagline}"
            );
            assert!(tagline.ends_with(env!("CARGO_PKG_VERSION")), "{tagline}");
            assert_eq!(lines[3 + art], "");
        }
    }

    #[test]
    fn the_art_draws_only_with_its_glyphs() {
        for plan in every_plan() {
            let text = render(plan);
            let allowed: &[char] = match plan.depth {
                Depth::TrueColor | Depth::Indexed256 => &QUADRANTS,
                // The Linux console's built-in font has these and no
                // quadrants.
                Depth::Ansi16 => &[' ', '▀', '▄', '▌', '▐', '█', '░', '▒', '▓'],
            };
            let scene = match plan.form {
                Form::Wide => &WIDE,
                Form::Narrow => &NARROW,
            };
            let art: Vec<&str> = text.lines().skip(2).take(scene.height / 2).collect();
            for line in art {
                for c in visible(line).chars() {
                    assert!(allowed.contains(&c), "{plan:?}: {c:?}");
                }
            }
        }
    }

    /// The attributes an SGR parameter list sets, one string each:
    /// `38;2;r;g;b` and `38;5;n` whole, and any other number alone.
    fn attributes(sgr: &str) -> Vec<String> {
        let params: Vec<&str> = sgr.split(';').collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < params.len() {
            let take = match (params[i], params.get(i + 1)) {
                ("38" | "48", Some(&"2")) => 5,
                ("38" | "48", Some(&"5")) => 3,
                _ => 1,
            };
            let end = (i + take).min(params.len());
            out.push(params[i..end].join(";"));
            i = end;
        }
        out
    }

    #[test]
    fn each_depth_sends_only_its_colors() {
        for plan in every_plan() {
            for sgr in sgrs(&render(plan)) {
                for attr in attributes(&sgr) {
                    let n: Vec<u16> = attr.split(';').map(|p| p.parse().unwrap_or(999)).collect();
                    let ok = match (plan.depth, n.as_slice()) {
                        (_, [0] | [49]) => true,
                        (Depth::TrueColor, [38 | 48, 2, r, g, b]) => {
                            [r, g, b].iter().all(|c| **c < 256)
                        }
                        (Depth::Indexed256, [38 | 48, 5, i]) => *i < 256,
                        // The Linux console has no bright backgrounds.
                        (Depth::Ansi16, [30..=37 | 40..=47 | 90..=97]) => true,
                        _ => false,
                    };
                    assert!(ok, "{plan:?}: {attr} in {sgr}");
                }
            }
        }
    }

    #[test]
    fn the_glass_is_drawn_in_more_than_one_color() {
        for plan in every_plan() {
            let colors: std::collections::BTreeSet<String> = sgrs(&render(plan))
                .into_iter()
                .filter(|s| s != "0")
                .collect();
            assert!(colors.len() >= 4, "{plan:?}: {colors:?}");
        }
    }
}
