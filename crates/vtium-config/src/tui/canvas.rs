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

/// One character as the console shows it: a base character and the marks
/// that join it, which the console keeps in one cell of one or two cells'
/// width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster<'a> {
    /// The cluster's characters, as the text holds them.
    pub text: &'a str,
    /// How many characters of the text the cluster holds.
    pub chars: usize,
    /// The cells the cluster takes on the screen. A cluster that starts
    /// with a mark, which only the start of a text can, has nothing to
    /// join and takes no cell of its own; [`shown`] gives it one.
    pub width: u16,
}

thread_local! {
    /// The `cluster-width` rule this thread measures text by.
    static RULE: std::cell::Cell<vtium_term::ClusterWidth> =
        const { std::cell::Cell::new(vtium_term::ClusterWidth::Double) };
}

/// The console's `cluster-width` rule that [`clusters`] and the Backend
/// measure text by on this thread: `double`, the console's default,
/// unless a [`MeasureBy`] is alive.
pub fn rule() -> vtium_term::ClusterWidth {
    RULE.with(std::cell::Cell::get)
}

/// Measures text by `rule` on this thread until the returned guard drops,
/// and then by the rule before it. A console under `cluster-width =
/// single` gives a syllable with a spacing vowel sign one cell where
/// `double` gives it two. Update handles each event, and the view draws
/// each frame, inside a guard set to the rule the console started with
/// (`Shared::measure`). So the rows update counts, ratatui's buffer, the
/// rows the view lays out and the Backend's cursor all count the console's
/// cells.
pub fn measure_by(rule: vtium_term::ClusterWidth) -> MeasureBy {
    MeasureBy {
        before: RULE.with(|r| r.replace(rule)),
    }
}

/// The guard [`measure_by`] returns.
#[derive(Debug)]
#[must_use = "the rule holds only while the guard lives"]
pub struct MeasureBy {
    before: vtium_term::ClusterWidth,
}

impl Drop for MeasureBy {
    fn drop(&mut self) {
        RULE.with(|r| r.set(self.before));
    }
}

/// The characters `text` shows as on the console, each a base character
/// with the marks that join it, in order. Every character of `text` is in
/// one cluster, so the clusters' texts joined give `text` back.
///
/// The console joins a combining mark, a vowel sign or a joiner to the
/// character before it and draws them as one character, of the width the
/// console's rule gives the whole cluster. So a row laid out a character
/// to a cell does not match the console's cells, and neither do the
/// clusters of Unicode's own tables, which ratatui reads. vtium-config
/// asks vtium-term, the console's own terminal, which cuts the text into
/// clusters and measures each one exactly as its screen does when the
/// text is printed, under the [`rule`] this thread measures by. Plain
/// ASCII text joins nothing, so it skips that step.
///
/// References:
/// - ADR-1206 D1, "The console's width rule decides where the cursor
///   goes": the console's clusters and widths lay the TUI's text out.
pub fn clusters(text: &str) -> Vec<Cluster<'_>> {
    if text.is_ascii() {
        return (0..text.len())
            .map(|i| Cluster {
                text: text.get(i..i.saturating_add(1)).unwrap_or_default(),
                chars: 1,
                width: 1,
            })
            .collect();
    }
    vtium_term::clusters(text, rule())
        .map(|(text, width)| Cluster {
            text,
            chars: text.chars().count(),
            width: u16::from(width),
        })
        .collect()
}

/// The symbol a cell of the TUI holds for `cluster`, and the cells it
/// takes: the cluster itself, or for a cluster that starts with a mark, the
/// mark on a space. A mark the console found nothing to join would join
/// the character left of the cell it is written in, so it gets a space of
/// its own, as Unicode shows a mark alone.
pub fn shown(cluster: &Cluster<'_>) -> (String, u16) {
    if cluster.width > 0 {
        return (cluster.text.to_owned(), cluster.width);
    }
    let on_space = format!(" {}", cluster.text);
    let width = super::backend::console_width(&on_space).max(1);
    (on_space, width)
}

/// How many cells `text` takes on the console's screen. A character the
/// console draws two cells wide counts twice, so a name in Chinese takes
/// twice as many cells as it has characters, and a mark that joins the
/// character before it counts nothing.
pub fn cells(text: &str) -> usize {
    clusters(&printable(text))
        .iter()
        .map(|c| usize::from(shown(c).1))
        .sum()
}

