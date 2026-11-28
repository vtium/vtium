//! Show changes' lines and how they are drawn. A line of a hunk carries its
//! number in the document's vtium.conf: an added line reads `  7 +text`
//! and is green, a removed one `  15 -text` and is red, and a line of
//! context reads `    13  text`, two cells further in, as ADR-1403 D11's
//! figure draws it. The words that changed inside a line are bold. In the
//! whole file, a line the check complains about starts with `!`, its
//! number in the problems' role.
//!
//! The first row holds the title, its grey words and the checked pair at
//! the right; the second is blank; the lines follow from the third. Where
//! they do not fit, the last row says which lines show.
//!
//! References:
//! - ADR-1403 D11, "Show changes and Edit as text": the figure's layout,
//!   its colors and its marks.
//! - ADR-1206 D4, the roles' table: the diff roles.
//! - ADR-1206 D10: every mark a color carries is also a character or an
//!   attribute, so the screen reads the same under `NO_COLOR`.

use std::ops::Range;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::text::Span;
use vtium_config_core::document::Review;
use vtium_config_core::drafts::FritEdit;
use vtium_config_core::versions::diff::{Diff, Line, Mark};
use vtium_config_widgets::roles::Role;

/// The cells the checked pair takes: `✓ Changes only    Whole file`.
pub const PAIR_CELLS: u16 = 28;

/// The cells of the pair's first item, `✓ Changes only`.
pub const CHANGES_ONLY_CELLS: u16 = 14;

/// Where the pair's second item starts, from the pair's first cell.
pub const WHOLE_FILE_AT: u16 = 16;

/// The most cells the header leaves at the right of the pair.
const RIGHT: u16 = 6;

/// The fewest cells a line's number takes beside its `+` or `-`.
const NUMBER: usize = 3;

/// One line under the title, as the screen draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The line from the viewer's first column.
    pub text: String,
    pub role: Role,
    /// The byte ranges of `text` whose words changed.
    pub words: Vec<Range<usize>>,
    /// The line's number in the document's vtium.conf, where it has one.
    pub line: Option<usize>,
    /// Whether the check complains about the line.
    pub complained: bool,
}

impl Shown {
    fn of(text: String, role: Role) -> Shown {
        Shown {
            text,
            role,
            words: Vec::new(),
            line: None,
            complained: false,
        }
    }

    /// An empty line.
    pub fn blank() -> Shown {
        Shown::of(String::new(), Role::Row)
    }

    /// A line of words, after the margin.
    pub fn note(text: &str) -> Shown {
        Shown::of(format!(" {text}"), Role::Row)
    }

    fn grey(text: &str) -> Shown {
        Shown::of(format!(" {text}"), Role::Dim)
    }
}

/// The cells a line's number takes beside its `+` or `-`, for numbers up
/// to `last`.
fn number_cells(last: usize) -> usize {
    last.to_string().len().max(NUMBER)
}

/// The width of the numbers of `diff`'s lines.
pub fn width_of(diff: &Diff) -> usize {
    number_cells(
        diff.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| l.at)
            .max()
            .unwrap_or(0),
    )
}

/// `line` as the screen draws it, its number `cells` wide.
fn line(line: &Line, cells: usize) -> Shown {
    let (text, role) = match line.mark {
        Mark::Same => (
            format!(
                " {:>w$}  {}",
                line.at,
                line.text,
                w = cells.saturating_add(2)
            ),
            Role::DiffContext,
        ),
        Mark::Added => (
            format!(" {:>cells$} +{}", line.at, line.text),
            Role::DiffAdded,
        ),
        Mark::Removed => (
            format!(" {:>cells$} -{}", line.at, line.text),
            Role::DiffRemoved,
        ),
    };
    // a changed word's place in the line, after the number and the sign
    let at = cells.saturating_add(3);
    let words = line
        .words
        .iter()
        .map(|r| r.start.saturating_add(at)..r.end.saturating_add(at))
        .collect();
    Shown {
        text,
        role,
        words,
        line: line.new,
        complained: false,
    }
}

