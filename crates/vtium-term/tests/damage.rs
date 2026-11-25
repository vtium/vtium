//! Change tracking, as a property: after any change, every cell a display
//! would draw differently is marked changed for it.
//!
//! Each step takes the frame a display drew, makes one change through the
//! byte stream or the screen's own calls (the pen, the modes, resizing,
//! the view, the selection), and compares the frame after it cell by cell.
//! A cell that differs and is not marked fails the test, which prints the
//! seed, the step and the change. A region scroll marks only the region,
//! so the marks may spare rows, and this test checks that every row they
//! spare draws the same.

mod common;

use common::{Rng, SEQUENCES, TEXT};
use vtium_term::{
    Attrs, Color, CursorShape, Glyph, Palette, Rgb, Screen, Style, Terminal, UnderlineStyle, Width,
    WidthMethod,
};

/// One cell as a display draws it.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    glyph: String,
    width: Width,
    fg: Rgb,
    bg: Rgb,
    underline: Option<Rgb>,
    attrs: Attrs,
    default_bg: bool,
    /// The link's URI: a sweep of the link store may renumber it.
    link: Option<String>,
    hovered: bool,
    /// The URI the overlay shows over this cell's row.
    overlay: Option<String>,
    selected: bool,
}

fn palette() -> Palette {
    let mut colors = [Rgb::default(); 256];
    for (i, c) in colors.iter_mut().enumerate() {
        let i = i as u8;
        *c = Rgb::new(i, i.wrapping_mul(3), i.wrapping_mul(7));
    }
    Palette::new(colors, Rgb::new(0xe5, 0xe5, 0xe5), Rgb::new(0, 0, 0))
}

/// The view as a display draws it, and which cells the frame marks
/// changed since `since`.
fn draw(screen: &Screen, since: vtium_term::Age) -> Vec<Vec<(Drawn, bool)>> {
    let palette = palette();
    let frame = screen.frame(&palette, since);
    (0..frame.rows())
        .map(|r| {
            (0..frame.cols())
                .map(|c| {
                    let cell = frame.cell(r, c).unwrap();
                    let glyph = match cell.glyph {
                        Glyph::Empty => String::new(),
                        Glyph::Char(ch) => ch.into(),
                        Glyph::Cluster(s) => s.into(),
                    };
                    let drawn = Drawn {
                        glyph,
                        width: cell.width,
                        fg: cell.fg,
                        bg: cell.bg,
                        underline: cell.underline_color,
                        attrs: cell.attrs,
                        default_bg: cell.default_bg,
                        link: cell.link.and_then(|l| screen.link_uri(l)).map(String::from),
                        hovered: cell.hovered,
                        overlay: frame.overlay().filter(|o| o.row == r).map(|o| o.uri.into()),
                        selected: cell.selected,
                    };
                    (drawn, cell.changed)
                })
                .collect()
        })
        .collect()
}

fn random_style(rng: &mut Rng) -> Style {
    let color = |rng: &mut Rng| match rng.below(3) {
        0 => Color::Default,
        1 => Color::Indexed(rng.next() as u8),
        _ => Color::Rgb(Rgb::new(rng.next() as u8, 1, 2)),
    };
    let mut style = Style {
        fg: color(rng),
        bg: color(rng),
        underline_color: color(rng),
        ..Style::DEFAULT
    };
    for flag in [Attrs::BOLD, Attrs::INVERSE, Attrs::PROTECTED, Attrs::STRIKE] {
        if rng.below(3) == 0 {
            style.attrs.insert(flag);
        }
    }
    if rng.below(4) == 0 {
        style.attrs.set_underline(UnderlineStyle::Curly);
    }
    style
}

