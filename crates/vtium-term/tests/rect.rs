//! Rectangular areas and protection through the byte stream: DECCRA,
//! DECFRA, DECERA, DECSERA, DECCARA, DECRARA and DECSACE, and DECSCA with
//! DECSED and DECSEL.
//!
//! The behaviour is xterm's ctlseqs and DEC STD 070's, as the crate's
//! README states with each difference from xterm. The C reference had no
//! rectangle operations, and honoured a protection nothing set.

mod common;

use common::{rows, term};
use vtium_term::{Attrs, Color, LineFlags, Rgb, Style, Terminal, UnderlineStyle, Width};

fn text(t: &Terminal) -> Vec<String> {
    rows(t.screen())
}

fn style(t: &Terminal, row: usize, col: usize) -> Style {
    let line = t.screen().line(row).unwrap();
    line.style(line.cell(col))
}

fn attrs(t: &Terminal, row: usize, col: usize) -> Attrs {
    style(t, row, col).attrs
}

/// Four rows of six letters, `abcdef` to `stuvwx`, the cursor home.
const FILL: &[u8] = b"abcdef\r\nghijkl\r\nmnopqr\r\nstuvwx\x1b[H";

fn filled() -> Terminal {
    term(6, 4, FILL)
}

fn wrapped(t: &Terminal, row: usize) -> bool {
    t.screen()
        .line(row)
        .unwrap()
        .flags()
        .contains(LineFlags::WRAPPED)
}

// ctlseqs, DECSCA: 1 protects what is printed after it from DECSED and
// DECSEL, 0, 2 and an omitted parameter end it, and any other value
// changes nothing. SGR 0 keeps it; ED, EL and DECSTR do not spare it.
// covers: ctl.protect
#[test]
fn decsca_protects_printed_cells_from_decsed_and_decsel() {
    let mut t = term(
        10,
        2,
        b"ab\x1b[1\"qcd\x1b[0\"qef\x1b[1\"q\x1b[mgh\x1b[2\"qij",
    );
    t.input(b"\x1b[?2K");
    assert_eq!(text(&t), ["  cd  gh", ""]);
    t.input(b"\x1b[?2J");
    assert_eq!(text(&t), ["  cd  gh", ""]);
    t.input(b"\x1b[2J");
    assert_eq!(text(&t), ["", ""]);

    let mut t = term(8, 2, b"\x1b[1\"qab\x1b[3\"qcd\x1b[\"qef\x1b[H\x1b[?K");
    assert_eq!(text(&t), ["abcd", ""]);
    t.input(b"\x1b[H\x1b[?1K");
    assert_eq!(text(&t), ["abcd", ""]);
    t.input(b"\x1b[K");
    assert_eq!(text(&t), ["", ""]);

    // DECSTR ends it.
    let mut t = term(8, 1, b"\x1b[1\"q\x1b[!pab\x1b[?2K");
    assert_eq!(text(&t), [""]);
    // A protected wide character stays whole.
    t.input("\x1b[H\x1b[1\"q\u{4E2D}\x1b[0\"qxy\x1b[?2K".as_bytes());
    assert_eq!(text(&t), ["\u{4E2D}"]);
}

// ctlseqs, DECERA: every cell of the rectangle is erased, protected or
// not, as ED erases, in the pen's background. 0 or an omitted parameter
// takes the default, the screen's edge; a parameter past the edge is the
// edge; an inverted rectangle does nothing.
// covers: ctl.rect
#[test]
fn decera_erases_a_rectangle_clipped_to_the_screen() {
    let mut t = filled();
    t.input(b"\x1b[2;2;3;4$z");
    assert_eq!(text(&t), ["abcdef", "g   kl", "m   qr", "stuvwx"]);
    t.input(b"\x1b[3;5;99;99$z");
    assert_eq!(text(&t), ["abcdef", "g   kl", "m", "stuv"]);
    t.input(b"\x1b[3;1;2;6$z\x1b[1;4;1;3$z");
    assert_eq!(text(&t), ["abcdef", "g   kl", "m", "stuv"]);
    t.input(b"\x1b[0;0;1;0$z");
    assert_eq!(text(&t), ["", "g   kl", "m", "stuv"]);
    t.input(b"\x1b[$z");
    assert_eq!(text(&t), ["", "", "", ""]);

    let t = term(6, 2, b"\x1b[1\"qabcdef\x1b[0\"q\x1b[44m\x1b[1;1;1;2$z");
    assert_eq!(text(&t), ["  cdef", ""]);
    let blank = Style {
        bg: Color::Indexed(4),
        ..Style::DEFAULT
    };
    assert_eq!(style(&t, 0, 0), blank);
    assert_eq!(t.screen().cursor(), (6, 0));
}

