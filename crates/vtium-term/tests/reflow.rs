//! Reflow on a width change (ADR-1003): a case for each rule of the
//! record, under the decision it pins, and the property tests it names.

mod common;

use core::mem;

use common::{Rng, rows, scrollback, term};
use vtium_term::{
    Age, Color, Content, Glyph, Line, LineFlags, LinkId, Palette, Rgb, Style, Terminal, Width,
};

fn cursor(t: &Terminal) -> (usize, usize) {
    t.screen().cursor()
}

/// A terminal `cols` by `rows` keeping `sb` scrollback rows, fed `bytes`.
fn with_scrollback(cols: usize, rows: usize, sb: usize, bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(cols, rows, sb);
    t.screen_mut().set_scrollback_max(sb);
    t.input(bytes);
    t
}

/// Every row's text, the scrollback's first, all of it rewrapped.
fn all_rows(t: &mut Terminal) -> Vec<String> {
    t.screen_mut().rewrap_scrollback();
    let mut out = scrollback(t.screen());
    out.extend(rows(t.screen()));
    out
}

fn blue() -> Style {
    Style {
        bg: Color::Indexed(4),
        ..Style::DEFAULT
    }
}

// The probe `reflow`: a line wrapped at 10 columns is one row at 20.
// covers: text.reflow
#[test]
fn a_wrapped_line_rejoins_when_the_screen_widens() {
    let mut t = term(10, 4, b"abcdefghijklmno");
    assert_eq!(rows(t.screen()), ["abcdefghij", "klmno", "", ""]);
    t.resize(20, 4);
    assert_eq!(rows(t.screen()), ["abcdefghijklmno", "", "", ""]);
    assert_eq!(cursor(&t), (15, 0));
    assert!(
        !t.screen()
            .line(0)
            .unwrap()
            .flags()
            .contains(LineFlags::WRAPPED)
    );
}

// "Which lines rewrap", "Logical lines": a line the program ended with a
// newline wraps onto extra rows when longer than the new width and
// rejoins when it widens, and never joins the line after it (the
// record's confident probe).
#[test]
fn a_long_line_ended_by_a_newline_wraps_and_rejoins_but_joins_nothing() {
    let long: String = ('a'..='z').cycle().take(120).collect();
    let mut t = with_scrollback(120, 4, 100, format!("{long}\r\nnext\r\n").as_bytes());
    t.resize(80, 4);
    assert_eq!(
        all_rows(&mut t),
        [&long[..80], &long[80..], "next", ""],
        "narrowed"
    );
    t.resize(120, 4);
    assert_eq!(all_rows(&mut t), [&long[..], "next", "", ""], "widened");
}

// "Logical lines": a row whose text the program placed by moving the
// cursor joins nothing after it, and nothing joins it.
#[test]
fn text_placed_by_cursor_movement_does_not_move() {
    let mut t = term(6, 3, b"abc\x1b[2;1Hdef\x1b[3;1H");
    t.resize(3, 3);
    assert_eq!(rows(t.screen()), ["abc", "def", ""]);
    t.resize(8, 3);
    assert_eq!(rows(t.screen()), ["abc", "def", ""]);
}

// "Which lines rewrap": the alternate screen is cut at the new width and
// never rewrapped; a widening's columns are blank; the main screen
// rewraps around the saved cursor meanwhile.
#[test]
fn the_alternate_screen_is_cut_and_never_rewrapped() {
    let mut t = with_scrollback(
        8,
        2,
        100,
        b"abcdefghij\x1b[?1049h\x1b[44m\x1b[2J\x1b[m\x1b[HABCDEFGHIJ",
    );
    t.resize(4, 2);
    assert_eq!(rows(t.screen()), ["ABCD", "IJ"]);
    t.resize(8, 2);
    assert_eq!(rows(t.screen()), ["ABCD", "IJ"]);
    let line = t.screen().line(0).unwrap();
    assert!(line.cells()[4..].iter().all(|c| c.is_blank()));
    t.input(b"\x1b[?1049l");
    assert_eq!(rows(t.screen()), ["abcdefgh", "ij"]);
}

