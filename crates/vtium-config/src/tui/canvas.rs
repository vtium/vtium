//! The TUI's text helpers: a row of styled text, writing text on a row at
//! a column, and fitting a name, a sentence or a list of words to the room
//! a screen has for it. At 80×25 a long name is cut in the middle with `…`,
//! so the end that tells two devices apart still shows.
//!
//! References:
//! - ADR-1403 D7: how a long name is cut at the smallest screen.

use super::color::Style;

/// One run of text on a row, in one style. A raw span is text vtium-config
/// styled itself, the About page's banner, written to the terminal as it
/// is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub raw: bool,
}

/// One row of the screen.
pub type Row = Vec<Span>;

/// A span of plain text.
pub fn span(text: impl Into<String>, style: Style) -> Span {
    Span {
        text: text.into(),
        style,
        raw: false,
    }
}

/// Writes `text` in `style` on `row` from its character column `x`, and
/// returns the column after the text. A row shorter than `x` is padded
/// with blanks first. The text replaces the characters under it and leaves
/// the rest of the row as it was, so a value written at its own column
/// cuts a label that runs into that column, and the label's end still
/// shows past the value. A control character becomes `?`, so a name read
/// from a file never carries a control onto the screen. The row stays a
/// run of spans, one for each stretch of one style.
///
/// The columns count characters, not cells: the row is laid out on the
/// screen afterwards by the console's width rule, which moves everything
/// after a wide character one cell to the right.
pub fn put(row: &mut Row, x: usize, text: &str, style: Style) -> usize {
    let mut cells: Vec<(char, Style)> = row
        .iter()
        .flat_map(|s| s.text.chars().map(move |c| (c, s.style)))
        .collect();
    if cells.len() < x {
        cells.resize(x, (' ', Style::PLAIN));
    }
    let written: Vec<(char, Style)> = printable(text).chars().map(|c| (c, style)).collect();
    let end = x.saturating_add(written.len());
    let tail = cells.split_off(end.min(cells.len()));
    cells.truncate(x);
    cells.extend(written);
    cells.extend(tail);
    let mut out: Row = Vec::new();
    for (c, style) in cells {
        match out.last_mut() {
            Some(s) if s.style == style => s.text.push(c),
            _ => out.push(span(c.to_string(), style)),
        }
    }
    *row = out;
    end
}