// ctlseqs, DECSERA: the rectangle's cells DECSCA did not protect.
// covers: ctl.protect
// covers: ctl.rect
#[test]
fn decsera_leaves_protected_cells() {
    let mut t = term(
        6,
        3,
        b"ab\x1b[1\"qcd\x1b[0\"qef\r\ngh\x1b[1\"qij\x1b[0\"qkl",
    );
    t.input(b"\x1b[1;2;2;5${");
    assert_eq!(text(&t), ["a cd f", "g ij l", ""]);
    t.input(b"\x1b[${");
    assert_eq!(text(&t), ["  cd", "  ij", ""]);
}

// DEC STD 070: in origin mode a rectangle's rows count from the top
// margin and stop at the bottom one.
// covers: ctl.rect
#[test]
fn a_rectangle_in_origin_mode_keeps_to_the_margins() {
    let mut t = filled();
    t.input(b"\x1b[2;3r\x1b[?6h\x1b[1;2;9;3$z");
    assert_eq!(text(&t), ["abcdef", "g  jkl", "m  pqr", "stuvwx"]);
    t.input(b"\x1b[120;2;1;5;9$x");
    assert_eq!(text(&t), ["abcdef", "g  jkl", "xxxxxx", "stuvwx"]);
    // The copy's source and target count from the margin, and the target
    // stops at it.
    t.input(b"\x1b[1;4;2;5;1;2;1;1$v");
    assert_eq!(text(&t), ["abcdef", "g  jkl", "jkxxxx", "stuvwx"]);
    t.input(b"\x1b[2*x\x1b[1;1;9;1;1$r");
    assert!(!attrs(&t, 0, 0).contains(Attrs::BOLD));
    assert!(attrs(&t, 1, 0).contains(Attrs::BOLD));
    assert!(attrs(&t, 2, 0).contains(Attrs::BOLD));
    assert!(!attrs(&t, 3, 0).contains(Attrs::BOLD));
    t.input(b"\x1b[?6l\x1b[1;1;1;1;1$r");
    assert!(attrs(&t, 0, 0).contains(Attrs::BOLD));
}

// ctlseqs, DECFRA: the character, in the pen, through the character set
// in GL; a character that is not one printable cell does nothing, and
// neither does the sequence with no parameter.
// covers: ctl.rect
#[test]
fn decfra_fills_a_rectangle_with_a_character() {
    let mut t = filled();
    t.input(b"\x1b[1;31m\x1b[120;2;2;3;4$x");
    assert_eq!(text(&t), ["abcdef", "gxxxkl", "mxxxqr", "stuvwx"]);
    let pen = Style {
        fg: Color::Indexed(1),
        attrs: Attrs::BOLD,
        ..Style::DEFAULT
    };
    assert_eq!(style(&t, 1, 1), pen);
    assert_eq!(style(&t, 1, 0), Style::DEFAULT);
    t.input(b"\x1b[m\x1b[121;4;5;99;99$x");
    assert_eq!(text(&t), ["abcdef", "gxxxkl", "mxxxqr", "stuvyy"]);
    for refused in [
        &b"\x1b[19968;1;1;1;6$x"[..],
        b"\x1b[7$x",
        b"\x1b[127$x",
        b"\x1b[150$x",
    ] {
        t.input(refused);
        assert_eq!(text(&t), ["abcdef", "gxxxkl", "mxxxqr", "stuvyy"]);
    }
    t.input(b"\x1b[$x");
    assert_eq!(text(&t), ["abcdef", "gxxxkl", "mxxxqr", "stuvyy"]);
    t.input(b"\x1b[;1;1;1;2$x\x1b[0;2;1;2;2$x");
    assert_eq!(text(&t), ["  cdef", "  xxkl", "mxxxqr", "stuvyy"]);
    t.input(b"\x1b(0\x1b[113;3;1;3;3$x\x1b(B\x1b[233;4;1;4;1$x");
    assert_eq!(
        text(&t),
        [
            "  cdef",
            "  xxkl",
            "\u{2500}\u{2500}\u{2500}xqr",
            "\u{E9}tuvyy"
        ]
    );
    t.input("\x1b[19990;1;1;1;1$x".as_bytes());
    assert_eq!(text(&t)[0], "  cdef");
    t.input("\x1b[945;1;1;1;1$x".as_bytes());
    assert_eq!(text(&t)[0], "\u{3B1} cdef");
}

