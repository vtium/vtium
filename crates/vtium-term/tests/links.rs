//! OSC 8 hyperlinks (ADR-1002, sections 1, 2 and 4): the cells printed
//! while a link is open name it through their line's link table, the store
//! holds each distinct link once within `LINKS_BYTES_MAX`, what a link may
//! hold, and the hovered link and its overlay a frame carries.

mod common;

use common::{rows, scrollback, term};
use vtium_term::{Age, LINKS_BYTES_MAX, Line, LineFlags, LinkId, Palette, Screen, Terminal};

/// `OSC 8 ; params ; uri ST`.
fn osc8(params: &str, uri: &str) -> String {
    format!("\x1b]8;{params};{uri}\x1b\\")
}

/// The link of cell `x` of the screen's row `y`.
fn link(s: &Screen, x: usize, y: usize) -> Option<LinkId> {
    let line = s.line(y)?;
    line.link(line.cell(x))
}

/// The URI of cell `x` of the screen's row `y`.
fn uri(s: &Screen, x: usize, y: usize) -> Option<&str> {
    s.link_uri(link(s, x, y)?)
}

/// A terminal `cols` by `rows` keeping `sb` scrollback rows.
fn with_scrollback(cols: usize, rows: usize, sb: usize) -> Terminal {
    let mut t = Terminal::new(cols, rows, sb);
    t.screen_mut().set_scrollback_max(sb);
    t
}

// "What a link may hold": an open link is part of the pen, printed cells
// take it, and an empty URI closes it.
// covers: osc.8
#[test]
fn the_cells_printed_while_a_link_is_open_name_it() {
    let bytes = format!("a{}bc{}d", osc8("", "https://vtium.example/"), osc8("", ""));
    let t = term(10, 2, bytes.as_bytes());
    let s = t.screen();
    assert_eq!(rows(s)[0], "abcd");
    assert_eq!(link(s, 0, 0), None);
    assert_eq!(uri(s, 1, 0), Some("https://vtium.example/"));
    assert_eq!(link(s, 1, 0), link(s, 2, 0));
    assert_eq!(link(s, 3, 0), None);
    assert_eq!(s.link(), None);
    assert!(t.replies().is_empty(), "OSC 8 has no reply");
}

// "Link storage": the same id and URI take the entry the store holds, and
// links with no id and the same URI are one link; another id is another.
// covers: osc.8
#[test]
fn a_link_sent_again_takes_the_entry_the_store_holds() {
    let a = osc8("", "https://a.example/");
    let a1 = osc8("id=1", "https://a.example/");
    let a2 = osc8("id=2:x=y", "https://a.example/");
    let b = osc8("", "https://b.example/");
    let bytes = format!("{a}x{b}y{a}z{a1}p{a2}q{a1}r");
    let t = term(10, 2, bytes.as_bytes());
    let s = t.screen();
    let at = |x| link(s, x, 0).unwrap();
    assert_eq!(at(0), at(2));
    assert_ne!(at(0), at(1));
    assert_ne!(at(0), at(3), "an id makes another link");
    assert_ne!(at(3), at(4));
    assert_eq!(at(3), at(5));
    assert_eq!(s.link_id_param(at(3)), Some("1"));
    assert_eq!(s.link_id_param(at(4)), Some("2"), "other keys are ignored");
    assert_eq!(s.link_id_param(at(0)), None);
    let charged = s.link_store_bytes();
    let again = format!("{a}x{b}y{a1}p{a2}q");
    let mut t2 = t.clone();
    t2.input(again.as_bytes());
    assert_eq!(t2.screen().link_store_bytes(), charged, "nothing is added");
}

// "What a link may hold": a URI of 1 to 2,083 bytes, each 0x21 to 0x7E,
// starting with a scheme; one that fails closes the open link.
// covers: osc.8
#[test]
fn a_uri_that_fails_its_rule_closes_the_open_link() {
    let long = format!("https://{}", "a".repeat(2083 - 8));
    assert_eq!(long.len(), 2083);
    let fits = term(4, 1, format!("{}x", osc8("", &long)).as_bytes());
    assert_eq!(uri(fits.screen(), 0, 0), Some(long.as_str()));

    for bad in [
        format!("{long}a"),
        "https://a.example/a b".into(),
        "https://a.example/\u{E9}".into(),
        "https://a.example/\x7f".into(),
        "a.example/no-scheme".into(),
        "1http://a.example/".into(),
        ":a".into(),
        "ht_tp://a.example/".into(),
    ] {
        let bytes = format!("{}x{}y", osc8("", "https://ok.example/"), osc8("", &bad));
        let t = term(4, 1, bytes.as_bytes());
        let s = t.screen();
        assert_eq!(uri(s, 0, 0), Some("https://ok.example/"), "{bad:?}");
        assert_eq!(link(s, 1, 0), None, "{bad:?} closes the link");
        assert_eq!(s.link(), None, "{bad:?}");
    }
    for good in ["a:", "z+.-9:x", "file://pixie/home/a%20b"] {
        let t = term(4, 1, format!("{}x", osc8("", good)).as_bytes());
        assert_eq!(uri(t.screen(), 0, 0), Some(good), "{good:?}");
    }
}