// "Wide characters and clusters": a wide character that no longer fits
// moves whole to the next row and leaves a pad; widening drops the pad;
// one column wide it becomes one empty cell.
#[test]
fn a_wide_character_moves_whole_and_leaves_a_pad() {
    let mut t = term(6, 3, "wxyz\u{4E2D}\r\n".as_bytes());
    assert_eq!(rows(t.screen()), ["wxyz\u{4E2D}", "", ""]);
    t.resize(5, 3);
    assert_eq!(rows(t.screen()), ["wxyz", "\u{4E2D}", ""]);
    let line = t.screen().line(0).unwrap();
    assert_eq!(line.cell(4).width(), Width::Pad);
    assert!(line.flags().contains(LineFlags::WRAPPED));
    t.resize(14, 3);
    assert_eq!(rows(t.screen()), ["wxyz\u{4E2D}", "", ""]);
    assert!(
        t.screen().line(0).unwrap().cells()[6..]
            .iter()
            .all(|c| c.is_blank())
    );

    // Printing at a width and rewrapping to it give the same cells.
    let printed = term(5, 3, "wxyz\u{4E2D}\r\n".as_bytes());
    let mut rewrapped = term(9, 3, "wxyz\u{4E2D}\r\n".as_bytes());
    rewrapped.resize(5, 3);
    for row in 0..2 {
        assert_eq!(
            printed.screen().line(row).unwrap().cells(),
            rewrapped.screen().line(row).unwrap().cells(),
            "row {row}"
        );
    }

    let mut t = term(4, 3, "\u{4E2D}a\r\n".as_bytes());
    t.resize(1, 3);
    let line = t.screen().line(0).unwrap();
    assert_eq!(line.cell(0).content(), Content::Empty);
    assert_eq!(rows(t.screen())[1], "a");
}

// "Wide characters and clusters": a cluster moves whole with its cell,
// and so do its style and link.
#[test]
fn clusters_styles_and_links_move_with_their_cells() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let mut t = term(4, 3, b"ab");
    t.screen_mut().set_link(Some(LinkId::new(7)));
    t.input(format!("\x1b[31mc{family}\x1b[m").as_bytes());
    t.resize(3, 3);
    let s = t.screen();
    assert_eq!(rows(s), ["abc", family, ""]);
    let line = s.line(1).unwrap();
    let cell = line.cell(0);
    assert_eq!(line.glyph(cell), Glyph::Cluster(family));
    assert_eq!(line.style(cell).fg, Color::Indexed(1));
    assert_eq!(line.link(cell), Some(LinkId::new(7)));
    let line = s.line(0).unwrap();
    assert_eq!(line.link(line.cell(2)), Some(LinkId::new(7)));
    assert_eq!(line.link(line.cell(1)), None);
}

// "The cursor and the saved cursors": the cursor keeps its place in the
// text; a pending wrap stands after its cell.
#[test]
fn the_cursor_keeps_its_place_in_the_text() {
    let mut t = term(10, 3, b"abcdefghijklm\x1b[1;3H");
    t.resize(4, 3);
    assert_eq!(cursor(&t), (2, 0));
    let mut t = term(10, 3, b"abcdefghijklm\x1b[2;2H");
    t.resize(4, 4);
    assert_eq!(
        (rows(t.screen()), cursor(&t)),
        (
            vec!["abcd".into(), "efgh".into(), "ijkl".into(), "m".into()],
            (3, 2)
        )
    );

    // A pending wrap after the line's last cell.
    let mut t = term(4, 3, b"abcdefgh");
    assert_eq!(cursor(&t), (4, 1));
    t.resize(8, 3);
    assert_eq!(cursor(&t), (8, 0));
    t.resize(3, 3);
    assert_eq!(cursor(&t), (2, 2));
    t.input(b"X");
    assert_eq!(rows(t.screen()), ["abc", "def", "ghX"]);
}

// "The cursor and the saved cursors": the main screen's saved cursor is
// carried as the cursor is, through the terminal's resize.
#[test]
fn the_saved_cursor_is_carried() {
    let mut t = with_scrollback(10, 3, 100, b"abcdefghijklm\x1b[2;2H\x1b7\x1b[2;4H");
    t.resize(4, 3);
    assert_eq!(scrollback(t.screen()), ["abcd"]);
    t.input(b"\x1b8X");
    assert_eq!(rows(t.screen()), ["efgh", "ijkX", "m"]);

    // While the alternate screen is shown, the main screen rewraps around
    // the cursor 1049 saved.
    let mut t = with_scrollback(10, 3, 100, b"abcdefghijklm\x1b[?1049hALT");
    t.resize(4, 3);
    t.input(b"\x1b[?1049lX");
    assert_eq!(rows(t.screen()), ["efgh", "ijkl", "mX"]);
}