// ctlseqs, DECCRA: the source rectangle copied to the target's top left,
// with each cell's style, as if read whole first; the target is clipped to
// the screen; both page numbers name vtium's one page.
// covers: ctl.rect
#[test]
fn deccra_copies_a_rectangle_whole() {
    let mut t = filled();
    t.input(b"\x1b[1;1;2;2;1;3;4;1$v");
    assert_eq!(text(&t), ["abcdef", "ghijkl", "mnoabr", "stughx"]);

    // Overlapping, down and right, then up and left, then in one row.
    let mut t = filled();
    t.input(b"\x1b[1;1;2;3;1;2;2;1$v");
    assert_eq!(text(&t), ["abcdef", "gabckl", "mghiqr", "stuvwx"]);
    let mut t = filled();
    t.input(b"\x1b[2;2;3;4;1;1;1;1$v");
    assert_eq!(text(&t), ["hijdef", "nopjkl", "mnopqr", "stuvwx"]);
    let mut t = filled();
    t.input(b"\x1b[1;1;1;4;1;1;3;1$v");
    assert_eq!(text(&t)[0], "ababcd");
    let mut t = filled();
    t.input(b"\x1b[1;3;1;6;1;1;1;1$v");
    assert_eq!(text(&t)[0], "cdefef");

    // Clipped at the screen, and the page numbers changing nothing.
    let mut t = filled();
    t.input(b"\x1b[1;1;2;6;2;3;4;9$v");
    assert_eq!(text(&t), ["abcdef", "ghijkl", "mnoabc", "stughi"]);
    let mut t = filled();
    t.input(b"\x1b[1;1;9;9;1;3;4$v");
    assert_eq!(text(&t), ["abcdef", "ghijkl", "mnoabc", "stughi"]);
    // An inverted source does nothing.
    t.input(b"\x1b[2;1;1;6;1;1;1;1$v");
    assert_eq!(text(&t), ["abcdef", "ghijkl", "mnoabc", "stughi"]);

    // Styles and clusters go with the cells.
    let mut t = term(6, 2, "\x1b[31ma\x1b[me\u{301}".as_bytes());
    t.input(b"\x1b[1;1;1;2;1;2;3;1$v");
    assert_eq!(text(&t), ["ae\u{301}", "  ae\u{301}"]);
    assert_eq!(style(&t, 1, 2).fg, Color::Indexed(1));
    assert_eq!(style(&t, 1, 3), Style::DEFAULT);
}

