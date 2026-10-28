//! The resident size of the cell model on tier 3's x86 variant of the
//! minspec machine (docs/architecture.md, "The minspec build"): a 1366x768
//! display in unifont's 8x16 cells is 170 columns by 48 rows (ADR-1001,
//! "Memory"). Each figure is computed from the structures themselves, by
//! capacity, and pinned, so a change to the model changes the record.
//!
//! `cargo test -p vtium-term --test memory -- --nocapture` prints the table.

use vtium_term::{
    CLUSTER_MAX, Color, Damage, Glyph, Line, LineId, LinkId, Rgb, SB_SIZE_CAP_LARGE,
    SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_DEFAULT, Style, Width,
};

const COLS: usize = 170;
const ROWS: usize = 48;
const MIB: f64 = 1024.0 * 1024.0;

/// The C reference's figures with unifont on the minspec machine, measured
/// (kmscon's README, "Minimal build"; 1 MiB is 1,024 KiB): resident after
/// the first screen, and the peak once 500,000 lines had filled its
/// 1,000-line scrollback.
const C_FIRST_SCREEN: f64 = 10.3 * MIB;
const C_PEAK: f64 = 15.7 * MIB;

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
    let cluster: String = std::iter::once('e')
        .chain(std::iter::repeat_n('\u{301}', CLUSTER_MAX - 1))
        .collect();
    let mut line = Line::new(LineId::new(0), COLS);
    for (col, n) in (0..COLS).zip(0u32..) {
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

/// vtium's main and alternate screens, 48 full lines each, with their
/// damage: like for like with `C_SCREENS`.
fn screens_bytes() -> usize {
    let line = Line::new(LineId::new(0), COLS);
    2 * (ROWS * line_bytes(&line) + Damage::new(COLS, ROWS).heap_bytes())
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
        assert_eq!(screens, 265_728);
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
const SCREENS_64: usize = 267_264;
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

// The worst case at the default scrollback stays inside the C reference's
// measured peak, taking the rest of vtium's process at what the rest of
// the C reference's cost: its first screen less its two screens.
#[test]
fn the_worst_default_scrollback_fits_the_minspec_peak() {
    let rest = C_FIRST_SCREEN - C_SCREENS as f64;
    let first = rest + screens_bytes() as f64;
    let peak = first + (SB_SIZE_DEFAULT * line_bytes(&worst())) as f64;
    println!(
        "first screen about {:.2} MiB; worst peak at {SB_SIZE_DEFAULT} lines about {:.2} MiB, against {:.2} MiB",
        first / MIB,
        peak / MIB,
        C_PEAK / MIB
    );
    assert!(first <= C_FIRST_SCREEN);
    assert!(peak <= C_PEAK);
}

// The C reference's 1,000 lines of 32-byte cells, derived from its layout.
#[test]
fn the_default_scrollback_costs_less_than_the_c_references() {
    let c_cells = SB_SIZE_DEFAULT * COLS * C_CELL;
    let ours = SB_SIZE_DEFAULT * line_bytes(&shell(COLS));
    assert!(ours * 3 < c_cells);
}