// "The cursor and the saved cursors": a narrowing fills the blank rows
// below the cursor first, then pushes rows from the top into the
// scrollback; a widening refills the screen from the scrollback, newest
// first, and with none the text moves up.
#[test]
fn rows_below_the_cursor_keep_their_number() {
    let mut t = with_scrollback(8, 4, 100, b"abcdefgh\x1b[H");
    t.resize(4, 4);
    assert_eq!(rows(t.screen()), ["abcd", "efgh", "", ""]);
    assert_eq!(scrollback(t.screen()), Vec::<String>::new());

    // The cursor after the text is on the cell after it, which a
    // narrowing puts on a row of its own.
    let mut t = with_scrollback(8, 3, 100, b"one\r\nabcdefghijkl");
    t.resize(4, 3);
    assert_eq!(rows(t.screen()), ["efgh", "ijkl", ""]);
    assert_eq!(scrollback(t.screen()), ["one", "abcd"]);
    assert_eq!(cursor(&t), (0, 2));
    t.resize(8, 3);
    assert_eq!(rows(t.screen()), ["one", "abcdefgh", "ijkl"]);
    assert!(scrollback(t.screen()).is_empty());

    let mut t = term(4, 3, b"abcdefgh");
    t.resize(8, 3);
    assert_eq!(rows(t.screen()), ["abcdefgh", "", ""]);
}

// "The margin": a widening shows only text a rewrap brings back and the
// line's fill, which is the color an erase to the line's end left.
#[test]
fn a_widening_paints_the_lines_fill() {
    let mut t = term(4, 2, b"ab\x1b[44m\x1b[K\x1b[m\r\ncd");
    t.resize(6, 2);
    let s = t.screen();
    let line = s.line(0).unwrap();
    assert_eq!(line.style(line.cell(5)), blue());
    let line = s.line(1).unwrap();
    assert!(line.cells()[2..].iter().all(|c| c.is_blank()));

    // A line erased in color keeps its fill through the scrollback.
    let mut t = with_scrollback(4, 1, 10, b"\x1b[44m\x1b[2Kab\x1b[m\r\nx");
    t.resize(8, 1);
    t.screen_mut().scroll_view_up(1);
    let line = t.screen().scrollback().next().unwrap();
    assert_eq!(line.fill(), blue());
    assert_eq!(line.style(line.cell(7)), blue());
}

// "Line identity and marks": a logical line's first row keeps its id,
// and each other row takes a new one.
#[test]
fn the_first_row_keeps_its_id() {
    let mut t = term(8, 5, b"$ abcdefghij");
    let id = t.screen().line(0).unwrap().id();
    let second = t.screen().line(1).unwrap().id();
    t.resize(4, 5);
    let s = t.screen();
    assert_eq!(rows(s), ["$ ab", "cdef", "ghij", "", ""]);
    assert_eq!(s.line(0).unwrap().id(), id);
    let ids: Vec<_> = s.lines().iter().map(Line::id).collect();
    assert!(ids[1] > second && ids[2] > ids[1], "{ids:?}");
}

// "The scrollback's bound": a narrowing that leaves more rows than
// `sb-size` drops the oldest, and widening brings nothing back.
#[test]
fn a_narrowing_past_sb_size_drops_the_oldest_rows() {
    let mut t = with_scrollback(8, 1, 2, b"abcdefgh\r\nijklmnop\r\n");
    assert_eq!(all_rows(&mut t), ["abcdefgh", "ijklmnop", ""]);
    t.resize(4, 1);
    assert_eq!(all_rows(&mut t), ["ijkl", "mnop", ""]);
    t.resize(8, 1);
    assert_eq!(all_rows(&mut t), ["ijklmnop", ""]);
}