/// One change, described for a failure's message.
fn change(t: &mut Terminal, rng: &mut Rng, text: &[&str]) -> String {
    let s = t.screen_mut();
    let (cols, rows) = (s.cols(), s.rows());
    match rng.below(24) {
        0 => {
            s.set_pen(random_style(rng));
            "pen".into()
        }
        1 => {
            let link = match rng.below(5) {
                4 => String::new(),
                n => format!("https://{n}.example/"),
            };
            s.set_hyperlink(format!(";{link}").as_bytes());
            format!("link {link:?}")
        }
        2 => {
            let on = rng.below(2) == 0;
            s.set_reverse_video(on);
            format!("reverse {on}")
        }
        3 => {
            let on = rng.below(2) == 0;
            s.set_alternate(on);
            format!("alternate {on}")
        }
        4 => {
            let (c, r) = (1 + rng.below(12), 1 + rng.below(8));
            s.resize(c, r);
            format!("resize {c}x{r}")
        }
        5 => {
            let n = rng.below(4);
            s.scroll_view_up(n);
            format!("view up {n}")
        }
        6 => {
            let n = rng.below(4);
            s.scroll_view_down(n);
            format!("view down {n}")
        }
        7 => {
            s.reset_view();
            "view reset".into()
        }
        8 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_start(x, y);
            format!("select start {x},{y}")
        }
        9 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_to(x, y);
            format!("select to {x},{y}")
        }
        10 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_word(x, y);
            format!("select word {x},{y}")
        }
        11 => {
            s.clear_selection();
            "select clear".into()
        }
        12 => {
            let on = rng.below(2) == 0;
            s.set_insert_mode(on);
            format!("insert {on}")
        }
        13 => {
            let on = rng.below(2) == 0;
            s.set_auto_wrap(on);
            format!("autowrap {on}")
        }
        14 => {
            let on = rng.below(2) == 0;
            s.set_origin_mode(on);
            format!("origin {on}")
        }
        15 => {
            let m = if rng.below(2) == 0 {
                WidthMethod::Unicode
            } else {
                WidthMethod::Wcwidth
            };
            s.set_width_method(m);
            format!("width {m:?}")
        }
        16 => {
            let n = rng.below(4);
            s.set_scrollback_max(n);
            format!("scrollback {n}")
        }
        17 => {
            s.set_cursor_style(CursorShape::Bar, false);
            s.set_cursor_visible(rng.below(2) == 0);
            "cursor".into()
        }
        18 => {
            let at = (rng.below(4) != 0).then(|| (rng.below(cols), rng.below(rows)));
            let ctrl = rng.below(2) == 0;
            s.set_pointer(at, ctrl);
            format!("pointer {at:?} ctrl {ctrl}")
        }
        _ => {
            let mut bytes = Vec::new();
            for _ in 0..1 + rng.below(3) {
                let piece = if rng.below(2) == 0 {
                    rng.pick(SEQUENCES)
                } else {
                    rng.pick(text)
                };
                bytes.extend_from_slice(piece.as_bytes());
            }
            t.input(&bytes);
            format!("bytes {:?}", String::from_utf8_lossy(&bytes))
        }
    }
}

// The frame's changed set covers every cell that differs, through every
// kind of change.
#[test]
fn a_frames_changed_set_covers_every_cell_that_differs() {
    for seed in 0..common::seeds(80) {
        covers_every_change(seed, TEXT);
    }
}

/// The text pieces with Hebrew, Arabic and the sequences that set a
/// paragraph's direction, for a build with the bidi feature.
#[cfg(feature = "bidi")]
const BIDI_TEXT: &[&str] = &[
    "a",
    " ",
    "1",
    "(",
    ")",
    "\u{05D0}",
    "\u{05E9}\u{05DC}\u{05D5}\u{05DD}",
    "\u{0628}\u{0628}",
    "\u{0627}",
    "\u{0661}",
    "\u{2500}\u{250C}",
    "\x1b[2 k",
    "\x1b[1 k",
    "\x1b[?2501h",
    "\x1b[?2501l",
    "\x1b[?2500h",
    "\x1b[8l",
    "\x1b[8h",
    "\x1b[X",
    "\x1b[K",
    "\x1b[1;1H",
];

// ADR-1004 D8: with right-to-left text, the frame's changed set still
// covers every cell that is drawn differently, through every kind of
// change, reordered paragraphs, split paragraphs and property changes
// among them.
#[cfg(feature = "bidi")]
#[test]
fn a_frames_changed_set_covers_every_cell_of_right_to_left_text() {
    for seed in 0..common::seeds(80) {
        covers_every_change(seed, BIDI_TEXT);
    }
}

// Seed 218 found an edit that ends a row's wrap, IL here, turning the pad
// in its last column into an empty narrow cell unmarked (ADR-1003).
#[test]
fn an_ended_wrap_marks_the_pad_it_empties() {
    covers_every_change(218, TEXT);
}

/// A frame as one display drew it: each cell, and whether it was marked.
type Drawing = Vec<Vec<(Drawn, bool)>>;

/// The displays the property keeps at once: each drew a step older than
/// the one before it.
const HELD: usize = 5;

/// The cells of the view that the scroll record marks changed in place
/// for a display at `since`, with the rows it says the view moved.
fn in_place(screen: &Screen, since: vtium_term::Age) -> (Option<isize>, Vec<Vec<bool>>) {
    let palette = palette();
    let frame = screen.frame(&palette, since);
    let marks = (0..frame.rows())
        .map(|r| {
            (0..frame.cols())
                .map(|c| frame.changed_in_place(r, c))
                .collect()
        })
        .collect();
    (frame.scrolled(), marks)
}

