//! OSC 66, kitty's text sizing protocol: `OSC 66 ; metadata ; text`.
//!
//! The metadata is a list of `key=value` pairs separated by colons, and
//! either part of the sequence may be empty. vtium reads the six keys the
//! protocol defines: the scale `s`, the width `w`, the fraction `n` over
//! `d`, and the alignments `v` and `h`. A key vtium does not know is
//! ignored, so that a program written for a later version of the protocol
//! still draws. A known key whose value is not a whole number inside its
//! range discards the whole sequence, text included, and so does a
//! fraction whose numerator is not below its denominator. Text over 4096
//! bytes discards the sequence too. vtium decodes the text as UTF-8, with
//! U+FFFD for each ill-formed part, and drops each control character in it.
//! The screen then writes the text at that size (`Screen::print_sized`).
//! OSC 66 has no reply.
//!
//! References:
//! - ADR-1308 D2: the keys, their ranges, and what vtium discards.

use super::State;
use crate::line::TextSize;

/// The most bytes of text one OSC 66 sequence carries, as kitty's text
/// sizing protocol limits it.
const TEXT_BYTES_MAX: usize = 4096;

impl State {
    /// OSC 66 with `args`, the bytes after `66;`: writes its text at the
    /// size its metadata gives, or discards the sequence.
    pub(super) fn text_size(&mut self, args: &[u8]) {
        if let Some((size, text)) = parse(args) {
            self.screen.print_sized(&text, size);
        }
    }
}

/// The size and the text of an OSC 66 sequence, from the bytes after
/// `66;`, or `None` when vtium discards the sequence.
fn parse(args: &[u8]) -> Option<(TextSize, String)> {
    let split = args.iter().position(|&b| b == b';')?;
    let (metadata, text) = (args.get(..split)?, args.get(split.saturating_add(1)..)?);
    if text.len() > TEXT_BYTES_MAX {
        return None;
    }
    let mut size = TextSize::NORMAL;
    for pair in metadata.split(|&b| b == b':') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.iter().position(|&b| b == b'=') {
            Some(at) => (pair.get(..at)?, pair.get(at.saturating_add(1)..)?),
            None => (pair, &[][..]),
        };
        let (slot, low, high) = match key {
            b"s" => (&mut size.scale, 1, 7),
            b"w" => (&mut size.width, 0, 7),
            b"n" => (&mut size.numerator, 0, 15),
            b"d" => (&mut size.denominator, 0, 15),
            b"v" => (&mut size.vertical, 0, 2),
            b"h" => (&mut size.horizontal, 0, 2),
            _ => continue,
        };
        *slot = number(value).filter(|v| (low..=high).contains(v))?;
    }
    if size.denominator != 0 && size.numerator >= size.denominator {
        return None;
    }
    let text = String::from_utf8_lossy(text)
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    Some((size, text))
}