// "Cost": the scrollback past the screen and its margin waits, and
// rewraps when the view reaches it, as it would have at the resize.
#[test]
fn the_scrollback_rewraps_when_the_view_reaches_it() {
    let text: Vec<u8> = (0..20)
        .flat_map(|i| format!("line{i:02}-abcdef\r\n").into_bytes())
        .collect();
    let mut t = with_scrollback(13, 2, 100, &text);
    t.resize(7, 2);
    let rewrapped = t.screen().scrollback().len();
    assert!(rewrapped < 10, "{rewrapped} rows rewrapped at once");
    t.screen_mut().scroll_view_up(39);
    assert_eq!(t.screen().scrollback().len(), 39);
    let palette = Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
    let frame = t.screen().frame(&palette, Age::NEVER);
    let top: String = (0..7)
        .filter_map(|c| match frame.cell(0, c)?.glyph {
            Glyph::Char(ch) => Some(ch),
            _ => None,
        })
        .collect();
    assert_eq!(top, "line00-");
}

// "The cursor and the saved cursors": the view keeps the text of its top
// row at its top, and the selection keeps its text.
#[test]
fn the_view_and_the_selection_keep_their_text() {
    let text: Vec<u8> = (0..6)
        .flat_map(|i| format!("row{i}-abcdefgh\r\n").into_bytes())
        .collect();
    let mut t = with_scrollback(14, 2, 100, &text);
    t.screen_mut().scroll_view_up(3);
    t.screen_mut().select_start(4, 0);
    t.screen_mut().select_to(6, 0);
    let selected = t.screen().selection_text();
    assert_eq!(selected.as_deref(), Some("-ab"));
    t.resize(5, 2);
    assert_eq!(t.screen().selection_text(), selected);
    let palette = Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
    let frame = t.screen().frame(&palette, Age::NEVER);
    let top: String = (0..5)
        .filter_map(|c| match frame.cell(0, c)?.glyph {
            Glyph::Char(ch) => Some(ch),
            _ => None,
        })
        .collect();
    assert_eq!(top, "row2-");
}

// "Mode 2028": a row printed while mode 2028 is reset joins nothing, is
// joined into nothing and is cut at a narrower margin.
#[test]
fn a_row_printed_with_mode_2028_reset_is_cut() {
    let mut t = Terminal::new(8, 3, 0);
    t.screen_mut().set_reflow(false);
    t.input(b"abcdefghij");
    t.screen_mut().set_reflow(true);
    t.input(b"\r\nxyz");
    t.resize(4, 3);
    assert_eq!(rows(t.screen()), ["abcd", "ij", "xyz"]);
    t.resize(8, 3);
    assert_eq!(rows(t.screen()), ["abcd", "ij", "xyz"]);
    assert!(t.screen().reflow());
}

// "Cost": a resize whose margin takes rows still waiting from an earlier
// resize keeps the view on its text, as rewrapping at once does.
#[test]
fn the_view_keeps_its_text_when_the_margin_takes_waiting_rows() {
    let view = |t: &mut Terminal| {
        let at = t.screen().scrolled_back();
        snapshot(t)
            .into_iter()
            .filter(|l| l.starts_with("view"))
            .chain([format!("{at}")])
            .collect::<Vec<_>>()
    };
    for len in 1..8 {
        let text: Vec<u8> = (0..30)
            .flat_map(|i| format!("{}\r\n", format!("{i:02}").repeat(len)).into_bytes())
            .collect();
        for (rows, first, second) in [(5, 2, 18), (2, 3, 9), (3, 1, 7), (5, 3, 30), (4, 3, 1)] {
            for up in 1..24 {
                let mut lazy = with_scrollback(4, rows, 100, &text);
                let mut eager = with_scrollback(4, rows, 100, &text);
                for t in [&mut lazy, &mut eager] {
                    t.resize(first, rows);
                }
                eager.screen_mut().rewrap_scrollback();
                for t in [&mut lazy, &mut eager] {
                    t.screen_mut().scroll_view_up(up);
                    t.resize(second, rows);
                }
                let case = format!("lines of {len}, {rows} rows, to {first}, {up} up, to {second}");
                assert_eq!(view(&mut lazy), view(&mut eager), "{case}");
            }
        }
    }
}

