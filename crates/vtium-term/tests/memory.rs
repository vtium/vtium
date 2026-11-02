//! The resident size of the cell model on tier 3's x86 variant of the
//! minspec machine (docs/architecture.md, "The minspec build"): a 1366x768
//! display in unifont's 8x16 cells is 170 columns by 48 rows (ADR-1001,
//! "Memory"). Each figure is computed from the structures themselves, by
//! capacity, and pinned, so a change to the model changes the record. A
//! screen row's worst cost is measured by driving real rows through the
//! paths that fill them, and the minspec peak is checked at the minspec
//! grid and at the smaller grids of a 640x480 and a 320x240 display.
//!
//! `cargo test -p vtium-term --test memory -- --nocapture` prints the table.

mod common;

use vtium_term::{
    CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, Color, Damage, Glyph, LINKS_BYTES_MAX, Line, LineId,
    LinkId, MINSPEC_GRID, MINSPEC_PEAK, MINSPEC_REST, Rgb, SB_MEMORY_DEFAULT, SB_MEMORY_MIN,
    SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_DEFAULT, Screen, Style,
    Terminal, Width, sb_memory_default, screens_bytes_max,
};

const COLS: usize = 170;
const ROWS: usize = 48;
const MIB: f64 = 1024.0 * 1024.0;

/// The C reference's figure with unifont on the minspec machine, measured
/// (kmscon's README, "Minimal build"; 1 MiB is 1,024 KiB): resident after
/// the first screen. Its peak, once 500,000 lines had filled its
/// 1,000-line scrollback, was 15.7 MiB.
const C_FIRST_SCREEN: f64 = 10.3 * MIB;

/// The minspec peak vtium holds to: the operator's round figure
/// (vtium/vtium#131).
const PEAK: f64 = 16.0 * MIB;

/// The C reference's cell was 32 bytes, its age included, and it allocated
/// the main and the alternate screen (libtsm's tsm-screen.c:801-845). These
/// are derived from that layout, not measured.
const C_CELL: usize = 32;
const C_SCREENS: usize = 2 * ROWS * COLS * C_CELL;

/// The bytes one line costs: its struct, held in the scrollback's ring or
/// the screen's rows, and its heap. A clone holds exactly its length, as a
/// trimmed line does.
fn line_bytes(line: &Line) -> usize {
    size_of::<Line>() + line.clone().heap_bytes()
}

fn line_of(style: impl Fn(usize) -> Style, glyph: impl Fn(usize) -> String, len: usize) -> Line {
    let mut line = Line::new(LineId::new(0), COLS);
    for col in 0..len {
        let g = glyph(col);
        let glyph = if g.chars().count() == 1 {
            Glyph::Char(g.chars().next().unwrap())
        } else {
            Glyph::Cluster(&g)
        };
        line.write(col, glyph, Width::Narrow, &style(col), None);
    }
    line.trim();
    line
}

/// A line of `len` cells of plain text.
fn plain(len: usize) -> Line {
    line_of(|_| Style::DEFAULT, |_| "x".into(), len)
}

/// A shell's line: a green prompt, a bold blue path, then plain text to
/// `len` cells.
fn shell(len: usize) -> Line {
    let green = Style {
        fg: Color::Indexed(2),
        ..Style::DEFAULT
    };
    let mut blue = Style {
        fg: Color::Indexed(4),
        ..Style::DEFAULT
    };
    blue.attrs.insert(vtium_term::Attrs::BOLD);
    let style = move |col| match col {
        0..13 => green,
        13..21 => blue,
        _ => Style::DEFAULT,
    };
    line_of(style, |_| "x".into(), len)
}

fn distinct(col: usize) -> Style {
    let c = u8::try_from(col).unwrap();
    Style {
        fg: Color::Rgb(Rgb::new(c, 0, 0)),
        bg: Color::Rgb(Rgb::new(0, c, 0)),
        ..Style::DEFAULT
    }
}