// A wide character cut by a rectangle's edge goes whole: the half inside a
// copy's source arrives as a blank, and a character the target, an erase
// or a fill cuts is erased whole.
// covers: ctl.rect
#[test]
fn a_wide_character_cut_at_a_rectangle_edge_goes_whole() {
    let wide = "\u{4E2D}\u{6587}ab\r\nghijkl".as_bytes();
    let mut t = term(6, 2, wide);
    t.input(b"\x1b[1;2;1;3;1;2;1;1$v");
    assert_eq!(text(&t), ["\u{4E2D}\u{6587}ab", "  ijkl"]);
    let mut t = term(6, 2, wide);
    t.input(b"\x1b[2;5;2;6;1;1;2;1$v");
    assert_eq!(text(&t), [" kl ab", "ghijkl"]);
    let mut t = term(6, 2, wide);
    t.input(b"\x1b[2;1;2;1;1;1;2;1$v");
    assert_eq!(text(&t), [" g\u{6587}ab", "ghijkl"]);
    let line = t.screen().line(0).unwrap();
    assert_eq!(line.cell(2).width(), Width::Wide);
    assert_eq!(line.cell(3).width(), Width::WideTail);

    let mut t = term(6, 2, wide);
    t.input(b"\x1b[1;2;1;2$z");
    assert_eq!(text(&t)[0], "  \u{6587}ab");
    let mut t = term(6, 2, wide);
    t.input(b"\x1b[120;1;3;1;3$x");
    assert_eq!(text(&t)[0], "\u{4E2D}x ab");
}

// ctlseqs, DECSACE and DECCARA: 2 selects the rectangle; 0, 1 and an
// omitted parameter the stream from the first position to the last,
// wrapping at the screen's edge. The stream changes only cells where a
// character was printed; the rectangle changes every cell.
// covers: ctl.rect
#[test]
fn deccara_sets_attributes_in_a_rectangle_or_a_stream() {
    let fill = b"abcdef\r\ngh\r\nmnopqr";
    let mut t = term(6, 3, fill);
    t.input(b"\x1b[2*x\x1b[1;2;3;3;1;4$r");
    let bold_underlined =
        |a: Attrs| a.contains(Attrs::BOLD) && a.underline() == UnderlineStyle::Single;
    for (row, col, set) in [
        (0, 0, false),
        (0, 1, true),
        (0, 2, true),
        (0, 3, false),
        (1, 1, true),
        (1, 2, true),
        (1, 3, false),
        (2, 2, true),
        (2, 3, false),
    ] {
        assert_eq!(bold_underlined(attrs(&t, row, col)), set, "{row},{col}");
    }

    for decsace in [&b"\x1b[*x"[..], b"\x1b[0*x", b"\x1b[1*x", b"\x1b[3*x"] {
        let mut t = term(6, 3, fill);
        t.input(b"\x1b[2*x");
        t.input(decsace);
        t.input(b"\x1b[1;2;3;3;1;4$r");
        let expected = [
            (0, 0, false),
            (0, 1, true),
            (0, 5, true),
            (1, 0, true),
            (1, 1, true),
            // Never printed.
            (1, 2, false),
            (1, 5, false),
            (2, 0, true),
            (2, 2, true),
            (2, 3, false),
        ];
        let tail: Vec<_> = expected
            .into_iter()
            .filter(|&(row, col, set)| bold_underlined(attrs(&t, row, col)) != set)
            .collect();
        assert!(tail.is_empty(), "{decsace:?}: {tail:?}");
    }

    // The resets, and 0 for every attribute.
    let mut t = term(6, 1, b"\x1b[1;4;5;7;3mabcde\x1b[m");
    t.input(b"\x1b[1;1;1;1;22$r\x1b[1;2;1;2;24$r\x1b[1;3;1;3;25$r");
    t.input(b"\x1b[1;4;1;4;27$r\x1b[1;5;1;5;0$r");
    let all = |a: Attrs| {
        [Attrs::BOLD, Attrs::BLINK, Attrs::INVERSE, Attrs::ITALIC]
            .map(|f| a.contains(f))
            .into_iter()
            .chain([a.underline() == UnderlineStyle::Single])
            .collect::<Vec<_>>()
    };
    assert_eq!(all(attrs(&t, 0, 0)), [false, true, true, true, true]);
    assert_eq!(all(attrs(&t, 0, 1)), [true, true, true, true, false]);
    assert_eq!(all(attrs(&t, 0, 2)), [true, false, true, true, true]);
    assert_eq!(all(attrs(&t, 0, 3)), [true, true, false, true, true]);
    assert_eq!(all(attrs(&t, 0, 4)), [false; 5]);
}