// "Mode 2028" (ADR-1000's DEC-2028): set by default and by RIS, kept by
// DECSTR and reported by DECRQM; a row printed while it is reset is cut at
// a narrower width, and SM marks the cursor's row as rewrapping again.
// covers: DEC-2028
#[test]
fn mode_2028_marks_the_rows_a_resize_cuts() {
    let mut t = term(8, 3, b"\x1b[?2028$p");
    assert_eq!(t.take_replies(), b"\x1b[?2028;1$y");
    t.input(b"\x1b[?2028labcdefghij\x1b[?2028h\r\nxyz\x1b[!p\x1b[?2028$p");
    assert_eq!(t.take_replies(), b"\x1b[?2028;1$y");
    t.resize(4, 3);
    assert_eq!(rows(t.screen()), ["abcd", "ij", "xyz"]);

    let mut t = term(4, 3, b"\x1b[?2028l\x1b[!p\x1b[?2028$p");
    assert_eq!(t.take_replies(), b"\x1b[?2028;2$y");
    t.input(b"\x1bc\x1b[?2028$p");
    assert_eq!(t.take_replies(), b"\x1b[?2028;1$y");
    t.input(b"abcdef");
    t.resize(8, 3);
    assert_eq!(rows(t.screen()), ["abcdef", "", ""]);
}

/// A stream of shell-like output: text, wide characters, clusters,
/// newlines, erases in color, and the cursor left on the last row.
fn shell_stream(rng: &mut Rng, rows: usize) -> Vec<u8> {
    const PIECES: &[&str] = &[
        "a",
        "bc",
        "defgh",
        " ",
        "\u{4E2D}",
        "\u{1F600}",
        "e\u{301}",
        "\u{2764}\u{FE0F}",
        "\r\n",
        "\r\n",
        "\x1b[44m\x1b[K\x1b[m",
        "\x1b[31m",
        "\x1b[m",
    ];
    let mut out = Vec::new();
    for _ in 0..rng.below(300) {
        out.extend_from_slice(rng.pick(PIECES).as_bytes());
    }
    out.extend_from_slice(format!("\x1b[{rows};1H$ ").as_bytes());
    out
}

/// Each row's cells as they draw: glyph, width, style and link, and the
/// fill, flags and id; every scrollback row, then the screen's.
fn snapshot(t: &mut Terminal) -> Vec<String> {
    t.screen_mut().rewrap_scrollback();
    let s = t.screen();
    let describe = |line: &Line| {
        let cells: Vec<String> = line
            .cells()
            .iter()
            .map(|&c| {
                format!(
                    "{:?}/{:?}/{:?}/{:?}",
                    line.glyph(c),
                    c.width(),
                    line.style(c),
                    line.link(c)
                )
            })
            .collect();
        format!(
            "{:?} {:?} fill {:?}: {}",
            line.id(),
            line.flags(),
            line.fill(),
            cells.join(" ")
        )
    };
    let mut out: Vec<String> = s.scrollback().map(describe).collect();
    out.extend(s.lines().iter().map(describe));
    out.push(format!("cursor {:?}", s.cursor()));
    let palette = Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
    let frame = s.frame(&palette, Age::NEVER);
    for row in 0..frame.rows() {
        let text: String = (0..s.cols())
            .filter_map(|c| match frame.cell(row, c)?.glyph {
                Glyph::Char(ch) => Some(ch),
                _ => Some(' '),
            })
            .collect();
        out.push(format!("view {row} {:?} {text}", s.scrolled_back()));
    }
    out.push(format!("selection {:?}", s.selection_text()));
    out
}