/// Checks the scroll record for a display that drew `before` at `since`:
/// once the display moves its picture by the rows the record gives, every
/// cell that differs from `after` is marked changed in place, and so is
/// every cell of a row that came into view.
fn moved_picture_matches(
    screen: &Screen,
    since: vtium_term::Age,
    before: &[Vec<(Drawn, bool)>],
    after: &[Vec<(Drawn, bool)>],
    at: &str,
) {
    let (Some(by), marks) = in_place(screen, since) else {
        return;
    };
    if before.len() != after.len() || before.first().map(Vec::len) != after.first().map(Vec::len) {
        panic!("{at}: the record moved the rows by {by} across a resize");
    }
    for (r, (now, marked)) in after.iter().zip(&marks).enumerate() {
        let from = isize::try_from(r)
            .ok()
            .and_then(|r| r.checked_add(by))
            .and_then(|r| usize::try_from(r).ok())
            .and_then(|r| before.get(r));
        for (c, ((now, _), &marked)) in now.iter().zip(marked).enumerate() {
            let was = from.and_then(|row| row.get(c)).map(|(was, _)| was);
            assert!(
                marked || was == Some(now),
                "{at}: after moving by {by}, cell {r},{c} went from {was:?} to {now:?} unmarked in place"
            );
        }
    }
}

/// Runs the property's 100 random changes from `seed`, the text it writes
/// picked from `text`. Beside the display that drew just before each
/// change, it keeps displays that drew up to [`HELD`] steps before it, and
/// checks the scroll record for each of them.
fn covers_every_change(seed: u64, text: &[&str]) {
    {
        let mut rng = Rng(seed);
        let mut t = Terminal::new(1 + rng.below(12), 1 + rng.below(8), 6);
        t.screen_mut().set_scrollback_max(rng.below(6));
        let mut held: Vec<(vtium_term::Age, Drawing)> = Vec::new();
        for step in 0..100 {
            let seen = t.screen_mut().checkpoint();
            let before = draw(t.screen(), seen);
            held.insert(0, (seen, before.clone()));
            held.truncate(HELD);
            let what = change(&mut t, &mut rng, text);
            let after = draw(t.screen(), seen);
            for (i, (age, drew)) in held.iter().enumerate() {
                let at = format!("seed {seed}, step {step}, {what}, a display {i} steps older");
                moved_picture_matches(t.screen(), *age, drew, &after, &at);
            }
            if before.len() != after.len()
                || before.first().map(Vec::len) != after.first().map(Vec::len)
            {
                let all = after.iter().flatten().all(|&(_, changed)| changed);
                assert!(
                    all,
                    "seed {seed}, step {step}, {what}: a resize left cells unmarked"
                );
                continue;
            }
            for (r, (b, a)) in before.iter().zip(&after).enumerate() {
                for (c, ((was, _), (now, changed))) in b.iter().zip(a).enumerate() {
                    assert!(
                        was == now || *changed,
                        "seed {seed}, step {step}, {what}: cell {r},{c} went from {was:?} to {now:?} unmarked"
                    );
                }
            }
        }
    }
}

// A change that alters nothing visible may mark cells, but a display that
// drew after it sees nothing changed until the next change.
#[test]
fn after_a_checkpoint_nothing_is_changed() {
    let mut t = Terminal::new(8, 4, 10);
    t.input("ab\u{4E2D}\x1b[2;3r\n\n\n".as_bytes());
    let seen = t.screen_mut().checkpoint();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
    // Moving the cursor and changing the pen mark nothing (ADR-1001).
    t.input(b"\x1b[3;3H\x1b[A\x1b[2C");
    t.screen_mut().set_pen(Style {
        bg: Color::Indexed(2),
        ..Style::DEFAULT
    });
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
}

// Clearing the selection marks every cell, since its cells drew inverted;
// with none to clear it marks nothing, as the host clears it at each key.
#[test]
fn clearing_no_selection_marks_nothing() {
    let mut t = Terminal::new(8, 4, 10);
    t.input(b"ab");
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().clear_selection();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
    t.screen_mut().select_start(0, 0);
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().clear_selection();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| c));
}

// A palette the host sets while text is on screen recolors every cell, so
// a display that drew before it sees every cell changed.
#[test]
fn a_new_palette_marks_every_cell() {
    let mut t = Terminal::new(8, 4, 10);
    t.input(b"ab\x1b[31mcd");
    let seen = t.screen_mut().checkpoint();
    let mut changed = palette();
    changed.set_color(1, Rgb::new(1, 2, 3));
    t.set_palette(changed.clone());
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| c));
    // The same palette again changes nothing a display draws.
    let seen = t.screen_mut().checkpoint();
    t.set_palette(changed);
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
}