// "What a link may hold": an id of 1 to 250 bytes, each 0x21 to 0x7E; a
// longer or malformed id is ignored and the link has none.
// covers: osc.8
#[test]
fn an_id_that_fails_its_rule_leaves_the_link_with_none() {
    let id250 = "i".repeat(250);
    let cases = [
        (format!("id={id250}"), Some(id250.as_str())),
        (format!("id={id250}i"), None),
        ("id=".into(), None),
        ("id=a\u{E9}".into(), None),
        ("ID=a".into(), None),
        ("foo=1:id=b".into(), Some("b")),
    ];
    for (params, want) in cases {
        let bytes = format!("{}x", osc8(&params, "https://a.example/"));
        let t = term(4, 1, bytes.as_bytes());
        let s = t.screen();
        let l = link(s, 0, 0).expect("the link opens");
        assert_eq!(s.link_id_param(l), want, "{params:?}");
    }
}

// A truncated or cancelled string sets nothing: the open link stays.
// covers: osc.8
#[test]
fn a_cancelled_osc_8_changes_nothing() {
    let bytes = format!(
        "{}x\x1b]8;;https://b.example/\x18y",
        osc8("", "https://a.example/")
    );
    let t = term(4, 1, bytes.as_bytes());
    assert_eq!(uri(t.screen(), 1, 0), Some("https://a.example/"));
}

// "What a link may hold": erased cells take no link, and RIS and a switch
// between the main and alternate screens end the open link.
// covers: osc.8
#[test]
fn erases_resets_and_screen_switches_end_or_drop_the_link() {
    let open = osc8("", "https://a.example/");
    let t = term(6, 2, format!("{open}abcd\x1b[3G\x1b[X").as_bytes());
    assert!(link(t.screen(), 1, 0).is_some());
    assert_eq!(link(t.screen(), 2, 0), None, "ECH erases the link");
    assert!(t.screen().link().is_some(), "an erase keeps the pen's link");

    for switch in ["\x1bc", "\x1b[?1049h", "\x1b[?47h", "\x1b[?1047h"] {
        let t = term(6, 2, format!("{open}a{switch}b").as_bytes());
        assert_eq!(t.screen().link(), None, "{switch:?}");
        let (x, y) = (t.screen().cursor().0 - 1, t.screen().cursor().1);
        assert_eq!(link(t.screen(), x, y), None, "{switch:?}");
    }
    let t = term(6, 2, format!("\x1b[?1049h{open}a\x1b[?1049lb").as_bytes());
    assert_eq!(t.screen().link(), None, "leaving the alternate screen");
}

// The observable: a link printed across a wrap, scrolled into the
// scrollback and resized keeps every cell's link.
// covers: osc.8
#[test]
fn a_link_keeps_its_cells_through_a_wrap_the_scrollback_and_a_resize() {
    let mut t = with_scrollback(6, 2, 10);
    let open = osc8("id=w", "https://wrap.example/");
    t.input(format!("ab{open}cdefghij{}k\r\n\r\n\r\n", osc8("", "")).as_bytes());
    let s = t.screen();
    assert_eq!(scrollback(s), ["abcdef", "ghijk", ""]);
    let first = s.scrollback().next().unwrap();
    let id = first.link(first.cell(2)).unwrap();
    assert_eq!(s.link_uri(id), Some("https://wrap.example/"));
    t.resize(12, 2);
    t.screen_mut().rewrap_scrollback();
    let s = t.screen();
    let line = s.scrollback().next().unwrap();
    assert_eq!(scrollback(s)[0], "abcdefghijk");
    for x in 0..12 {
        let want = (2..10).contains(&x).then_some("https://wrap.example/");
        assert_eq!(line.link(line.cell(x)).and_then(|l| s.link_uri(l)), want);
    }
}