// kitty's DECCARA extension: every SGR parameter, colors and underline
// styles in both forms included, as SGR applies it to the pen; SGR 0 keeps
// DECSCA's protection. The rectangle's own parameters are not attributes,
// and with no attribute nothing changes.
// covers: ctl.rect
#[test]
fn deccara_takes_every_sgr_attribute() {
    let mut t = term(6, 2, b"abcdef\r\nghijkl");
    t.input(b"\x1b[2*x\x1b[4;1;4;1;7$r");
    assert_eq!(attrs(&t, 1, 0), Attrs::INVERSE);
    t.input(b"\x1b[1;1;2;2;3;44;4:3;38:2::1:2:3;58;5;9$r");
    let s = style(&t, 1, 1);
    assert!(s.attrs.contains(Attrs::ITALIC));
    assert_eq!(s.attrs.underline(), UnderlineStyle::Curly);
    assert_eq!(s.bg, Color::Indexed(4));
    assert_eq!(s.fg, Color::Rgb(Rgb::new(1, 2, 3)));
    assert_eq!(s.underline_color, Color::Indexed(9));
    assert_eq!(style(&t, 0, 2), Style::DEFAULT);
    t.input(b"\x1b[1;1;2;2$r");
    assert_eq!(style(&t, 1, 1), s);
    // An empty attribute is SGR's 0.
    t.input(b"\x1b[1;1;1;1;$r");
    assert_eq!(style(&t, 0, 0), Style::DEFAULT);

    // A rectangle's own parameter with sub-parameters is not one.
    t.input(b"\x1b[1:1;1;2;2;1$r");
    assert!(!attrs(&t, 0, 0).contains(Attrs::BOLD));

    let mut t = term(6, 1, b"\x1b[1\"qab\x1b[0\"q");
    t.input(b"\x1b[1;1;1;6;0;1$r\x1b[1;1;1;6${");
    assert_eq!(text(&t), ["ab"]);
    assert!(attrs(&t, 0, 0).contains(Attrs::BOLD));

    // A wide character the rectangle cuts changes whole.
    let mut t = term(6, 1, "a\u{4E2D}b".as_bytes());
    t.input(b"\x1b[2*x\x1b[1;3;1;4;7$r");
    for col in [1, 2, 3] {
        assert!(attrs(&t, 0, col).contains(Attrs::INVERSE), "{col}");
    }
    assert!(!attrs(&t, 0, 0).contains(Attrs::INVERSE));
}

// ctlseqs, DECRARA: 1, 4, 5, 7 and 8 reverse bold, underline, blink,
// inverse and invisible; 0 reverses the first four; others change
// nothing. DECSACE applies as for DECCARA.
// covers: ctl.rect
#[test]
fn decrara_reverses_attributes() {
    let mut t = term(6, 2, b"\x1b[1;4mab\x1b[mcd\r\ne");
    t.input(b"\x1b[2*x\x1b[1;1;2;3;1;4;5;7;8;3;9$t");
    let a = attrs(&t, 0, 0);
    assert!(!a.contains(Attrs::BOLD));
    assert_eq!(a.underline(), UnderlineStyle::None);
    assert!(a.contains(Attrs::BLINK) && a.contains(Attrs::INVERSE) && a.contains(Attrs::CONCEAL));
    assert!(!a.contains(Attrs::ITALIC) && !a.contains(Attrs::STRIKE));
    let c = attrs(&t, 0, 2);
    assert!(c.contains(Attrs::BOLD) && c.underline() == UnderlineStyle::Single);
    assert!(attrs(&t, 1, 2).contains(Attrs::BOLD));
    assert!(!attrs(&t, 0, 3).contains(Attrs::BOLD));

    t.input(b"\x1b[1;1;1;1;0$t");
    let a = attrs(&t, 0, 0);
    assert!(a.contains(Attrs::BOLD) && a.underline() == UnderlineStyle::Single);
    assert!(!a.contains(Attrs::BLINK) && !a.contains(Attrs::INVERSE));
    assert!(a.contains(Attrs::CONCEAL));

    // In the stream, cells never printed stay. As in xterm, a stream whose
    // first column is right of its last does nothing.
    let mut t = term(6, 2, b"abcdef\r\ne");
    t.input(b"\x1b[1;5;2;3;7$t");
    assert!(!attrs(&t, 0, 5).contains(Attrs::INVERSE));
    t.input(b"\x1b[1;3;2;3;7$t");
    assert!(attrs(&t, 0, 5).contains(Attrs::INVERSE));
    assert!(attrs(&t, 1, 0).contains(Attrs::INVERSE));
    assert!(!attrs(&t, 1, 1).contains(Attrs::INVERSE));
    assert!(!attrs(&t, 0, 1).contains(Attrs::INVERSE));
}