/// The rows the scroll record says the view moved for a display at
/// `since`, and the cells it marks changed in place, row by row.
fn record(t: &Terminal, since: vtium_term::Age) -> (Option<isize>, Vec<(usize, usize)>) {
    let (by, marks) = in_place(t.screen(), since);
    let cells = marks
        .iter()
        .enumerate()
        .flat_map(|(r, row)| {
            row.iter()
                .enumerate()
                .filter(|&(_, &m)| m)
                .map(move |(c, _)| (r, c))
        })
        .collect();
    (by, cells)
}

/// Every cell of the rows in `rows` on a screen `cols` wide.
fn rows_of(rows: core::ops::Range<usize>, cols: usize) -> Vec<(usize, usize)> {
    rows.flat_map(|r| (0..cols).map(move |c| (r, c))).collect()
}

// One line feed on the bottom row moves the view up a row. The cell
// written there before the line feed is changed in place one row higher,
// the new bottom row is changed in place, and nothing else is, though by
// position every row is changed.
#[test]
fn a_line_feed_moves_the_ages_with_the_rows() {
    let mut t = Terminal::new(6, 4, 10);
    t.input(b"one\r\ntwo\r\nsix\r\nten");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b[4;2HX\n");
    let (by, cells) = record(&t, seen);
    assert_eq!(by, Some(1));
    let mut want = vec![(2, 1)];
    want.extend(rows_of(3..4, 6));
    assert_eq!(cells, want);
    let palette = palette();
    let frame = t.screen().frame(&palette, seen);
    let rows: Vec<bool> = (0..4).map(|r| frame.row_changed_in_place(r)).collect();
    assert_eq!(rows, [false, false, true, true]);
    assert!((0..4).all(|r| frame.row_changed(r)));
    let drawn = draw(t.screen(), seen);
    assert!(drawn.iter().flatten().all(|&(_, changed)| changed));
}

// A reverse index on the top row moves the view down a row, and SU and SD
// with the margins at the screen's edges count as a line feed and a
// reverse index do.
#[test]
fn a_reverse_index_on_the_top_row_counts_down() {
    let mut t = Terminal::new(6, 4, 10);
    t.input(b"one\r\ntwo\r\nsix\r\nten");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b[1;1H\x1bM");
    assert_eq!(record(&t, seen), (Some(-1), rows_of(0..1, 6)));
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b[2T");
    assert_eq!(record(&t, seen), (Some(-2), rows_of(0..2, 6)));
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b[3S");
    assert_eq!(record(&t, seen), (Some(3), rows_of(1..4, 6)));
}

// Two displays that drew at different ages read the count each since its
// own age.
#[test]
fn two_displays_at_different_ages_read_different_counts() {
    let mut t = Terminal::new(6, 4, 10);
    t.input(b"\x1b[4;1H");
    let old = t.screen_mut().checkpoint();
    t.input(b"a\n");
    let new = t.screen_mut().checkpoint();
    t.input(b"b\n\n");
    let frame_old = record(&t, old);
    let frame_new = record(&t, new);
    assert_eq!(frame_old.0, Some(3));
    assert_eq!(frame_new.0, Some(2));
    // `a` sits on the top row and `b` one row and column below it, since a
    // line feed keeps the column. Both were written after the old display
    // drew, and only `b` after the new one drew.
    let mut want_old = vec![(0, 0)];
    want_old.extend(rows_of(1..4, 6));
    let mut want_new = vec![(1, 1)];
    want_new.extend(rows_of(2..4, 6));
    assert_eq!(frame_old.1, want_old);
    assert_eq!(frame_new.1, want_new);
}

// Moving the view back through the scrollback counts down by the rows it
// moved, and forward counts up; a move past the scrollback's end counts
// only the rows the view moved.
#[test]
fn moving_the_view_counts_its_rows() {
    let mut t = Terminal::new(6, 4, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().scroll_view_up(2);
    assert_eq!(record(&t, seen), (Some(-2), rows_of(0..2, 6)));
    let back = t.screen_mut().checkpoint();
    t.screen_mut().scroll_view_up(100);
    assert_eq!(record(&t, back).0, Some(-1));
    let top = t.screen_mut().checkpoint();
    t.screen_mut().scroll_view_down(1);
    t.screen_mut().reset_view();
    assert_eq!(record(&t, top), (Some(3), rows_of(1..4, 6)));
    assert_eq!(record(&t, seen).0, Some(0));
}

// The scrolls the record cannot state as every row moving by one count
// mark every row they touch, as they did before the record: a scroll
// inside margins narrower than the screen, a scroll on the alternate
// screen, a scroll while a selection is shown, a scroll while the view is
// back in the scrollback, and a move of the view while a selection is
// shown.
#[test]
fn the_scrolls_the_record_cannot_state_mark_every_row_they_touch() {
    let text = b"one\r\ntwo\r\nsix\r\nten";

    let mut t = Terminal::new(6, 4, 10);
    t.input(text);
    t.input(b"\x1b[2;3r\x1b[3;1H");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\n");
    assert_eq!(record(&t, seen), (Some(0), rows_of(1..3, 6)), "margins");

    let mut t = Terminal::new(6, 4, 10);
    t.input(b"\x1b[?1049h");
    t.input(text);
    let seen = t.screen_mut().checkpoint();
    t.input(b"\n");
    assert_eq!(record(&t, seen), (Some(0), rows_of(0..4, 6)), "alternate");

    let mut t = Terminal::new(6, 4, 10);
    t.input(text);
    t.screen_mut().select_start(0, 0);
    t.screen_mut().select_to(2, 1);
    let seen = t.screen_mut().checkpoint();
    t.input(b"\n");
    assert_eq!(record(&t, seen), (None, rows_of(0..4, 6)), "selection");

    let mut t = Terminal::new(6, 4, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"1\r\n2\r\n3\r\n4\r\n5\r\n6");
    t.screen_mut().scroll_view_up(1);
    let seen = t.screen_mut().checkpoint();
    t.input(b"\n");
    assert_eq!(record(&t, seen), (None, rows_of(0..4, 6)), "view back");

    let mut t = Terminal::new(6, 4, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"1\r\n2\r\n3\r\n4\r\n5\r\n6");
    t.screen_mut().select_start(0, 0);
    t.screen_mut().select_to(2, 1);
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().scroll_view_up(1);
    assert_eq!(record(&t, seen), (None, rows_of(0..4, 6)), "view move");
}

// A display older than the scroll log's oldest entry reads no count, and
// its frame's changes in place are its changes by position, which cover
// every row that moved.
#[test]
fn a_display_older_than_the_log_reads_every_moved_row() {
    let mut t = Terminal::new(6, 4, 10);
    t.input(b"\x1b[4;1H");
    let old = t.screen_mut().checkpoint();
    // more checkpoints with a scroll than the log keeps
    for _ in 0..1000 {
        t.input(b"\n");
        t.screen_mut().checkpoint();
    }
    assert_eq!(record(&t, old), (None, rows_of(0..4, 6)));
}

// A scroll that drops the first line of a right-to-left paragraph leaves
// the rest of it in a paragraph of its own, drawn in another order: the
// record marks those rows changed in place, though they only moved.
#[cfg(feature = "bidi")]
#[test]
fn a_scroll_that_drops_a_paragraphs_first_line_marks_the_rest() {
    let mut t = Terminal::new(4, 3, 10);
    t.screen_mut().set_scrollback_max(0);
    t.input("\x1b[?2501h\u{05D0}\u{05D1}\u{05D2}\u{05D3}12 3".as_bytes());
    let seen = t.screen_mut().checkpoint();
    let before = draw(t.screen(), seen);
    t.input(b"\x1b[S");
    let after = draw(t.screen(), seen);
    // "3 12" was drawn on the second row and moves to the first as "12 3"
    assert_ne!(
        before[1].iter().map(|(d, _)| &d.glyph).collect::<Vec<_>>(),
        after[0].iter().map(|(d, _)| &d.glyph).collect::<Vec<_>>()
    );
    let mut want = rows_of(0..1, 4);
    want.extend(rows_of(2..3, 4));
    assert_eq!(record(&t, seen), (Some(1), want));
}

// Seed 4783 found IL cutting a right-to-left paragraph at the cursor's row
// and leaving the paragraph's row above it, drawn in another order without
// its tail, unmarked by position (ADR-1004 D8).
#[cfg(feature = "bidi")]
#[test]
fn an_insert_that_cuts_a_paragraph_marks_its_rows_above() {
    covers_every_change(4783, BIDI_TEXT);
}

// Seed 3856 found SD dropping the tail of a right-to-left paragraph at the
// bottom and leaving the rest of it, moved down a row and drawn in another
// order, unmarked in place.
#[cfg(feature = "bidi")]
#[test]
fn a_scroll_down_that_drops_a_paragraphs_tail_marks_the_rest() {
    covers_every_change(3856, BIDI_TEXT);
}