/// The hunks of `diff`, each headed by `@@` and the part of the file it is
/// in, a blank line between two.
pub fn hunks(diff: &Diff) -> Vec<Shown> {
    let cells = width_of(diff);
    let mut out = Vec::new();
    for hunk in &diff.hunks {
        if !out.is_empty() {
            out.push(Shown::blank());
        }
        out.push(Shown::of(
            format!(" @@ {}", hunk.section),
            Role::DiffContext,
        ));
        out.extend(hunk.lines.iter().map(|l| line(l, cells)));
    }
    out
}

/// The font set changes and the active fonts the document changes, after
/// the hunks, their `+` and `-` under the hunks' signs for numbers
/// `cells` wide. Where it changes neither, one grey line says so.
pub fn fonts(review: &Review, cells: usize) -> Vec<Shown> {
    let pad = " ".repeat(cells.saturating_add(2));
    let mut out = Vec::new();
    if !review.font_sets.is_empty() {
        out.push(Shown::blank());
        out.push(Shown::grey(words::FONT_SETS));
        for (name, edit) in &review.font_sets {
            out.push(match edit {
                FritEdit::Add => Shown::of(format!("{pad}+{name}"), Role::DiffAdded),
                FritEdit::Replace => {
                    Shown::of(format!("{pad}+{name}{}", words::REPLACES), Role::DiffAdded)
                }
                FritEdit::Remove => Shown::of(format!("{pad}-{name}"), Role::DiffRemoved),
            });
        }
    }
    if !review.active.is_empty() {
        out.push(Shown::blank());
        out.push(Shown::grey(words::ACTIVE));
        for (path, change) in &review.active {
            let path = path.display();
            out.push(match change {
                vtium_config_core::batch::Change::Activate => {
                    Shown::of(format!("{pad}+{path}"), Role::DiffAdded)
                }
                vtium_config_core::batch::Change::Deactivate => {
                    Shown::of(format!("{pad}-{path}"), Role::DiffRemoved)
                }
            });
        }
    }
    if out.is_empty() {
        out.push(Shown::blank());
        out.push(Shown::grey(words::UNCHANGED));
    }
    out
}

/// The whole file, `lines` in order, with each line numbered in
/// `complaints` marked.
pub fn file(lines: &[Line], complaints: &[usize]) -> Vec<Shown> {
    let cells = number_cells(lines.iter().map(|l| l.at).max().unwrap_or(0));
    lines
        .iter()
        .map(|l| {
            let mut shown = line(l, cells);
            if l.new.is_some_and(|n| complaints.contains(&n)) {
                shown.text.replace_range(..1, "!");
                shown.complained = true;
            }
            shown
        })
        .collect()
}

/// Where the header's parts go on a screen `width` cells wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Whether the grey words fit after the title.
    pub grey: bool,
    /// The checked pair's first cell.
    pub pair: u16,
}

/// The header of the title `title` and its grey words `grey`: the pair at
/// the right, with up to six cells after it, and the grey words only
/// where they fit.
pub fn header(width: u16, title: &str, grey: &str) -> Header {
    let cells = |s: &str| u16::try_from(Span::raw(s).width()).unwrap_or(u16::MAX);
    let title = cells(title).saturating_add(1);
    let with_grey = title
        .saturating_add(2)
        .saturating_add(cells(grey))
        .saturating_add(2)
        .saturating_add(PAIR_CELLS);
    let (grey, needed) = if with_grey <= width {
        (true, with_grey)
    } else {
        (false, title.saturating_add(2).saturating_add(PAIR_CELLS))
    };
    let right = RIGHT.min(width.saturating_sub(needed));
    Header {
        grey,
        pair: width.saturating_sub(right).saturating_sub(PAIR_CELLS),
    }
}

/// What a page of Show changes shows.
pub struct Page<'a> {
    pub title: &'a str,
    pub grey: &'a str,
    pub whole: bool,
    pub lines: &'a [Shown],
    pub top: usize,
}

