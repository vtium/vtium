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

/// The minspec target for the resident size after the first screen, with
/// unifont: 10.3 MiB, 1 MiB being 1,024 KiB (ADR-1001, "Memory"). The
/// figure was measured on a console whose two screens held 32-byte cells,
/// [`WIDE_SCREENS`], so the rest of the process costs this less those
/// screens.
const FIRST_SCREEN_TARGET: f64 = 10.3 * MIB;

/// The minspec peak vtium holds to: the maintainer's round figure
/// (vtium/vtium#131).
const PEAK: f64 = 16.0 * MIB;

/// The cell of the console the first-screen figure was measured on: 32
/// bytes, its change age included, in a main and an alternate screen.
/// These are derived from that layout, not measured.
const WIDE_CELL: usize = 32;
const WIDE_SCREENS: usize = 2 * ROWS * COLS * WIDE_CELL;

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
/// like with `WIDE_SCREENS`.
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
        "screens {COLS}x{ROWS}, main and alternate: {screens} bytes, {:.2} MiB; at 32 bytes a cell {:.2} MiB",
        mib(screens),
        mib(WIDE_SCREENS)
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

/// A kitty placeholder cluster, U+10EEEE and `marks` 4-byte marks, the
/// first three of which a line keeps: up to 17 bytes with its length byte.
/// A placeholder alone is a character, so it has at least one mark.
fn placeholder(marks: usize) -> String {
    std::iter::once('\u{10EEEE}')
        .chain(std::iter::repeat_n('\u{E0100}', marks.max(1)))
        .collect()
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
    // Rows of placeholder clusters, which sit outside the budget and raise
    // the arena's limit by 17 bytes a cell (ADR-1001, "Placeholder
    // cells"), each with its three marks or fewer, the first cell's size
    // swept as above, beside rows of placeholders among long clusters.
    for first in 0..=3 {
        for long in [false, true] {
            let mut line = Line::new(LineId::new(0), cols);
            for sweep in 0..6 {
                for col in 0..cols {
                    let n = next();
                    let c = match col {
                        0 => placeholder(first),
                        _ if long && sweep % 3 == 2 && col % 17 == 0 => cluster_of(120),
                        _ => placeholder(3),
                    };
                    line.write(
                        col,
                        Glyph::Cluster(&c),
                        Width::Narrow,
                        &style_of(n),
                        Some(LinkId::new(n)),
                    );
                    most = most.max(line_bytes_as_is(&line));
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
    {
        #[cfg(feature = "images")]
        let want = [15_428, 7_328, 3_728];
        #[cfg(not(feature = "images"))]
        let want = [9_648, 4_608, 2_368];
        assert_eq!(GRIDS.map(|(cols, _)| Line::bytes_max(cols)), want);
    }
}

// A rewrap moves clusters with their cells, so a row it builds may name
// more than its own cells' budget. A row that lands on the screen keeps at
// most twice that budget, the arena's whole limit; here every row reaches
// it. Written to again, such a row compacts and costs no more than a row
// `write` fills.
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

/// A cell printed in a truecolor style and an OSC 8 link of its own,
/// numbered `n`, holding a cluster of 125 bytes, `e` and 31 marks, or a
/// plain `x` when it is not `heavy`.
fn dense_cell(n: u32, heavy: bool) -> String {
    let [a, b, c, d] = n.to_le_bytes();
    let glyph = if heavy { cluster_of(125) } else { "x".into() };
    format!("\x1b[38;2;{a};{b};{c};48;2;{d};1;0m\x1b]8;;http://x/{n}\x1b\\{glyph}")
}

/// `cells` cells, numbered on from `n`, the first `heavy` of them holding
/// a long cluster, then the style and the link closed.
fn dense_run(n: &mut u32, cells: usize, heavy: usize) -> String {
    let mut s = String::new();
    for i in 0..cells {
        *n += 1;
        s.push_str(&dense_cell(*n, i < heavy));
    }
    s.push_str("\x1b]8;;\x1b\\\x1b[m");
    s
}

/// What each row of a screen shows, cell by cell, and the cluster bytes it
/// names.
fn shown(screen: &Screen) -> Vec<(Vec<String>, usize)> {
    screen
        .lines()
        .iter()
        .map(|line| {
            let cells = line
                .cells()
                .iter()
                .map(|&c| match line.glyph(c) {
                    Glyph::Empty => String::new(),
                    Glyph::Char(c) => c.to_string(),
                    Glyph::Cluster(s) => s.to_string(),
                })
                .collect();
            (cells, line.cluster_bytes())
        })
        .collect()
}

/// Holds every row of `screen`, `cols` wide, to `Line::bytes_max`, and the
/// screen to its share of `screens_bytes_max`.
fn check_rows(screen: &Screen, cols: usize, what: &str) {
    let mut total = 0;
    for (y, line) in screen.lines().iter().enumerate() {
        let bytes = line_bytes_as_is(line);
        assert!(
            bytes <= Line::bytes_max(cols),
            "{what}, row {y}: {bytes} bytes, bound {}",
            Line::bytes_max(cols)
        );
        total += bytes;
    }
    assert!(
        total <= screen.rows() * Line::bytes_max(cols),
        "{what}: {total} bytes"
    );
}

// A row narrowed from a much wider grid keeps the clusters writing it at
// the new width keeps, so it costs no more than `Line::bytes_max`: on the
// main screen, which rewraps, and on the alternate screen, which cuts its
// rows. Each narrowed screen shows what the same text printed at the
// narrow width shows.
#[test]
fn a_screen_narrowed_from_a_wide_grid_holds_its_rows_to_their_bound() {
    let (cols, rows) = MINSPEC_GRID;
    // Lines of 170 long clusters on a grid 4,000 wide, each its own row,
    // and lines as wide as a grid 1,300 wide, which rewrap onto eight rows
    // each: their 61 long clusters fit the wide line's budget and land on
    // its first row.
    for (wide, cells, heavy, lines) in [(4000, 170, 170, rows + 4), (1300, 1300, 61, 7)] {
        let mut n = 0;
        let text: String = (0..lines)
            .map(|_| dense_run(&mut n, cells, heavy) + "\r\n")
            .collect();
        let mut t = Terminal::new(wide, rows, 1000);
        t.input(text.as_bytes());
        t.screen_mut().resize(cols, rows);
        check_rows(t.screen(), cols, &format!("main, from {wide}"));
        let mut narrow = Terminal::new(cols, rows, 1000);
        narrow.input(text.as_bytes());
        assert!(
            shown(t.screen()) == shown(narrow.screen()),
            "main, from {wide}"
        );
    }
    for wide in [4000, 1300] {
        let mut n = 0;
        let text: String = std::iter::once("\x1b[?1049h".to_string())
            .chain((1..=rows).map(|row| format!("\x1b[{row};1H{}", dense_run(&mut n, cols, cols))))
            .collect();
        let mut t = Terminal::new(wide, rows, 1000);
        t.input(text.as_bytes());
        t.screen_mut().resize(cols, rows);
        check_rows(t.screen(), cols, &format!("alternate, from {wide}"));
        let mut narrow = Terminal::new(cols, rows, 1000);
        narrow.input(text.as_bytes());
        assert!(
            shown(t.screen()) == shown(narrow.screen()),
            "alternate, from {wide}"
        );
    }
}

/// Clusters that fill `budget` bytes of a row's arena, length bytes
/// included, in as few cells as they fit in: clusters of 125 bytes, and
/// shorter ones for the rest.
fn budget_in_few_cells(budget: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut left = budget;
    while left >= 3 {
        let mut take = left.min(126);
        if matches!(left - take, 1 | 2) {
            take -= 3;
        }
        out.push(cluster_of(take - 1));
        left -= take;
    }
    out
}

/// One logical line printed `w` columns wide, which a widening to `cols`
/// gathers onto one row: the source row that ends `p` cells into a new
/// row holds its whole budget in its last cells, and the two rows after
/// it hold theirs in their first cells, so the new row takes the end of
/// one source row, the whole of the next and the start of the one after.
/// Every cell has its own style, and each cell of a row its own link, the
/// links shared by column so the link store holds them all. Returns the
/// text and its rows, or `None` when no source row ends `p` cells into a
/// new row among the first `rows`, or when the budget takes more than `p`
/// cells.
fn gathered_by_widening(w: usize, cols: usize, p: usize, rows: usize) -> Option<(String, usize)> {
    let j = (0..rows).find(|&j| (cols * j) % w == w - p)?;
    let a = cols * j / w;
    let lines = a + 4;
    let full = budget_in_few_cells(w * CLUSTER_BYTES_PER_CELL);
    if lines > rows || full.len() > p {
        return None;
    }
    let mut n = 0u32;
    let mut s = String::new();
    for r in 0..lines {
        for c in 0..w {
            n += 1;
            let glyph = if r == a && c >= w - full.len() {
                full[c - (w - full.len())].clone()
            } else if (r == a + 1 || r == a + 2) && c < full.len() {
                full[c].clone()
            } else {
                "x".into()
            };
            let [b0, b1, b2, b3] = n.to_le_bytes();
            s.push_str(&format!(
                "\x1b[38;2;{b0};{b1};{b2};48;2;{b3};1;0m\x1b]8;;http://x/{c}\x1b\\{glyph}"
            ));
        }
    }
    s.push_str("\x1b]8;;\x1b\\\x1b[m");
    Some((s, lines))
}

/// Writes a new style and a new link into every cell of `t`'s screen that
/// holds no cluster, six times over, so each row's style and link tables
/// fill to the limit they compact at. The links are shared by sweep and
/// column, so the link store holds them all.
fn sweep_styles_and_links(t: &mut Terminal, mut n: u32) {
    for sweep in 0..6 {
        for row in 0..t.screen().rows() {
            let line = &t.screen().lines()[row];
            let free: Vec<usize> = (0..line.len())
                .filter(|&c| !matches!(line.glyph(line.cell(c)), Glyph::Cluster(_)))
                .collect();
            let mut s = String::new();
            for c in free {
                n += 1;
                let [b0, b1, b2, b3] = n.to_le_bytes();
                s.push_str(&format!(
                    "\x1b[{};{}H\x1b[38;2;{b0};{b1};{b2};48;2;{b3};1;0m\x1b]8;;http://y/{sweep}/{c}\x1b\\y",
                    row + 1,
                    c + 1
                ));
            }
            s.push_str("\x1b]8;;\x1b\\\x1b[m");
            t.input(s.as_bytes());
        }
    }
}

// A widening by a factor that is not a whole number can gather onto one
// row the end of one source row, the whole of the next and the start of
// the one after. When each of those source rows holds its whole budget in
// a few cells, the new row names up to about 16.5 bytes a cell, past its arena's
// limit of 12. On the screen, such a row keeps the clusters that fit its
// arena's limit, from its first cell on, so with its own style and link in
// every other cell it costs no more than `Line::bytes_max`: as the screen
// widens, and as it grows taller and pulls the row back from the
// scrollback. The scrollback keeps the row's clusters whole.
#[test]
fn a_screen_widened_by_a_fraction_holds_its_rows_to_their_bound() {
    let rows = 60;
    for (w, cols, p) in [(15, 17, 1), (28, 33, 2), (53, 65, 5), (110, 129, 7)] {
        let arena = 2 * CLUSTER_BYTES_PER_CELL * cols;
        let (text, lines) = gathered_by_widening(w, cols, p, rows).unwrap();
        let mut t = Terminal::new(w, rows, 1000);
        t.input(text.as_bytes());
        t.screen_mut().resize(cols, rows);
        sweep_styles_and_links(&mut t, 1 << 20);
        check_rows(t.screen(), cols, &format!("{w} to {cols}"));
        let most = t.screen().lines().iter().map(Line::cluster_bytes).max();
        assert!(
            most.is_some_and(|b| b > arena / 2 && b <= arena),
            "{w} to {cols}: {most:?}"
        );

        // Printed onto a screen of 4 rows and pushed into the scrollback,
        // the line keeps its clusters whole there through the widening.
        let mut t = Terminal::new(w, 4, 1000);
        t.screen_mut().set_scrollback_max(1000);
        t.input(text.as_bytes());
        t.input("\r\n".repeat(4).as_bytes());
        t.screen_mut().resize(cols, 4);
        t.screen_mut().rewrap_scrollback();
        let most = t.screen().scrollback().map(Line::cluster_bytes).max();
        assert!(
            most.is_some_and(|b| b > arena),
            "{w} to {cols}, scrollback: {most:?}"
        );
        t.screen_mut().resize(cols, lines + 4);
        sweep_styles_and_links(&mut t, 1 << 20);
        check_rows(t.screen(), cols, &format!("{w} to {cols}, pulled back"));
        let most = t.screen().lines().iter().map(Line::cluster_bytes).max();
        assert!(
            most.is_some_and(|b| b > arena / 2 && b <= arena),
            "{w} to {cols}, pulled back: {most:?}"
        );
    }
}

// Rows edited, cut and widened at random hold to `Line::bytes_max` after
// every step: one row on its own, and the rows of both screens of a
// terminal whose grid grows and shrinks, with the scrollback's rows
// pulled back onto a taller screen.
#[test]
fn rows_hold_to_their_bound_through_random_edits_and_resizes() {
    let mut rng = common::Rng(0x0fee_d213);
    for trial in 0..150 {
        let cols = 1 + rng.below(120);
        let mut line = Line::new(LineId::new(1), cols);
        for n in 0..20 * u32::try_from(cols).unwrap() {
            let len = line.len().max(1);
            match rng.below(10) {
                0..=5 => {
                    let bytes = match rng.below(4) {
                        0 => 2 + rng.below(127),
                        _ => 2 + rng.below(8),
                    };
                    let c = cluster_of(bytes);
                    let glyph = match rng.below(5) {
                        0 => Glyph::Char('x'),
                        _ => Glyph::Cluster(&c),
                    };
                    let width = match rng.below(8) {
                        0 => Width::Wide,
                        _ => Width::Narrow,
                    };
                    let link = (rng.below(3) != 0).then(|| LinkId::new(n));
                    line.write(rng.below(len), glyph, width, &style_of(n), link);
                }
                6 => {
                    let at = rng.below(len);
                    line.erase(at..at + rng.below(len), &style_of(n));
                }
                7 => line.insert_cells(rng.below(len), 1 + rng.below(4), len, &style_of(n)),
                8 => line.delete_cells(rng.below(len), 1 + rng.below(4), len, &style_of(n)),
                _ if rng.below(10) == 0 => line.resize(1 + rng.below(200)),
                _ => {}
            }
            let bytes = line_bytes_as_is(&line);
            assert!(
                bytes <= Line::bytes_max(line.len()),
                "trial {trial}: a row of {} cells holds {bytes} bytes, bound {}",
                line.len(),
                Line::bytes_max(line.len())
            );
        }
    }

    let mut t = Terminal::new(30, 6, 200);
    t.screen_mut().set_scrollback_max(200);
    let mut n = 0;
    for step in 0..1500 {
        let cells = 1 + rng.below(40);
        let heavy = rng.below(cells + 1);
        let mut s = dense_run(&mut n, cells, heavy);
        if rng.below(3) == 0 {
            s.push_str("\r\n");
        }
        t.input(s.as_bytes());
        match rng.below(12) {
            0..=2 => t.screen_mut().resize(1 + rng.below(90), 1 + rng.below(10)),
            3 => {
                let on = rng.below(2) == 0;
                t.input(if on { b"\x1b[?1049h" } else { b"\x1b[?1049l" });
            }
            _ => {}
        }
        let cols = t.screen().cols();
        check_rows(t.screen(), cols, &format!("step {step}"));
    }
}

// The worst default scrollback, every row at the bound a rewrap allows,
// the two screens every row at its worst and the link store full, stays
// inside the minspec peak on each of minspec's grids, taking the rest of vtium's
// process at the first-screen target less two screens of 32-byte cells
// (ADR-1002, "Link storage"). `sb-memory`'s default
// follows the grid: it is what the peak leaves after the rest, so a
// smaller grid keeps more, and at the minspec grid it drops the oldest of
// the worst rows before the default 1,000 lines (`tests/flood.rs` holds
// the scrollback to it). `tests/links.rs` holds the store's heap at or
// under its charge.
#[test]
fn the_worst_default_scrollback_fits_the_minspec_peak() {
    let rest = FIRST_SCREEN_TARGET - WIDE_SCREENS as f64;
    assert!(MINSPEC_REST as f64 >= rest && MINSPEC_REST as f64 - rest < 1024.0);
    assert_eq!(MINSPEC_PEAK as f64, PEAK);
    let first = rest + screens_bytes() as f64;
    assert!(first <= FIRST_SCREEN_TARGET);
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
        // with images, a screen row may hold placeholder clusters outside
        // its budget (ADR-1001, "Placeholder cells")
        #[cfg(feature = "images")]
        let want = (
            10_450,
            [
                (1_547_136, 4_653_056, 16_741_248),
                (459_360, 5_767_168, 15_984_416),
                (233_760, 5_963_776, 13_330_816),
            ],
        );
        #[cfg(not(feature = "images"))]
        let want = (
            7_560,
            [
                (992_256, 5_242_880, 16_776_192),
                (296_160, 5_898_240, 14_461_216),
                (152_160, 6_029_312, 12_569_216),
            ],
        );
        assert_eq!(
            (worst_row(), figures.as_slice()),
            (want.0, want.1.as_slice())
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
    // Past minspec's grids, as on a 3840x2160 display in 8x16 cells, the
    // default stays at the minspec grid's value and never falls to the
    // fence's floor; the console uses more than the minspec peak.
    let mut t = Terminal::new(COLS, ROWS, 1000);
    assert_eq!(t.screen().scrollback_memory(), SB_MEMORY_DEFAULT);
    t.screen_mut().resize(480, 135);
    assert_eq!(t.screen().scrollback_memory(), SB_MEMORY_DEFAULT);
    assert!(screens_bytes_max(480, 135) > screens_bytes_max(COLS, ROWS));
}

// The default 1,000 lines of a shell cost under a third of 1,000 lines of
// 32-byte cells, derived from that layout.
#[test]
fn the_default_scrollback_costs_under_a_third_of_32_byte_cells() {
    let wide_cells = SB_SIZE_DEFAULT * COLS * WIDE_CELL;
    let ours = SB_SIZE_DEFAULT * line_bytes(&shell(COLS));
    assert!(ours * 3 < wide_cells);
}