/// Truecolor art: half blocks, every cell its own foreground and
/// background, as an image printed as text gives.
fn art() -> Line {
    line_of(distinct, |_| "\u{2580}".into(), COLS)
}

/// The worst a program can make a line cost: every cell its own style and
/// its own link, and a cluster of `CLUSTER_MAX` codepoints written into
/// every cell, which fill the line's cluster budget.
fn worst() -> Line {
    worst_at(COLS)
}

/// [`worst`] at `cols` columns.
fn worst_at(cols: usize) -> Line {
    let cluster: String = std::iter::once('e')
        .chain(std::iter::repeat_n('\u{301}', CLUSTER_MAX - 1))
        .collect();
    let mut line = Line::new(LineId::new(0), cols);
    for (col, n) in (0..cols).zip(0u32..) {
        let glyph = Glyph::Cluster(&cluster);
        line.write(
            col,
            glyph,
            Width::Narrow,
            &distinct(col),
            Some(LinkId::new(n)),
        );
    }
    line.trim();
    line
}

/// vtium's main and alternate screens, 48 blank lines each, and the one
/// set of change ages they share, as the first screen holds them: like for
/// like with `C_SCREENS`.
fn screens_bytes() -> usize {
    let line = Line::new(LineId::new(0), COLS);
    2 * ROWS * line_bytes(&line) + Damage::new(COLS, ROWS).heap_bytes()
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / MIB
}

#[test]
fn the_minspec_grid_and_scrollback() {
    let screens = screens_bytes();
    println!(
        "screens {COLS}x{ROWS}, main and alternate: {screens} bytes, {:.2} MiB; the C reference's {:.2} MiB",
        mib(screens),
        mib(C_SCREENS)
    );
    let common = [SB_SIZE_DEFAULT, SB_SIZE_CAP_SMALL];
    let caps = [
        SB_SIZE_DEFAULT,
        SB_SIZE_CAP_SMALL,
        SB_SIZE_CAP_MEDIUM,
        SB_SIZE_CAP_LARGE,
    ];
    let cases: [(&str, Line, &[usize]); 6] = [
        ("plain text, full width", plain(COLS), &common),
        ("a shell's lines, full width", shell(COLS), &common),
        ("a shell's lines, 60 cells", shell(60), &common),
        ("truecolor art, full width", art(), &common),
        ("the worst case", worst(), &caps),
        (
            "plain text, full width, at each cap",
            plain(COLS),
            &caps[2..],
        ),
    ];
    let mut figures = Vec::new();
    for (name, line, counts) in &cases {
        for &n in *counts {
            let sb = n * line_bytes(line);
            println!(
                "{name}, {n} lines: line {} bytes, scrollback {sb} bytes, {:.2} MiB",
                line_bytes(line),
                mib(sb)
            );
            figures.push(sb);
        }
    }
    // Pinned: ADR-1001's "Memory" quotes the 64-bit figures.
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(screens, SCREENS_64);
        assert_eq!(figures, FIGURES_64);
    }
    // A 32-bit target's lines are smaller (armv7, vtium/vtium#75): pinned,
    // and never above the 64-bit figures.
    #[cfg(target_pointer_width = "32")]
    {
        assert_eq!(screens, 199_680);
        assert_eq!(
            figures,
            [
                1_392_000,
                13_920_000, // plain
                1_460_000,
                14_600_000, // shell, full width
                580_000,
                5_800_000, // shell, 60 cells
                3_812_000,
                38_120_000, // art
                5_512_000,
                55_120_000,
                165_360_000,
                551_200_000, // worst
                41_760_000,
                139_200_000, // plain, the larger caps
            ]
        );
        assert!(screens <= SCREENS_64);
        assert!(figures.iter().zip(FIGURES_64).all(|(&f, pin)| f <= pin));
    }
}

