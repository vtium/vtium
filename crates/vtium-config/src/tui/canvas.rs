//! A screen of cells the TUI draws into, each a character and its style,
//! and the text helpers that fit a name to a column (ADR-1403 D7: at 80 by
//! 25 a long name is cut in the middle with `…`).

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

/// The mark of a cell that the wide character before it covers, which no
/// row's text holds: [`Canvas::put`] writes a control as `?`.
const WIDE_TAIL: char = '\0';

/// A span of plain text.
pub fn span(text: impl Into<String>, style: Style) -> Span {
    Span {
        text: text.into(),
        style,
        raw: false,
    }
}

/// The screen being drawn: `cols` by `rows` cells.
#[derive(Clone, Debug)]
pub struct Canvas {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Vec<(char, Style)>>,
    /// A row drawn whole by a raw span, in place of its cells.
    raw: Vec<Option<Span>>,
}

impl Canvas {
    pub fn new(cols: usize, rows: usize) -> Canvas {
        Canvas {
            cols,
            rows,
            cells: vec![vec![(' ', Style::PLAIN); cols]; rows],
            raw: vec![None; rows],
        }
    }

    /// Writes `text` from column `x` of row `y` in `style`, as far as the
    /// screen's edge; returns the column after it.
    pub fn put(&mut self, x: usize, y: usize, text: &str, style: Style) -> usize {
        let Some(row) = self.cells.get_mut(y) else {
            return x;
        };
        let mut at = x;
        for c in text.chars() {
            let Some(cell) = row.get_mut(at) else {
                break;
            };
            // a name read from a file never carries a control onto the screen
            *cell = (if c.is_control() { '?' } else { c }, style);
            at = at.saturating_add(1);
        }
        at
    }

    /// Writes `text` from column `x` of row `y` in `style` as a terminal
    /// lays it out: a wide character, by the console's own width rule,
    /// takes two cells, and one that would cross the screen's edge ends
    /// the text there. Returns the column after it. The font preview in a
    /// vtium console writes the specimen's lines with it, which hold CJK
    /// and emoji (ADR-1201 D4).
    pub fn put_cells(&mut self, x: usize, y: usize, text: &str, style: Style) -> usize {
        let Some(row) = self.cells.get_mut(y) else {
            return x;
        };
        let mut at = x;
        for c in text.chars() {
            let c = if c.is_control() { '?' } else { c };
            let wide = vtium_term::cluster_width(&[c]) >= 2;
            let end = at.saturating_add(usize::from(wide));
            if end >= row.len() {
                break;
            }
            if let Some(cell) = row.get_mut(at) {
                *cell = (c, style);
            }
            if wide && let Some(cell) = row.get_mut(end) {
                *cell = (WIDE_TAIL, style);
            }
            at = end.saturating_add(1);
        }
        at
    }

    /// Writes `text` from `x` on `y`, cut to `width` and padded to it in
    /// `style`, so a block's color fills its whole width.
    pub fn fill(&mut self, x: usize, y: usize, width: usize, text: &str, style: Style) {
        let fitted = fit(text, width);
        let pad = width.saturating_sub(fitted.chars().count());
        self.put(x, y, &format!("{fitted}{}", " ".repeat(pad)), style);
    }

    /// The character at a cell, for the lines that join at it.
    pub fn at(&self, x: usize, y: usize) -> Option<char> {
        self.cells.get(y)?.get(x).map(|(c, _)| *c)
    }

    /// The style a cell is drawn in.
    pub fn style_at(&self, x: usize, y: usize) -> Option<Style> {
        self.cells.get(y)?.get(x).map(|(_, s)| *s)
    }

    /// Sets a row to a raw span, drawn as it is.
    pub fn raw(&mut self, y: usize, text: String) {
        if let Some(r) = self.raw.get_mut(y) {
            *r = Some(Span {
                text,
                style: Style::PLAIN,
                raw: true,
            });
        }
    }

    /// Writes each span of `row` from column 0 of `y`.
    pub fn row(&mut self, y: usize, row: &Row) {
        let mut x = 0;
        for s in row {
            if s.raw {
                self.raw(y, s.text.clone());
                return;
            }
            x = self.put(x, y, &s.text, s.style);
        }
    }

    /// The screen as rows, each cell run of one style a span.
    pub fn into_rows(self) -> Vec<Row> {
        self.cells
            .into_iter()
            .zip(self.raw)
            .map(|(cells, raw)| {
                if let Some(raw) = raw {
                    return vec![raw];
                }
                let mut row: Row = Vec::new();
                // the second cell of a wide character is the character's
                for (c, style) in cells.into_iter().filter(|(c, _)| *c != WIDE_TAIL) {
                    match row.last_mut() {
                        Some(s) if s.style == style => s.text.push(c),
                        _ => row.push(span(c.to_string(), style)),
                    }
                }
                row
            })
            .collect()
    }
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
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
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
    fn a_raw_span_s_width_leaves_out_its_escapes() {
        assert_eq!(width("\x1b[38;2;1;2;3mab\x1b[0mc"), 3);
    }

    #[test]
    fn a_wide_character_takes_two_cells_and_never_crosses_the_edge() {
        let mut c = Canvas::new(5, 1);
        // 日 takes two cells, as the console's width rule gives it
        let after = c.put_cells(0, 0, "a日b日", Style::PLAIN);
        assert_eq!(after, 4, "the second 日 would cross the edge");
        let rows = c.into_rows();
        assert_eq!(rows[0][0].text, "a日b ");
        // the row's cells still number five once drawn: 1 + 2 + 1 + 1
        let cells: usize = rows[0][0]
            .text
            .chars()
            .map(|c| usize::from(vtium_term::cluster_width(&[c])))
            .sum();
        assert_eq!(cells, 5);
    }

    #[test]
    fn a_block_fills_its_width_and_runs_merge_by_style() {
        let mut c = Canvas::new(6, 1);
        let s = Style::block(super::super::color::Color::BrightCyan);
        c.fill(1, 0, 4, "DP-8x", s);
        let rows = c.into_rows();
        let texts: Vec<(&str, Style)> =
            rows[0].iter().map(|s| (s.text.as_str(), s.style)).collect();
        assert_eq!(
            texts,
            [(" ", Style::PLAIN), ("DP-…", s), (" ", Style::PLAIN)]
        );
    }
}