// A rectangle operation that changes a row's last column ends the row's
// wrap, as EL 0 does; one that changes only attributes, or not the last
// column, keeps it. One that replaces the main screen's first row whole
// ends the wrap of the scrollback's newest line.
// covers: ctl.rect
#[test]
fn a_rectangle_over_the_last_column_ends_the_wrap() {
    let wrapping = b"abcdefgh";
    for op in [
        &b"\x1b[1;1;1;3$z"[..],
        b"\x1b[1;1;1;4;1$r",
        b"\x1b[1;1;1;4;1$t",
    ] {
        let t = term(4, 3, &[&wrapping[..], op].concat());
        assert!(wrapped(&t, 0), "{op:?}");
    }
    for op in [
        &b"\x1b[1;4;1;4$z"[..],
        b"\x1b[1;4;1;4${",
        b"\x1b[120;1;4;1;4$x",
        b"\x1b[2;1;2;1;1;1;4;1$v",
    ] {
        let t = term(4, 3, &[&wrapping[..], op].concat());
        assert!(!wrapped(&t, 0), "{op:?}");
    }

    let mut t = term(4, 2, b"\x1b[H");
    t.screen_mut().set_scrollback_max(10);
    t.input(b"abcdefghijkl");
    let newest = |t: &Terminal| {
        t.screen()
            .scrollback()
            .last()
            .unwrap()
            .flags()
            .contains(LineFlags::WRAPPED)
    };
    assert!(newest(&t));
    t.input(b"\x1b[1;2;1;4$z");
    assert!(newest(&t));
    t.input(b"\x1b[120;1;1;1;4$x");
    assert!(!newest(&t));
}

// Damage marks the cells a rectangle operation changed and no others.
// covers: ctl.rect
#[test]
fn a_rectangle_marks_only_its_cells() {
    let ops: [&[u8]; 6] = [
        b"\x1b[2;2;3;3$z",
        b"\x1b[2;2;3;3${",
        b"\x1b[120;2;2;3;3$x",
        b"\x1b[1;5;2;6;1;2;2;1$v",
        b"\x1b[2*x\x1b[2;2;3;3;1$r",
        b"\x1b[2*x\x1b[2;2;3;3;1$t",
    ];
    for op in ops {
        let mut t = filled();
        let since = t.screen_mut().checkpoint();
        t.input(op);
        let damage = t.screen().damage();
        for row in 0..4 {
            for col in 0..6 {
                let inside = (1..3).contains(&row) && (1..3).contains(&col);
                assert_eq!(
                    damage.changed(row, col, since),
                    inside,
                    "{op:?} {row},{col}"
                );
            }
        }
    }
}

// DECCRA gives back the target's cluster bytes before it writes, so a
// copy whose clusters fit the line's budget keeps them whole, even where
// the cluster it replaces comes after the one it writes.
// covers: ctl.rect
#[test]
fn a_copy_keeps_clusters_that_fit_the_line() {
    let [a, b, c] = ["a", "b", "c"].map(|base| format!("{base}{}", "\u{301}".repeat(5)));
    let mut t = term(4, 2, format!("{c}Z\r\nX{a}{b}Y").as_bytes());
    assert_eq!(text(&t), [format!("{c}Z"), format!("X{a}{b}Y")]);
    t.input(b"\x1b[1;1;1;2;1;2;1;1$v");
    assert_eq!(text(&t)[1], format!("{c}Z{b}Y"));
}