/// `text` in at most `n` cells of the console's screen, cut at its end
/// with `…`. A wide character that would straddle the end goes whole, so
/// the result can be a cell short of `n`. The cut never separates a
/// character from its marks.
pub fn fit(text: &str, n: usize) -> String {
    if cells(text) <= n {
        return text.to_owned();
    }
    let shown_text = printable(text);
    let mut out = String::new();
    let mut used = 0usize;
    for c in clusters(&shown_text) {
        let w = usize::from(shown(&c).1);
        if used.saturating_add(w).saturating_add(1) > n {
            break;
        }
        out.push_str(c.text);
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
    let text: String = row.iter().map(|s| printable(&s.text)).collect();
    let mut used = 0usize;
    let mut chars = 0usize;
    for c in clusters(&text) {
        if used >= cell {
            return chars;
        }
        used = used.saturating_add(usize::from(shown(&c).1));
        chars = chars.saturating_add(c.chars);
    }
    chars.saturating_add(cell.saturating_sub(used))
}

/// `text` in at most `n` cells of the console's screen, cut in the middle
/// with `…`, so the end that tells devices apart still shows.
pub fn mid(text: &str, n: usize) -> String {
    if cells(text) <= n {
        return text.to_owned();
    }
    if n < 5 {
        return fit(text, n);
    }
    let shown_text = printable(text);
    let parts = clusters(&shown_text);
    // the clusters from one end that fit in `room` cells
    let take = |room: usize, from_end: bool| -> String {
        let mut used = 0usize;
        let mut kept: Vec<&str> = Vec::new();
        let order: Box<dyn Iterator<Item = &Cluster<'_>>> = if from_end {
            Box::new(parts.iter().rev())
        } else {
            Box::new(parts.iter())
        };
        for c in order {
            used = used.saturating_add(usize::from(shown(c).1));
            if used > room {
                break;
            }
            kept.push(c.text);
        }
        if from_end {
            kept.reverse();
        }
        kept.concat()
    };
    let head = n.saturating_sub(1).div_ceil(2);
    let tail = n.saturating_sub(1).saturating_sub(head);
    let mut out = take(head, false);
    out.push('…');
    out.push_str(&take(tail, true));
    out
}

/// The first of `options` that fits in `n` cells, else the last cut to
/// `n`.
pub fn pick(n: usize, options: &[&str]) -> String {
    options.iter().find(|o| cells(o) <= n).map_or_else(
        || fit(options.last().copied().unwrap_or(""), n),
        |o| (*o).to_owned(),
    )
}

/// A value being typed as a one-line field shows it, in at most `n` cells
/// of the console's screen: ` value_ `, the cursor `_` after the text and
/// a blank on each side. When the whole value does not fit, the field
/// scrolls as a one-line field does. It shows the end of the value after
/// `…`, so the cursor and what the person types last stay in sight.
pub fn field(typed: &str, n: usize) -> String {
    let text = printable(typed);
    if cells(&text).saturating_add(3) <= n {
        return format!(" {text}_ ");
    }
    // the clusters from the end that fit beside ` …`, `_` and a blank
    let room = n.saturating_sub(4);
    let mut used = 0usize;
    let mut start = text.len();
    for c in clusters(&text).iter().rev() {
        used = used.saturating_add(usize::from(shown(c).1));
        if used > room {
            break;
        }
        start = start.saturating_sub(c.text.len());
    }
    format!(" …{}_ ", text.get(start..).unwrap_or_default())
}

/// `word` in pieces of at most `width` cells, cut between the characters
/// the console shows, so a mark stays with its character. A character
/// wider than `width` is a piece of its own.
fn pieces(word: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut piece = String::new();
    let mut used = 0usize;
    for c in clusters(word) {
        let w = usize::from(shown(&c).1);
        if used > 0 && used.saturating_add(w) > width {
            out.push(std::mem::take(&mut piece));
            used = 0;
        }
        piece.push_str(c.text);
        used = used.saturating_add(w);
    }
    out.push(piece);
    out
}