/// The figures on a 64-bit target, in `the_minspec_grid_and_scrollback`'s
/// order: the two screens, then each case at each count.
const SCREENS_64: usize = 201_216;
const FIGURES_64: [usize; 14] = [
    1_408_000,
    14_080_000, // plain
    1_516_000,
    15_160_000, // shell, full width
    636_000,
    6_360_000, // shell, 60 cells
    3_868_000,
    38_680_000, // art
    5_568_000,
    55_680_000,
    167_040_000,
    556_800_000, // worst
    42_240_000,
    140_800_000, // plain, the larger caps
];

/// The most a row of the scrollback can cost at `cols` columns: the worst
/// line as written, and the cluster bytes a rewrap may move into a row
/// past its own cells' budget (`tests/flood.rs`; ADR-1001's note of
/// 2026-10-28).
fn worst_row_at(cols: usize) -> usize {
    line_bytes(&worst_at(cols)) + common::row_bound(cols) - CLUSTER_BYTES_PER_CELL * cols
}

/// [`worst_row_at`] the minspec grid's width.
fn worst_row() -> usize {
    worst_row_at(COLS)
}

/// A cluster of exactly `bytes` UTF-8 bytes, 2 to 128, of at most
/// `CLUSTER_MAX` codepoints: a base of 1 to 4 bytes and 4-byte marks.
fn cluster_of(bytes: usize) -> String {
    match bytes {
        2 => return "ea".into(),
        3 => return "e\u{301}".into(),
        4 => return "e\u{20D0}".into(),
        _ => {}
    }
    let base = ['e', '\u{E9}', '\u{4E2D}', '\u{1F600}'][(bytes - 1) % 4];
    let s: String = std::iter::once(base)
        .chain(std::iter::repeat_n('\u{E0100}', (bytes - 1) / 4))
        .collect();
    assert_eq!(s.len(), bytes);
    assert!(s.chars().count() <= CLUSTER_MAX);
    s
}

/// A style no other write takes: its colors spell `n`.
fn style_of(n: u32) -> Style {
    let [a, b, c, d] = n.to_le_bytes();
    Style {
        fg: Color::Rgb(Rgb::new(a, b, c)),
        bg: Color::Rgb(Rgb::new(d, 1, 0)),
        ..Style::DEFAULT
    }
}

/// The most a screen row of `cols` cells costs, by capacity, measured: rows
/// written through `Line::write`, as printing writes them, each write with
/// a style and a link no other write takes and a cluster, swept across the
/// row until every table has compacted several times. The first cell's
/// cluster sets where the arena starts after each compaction, so every
/// size of it is tried, beside rows of small clusters with a long one now
/// and then and rows written at random places. A screen row is never
/// trimmed, so it is measured as it stands, not as a clone.
fn worst_screen_row(cols: usize) -> usize {
    let mut n = 0u32;
    let mut most = 0;
    let mut next = || {
        n += 1;
        n
    };
    let write = |line: &mut Line, col: usize, bytes: usize, n: u32| {
        let c = cluster_of(bytes);
        line.write(
            col,
            Glyph::Cluster(&c),
            Width::Narrow,
            &style_of(n),
            Some(LinkId::new(n)),
        );
        line_bytes_as_is(line)
    };
    for first in (2..=128).step_by(7).chain([126, 127, 128]) {
        for small in [2, 5, 11] {
            let mut line = Line::new(LineId::new(0), cols);
            for sweep in 0..6 {
                for col in 0..cols {
                    let bytes = match col {
                        0 => first,
                        _ if sweep % 3 == 2 && col % 17 == 0 => 120,
                        _ => small,
                    };
                    most = most.max(write(&mut line, col, bytes, next()));
                }
            }
        }
    }
    let mut rng = common::Rng(0x213);
    for _ in 0..20 {
        let mut line = Line::new(LineId::new(0), cols);
        for _ in 0..8 * cols {
            let col = rng.below(cols);
            let bytes = match rng.below(4) {
                0 => 2 + rng.below(127),
                _ => 2 + rng.below(8),
            };
            most = most.max(write(&mut line, col, bytes, next()));
        }
    }
    most
}