/// The text of each logical line of the scrollback and the screen, all
/// of it rewrapped, empty lines at the end dropped: every cell of a wrapped
/// row but a pad ending it, and a last row to its last cell of text.
fn logical_lines(t: &mut Terminal) -> Vec<String> {
    t.screen_mut().rewrap_scrollback();
    let s = t.screen();
    let mut out = Vec::new();
    let mut text = String::new();
    for line in s.scrollback().chain(s.lines()) {
        if line.flags().contains(LineFlags::WRAPPED) {
            let cells = line.cells();
            let cells = match cells.split_last() {
                Some((last, rest)) if last.width() == Width::Pad => rest,
                _ => cells,
            };
            for &c in cells {
                match line.glyph(c) {
                    Glyph::Empty if c.width() == Width::WideTail => {}
                    Glyph::Empty => text.push(' '),
                    Glyph::Char(ch) => text.push(ch),
                    Glyph::Cluster(cluster) => text.push_str(cluster),
                }
            }
        } else {
            text.push_str(&common::line_text(line));
        }
        if !line.flags().contains(LineFlags::WRAPPED) {
            out.push(mem::take(&mut text).trim_end().to_string());
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

// Rewrapping to another width and back restores the text of the
// scrollback and the screen; once rewrapped, it restores the rows and the
// cursor too. Rows a program left otherwise rewrap differently and hold
// the same text: a wrapped row onto a row it then emptied, or a pad
// before text it printed over the wide character's row.
#[test]
fn rewrapping_to_another_width_and_back_restores_the_text() {
    for seed in 0..common::seeds(200) {
        let mut rng = Rng(seed);
        let (a, rows_n) = (2 + rng.below(30), 1 + rng.below(6));
        let b = 2 + rng.below(30);
        let bytes = shell_stream(&mut rng, rows_n);
        let mut t = with_scrollback(a, rows_n, 10_000, &bytes);
        let before = logical_lines(&mut t);
        t.resize(b, rows_n);
        t.resize(a, rows_n);
        assert_eq!(
            before,
            logical_lines(&mut t),
            "seed {seed}: {a} to {b} and back"
        );
        let rewrapped = (all_rows(&mut t), cursor(&t));
        t.resize(b, rows_n);
        t.resize(a, rows_n);
        assert_eq!(
            rewrapped,
            (all_rows(&mut t), cursor(&t)),
            "seed {seed}: rows, {a} to {b} and back"
        );
    }
}

/// What the host does between two bytes of a stream.
#[derive(Clone, Copy, Debug)]
enum Event {
    Resize(usize, usize),
    ViewUp(usize),
    ViewDown(usize),
    /// A selection from one view cell to another.
    Select(usize, usize, usize, usize),
}

/// A random event, resizes up to 30 by 8.
fn event(rng: &mut Rng) -> Event {
    match rng.below(5) {
        0 | 1 => Event::Resize(1 + rng.below(30), 1 + rng.below(8)),
        2 => Event::ViewUp(1 + rng.below(40)),
        3 => Event::ViewDown(1 + rng.below(10)),
        _ => Event::Select(rng.below(30), rng.below(8), rng.below(30), rng.below(8)),
    }
}

/// Feeds `bytes` in chunks cut at `cuts`, applying each `(at, event)`
/// between the bytes before and after `at`, and rewrapping the whole
/// scrollback after each resize when `eager`.
fn feed(t: &mut Terminal, bytes: &[u8], cuts: &[usize], events: &[(usize, Event)], eager: bool) {
    let mut at = 0;
    let mut stops: Vec<usize> = cuts
        .iter()
        .chain(events.iter().map(|r| &r.0))
        .copied()
        .collect();
    stops.push(bytes.len());
    stops.sort_unstable();
    stops.dedup();
    for stop in stops {
        let stop = stop.min(bytes.len());
        if stop > at {
            t.input(&bytes[at..stop]);
            at = stop;
        }
        for &(_, e) in events.iter().filter(|r| r.0 == stop) {
            let s = t.screen_mut();
            match e {
                Event::Resize(cols, rows) => {
                    t.resize(cols, rows);
                    if eager {
                        t.screen_mut().rewrap_scrollback();
                    }
                }
                Event::ViewUp(n) => s.scroll_view_up(n),
                Event::ViewDown(n) => s.scroll_view_down(n),
                Event::Select(x, y, x2, y2) => {
                    s.select_start(x, y);
                    s.select_to(x2, y2);
                }
            }
        }
    }
}

// The screen after a stream with resizes, view moves and selections is
// the same fed whole or cut at random points, and the same with the
// scrollback rewrapped lazily or all at once at each resize, the view and
// the selection included.
#[test]
fn reflow_is_the_same_chunked_and_lazy_or_eager() {
    for seed in 0..common::seeds(150) {
        let mut rng = Rng(seed ^ 0x2028);
        let mut bytes = Vec::new();
        for _ in 0..1 + rng.below(4) {
            let rows_n = 1 + rng.below(5);
            bytes.extend(shell_stream(&mut rng, rows_n));
            let len = rng.below(40);
            bytes.extend(common::grammar_stream(&mut rng, len));
        }
        let mut events: Vec<(usize, Event)> = (0..1 + rng.below(10))
            .map(|_| (rng.below(bytes.len() + 1), event(&mut rng)))
            .collect();
        // A run of events with no output between them: resizes, view moves
        // and selections in turn reach rows still waiting.
        for _ in 0..rng.below(8) {
            events.push((bytes.len(), event(&mut rng)));
        }
        let cuts: Vec<usize> = (0..rng.below(20))
            .map(|_| rng.below(bytes.len() + 1))
            .collect();
        let sb = rng.below(60);
        let make = || {
            let mut t = Terminal::new(1 + rng_cols(seed), 4, 100);
            t.screen_mut().set_scrollback_max(sb);
            t
        };
        let mut whole = make();
        feed(&mut whole, &bytes, &[], &events, false);
        let mut chunked = make();
        feed(&mut chunked, &bytes, &cuts, &events, false);
        assert!(
            whole.screen() == chunked.screen(),
            "seed {seed}: chunked differs"
        );
        let mut eager = make();
        feed(&mut eager, &bytes, &[], &events, true);
        assert_eq!(
            snapshot(&mut whole),
            snapshot(&mut eager),
            "seed {seed}: lazy and eager differ"
        );
    }
}

fn rng_cols(seed: u64) -> usize {
    (seed as usize * 7) % 23
}

// A rewrap keeps the logical line's text whole in the scrollback: a
// narrowing then a widening equal the widening alone.
#[test]
fn rewrapping_through_several_widths_equals_rewrapping_once() {
    for seed in 0..common::seeds(150) {
        let mut rng = Rng(seed ^ 0x1003);
        let rows_n = 1 + rng.below(5);
        let a = 2 + rng.below(30);
        let bytes = shell_stream(&mut rng, rows_n);
        let widths: Vec<usize> = (0..1 + rng.below(4)).map(|_| 2 + rng.below(30)).collect();
        let last = 2 + rng.below(30);
        let mut once = with_scrollback(a, rows_n, 10_000, &bytes);
        let mut many = with_scrollback(a, rows_n, 10_000, &bytes);
        for &w in &widths {
            many.resize(w, rows_n);
        }
        many.resize(last, rows_n);
        once.resize(last, rows_n);
        assert_eq!(
            logical_lines(&mut once),
            logical_lines(&mut many),
            "seed {seed}: {a} through {widths:?} to {last}"
        );
    }
}

/// The scrollback's heap, its rows rewrapped.
fn scrollback_heap(t: &mut Terminal) -> usize {
    t.screen_mut().rewrap_scrollback();
    t.screen().scrollback().map(Line::heap_bytes).sum()
}

// "Cost" (vtium/vtium#41's gate): the time a resize takes with a
// 100,000-row scrollback 170 cells wide, the screen and its margin at once
// and then the rest of the scrollback, and the scrollback's heap before
// and after: full rows wrapped in pairs, one logical line spanning the
// scrollback, a shell's short lines, and a style a cell. Run with `cargo
// test --release -p vtium-term --test reflow reflow_cost -- --ignored
// --nocapture`; the figures go to the crate's README.
#[test]
#[ignore = "a measurement, not a check"]
#[expect(
    clippy::disallowed_methods,
    reason = "a measurement reads the clock; nothing it reads reaches the screen"
)]
fn reflow_cost() {
    use std::time::Instant;
    let shapes: [(&str, String, usize); 4] = [
        ("wrapped pairs", format!("{}\r\n", "a".repeat(340)), 100_000),
        ("one logical line", "a".repeat(170), 100_000),
        ("shell lines", format!("{}\r\n", "a".repeat(40)), 100_000),
        (
            "a style a cell",
            format!("{}\r\n", "\x1b[38;2;1;2;3ma".repeat(340)),
            10_000,
        ),
    ];
    for (name, unit, rows) in shapes {
        let mut t = with_scrollback(170, 48, rows, b"");
        let per = if name == "shell lines" || name == "one logical line" {
            1
        } else {
            2
        };
        t.input(unit.repeat((rows + 48) / per).as_bytes());
        let before = scrollback_heap(&mut t);
        for cols in [169, 170, 100] {
            let at = Instant::now();
            t.resize(cols, 48);
            let resize = at.elapsed();
            let at = Instant::now();
            t.screen_mut().rewrap_scrollback();
            let rest = at.elapsed();
            let n = t.screen().scrollback().len();
            eprintln!(
                "{name}: to {cols} columns, {n} rows: the resize {resize:?}, the rest {rest:?}, {:.1} ns a cell in all",
                (resize + rest).as_nanos() as f64 / (n * 170) as f64
            );
        }
        eprintln!(
            "{name}: heap {before} bytes before, {} after",
            scrollback_heap(&mut t)
        );
    }
}