/// `text` in lines of at most `width` cells of the console's screen,
/// broken between words, at most `most` of them, the last cut with `…`
/// where text was left out; at least one line. A character two cells wide
/// counts twice, and a mark that joins the character before it counts
/// nothing. Each control character becomes `?`, as [`printable`] shows
/// it, so a tab or an escape sequence in a message from a command takes
/// the one cell it shows in.
pub fn wrap(text: &str, width: usize, most: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut used = 0usize;
    // a word longer than the line, such as a path, breaks where the line
    // ends
    for word in printable(text).split(' ').flat_map(|w| pieces(w, width)) {
        let w = cells(&word);
        if used > 0 && used.saturating_add(1).saturating_add(w) > width {
            lines.push(std::mem::take(&mut line));
            used = 0;
        } else if !line.is_empty() {
            line.push(' ');
            used = used.saturating_add(1);
        }
        line.push_str(&word);
        used = used.saturating_add(w);
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
        assert_eq!(fit("ab键盘", 6), "ab键盘");
        // `键` and `…` take 3 cells; `盘` would straddle the end
        assert_eq!(fit("ab键盘", 5), "ab键…");
        assert_eq!(fit("ab键盘", 4), "ab…");
        assert_eq!(fit("", 0), "");
        assert_eq!(fit("abc", 0), "");
        let row = vec![span("a键b", Style::PLAIN)];
        // cell 3 shows `b`, the third character
        assert_eq!(column(&row, 3), 2);
        // cell 2 is inside `键`, so text goes after it
        assert_eq!(column(&row, 2), 2);
        // the row takes 4 cells, so cell 6 is two blanks past its end
        assert_eq!(column(&row, 6), 5);
    }

    #[test]
    fn wrap_and_fit_measure_text_in_the_console_s_cells() {
        // 键 and 盘 take two cells each on the console, and the marks in
        // é and கொ join the character before them
        assert_eq!(fit("键盘键盘", 5), "键盘…");
        assert_eq!(
            fit("e\u{301}e\u{301}e\u{301}", 3),
            "e\u{301}e\u{301}e\u{301}"
        );
        assert_eq!(fit("கொகொகொ", 5), "கொகொ…");
        assert_eq!(wrap("键盘 键盘 键盘", 9, 9), ["键盘 键盘", "键盘"]);
        // a word wider than the line breaks where the line ends, and
        // never between a character and its marks
        assert_eq!(wrap("键盘键盘键", 5, 9), ["键盘", "键盘", "键"]);
        assert_eq!(wrap("கொகொகொ", 4, 9), ["கொகொ", "கொ"]);
        // the last line kept marks the text left out
        assert_eq!(wrap("键盘 键盘 键盘", 9, 1), ["键盘 键…"]);
        assert_eq!(mid("键盘ab键盘cd键盘", 9), "键盘…键盘");
        assert_eq!(pick(4, &["键盘键", "键盘"]), "键盘");
    }

    #[test]
    fn wrap_measures_a_control_character_as_the_question_mark_it_shows_as() {
        // a tab or an escape would move the cursor of a terminal the text
        // is measured on; the TUI shows each control as `?`, one cell
        let lines = wrap("é\tabcdefghij klmnop", 6, 9);
        assert_eq!(lines, ["é?abcd", "efghij", "klmnop"]);
        assert_eq!(wrap("é\x1b[5Cab", 9, 9), ["é?[5Cab"]);
    }

    #[test]
    fn a_long_text_keeps_each_character_with_its_marks() {
        // 4,094 characters, then e and its accent, which a cut every
        // 4,095 characters would split
        let mut text = "é".repeat(4094);
        text.push_str("e\u{301}x");
        let parts = clusters(&text);
        let last: Vec<(&str, u16)> = parts
            .iter()
            .rev()
            .take(2)
            .map(|c| (c.text, c.width))
            .collect();
        assert_eq!(last, [("x", 1), ("e\u{301}", 1)]);
        assert_eq!(cells(&text), 4096);
    }

    #[test]
    fn a_typed_value_too_long_for_its_field_shows_its_end_and_the_cursor() {
        assert_eq!(field("abc", 6), " abc_ ");
        assert_eq!(field("abcd", 6), " …cd_ ");
        assert_eq!(field("abcdefgh", 8), " …efgh_ ");
        // 键 takes two cells, so only one fits in the three after `…`
        assert_eq!(field("ab键键", 7), " …键_ ");
        assert_eq!(field("a\tb", 9), " a?b_ ");
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