/// What a row costs where it stands, its spare capacity included.
fn line_bytes_as_is(line: &Line) -> usize {
    size_of::<Line>() + line.heap_bytes()
}

/// The grids the peak is checked at: the minspec machine's, and a 640x480
/// and a 320x240 display in 8x16 and 8x8 cells.
const GRIDS: [(usize, usize); 3] = [MINSPEC_GRID, (80, 30), (40, 30)];

// A screen row filled through `write` reaches `Line::bytes_max`, every
// table at the limit it compacts at, and no row passes it.
#[test]
fn a_screen_row_costs_at_most_its_bound() {
    for (cols, _) in GRIDS {
        let row = worst_screen_row(cols);
        println!(
            "a screen row of {cols} cells: at most {row} bytes, bound {}",
            Line::bytes_max(cols)
        );
        assert_eq!(row, Line::bytes_max(cols), "at {cols} columns");
    }
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        GRIDS.map(|(cols, _)| Line::bytes_max(cols)),
        [9_648, 4_608, 2_368]
    );
}

// A rewrap moves clusters with their cells, so a row it builds may name
// twice its own cells' budget, the arena's whole limit; written to again,
// such a row compacts and costs no more than a row `write` fills.
#[test]
fn a_rewrapped_screen_row_costs_at_most_its_bound() {
    let (cols, rows) = MINSPEC_GRID;
    // Rows printed 2 columns wide of a heavy cell and a pad, the wide
    // character after each erased, as `tests/flood.rs` builds them: every
    // row's budget on one cell of text.
    let heavy = format!("e{}", "\u{301}".repeat(5));
    let line = format!(
        "{}\r\n",
        format!("{heavy}\u{4E2D}\r\x1b[2X").repeat(2 * cols)
    );
    let mut t = Terminal::new(2, rows, 1000);
    t.screen_mut().set_scrollback_max(1000);
    t.input(line.repeat(4).as_bytes());
    t.screen_mut().resize(cols, rows);
    let most = t.screen().lines().iter().map(Line::cluster_bytes).max();
    assert_eq!(most, Some(2 * CLUSTER_BYTES_PER_CELL * cols));
    for line in t.screen().lines() {
        assert!(line_bytes_as_is(line) <= Line::bytes_max(cols));
    }
    for row in 1..=rows {
        t.input(format!("\x1b[{row};{cols}H\x1b[38;2;1;2;{row}me\u{301}").as_bytes());
    }
    for line in t.screen().lines() {
        assert!(line_bytes_as_is(line) <= Line::bytes_max(cols));
    }
}

