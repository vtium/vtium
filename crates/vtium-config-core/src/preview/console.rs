//! The lines a vtium console draws as a tab's preview, in its own font,
//! where a picture would go elsewhere, and the sample screen that the
//! Looks and Effects tabs' pictures show.
//!
//! The lines are the content the pictures show, taken from where the
//! pictures take it: the sample screen ([`sample_rows`]), or the detailed
//! specimen's lines for the locale ([`crate::specimen::detail_lines`]).
//! The console draws them in the font it draws now, so after `t` they show
//! the font tried, and the sample screen's colors are the console's own
//! palette. Another terminal draws none of them: there the preview is a
//! picture, or a line that describes the font.
//!
//! References:
//! - ADR-1201 D4: the preview as text in a vtium console.

use vtium_config_widgets::canvas::{self, Row, put, span};
use vtium_config_widgets::color::{Color, Style};

use super::{Place, Preview};

/// The sample content of the Looks and Effects tabs' preview, one line a
/// row, each run of text with its style: a shell prompt, a short listing,
/// the eight colors and their bright forms, bold and italic text, box
/// drawing and accented letters. It holds no wide character: the picture
/// of a screen gives each character one cell, and a wide one needs two. It
/// stays fixed, so the pictures stay comparable.
pub fn sample_rows() -> Vec<Row> {
    let c = Style::text;
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    let italic = Style {
        italic: true,
        ..Style::PLAIN
    };
    let plain = |text: &str| vec![span(text, Style::PLAIN)];
    let prompt = |command: &str| {
        vec![
            span("user@desk", c(Color::BrightGreen)),
            span(":", Style::PLAIN),
            span("~/notes", c(Color::Cyan)),
            span(command, Style::PLAIN),
        ]
    };
    vec![
        prompt("$ ls -l"),
        vec![
            span("drwxr-xr-x  2 user user 4096 Nov 10 09:12 ", Style::PLAIN),
            span("drafts", c(Color::Cyan)),
        ],
        plain("-rw-r--r--  1 user user 1823 Nov 10 09:14 plan.txt"),
        vec![
            span("-rwxr-xr-x  1 user user  512 Nov  9 17:40 ", Style::PLAIN),
            span("build.sh", c(Color::BrightGreen)),
        ],
        prompt("$ colors"),
        vec![
            span("red ", c(Color::Red)),
            span("green ", c(Color::Green)),
            span("yellow ", c(Color::Yellow)),
            span("magenta ", c(Color::Magenta)),
            span("cyan ", c(Color::Cyan)),
            span("white ", c(Color::White)),
        ],
        vec![
            span("grey ", c(Color::Grey)),
            span("red ", c(Color::BrightRed)),
            span("green ", c(Color::BrightGreen)),
            span("yellow ", c(Color::BrightYellow)),
            span("magenta ", c(Color::BrightMagenta)),
            span("cyan ", c(Color::BrightCyan)),
            span("white", c(Color::BrightWhite)),
        ],
        vec![
            span("Bold text", bold),
            span(" and ", Style::PLAIN),
            span("italic text", italic),
            span(" beside plain text.", Style::PLAIN),
        ],
        plain("┌──────────┬──────────┐"),
        plain("│ name     │ size     │"),
        plain("├──────────┼──────────┤"),
        plain("│ plan.txt │ 1.8 KiB  │"),
        plain("└──────────┴──────────┘"),
        plain("Accents: café, naïve, Ångström, Straße, žluťoučký"),
        prompt("$ "),
    ]
}

/// The sample screen laid out at `grid`, columns by rows, its content from
/// the top left: each row holds `cols` characters, the sample's text cut
/// at the right edge, then blanks.
pub fn sample_screen((cols, rows): (usize, usize)) -> Vec<Row> {
    let lines = sample_rows();
    (0..rows)
        .map(|y| {
            let mut row: Row = Vec::new();
            put(&mut row, 0, &" ".repeat(cols), Style::PLAIN);
            let mut x = 0usize;
            for s in lines.get(y).into_iter().flatten() {
                let shown: String = s.text.chars().take(cols.saturating_sub(x)).collect();
                x = put(&mut row, x, &shown, s.style);
            }
            row
        })
        .collect()
}

/// Which content a vtium console draws as the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The sample screen.
    Sample,
    /// The detailed specimen's lines for the locale.
    Detail,
}

/// The lines a vtium console draws as `preview`, in its own font: `shown`
/// where the preview is the console's own text, and none where it is a
/// picture or a line that describes the font, as in another terminal, or
/// where the tab previews nothing.
pub fn lines(preview: &Preview<'_>, shown: Shown) -> Vec<Row> {
    if !preview.in_console() || matches!(preview.source, super::Source::Nothing) {
        return Vec::new();
    }
    match shown {
        Shown::Sample => sample_rows(),
        Shown::Detail => crate::specimen::detail_lines(&preview.facts.locale)
            .into_iter()
            .map(|line| vec![span(line, Style::PLAIN)])
            .collect(),
    }
}

/// One line of the console's preview where the view draws it: its row and
/// first column on the screen, counted from 0, and the line cut to the
/// place's width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub row: u16,
    pub col: u16,
    pub line: Row,
}

/// `lines` laid out in `place`: as many as its rows hold, from its top
/// row, each cut at its right edge by the console's width rule, which
/// gives a wide character two cells.
pub fn placed(lines: Vec<Row>, place: Place) -> Vec<Placed> {
    let width = usize::from(place.width);
    (place.y..place.y.saturating_add(place.height))
        .zip(lines)
        .map(|(row, line)| Placed {
            row,
            col: place.x,
            line: cut(line, width),
        })
        .collect()
}

/// `row` cut to `cells` of the screen, a character that would cross the
/// edge left out whole.
fn cut(row: Row, cells: usize) -> Row {
    let mut used = 0usize;
    let mut out: Row = Vec::new();
    for s in row {
        let mut text = String::new();
        for c in s.text.chars() {
            let w = canvas::width(&c.to_string());
            if used.saturating_add(w) > cells {
                break;
            }
            used = used.saturating_add(w);
            text.push(c);
        }
        let full = text.len() == s.text.len();
        if !text.is_empty() {
            out.push(canvas::Span { text, ..s });
        }
        if !full {
            break;
        }
    }
    out
}