/// `n` distinct links of `len` bytes, one cell each, each on its own line.
fn flood(n: usize, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..n {
        let uri = format!("https://{i:08}.example/{}", "p".repeat(len - 25));
        assert_eq!(uri.len(), len);
        out.extend(osc8("", &uri).bytes());
        out.extend(b"x\r\n");
        out.extend(osc8("", "").bytes());
    }
    out
}

// The observable: a flood of distinct links past `LINKS_BYTES_MAX` strips
// the oldest scrollback lines' links first and keeps every link on the
// screen; fed whole and chunked, it ends in the same store.
// covers: osc.8
#[test]
fn a_flood_of_links_strips_the_oldest_scrollback_lines_first() {
    let bytes = flood(600, 1000);
    let mut whole = with_scrollback(4, 24, 1000);
    whole.input(&bytes);
    let s = whole.screen();
    assert!(s.link_store_bytes() <= LINKS_BYTES_MAX);
    // Every screen line but the cursor's names its link.
    for y in 0..23 {
        assert!(uri(s, 0, y).is_some(), "row {y} keeps its link");
    }
    let linked: Vec<bool> = s
        .scrollback()
        .map(|l| l.link(l.cell(0)).is_some())
        .collect();
    let first = linked.iter().position(|&l| l).unwrap();
    assert!(first > 0, "the oldest lines lost their links");
    assert!(linked[first..].iter().all(|&l| l), "only the oldest did");
    assert_eq!(scrollback(s).len(), 577, "the text stays");
    let held: usize = s
        .scrollback()
        .map(|l| size_of::<Line>() + l.heap_bytes())
        .sum();
    assert_eq!(
        s.scrollback_bytes(),
        held,
        "the byte count follows the strips"
    );

    let mut chunked = with_scrollback(4, 24, 1000);
    for piece in bytes.chunks(97) {
        chunked.input(piece);
    }
    assert_eq!(chunked.screen(), whole.screen());
}

// "Link storage": a sweep frees only links no line names, and keeps the
// screen's: with no scrollback, each sweep leaves the three links on the
// screen, and the store then grows by one entry a link until the next.
// covers: osc.8
#[test]
fn a_sweep_frees_only_links_no_line_names() {
    let (n, cost) = (600, 1000 + 64);
    let mut t = with_scrollback(4, 4, 0);
    t.input(&flood(n, 1000));
    let mut held = 0;
    for i in 0..n {
        if (held + 1) * cost > LINKS_BYTES_MAX {
            held = i.min(3);
        }
        held += 1;
    }
    let s = t.screen();
    assert_eq!(s.link_store_bytes(), held * cost);
    for y in 0..3 {
        assert!(uri(s, 0, y).is_some(), "row {y}");
    }
    // A link the sweeps kept is found again by its id and URI.
    let (kept, again) = (link(s, 0, 0), uri(s, 0, 0).unwrap().to_owned());
    t.input(format!("{}x", osc8("", &again)).as_bytes());
    assert_eq!(t.screen().link(), kept);
    assert_eq!(t.screen().link_store_bytes(), held * cost);
}

// "Link storage": the store is shared by both screens, so a sweep on the
// main screen keeps the links the alternate screen's lines name.
// covers: osc.8
#[test]
fn a_sweep_keeps_the_links_of_the_other_screen() {
    let mut t = with_scrollback(4, 4, 0);
    t.input(format!("\x1b[?47h{}a\x1b[?47l", osc8("", "https://alt.example/")).as_bytes());
    t.input(&flood(600, 1000));
    t.input(b"\x1b[?47h");
    assert_eq!(uri(t.screen(), 0, 0), Some("https://alt.example/"));
}

// "Link storage": when the screens alone hold more than the bound, the
// open is refused and the text prints unlinked.
// covers: osc.8
#[test]
fn when_the_screens_hold_the_bound_a_new_link_prints_unlinked() {
    let mut t = with_scrollback(200, 2, 0);
    let mut bytes = Vec::new();
    for i in 0..200 {
        let uri = format!("https://{i:04}.example/{}", "q".repeat(2000));
        assert_eq!(uri.len(), 2021);
        bytes.extend(osc8("", &uri).bytes());
        bytes.push(b'x');
    }
    t.input(&bytes);
    let s = t.screen();
    assert!(s.link_store_bytes() <= LINKS_BYTES_MAX);
    let held = (0..200).filter(|&x| link(s, x, 0).is_some()).count();
    assert_eq!(held, LINKS_BYTES_MAX / (2021 + 64));
    assert!((0..held).all(|x| link(s, x, 0).is_some()), "the first ones");
    assert_eq!(s.link(), None, "the refused open leaves none");
    assert_eq!(rows(s)[0].len(), 200, "the text prints");
}