/// `value` as a whole number of at most 255, written in decimal digits and
/// nothing else.
fn number(value: &[u8]) -> Option<u8> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    core::str::from_utf8(value).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(s: u8, w: u8, n: u8, d: u8, v: u8, h: u8) -> TextSize {
        TextSize {
            scale: s,
            width: w,
            numerator: n,
            denominator: d,
            vertical: v,
            horizontal: h,
        }
    }

    #[test]
    fn each_key_is_read_at_both_ends_of_its_range() {
        let cases: [(&[u8], TextSize); 8] = [
            (b";x", TextSize::NORMAL),
            (b"s=1;x", size(1, 0, 0, 0, 0, 0)),
            (b"s=7:w=7;x", size(7, 7, 0, 0, 0, 0)),
            (b"w=0;x", size(1, 0, 0, 0, 0, 0)),
            (b"n=0:d=15;x", size(1, 0, 0, 15, 0, 0)),
            (b"n=14:d=15;x", size(1, 0, 14, 15, 0, 0)),
            (b"v=2:h=2:d=2:n=1;x", size(1, 0, 1, 2, 2, 2)),
            (b"v=0:h=0;x", size(1, 0, 0, 0, 0, 0)),
        ];
        for (args, want) in cases {
            assert_eq!(parse(args).map(|(s, _)| s), Some(want), "{args:?}");
        }
    }

    #[test]
    fn a_value_out_of_range_or_not_a_number_discards_the_sequence() {
        for args in [
            &b"s=0;x"[..],
            b"s=8;x",
            b"w=8;x",
            b"n=16;x",
            b"d=16;x",
            b"v=3;x",
            b"h=3;x",
            b"s=;x",
            b"s;x",
            b"s=-1;x",
            b"s=+2;x",
            b"s=2.0;x",
            b"s=999999999999;x",
            b"n=2:d=2;x",
            b"n=3:d=2;x",
        ] {
            assert_eq!(parse(args), None, "{args:?}");
        }
    }

    #[test]
    fn an_unknown_key_and_empty_pairs_are_ignored() {
        let (size, text) = parse(b"::q=9:s=2:zz:;hi").unwrap_or((TextSize::NORMAL, String::new()));
        assert_eq!(size.scale, 2);
        assert_eq!(text, "hi");
    }

    #[test]
    fn the_text_drops_controls_and_replaces_ill_formed_bytes() {
        let parsed = parse(b"s=2;a\x07b\xffc\xc2\x85d");
        assert_eq!(parsed.map(|(_, t)| t), Some("ab\u{FFFD}cd".to_owned()));
    }

    #[test]
    fn text_over_4096_bytes_discards_the_sequence() {
        let mut args = b"s=2;".to_vec();
        args.extend(core::iter::repeat_n(b'a', TEXT_BYTES_MAX));
        assert!(parse(&args).is_some());
        args.push(b'a');
        assert_eq!(parse(&args), None);
    }

    #[test]
    fn a_sequence_with_no_second_separator_is_discarded() {
        assert_eq!(parse(b"s=2"), None);
    }

    /// A terminal `cols` × `rows` after `bytes`.
    fn term(cols: usize, rows: usize, bytes: &[u8]) -> crate::Terminal {
        let mut t = crate::Terminal::new(cols, rows, 100);
        t.input(bytes);
        t
    }

    /// Row `y` of the screen as text: a cell's text, or a space for an
    /// empty cell, with the trailing spaces trimmed.
    fn row(t: &crate::Terminal, y: usize) -> String {
        let Some(line) = t.screen().line(y) else {
            return String::new();
        };
        let text: String = line
            .cells()
            .iter()
            .map(|&c| match line.glyph(c) {
                crate::Glyph::Empty => " ".to_owned(),
                crate::Glyph::Char(ch) => ch.to_string(),
                crate::Glyph::Cluster(s) => s.to_owned(),
            })
            .collect();
        text.trim_end().to_owned()
    }

    // covers: text.kitty-sizing
    #[test]
    fn the_frame_carries_each_runs_place_scale_width_fraction_and_alignment() {
        let t = term(10, 3, b"x\x1b]66;s=2:w=3:n=1:d=2:v=1:h=2;Hi\x1b\\");
        let palette = crate::Palette::default();
        let frame = t.screen().frame(&palette, crate::Age::NEVER);
        let want = TextSize {
            scale: 2,
            width: 3,
            numerator: 1,
            denominator: 2,
            vertical: 1,
            horizontal: 2,
        };
        for y in 0..2 {
            let runs = frame.sized_runs(y);
            assert_eq!(runs.len(), 1, "row {y}");
            let run = runs.first().copied();
            assert_eq!(
                run.map(|r| (r.col(), r.cols(), r.row(), r.rows())),
                Some((1, 6, y, 2))
            );
            assert_eq!(run.map(|r| r.size()), Some(want));
            let line = frame.line(y);
            let marked: Vec<bool> = (0..8)
                .map(|x| line.is_some_and(|l| l.cell(x).in_run()))
                .collect();
            assert_eq!(
                marked,
                [false, true, true, true, true, true, true, false],
                "row {y}"
            );
            assert_eq!(row(&t, y), if y == 0 { "xHi" } else { " Hi" });
        }
        assert!(frame.sized_runs(2).is_empty());
        assert_eq!(t.screen().cursor(), (7, 0));
    }

    // covers: text.kitty-sizing
    #[test]
    fn rep_after_a_run_repeats_nothing() {
        let t = term(10, 3, b"a\x1b]66;s=2;B\x1b\\\x1b[3b");
        assert_eq!(row(&t, 0), "aB");
        assert_eq!(t.screen().cursor(), (3, 0));
    }

    // covers: text.kitty-sizing
    #[test]
    fn a_selective_erase_spares_a_protected_run_and_erases_another_whole() {
        // A protected run at columns 0 and 1, then an unprotected one at 2
        // and 3; DECSED from the lower row of the second erases it whole.
        let t = term(
            10,
            3,
            b"\x1b[1\"q\x1b]66;s=2;P\x1b\\\x1b[0\"q\x1b]66;s=2;U\x1b\\\x1b[2;4H\x1b[?J",
        );
        assert_eq!((row(&t, 0), row(&t, 1)), ("P".to_owned(), "P".to_owned()));
    }

    // covers: text.kitty-sizing
    #[test]
    fn decic_moves_a_tall_run_whole_and_ich_on_one_of_its_rows_erases_it() {
        let mut t = term(10, 3, b"\x1b]66;s=2;T\x1b\\\x1b[1;1H\x1b['}");
        assert_eq!((row(&t, 0), row(&t, 1)), (" T".to_owned(), " T".to_owned()));
        t.input(b"\x1b[2;1H\x1b[@");
        assert_eq!((row(&t, 0), row(&t, 1)), (String::new(), String::new()));
    }

    // covers: text.kitty-sizing
    #[test]
    fn a_scroll_inside_the_margins_moves_a_run_inside_them_and_erases_one_a_margin_cuts() {
        // DECLRMM on, margins at columns 3 to 6, then two runs of s=2 on
        // rows 2 and 3: I inside the margins and C across the left one.
        let mut t = term(
            10,
            4,
            b"\x1b[?69h\x1b[3;7s\x1b[3;4H\x1b]66;s=2;I\x1b\\\x1b[3;2H\x1b]66;s=2;C\x1b\\",
        );
        assert_eq!(row(&t, 2), " C I");
        t.input(b"\x1b[S");
        assert_eq!(
            (0..4).map(|y| row(&t, y)).collect::<Vec<_>>(),
            ["", "   I", "   I", ""]
        );
    }

    // covers: text.kitty-sizing
    #[test]
    fn a_run_whose_rows_scroll_off_with_no_scrollback_goes_whole() {
        // With no scrollback, the line feed that drops T's top row deletes
        // it, so T goes whole and its lower row is erased.
        let t = term(6, 3, b"\x1b[2;1H\x1b]66;s=2;T\x1b\\\x1b[3;1H\n\n");
        assert_eq!((0..3).map(|y| row(&t, y)).collect::<Vec<_>>(), ["", "", ""]);
    }

    // covers: text.kitty-sizing
    #[test]
    fn a_run_whose_top_row_scrolled_into_the_scrollback_keeps_it_there() {
        // T of s=2 on the last two rows, then two line feeds scroll its top
        // row into the scrollback; ED 2 then erases the row left on the
        // screen, and the scrollback's row keeps T.
        let mut t = crate::Terminal::new(6, 3, 100);
        t.screen_mut().set_scrollback_max(100);
        t.input(b"\x1b[2;1H\x1b]66;s=2;T\x1b\\\x1b[3;1H\n\n");
        assert_eq!(t.screen().scrollback().len(), 2);
        assert_eq!(row(&t, 0), "T");
        t.input(b"\x1b[2J");
        assert_eq!(row(&t, 0), "");
        let newest = t.screen().scrollback().last();
        assert_eq!(newest.map(|l| l.runs().len()), Some(1));
        assert_eq!(
            newest.and_then(|l| l.runs().first()).map(|r| r.row()),
            Some(0)
        );
    }
}
