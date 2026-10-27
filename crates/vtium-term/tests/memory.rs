//! The resident size of the cell model on the minspec machine: a 1366x768
//! display in unifont's 8x16 cells is 170 columns by 48 rows (ADR-1001,
//! "Memory"). Each figure is computed from the structures themselves,
//! by capacity, and pinned, so a change to the model changes the record.
//!
//! `cargo test -p vtium-term --test memory -- --nocapture` prints the table.

use vtium_term::{
    Color, Damage, Glyph, Line, LineId, Rgb, SB_SIZE_CAP_SMALL, SB_SIZE_DEFAULT, Style, Width,
};

const COLS: usize = 170;
const ROWS: usize = 48;
const MIB: f64 = 1024.0 * 1024.0;

/// The bytes one line costs: its struct, held in the scrollback's ring or
/// the screen's rows, and its heap.
fn line_bytes(line: &Line) -> usize {
    size_of::<Line>() + line.heap_bytes()
}

/// A line of `len` cells of plain text, trimmed as the scrollback keeps it.
fn plain(len: usize) -> Line {
    let mut line = Line::new(LineId::new(0), COLS);
    for col in 0..len {
        line.write(col, Glyph::Char('x'), Width::Narrow, &Style::DEFAULT, None);
    }
    line.trim();
    line
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
    let mut line = Line::new(LineId::new(0), COLS);
    for col in 0..len {
        let style = match col {
            0..13 => green,
            13..21 => blue,
            _ => Style::DEFAULT,
        };
        line.write(col, Glyph::Char('x'), Width::Narrow, &style, None);
    }
    line.trim();
    line
}

/// A line of truecolour art: half blocks, every cell its own foreground
/// and background, as an image printed as text gives.
fn art() -> Line {
    let mut line = Line::new(LineId::new(0), COLS);
    for col in 0..COLS {
        let c = u8::try_from(col).unwrap();
        let style = Style {
            fg: Color::Rgb(Rgb::new(c, 0, 0)),
            bg: Color::Rgb(Rgb::new(0, c, 0)),
            ..Style::DEFAULT
        };
        line.write(col, Glyph::Char('\u{2580}'), Width::Narrow, &style, None);
    }
    line.trim();
    line
}

/// The screen: 48 full lines and their damage.
fn screen_bytes() -> usize {
    let line = Line::new(LineId::new(0), COLS);
    ROWS * line_bytes(&line) + Damage::new(COLS, ROWS).heap_bytes()
}

/// `n` lines in the scrollback, each a copy of `line`. A copy holds exactly
/// its length, as a trimmed line does.
fn scrollback_bytes(line: &Line, n: usize) -> usize {
    let lines: Vec<Line> = (0..n).map(|_| line.clone()).collect();
    lines.iter().map(line_bytes).sum()
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / MIB
}

#[test]
fn the_minspec_grid_and_scrollback() {
    let screen = screen_bytes();
    let cases = [
        ("plain text, full width", plain(COLS)),
        ("a shell's lines, full width", shell(COLS)),
        ("a shell's lines, 60 cells", shell(60)),
        ("truecolour art, full width", art()),
    ];
    println!(
        "screen {COLS}x{ROWS}: {screen} bytes, {:.2} MiB",
        mib(screen)
    );
    let mut figures = Vec::new();
    for (name, line) in &cases {
        for n in [SB_SIZE_DEFAULT, SB_SIZE_CAP_SMALL] {
            let sb = scrollback_bytes(line, n);
            let total = screen + sb;
            println!(
                "{name}, {n} lines: line {} bytes, scrollback {:.2} MiB, with the screen {:.2} MiB",
                line_bytes(line),
                mib(sb),
                mib(total)
            );
            figures.push(total);
        }
    }
    // Pinned: ADR-1001's "Memory" quotes these.
    assert_eq!(screen, 133_632);
    assert_eq!(
        figures,
        [
            1_541_632, 14_213_632, // plain
            1_665_632, 15_453_632, // shell, full width
            785_632, 6_653_632, // shell, 60 cells
            4_017_632, 38_973_632, // art
        ]
    );
}

// The C reference's cell was 32 bytes; its measured peak (15.7 MiB with
// unifont) held a full 1,000-line scrollback of them, 5.19 MiB of cells.
#[test]
fn the_default_scrollback_costs_less_than_the_c_references() {
    let c_cells = SB_SIZE_DEFAULT * COLS * 32;
    let ours = scrollback_bytes(&shell(COLS), SB_SIZE_DEFAULT);
    println!(
        "1,000 full shell lines: {:.2} MiB, against the C reference's {:.2} MiB",
        mib(ours),
        mib(c_cells)
    );
    assert!(ours * 3 < c_cells);
}