// The worst default scrollback, every row at the bound a rewrap allows,
// the two screens every row at its worst and the link store full, stays
// inside the minspec peak at every grid, taking the rest of vtium's
// process at what the rest of the C reference's cost: its first screen
// less its two screens (ADR-1002, "Link storage"). `sb-memory`'s default
// follows the grid: it is what the peak leaves after the rest, so a
// smaller grid keeps more, and at the minspec grid it drops the oldest of
// the worst rows before the default 1,000 lines (`tests/flood.rs` holds
// the scrollback to it). `tests/links.rs` holds the store's heap at or
// under its charge.
#[test]
fn the_worst_default_scrollback_fits_the_minspec_peak() {
    let rest = C_FIRST_SCREEN - C_SCREENS as f64;
    assert!(MINSPEC_REST as f64 >= rest && MINSPEC_REST as f64 - rest < 1024.0);
    assert_eq!(MINSPEC_PEAK as f64, PEAK);
    let first = rest + screens_bytes() as f64;
    assert!(first <= C_FIRST_SCREEN);
    let mut figures = Vec::new();
    for (cols, rows) in GRIDS {
        let screens = 2 * rows * worst_screen_row(cols) + Damage::new(cols, rows).heap_bytes();
        assert_eq!(screens, screens_bytes_max(cols, rows), "at {cols}x{rows}");
        let budget = sb_memory_default(cols, rows);
        let screen = Screen::new(cols, rows, SB_SIZE_DEFAULT);
        assert_eq!(screen.scrollback_memory(), budget, "at {cols}x{rows}");
        assert!(budget >= SB_MEMORY_MIN);
        let row = worst_row_at(cols);
        let scrollback = (SB_SIZE_DEFAULT * row).min(budget);
        let peak = MINSPEC_REST + screens + scrollback + LINKS_BYTES_MAX;
        println!(
            "{cols}x{rows}: screens at worst {screens} bytes; sb-memory {budget} bytes, {} rows of at most {row} bytes; worst peak {peak} bytes, {:.2} MiB, against {:.2} MiB",
            budget / row,
            peak as f64 / MIB,
            PEAK / MIB
        );
        assert!(peak <= MINSPEC_PEAK, "at {cols}x{rows}: {peak} bytes");
        figures.push((screens, budget, peak));
        // The smaller grids keep the default 1,000 lines whatever they
        // hold; the minspec grid keeps them for full-width shell lines.
        if (cols, rows) == MINSPEC_GRID {
            assert!(SB_SIZE_DEFAULT * line_bytes(&shell(COLS)) <= budget);
        } else {
            assert!(SB_SIZE_DEFAULT * row <= budget, "at {cols}x{rows}");
        }
    }
    assert_eq!(figures[0].1, SB_MEMORY_DEFAULT);
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(worst_row(), 7_560);
        assert_eq!(
            figures,
            [
                (992_256, 5_242_880, 16_776_192),
                (296_160, 5_898_240, 14_461_216),
                (152_160, 6_029_312, 12_569_216),
            ]
        );
    }
}

// The scrollback's budget follows the grid while the configuration leaves
// `sb-memory` unset: a resize works it out again from the rows the screens
// hold, and the oldest lines past a lower budget go. A set budget stays
// through a resize.
#[test]
fn the_scrollback_budget_follows_the_grid() {
    let mut t = Terminal::new(40, 30, 10_000);
    t.screen_mut().set_scrollback_max(10_000);
    let budget = sb_memory_default(40, 30);
    assert_eq!(t.screen().scrollback_memory(), budget);
    // Lines of truecolor art, every cell its own colors.
    let line: String = (0..40)
        .map(|c| format!("\x1b[38;2;{c};0;0;48;2;0;{c};0m\u{2580}"))
        .chain(["\r\n".into()])
        .collect();
    for _ in 0..10_000 {
        if t.screen().scrollback_bytes() + 4096 >= budget {
            break;
        }
        t.input(line.as_bytes());
    }
    assert!(t.screen().scrollback_bytes() + 4096 >= budget);
    t.screen_mut().resize(COLS, ROWS);
    assert_eq!(t.screen().scrollback_memory(), SB_MEMORY_DEFAULT);
    assert!(t.screen().scrollback_bytes() <= SB_MEMORY_DEFAULT);
    // Shrunk again, the screens keep the 48 rows below the 30 they show,
    // and the budget prices them.
    t.screen_mut().resize(40, 30);
    assert!(t.screen().scrollback_memory() < budget);
    t.screen_mut().set_scrollback_memory_from_grid(2 << 20);
    assert_eq!(t.screen().scrollback_memory(), 2 << 20);
    t.screen_mut().set_scrollback_memory(3 << 20);
    t.screen_mut().resize(COLS, ROWS);
    assert_eq!(t.screen().scrollback_memory(), 3 << 20);
}

// The C reference's 1,000 lines of 32-byte cells, derived from its layout.
#[test]
fn the_default_scrollback_costs_less_than_the_c_references() {
    let c_cells = SB_SIZE_DEFAULT * COLS * C_CELL;
    let ours = SB_SIZE_DEFAULT * line_bytes(&shell(COLS));
    assert!(ours * 3 < c_cells);
}