// "Link storage": a test pins the store's heap at or under its charge.
// covers: osc.8
#[test]
fn the_stores_heap_stays_at_or_under_its_charge() {
    for (n, len) in [
        (1, 30),
        (2, 30),
        (5000, 30),
        (600, 1000),
        (300, 2083),
        (40, 60),
    ] {
        let mut t = with_scrollback(4, 24, 1000);
        t.input(&flood(n, len));
        let s = t.screen();
        assert!(
            s.link_store_heap() <= s.link_store_bytes(),
            "{n} links of {len}: heap {} over its charge {}",
            s.link_store_heap(),
            s.link_store_bytes()
        );
    }
}

// "Drawing": the hovered link is in the frame, every visible cell of it
// marked hovered, and a hover change marks only the rows holding the old
// or the new link.
// covers: osc.8
#[test]
fn the_hovered_link_and_its_rows_are_in_the_frame() {
    let a = osc8("", "https://a.example/");
    let b = osc8("", "https://b.example/");
    let end = osc8("", "");
    let bytes = format!("{a}aa{end}\r\nplain\r\n{b}bb{end}\r\n{a}a{end}");
    let mut t = term(6, 5, bytes.as_bytes());
    let palette = Palette::default();
    let s = t.screen_mut();
    let age = s.checkpoint();
    s.set_pointer(Some((1, 0)), false);
    let la = link(s, 0, 0);
    assert_eq!(s.hovered_link(), la);
    let frame = s.frame(&palette, age);
    assert_eq!(frame.hovered(), la);
    assert!(frame.cell(0, 0).unwrap().hovered);
    assert!(frame.cell(3, 0).unwrap().hovered, "every visible cell");
    assert!(!frame.cell(2, 0).unwrap().hovered);
    let changed: Vec<usize> = (0..5).filter(|&r| frame.row_changed(r)).collect();
    assert_eq!(changed, [0, 3]);
    assert_eq!(frame.overlay(), None);

    // A move within the link marks nothing.
    let age = s.checkpoint();
    s.set_pointer(Some((0, 0)), false);
    let frame = s.frame(&palette, age);
    assert!((0..5).all(|r| !frame.row_changed(r)));

    // Crossing to another link marks both links' rows.
    let age = s.checkpoint();
    s.set_pointer(Some((0, 2)), false);
    let frame = s.frame(&palette, age);
    let changed: Vec<usize> = (0..5).filter(|&r| frame.row_changed(r)).collect();
    assert_eq!(changed, [0, 2, 3]);

    // Off a link, and off the screen.
    s.set_pointer(Some((4, 1)), false);
    assert_eq!(s.hovered_link(), None);
    s.set_pointer(None, false);
    assert_eq!(s.hovered_link(), None);
}

// "Drawing": with Ctrl held the overlay shows the URI on the bottom row,
// or the top row when the pointer is on the bottom one, and marks its row.
// covers: osc.8
#[test]
fn the_overlay_shows_the_uri_on_the_bottom_row_or_the_top() {
    let a = osc8("", "https://a.example/");
    let end = osc8("", "");
    let bytes = format!("{a}aa{end}\r\n\r\n\r\n{a}a{end}");
    let mut t = term(6, 4, bytes.as_bytes());
    let palette = Palette::default();
    let s = t.screen_mut();
    let age = s.checkpoint();
    s.set_pointer(Some((0, 0)), true);
    let frame = s.frame(&palette, age);
    let overlay = frame.overlay().unwrap();
    assert_eq!((overlay.row, overlay.uri), (3, "https://a.example/"));
    assert!(frame.row_changed(3));

    s.set_pointer(Some((0, 3)), true);
    let frame = s.frame(&palette, Age::NEVER);
    assert_eq!(frame.overlay().unwrap().row, 0);

    let age = s.checkpoint();
    s.set_pointer(Some((0, 3)), false);
    let frame = s.frame(&palette, age);
    assert_eq!(frame.overlay(), None);
    assert!(
        frame.row_changed(0),
        "the overlay's row is marked as it goes"
    );
}

// "Drawing": a link arriving under a still pointer is hovered once the
// input is applied; one leaving is not.
// covers: osc.8
#[test]
fn a_link_arriving_under_a_still_pointer_is_hovered() {
    let mut t = term(6, 2, b"");
    t.screen_mut().set_pointer(Some((0, 0)), false);
    t.input(format!("{}x", osc8("", "https://a.example/")).as_bytes());
    assert!(t.screen().hovered_link().is_some());
    t.input(b"\r\x1b[K");
    assert_eq!(t.screen().hovered_link(), None);
}