/// `text` with each control character shown as `?`, so a name read from a
/// file never carries a control onto the screen, and ratatui, which
/// cannot measure a control, never meets one.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// `text` in at most `n` characters, cut at its end with `…`.
pub fn fit(text: &str, n: usize) -> String {
    let len = text.chars().count();
    if len <= n {
        return text.to_owned();
    }
    if n == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// How many cells the console gives `c` on the screen, as `view::fill`
/// lays a row out: the console's width rule for the character, and at
/// least one cell, since a control shows as `?`.
fn cells_of(c: char) -> usize {
    let mut symbol = [0u8; 4];
    usize::from(super::backend::console_width(c.encode_utf8(&mut symbol)).max(1))
}

/// How many cells `text` takes on the console's screen. A character the
/// console draws two cells wide counts twice, so a name in Chinese takes
/// twice as many cells as it has characters.
pub fn cells(text: &str) -> usize {
    text.chars().map(cells_of).sum()
}

/// `text` in at most `n` cells of the console's screen, cut at its end
/// with `…`. A wide character that would straddle the end goes whole, so
/// the result can be a cell short of `n`.
pub fn fit_cells(text: &str, n: usize) -> String {
    if cells(text) <= n {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in text.chars() {
        let w = cells_of(c);
        if used.saturating_add(w).saturating_add(1) > n {
            break;
        }
        out.push(c);
        used = used.saturating_add(w);
    }
    if n > 0 {
        out.push('…');
    }
    out
}

/// The character column of `row` that the console shows at screen cell
/// `cell`. Text written there with [`put`] starts at that cell, however
/// many wide characters come before it. Past the row's end, each missing
/// cell is a blank that [`put`] pads with. A cell inside a wide character
/// gives the column after it.
pub fn column(row: &Row, cell: usize) -> usize {
    let mut used = 0usize;
    let mut chars = 0usize;
    for c in row.iter().flat_map(|s| s.text.chars()) {
        if used >= cell {
            return chars;
        }
        used = used.saturating_add(cells_of(c));
        chars = chars.saturating_add(1);
    }
    chars.saturating_add(cell.saturating_sub(used))
}

/// `text` in at most `n` characters, cut in the middle with `…`, so the
/// end that tells devices apart still shows.
pub fn mid(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= n {
        return text.to_owned();
    }
    if n < 5 {
        return fit(text, n);
    }
    let head = n.saturating_sub(1).div_ceil(2);
    let tail = n.saturating_sub(1).saturating_sub(head);
    let mut out: String = chars.iter().take(head).collect();
    out.push('…');
    out.extend(chars.iter().skip(chars.len().saturating_sub(tail)));
    out
}

/// The first of `options` that fits in `n` characters, else the last cut
/// to `n`.
pub fn pick(n: usize, options: &[&str]) -> String {
    options.iter().find(|o| o.chars().count() <= n).map_or_else(
        || fit(options.last().copied().unwrap_or(""), n),
        |o| (*o).to_owned(),
    )
}

/// `text` in lines of at most `width` characters, broken between words,
/// at most `most` of them, the last cut with `…` where text was left out;
/// at least one line.
pub fn wrap(text: &str, width: usize, most: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    // a word longer than the line, such as a path, breaks where the line
    // ends
    let pieces: Vec<String> = text
        .split(' ')
        .flat_map(|w| {
            let chars: Vec<char> = w.chars().collect();
            if chars.is_empty() {
                vec![String::new()]
            } else {
                chars.chunks(width).map(|c| c.iter().collect()).collect()
            }
        })
        .collect();
    for word in &pieces {
        let len = line.chars().count();
        if len > 0 && len.saturating_add(1).saturating_add(word.chars().count()) > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    let most = most.max(1);
    if lines.len() > most {
        lines.truncate(most);
        if let Some(last) = lines.last_mut() {
            *last = fit(&format!("{last} …"), width);
            if !last.ends_with('…') {
                last.pop();
                last.push('…');
            }
        }
    }
    lines
}

/// How many cells `text` takes on the screen: its characters, without the
/// escape sequences of a raw span.
pub fn width(text: &str) -> usize {
    let mut n = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        n = n.saturating_add(1);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_is_cut_in_the_middle_and_keeps_its_end() {
        assert_eq!(mid("edid:AAA:0x0001:LARGE00001", 13), "edid:A…E00001");
        assert_eq!(mid("keyboard", 10), "keyboard");
        assert_eq!(mid("Logitech USB Receiver", 10), "Logit…iver");
        assert_eq!(fit("matches no display present", 10), "matches n…");
        assert_eq!(pick(8, &["the active head", "active"]), "active");
    }

    #[test]
    fn wrap_breaks_between_words_and_marks_what_it_left_out() {
        assert_eq!(
            wrap("configured, not found", 11, 2),
            ["configured,", "not found"]
        );
        assert_eq!(wrap("a b c d e f", 3, 2), ["a b", "c …"]);
    }

    #[test]
    fn text_is_fitted_and_placed_by_the_console_s_cells() {
        assert_eq!(cells("ab键盘"), 6);
        assert_eq!(fit_cells("ab键盘", 6), "ab键盘");
        // `键` and `…` take 3 cells; `盘` would straddle the end
        assert_eq!(fit_cells("ab键盘", 5), "ab键…");
        assert_eq!(fit_cells("ab键盘", 4), "ab…");
        assert_eq!(fit_cells("", 0), "");
        assert_eq!(fit_cells("abc", 0), "");
        let row = vec![span("a键b", Style::PLAIN)];
        // cell 3 shows `b`, the third character
        assert_eq!(column(&row, 3), 2);
        // cell 2 is inside `键`, so text goes after it
        assert_eq!(column(&row, 2), 2);
        // the row takes 4 cells, so cell 6 is two blanks past its end
        assert_eq!(column(&row, 6), 5);
    }

    #[test]
    fn a_raw_span_s_width_leaves_out_its_escapes() {
        assert_eq!(width("\x1b[38;2;1;2;3mab\x1b[0mc"), 3);
    }

    #[test]
    fn text_put_on_a_row_replaces_what_it_covers_and_keeps_the_rest() {
        let grey = Style::text(super::super::color::Color::Grey);
        let white = Style::text(super::super::color::Color::BrightWhite);
        let mut row: Row = Vec::new();
        // a label that runs past the column where its value starts
        assert_eq!(put(&mut row, 3, "Input A long name", grey), 20);
        assert_eq!(put(&mut row, 12, "connected", white), 21);
        // the value's column is reached by blanks, and a later run of the
        // label's style joins the earlier one only when they touch
        assert_eq!(
            row,
            [
                span("   ", Style::PLAIN),
                span("Input A l", grey),
                span("connected", white),
            ]
        );
        assert_eq!(put(&mut row, 24, "x\u{1b}", grey), 26);
        let text: String = row.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "   Input A lconnected   x?");
        // a shorter value leaves the end of the label in sight
        let mut row: Row = Vec::new();
        put(&mut row, 0, "abcdefgh", grey);
        put(&mut row, 2, "XY", white);
        let text: String = row.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "abXYefgh");
        assert_eq!(row.len(), 3);
    }
}