/// Draws `page` into `area`, from its first line `top`, and returns how
/// many rows of lines it showed.
pub fn draw(buf: &mut Buffer, area: Rect, page: &Page<'_>, color: bool) -> usize {
    clear(buf, area);
    if area.height == 0 {
        return 0;
    }
    let head = header(area.width, page.title, page.grey);
    let after = put(buf, area, 1, 0, page.title, Role::Heading, color);
    if head.grey {
        put(
            buf,
            area,
            after.saturating_add(2),
            0,
            page.grey,
            Role::Dim,
            color,
        );
    }
    let pair = if page.whole {
        "  Changes only  ✓ Whole file"
    } else {
        "✓ Changes only    Whole file"
    };
    put(buf, area, head.pair, 0, pair, Role::Row, color);
    let room = usize::from(area.height.saturating_sub(2));
    let len = page.lines.len();
    let over = len > room;
    let rows = if over { room.saturating_sub(1) } else { room };
    let top = page.top.min(len.saturating_sub(rows));
    let mut shown = 0;
    for (k, line) in page.lines.iter().skip(top).take(rows).enumerate() {
        let y = u16::try_from(k).unwrap_or(u16::MAX).saturating_add(2);
        draw_line(buf, area, y, line, color);
        shown = k.saturating_add(1);
    }
    if over && room > 0 {
        let at = words::lines(top.saturating_add(1), top.saturating_add(shown), len);
        let cells = u16::try_from(Span::raw(&at).width()).unwrap_or(u16::MAX);
        let right = area
            .width
            .saturating_sub(head.pair.saturating_add(PAIR_CELLS));
        let x = area.width.saturating_sub(right).saturating_sub(cells);
        put(
            buf,
            area,
            x,
            area.height.saturating_sub(1),
            &at,
            Role::Dim,
            color,
        );
    }
    rows
}

/// Draws `line` on row `y` of `area`: its role across the line, its
/// changed words bold, and a complained line's `!` and number in the
/// problems' role.
fn draw_line(buf: &mut Buffer, area: Rect, y: u16, line: &Shown, color: bool) {
    put(buf, area, 0, y, &line.text, line.role, color);
    let cell = |byte: usize| {
        let before = line.text.get(..byte).unwrap_or_default();
        u16::try_from(Span::raw(before).width()).unwrap_or(u16::MAX)
    };
    // a changed word keeps its line's color and gains the word's
    // attributes
    let word = ratatui_core::style::Style::default()
        .add_modifier(Role::DiffWord.style(color).cell().add_modifier);
    for r in &line.words {
        patch(buf, area, cell(r.start), cell(r.end), y, word);
    }
    if line.complained {
        let digits = line
            .text
            .char_indices()
            .skip(1)
            .find(|(_, c)| !c.is_ascii_whitespace() && !c.is_ascii_digit())
            .map_or(line.text.len(), |(at, _)| at);
        let problem = Role::Problems.style(color).cell();
        patch(buf, area, 0, cell(digits), y, problem);
    }
}

/// Puts `style` on the cells `from..to` of row `y` of `area`.
fn patch(
    buf: &mut Buffer,
    area: Rect,
    from: u16,
    to: u16,
    y: u16,
    style: ratatui_core::style::Style,
) {
    let to = to.min(area.width);
    if from < to && y < area.height {
        buf.set_style(
            Rect::new(
                area.x.saturating_add(from),
                area.y.saturating_add(y),
                to.saturating_sub(from),
                1,
            ),
            style,
        );
    }
}

/// Writes `text` at column `x` of row `y` of `area`, cut to the area's
/// width, in `role`'s style, and returns the column after it.
fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, text: &str, role: Role, color: bool) -> u16 {
    if y >= area.height || x >= area.width {
        return x;
    }
    let (end, _) = buf.set_stringn(
        area.x.saturating_add(x),
        area.y.saturating_add(y),
        text,
        usize::from(area.width.saturating_sub(x)),
        role.style(color).cell(),
    );
    end.saturating_sub(area.x)
}

/// Blanks `area`, so nothing of the tab under the screen shows.
fn clear(buf: &mut Buffer, area: Rect) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }
}

/// The words of Show changes' lines.
pub mod words {
    /// The heading over the font set changes.
    pub const FONT_SETS: &str = "Font sets";

    /// The heading over the active fonts the document changes.
    pub const ACTIVE: &str = "Active fonts";

    /// After a font set that replaces the installed one.
    pub const REPLACES: &str = " · replaces the installed one";

    /// The line where the document changes no font set and no active
    /// font.
    pub const UNCHANGED: &str = "Fonts unchanged";

    /// The last row while the lines do not fit.
    pub fn lines(first: usize, last: usize, of: usize) -> String {
        format!("lines {first} to {last} of {of}")
    }
}