// The prompt marks and the links share nothing: a link on a marked line
// keeps both.
#[test]
fn a_link_and_a_mark_share_a_line() {
    let bytes = format!("\x1b]133;A\x1b\\{}$", osc8("", "https://a.example/"));
    let t = term(6, 2, bytes.as_bytes());
    let line = t.screen().line(0).unwrap();
    assert!(line.flags().contains(LineFlags::PROMPT));
    assert!(line.link(line.cell(0)).is_some());
}

// A sweep reads the link under the pointer as the stream left it, so the
// store follows the bytes however they were cut: a hover the stream erased
// keeps nothing alive.
// covers: osc.8
#[test]
fn a_sweep_after_an_erase_under_the_pointer_follows_the_bytes() {
    let long = format!("https://hover.example/{}", "h".repeat(1999));
    assert_eq!(long.len(), 2021);
    let start = || {
        let mut t = with_scrollback(4, 24, 1000);
        t.input(format!("{}x{}", osc8("", &long), osc8("", "")).as_bytes());
        t.screen_mut().set_pointer(Some((0, 0)), false);
        assert!(t.screen().hovered_link().is_some());
        t
    };
    let erase = b"\x1b[2J\x1b[3J";
    let rest = flood(600, 1000);
    let mut whole = start();
    whole.input(&[&erase[..], &rest].concat());
    let mut split = start();
    split.input(erase);
    split.input(&rest);
    assert_eq!(split.screen(), whole.screen());
}

// "Drawing": the overlay's row is marked whenever what it shows changes:
// crossing from one link to another under Ctrl, and a link arriving under
// a still pointer.
// covers: osc.8
#[test]
fn the_overlay_row_is_marked_when_its_uri_changes() {
    let a = osc8("", "https://a.example/");
    let b = osc8("", "https://b.example/");
    let end = osc8("", "");
    let mut t = term(6, 4, format!("{a}aa{b}bb{end}").as_bytes());
    let palette = Palette::default();
    t.screen_mut().set_pointer(Some((0, 0)), true);
    let age = t.screen_mut().checkpoint();
    t.screen_mut().set_pointer(Some((2, 0)), true);
    let frame = t.screen().frame(&palette, age);
    assert_eq!(frame.overlay().unwrap().uri, "https://b.example/");
    assert!(frame.row_changed(3), "crossing to another link");

    let age = t.screen_mut().checkpoint();
    t.input(format!("\x1b[H{a}aaaa{end}").as_bytes());
    let frame = t.screen().frame(&palette, age);
    assert_eq!(frame.overlay().unwrap().uri, "https://a.example/");
    assert!(frame.row_changed(3), "a link arriving under the pointer");
}

// "Link storage": a sweep renumbers the links it keeps in every line's
// table, on both screens and in the scrollback. Each line prints its own
// name under a link carrying that name, the alternate screen's opened
// after links that die, so every renumbering moves it.
// covers: osc.8
#[test]
fn every_line_keeps_its_own_link_through_sweeps_and_strips() {
    let mut t = with_scrollback(8, 4, 1000);
    for i in 0..10 {
        t.input(osc8("", &format!("https://dead{i}.example/")).as_bytes());
    }
    let alt = format!("https://alt.example/{}", "a".repeat(900));
    t.input(format!("\x1b[?47h{}alt{}\x1b[?47l", osc8("", &alt), osc8("", "")).as_bytes());
    for i in 0..600 {
        let uri = format!("https://n{i}.example/{}", "p".repeat(1000));
        t.input(format!("{}n{i}{}\r\n", osc8("", &uri), osc8("", "")).as_bytes());
    }
    t.screen_mut().rewrap_scrollback();
    let check = |t: &Terminal| {
        let s = t.screen();
        let mut linked = 0;
        for line in s.scrollback().chain(s.lines()) {
            let text = common::line_text(line);
            for x in 0..line.len() {
                if let Some(l) = line.link(line.cell(x)) {
                    let uri = s.link_uri(l).unwrap();
                    assert!(
                        uri.starts_with(&format!("https://{text}.")),
                        "{text:?} names {uri}"
                    );
                    linked += 1;
                }
            }
        }
        linked
    };
    assert!(check(&t) > 0);
    let oldest = t.screen().scrollback().next().unwrap();
    assert_eq!(
        oldest.link(oldest.cell(0)),
        None,
        "the oldest were stripped"
    );
    t.input(b"\x1b[?47h");
    let s = t.screen();
    assert_eq!(rows(s)[0], "alt");
    assert_eq!(uri(s, 0, 0), Some(alt.as_str()));
}
