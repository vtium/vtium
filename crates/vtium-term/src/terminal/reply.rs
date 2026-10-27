//! The replies: what the terminal answers a program's queries with.
//!
//! Every reply follows the reply rule (docs/architecture.md,
//! "Determinism"): it is built from vtium's state, fixed strings, and
//! values parsed from the request and written again (numbers, a selector
//! from a fixed set, hex), never from the request's bytes or text kept
//! from the stream. Each reply is written through [`State::reply`], which
//! names the [`Query`] it answers; the reply rule's test sends every query
//! of [`Query::ALL`] with hostile text in each field it takes.

use vtium_parse::{Params, StringEnd};

use super::State;
use crate::bounds::VERSION_MAX;
use crate::style::{Attrs, Color, Rgb, UnderlineStyle};

macro_rules! queries {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// Every query vtium answers.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(super) enum Query {
            $($(#[$doc])* $name,)*
        }

        impl Query {
            /// Every query, for the reply rule's test.
            #[cfg(test)]
            pub(super) const ALL: &[Query] = &[$(Query::$name,)*];
        }
    };
}

queries! {
    /// DECRQM, `CSI Ps $ p` and `CSI ? Ps $ p`.
    Decrqm,
    /// DA1, `CSI c`, and DECID, `ESC Z`.
    Da1,
    /// DA2, `CSI > c`.
    Da2,
    /// DA3, `CSI = c`.
    Da3,
    /// DSR 5, `CSI 5 n`.
    Status,
    /// DSR 6 (CPR), `CSI 6 n`.
    CursorPosition,
    /// DECXCPR, `CSI ? 6 n`.
    ExtendedCursorPosition,
    /// XTVERSION, `CSI > q`.
    Version,
    /// XTGETTCAP, `DCS + q Pt ST`.
    Xtgettcap,
    /// DECRQSS, `DCS $ q Pt ST`.
    Decrqss,
    /// OSC 4's query of a palette colour.
    PaletteColor,
    /// OSC 10's and 11's queries of the default colours.
    DefaultColor,
    /// OSC 12's query of the cursor colour.
    CursorColor,
    /// OSC 13's and 14's queries of the pointer's colours.
    PointerColor,
    /// OSC 17's and 19's queries of the selection's colours.
    SelectionColor,
    /// OSC 21's queries, kitty's colour protocol.
    KittyColor,
    /// XTWINOPS 14, 15, 16, 18 and 19: sizes.
    WindowSize,
    /// XTWINOPS 21: the title.
    Title,
    /// XTQMODKEYS, `CSI ? 4 m`.
    ModifyOtherKeys,
    /// Mode 2048's report of the text area, as it is set and on a resize.
    TextArea,
    /// `CSI ? 998 n`, and mode 2033's reports.
    Visibility,
    /// `CSI ? 996 n`, and mode 2031's reports.
    ColorScheme,
    /// XTREPORTCOLORS, `CSI # R`.
    ColorStack,
}

/// XTVERSION's text until the host sets one.
pub(super) const DEFAULT_VERSION: &str = concat!("vtium(", env!("CARGO_PKG_VERSION"), ")");

/// Why [`Terminal::set_version`](crate::Terminal::set_version) refused a
/// text: it goes to the program inside a DCS string, which a control or a
/// byte past ASCII could end or break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionError {
    /// Longer than [`VERSION_MAX`](crate::VERSION_MAX) bytes.
    TooLong,
    /// A byte outside printable ASCII, 0x20 to 0x7E.
    NotPrintable,
}

/// The text, when XTVERSION may send it.
pub(super) fn checked_version(text: &str) -> Result<&str, VersionError> {
    if text.len() > VERSION_MAX {
        Err(VersionError::TooLong)
    } else if !text.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        Err(VersionError::NotPrintable)
    } else {
        Ok(text)
    }
}

/// The OSC terminator a reply takes: BEL for a query ended by BEL, ST
/// otherwise.
pub(super) fn osc_end(end: StringEnd) -> &'static str {
    if end == StringEnd::Bel {
        "\x07"
    } else {
        "\x1b\\"
    }
}

/// An X11 colour specification, `rgb:rrrr/gggg/bbbb`, each channel's byte
/// written twice, as xterm answers.
pub(super) fn xcolor(rgb: Rgb) -> String {
    format!(
        "rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
        rgb.r, rgb.g, rgb.b
    )
}

/// A colour as SGR sets it, for DECRQSS: `base` plus the index for the
/// eight colours, `bright` plus it for the next eight, else `ext` with the
/// colon forms. A `base` or `bright` of 0 is a colour with no short form.
fn sgr_color(out: &mut String, color: Color, base: u8, bright: u8, ext: u8) {
    match color {
        Color::Default => {}
        Color::Indexed(i @ 0..=7) if base != 0 => *out += &format!(";{}", base.saturating_add(i)),
        Color::Indexed(i @ 8..=15) if bright != 0 => {
            *out += &format!(";{}", bright.saturating_add(i).saturating_sub(8));
        }
        Color::Indexed(i) => *out += &format!(";{ext}:5:{i}"),
        Color::Rgb(c) => *out += &format!(";{ext}:2::{}:{}:{}", c.r, c.g, c.b),
    }
}

impl State {
    /// Writes a reply to the program.
    pub(super) fn reply(&mut self, query: Query, bytes: &[u8]) {
        #[cfg(test)]
        self.answered.push(query);
        let _ = query;
        self.out.extend_from_slice(bytes);
    }

    /// DECRQM: the mode table's report for the number asked about.
    pub(super) fn reply_mode(&mut self, params: &Params, dec: bool) {
        let number = params.get(0).unwrap_or(0);
        let value = super::modes::report(self, dec, number);
        let q = if dec { "?" } else { "" };
        let reply = format!("\x1b[{q}{number};{value}$y");
        self.reply(Query::Decrqm, reply.as_bytes());
    }

    /// DA1: a VT220 (62), one of xterm's defined levels, with ANSI colour
    /// (22). Attribute 4 joins it when sixel is built (vtium/vtium#48).
    pub(super) fn reply_da1(&mut self) {
        self.reply(Query::Da1, b"\x1b[?62;22c");
    }

    /// DA2: a VT220, firmware 1, as the C reference answered.
    pub(super) fn reply_da2(&mut self) {
        self.reply(Query::Da2, b"\x1b[>1;1;0c");
    }

    /// DA3, DECRPTUI: a unit id of zeros, as xterm answers.
    pub(super) fn reply_da3(&mut self) {
        self.reply(Query::Da3, b"\x1bP!|00000000\x1b\\");
    }

    /// DSR: 5 reports no malfunction and 6 the cursor's position.
    pub(super) fn reply_dsr(&mut self, params: &Params) {
        match params.get(0) {
            Some(5) => self.reply(Query::Status, b"\x1b[0n"),
            Some(6) => self.reply_cursor(false),
            _ => {}
        }
    }

    /// CPR, or with `extended` DECXCPR, which adds the page (always 1).
    /// The row counts from the region's top in origin mode, and a pending
    /// wrap reports the last column, where the cursor stands.
    pub(super) fn reply_cursor(&mut self, extended: bool) {
        let (x, y) = self.screen.cursor();
        let col = x
            .min(self.screen.cols().saturating_sub(1))
            .saturating_add(1);
        let top = if self.screen.origin_mode() {
            self.screen.margins().0
        } else {
            0
        };
        let row = y.saturating_sub(top).saturating_add(1);
        if extended {
            let reply = format!("\x1b[?{row};{col};1R");
            self.reply(Query::ExtendedCursorPosition, reply.as_bytes());
        } else {
            let reply = format!("\x1b[{row};{col}R");
            self.reply(Query::CursorPosition, reply.as_bytes());
        }
    }

    /// XTVERSION: `DCS > | text ST`, the host's text.
    pub(super) fn reply_version(&mut self) {
        let reply = format!("\x1bP>|{}\x1b\\", self.host.version);
        self.reply(Query::Version, reply.as_bytes());
    }

    /// XTQMODKEYS for modifyOtherKeys: the XTMODKEYS that restores the
    /// level.
    pub(super) fn reply_modify_other_keys(&mut self) {
        let reply = format!("\x1b[>4;{}m", self.modify_other_keys);
        self.reply(Query::ModifyOtherKeys, reply.as_bytes());
    }

    /// XTWINOPS's reports: a size only where vtium has the thing it
    /// measures (ADR-1000, "Replies and policy"), and a fixed empty title.
    /// The window's state, position and pixels (11, 13, 14;2) and the
    /// icon's label (20) get no reply, as vtium has no window or icon.
    pub(super) fn reply_window(&mut self, params: &Params) {
        let (cols, rows) = (self.screen.cols(), self.screen.rows());
        let px = |n: usize, cell: u32| u64::try_from(n).unwrap_or(0).saturating_mul(cell.into());
        let reply = match (
            params.get(0),
            params.get(1),
            self.host.cell,
            self.host.display,
        ) {
            (Some(14), None | Some(0 | 1), Some((w, h)), _) => {
                format!("\x1b[4;{};{}t", px(rows, h), px(cols, w))
            }
            (Some(15), _, _, Some((w, h))) => format!("\x1b[5;{h};{w}t"),
            (Some(16), _, Some((w, h)), _) => format!("\x1b[6;{h};{w}t"),
            (Some(18), _, _, _) => format!("\x1b[8;{rows};{cols}t"),
            (Some(19), _, _, _) => format!("\x1b[9;{rows};{cols}t"),
            (Some(21), _, _, _) => {
                return self.reply(Query::Title, b"\x1b]l\x1b\\");
            }
            _ => return,
        };
        self.reply(Query::WindowSize, reply.as_bytes());
    }

    /// DECRQSS: the setting asked for, `DCS 1 $ r Pt ST`, for SGR, DECSTBM,
    /// DECSCUSR and DECSCL; anything else, and a request longer than any of
    /// these, the error form `DCS 0 $ r ST`, with no echo (ADR-1000).
    pub(super) fn reply_status(&mut self, request: Option<&[u8]>) {
        let setting = match request {
            Some(b"m") => Some(format!("{}m", self.pen_sgr())),
            Some(b"r") => {
                let (top, bottom) = self.screen.margins();
                Some(format!(
                    "{};{}r",
                    top.saturating_add(1),
                    bottom.saturating_add(1)
                ))
            }
            Some(b" q") => Some(format!("{} q", self.cursor_style.param())),
            // Pc 1: the replies stay 7-bit whatever DECSCL asked.
            Some(b"\"p") => Some(format!("{};1\"p", self.conformance)),
            _ => None,
        };
        let reply = match setting {
            Some(s) => format!("\x1bP1$r{s}\x1b\\"),
            None => "\x1bP0$r\x1b\\".into(),
        };
        self.reply(Query::Decrqss, reply.as_bytes());
    }

    /// The pen as the SGR parameters that set it, from 0.
    fn pen_sgr(&self) -> String {
        let pen = self.screen.pen();
        let a = pen.attrs;
        let mut out = String::from("0");
        for (attr, code) in [(Attrs::BOLD, "1"), (Attrs::DIM, "2"), (Attrs::ITALIC, "3")] {
            if a.contains(attr) {
                out += ";";
                out += code;
            }
        }
        out += match a.underline() {
            UnderlineStyle::None => "",
            UnderlineStyle::Single => ";4",
            UnderlineStyle::Double => ";4:2",
            UnderlineStyle::Curly => ";4:3",
            UnderlineStyle::Dotted => ";4:4",
            UnderlineStyle::Dashed => ";4:5",
        };
        for (attr, code) in [
            (Attrs::BLINK, "5"),
            (Attrs::INVERSE, "7"),
            (Attrs::STRIKE, "9"),
        ] {
            if a.contains(attr) {
                out += ";";
                out += code;
            }
        }
        sgr_color(&mut out, pen.fg, 30, 90, 38);
        sgr_color(&mut out, pen.bg, 40, 100, 48);
        sgr_color(&mut out, pen.underline_color, 0, 0, 58);
        out
    }
}

/// A decimal number of at most nine digits, with nothing else.
pub(super) fn decimal(text: &[u8]) -> Option<u32> {
    if text.is_empty() || text.len() > 9 || !text.iter().all(u8::is_ascii_digit) {
        return None;
    }
    core::str::from_utf8(text).ok()?.parse().ok()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking, and its arithmetic is on small constants"
)]
mod tests {
    use super::*;
    use crate::{Termcap, Terminal};

    /// splitmix64, seeded.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        /// Eight random letters.
        fn token(&mut self) -> Vec<u8> {
            (0..8)
                .map(|_| {
                    let n = u8::try_from(self.next() % 52).expect("below 52");
                    if n < 26 { b'a' + n } else { b'A' + n - 26 }
                })
                .collect()
        }

        /// Eight random letters whose hex holds a lowercase digit in every
        /// byte (0x6A to 0x6F, 0x7A), so the uppercase hex of a reply can
        /// hold none of it by chance.
        fn hex_token(&mut self) -> Vec<u8> {
            const LETTERS: &[u8] = b"jklmnoz";
            (0..8)
                .map(|_| {
                    let i = usize::try_from(self.next() % 7).expect("below 7");
                    LETTERS.get(i).copied().unwrap_or(b'j')
                })
                .collect()
        }

        /// A number with leading zeros, which a reply may carry only
        /// re-encoded.
        fn padded(&mut self, n: u32) -> Vec<u8> {
            let zeros = 2 + self.next() % 3;
            let mut out = vec![b'0'; usize::try_from(zeros).expect("small")];
            out.extend(n.to_string().bytes());
            out
        }
    }

    fn lower_hex(bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .flat_map(|b| format!("{b:02x}").into_bytes())
            .collect()
    }

    /// The terminal every reply test starts from.
    fn fixture() -> Terminal {
        let mut t = Terminal::new(10, 5, 10);
        let caps: [(&[u8], Option<&[u8]>); 3] = [
            (b"TN", Some(b"vtium")),
            (b"Co", Some(b"256")),
            (b"am", None),
        ];
        t.set_termcap(Termcap::new(caps).expect("a table"));
        t.set_version("vtium(test)").expect("printable");
        t.set_cell_size(8, 16);
        t.set_display_size(1366, 768);
        t.input(b"\x1b[3;5H\x1b[1;31m");
        t.take_replies();
        t
    }

    /// A query with hostile text in each field it takes: the request, the
    /// text fields, which no reply may hold 4 bytes of, and the numbers,
    /// which a reply may hold only re-encoded.
    struct Probe {
        request: Vec<u8>,
        text: Vec<Vec<u8>>,
        numbers: Vec<Vec<u8>>,
    }

    fn probe(query: Query, rng: &mut Rng) -> Probe {
        let tok = rng.token();
        let cat = |parts: &[&[u8]]| parts.concat();
        let (request, text, numbers) = match query {
            Query::Decrqm => {
                let number = u32::try_from(rng.next() % 70_000).expect("small");
                let n = rng.padded(number);
                (cat(&[b"\x1b[?", &n, b"$p"]), vec![], vec![n])
            }
            Query::Da1 => {
                let n = rng.padded(0);
                (cat(&[b"\x1b[", &n, b"c"]), vec![], vec![n])
            }
            Query::Da2 => {
                let n = rng.padded(0);
                (cat(&[b"\x1b[>", &n, b"c"]), vec![], vec![n])
            }
            // DA3 takes no field but its 0, which its fixed reply holds.
            Query::Da3 => (b"\x1b[=c".to_vec(), vec![], vec![]),
            Query::Status => {
                let n = rng.padded(5);
                (cat(&[b"\x1b[", &n, b"n"]), vec![], vec![n])
            }
            Query::CursorPosition => {
                let n = rng.padded(6);
                (cat(&[b"\x1b[", &n, b"n"]), vec![], vec![n])
            }
            Query::ExtendedCursorPosition => {
                let n = rng.padded(6);
                (cat(&[b"\x1b[?", &n, b"n"]), vec![], vec![n])
            }
            Query::Version => {
                let n = rng.padded(0);
                (cat(&[b"\x1b[>", &n, b"q"]), vec![], vec![n])
            }
            // A known name spelled in lowercase hex, then an unknown one.
            Query::Xtgettcap => {
                let known = b"544e".to_vec();
                let unknown = lower_hex(&rng.hex_token());
                let request = cat(&[b"\x1bP+q", &known, b";", &unknown, b"\x1b\\"]);
                (request, vec![known, unknown], vec![])
            }
            Query::Decrqss => (cat(&[b"\x1bP$q", &tok, b"\x1b\\"]), vec![tok], vec![]),
            Query::PaletteColor => {
                let n = rng.padded(1);
                let request = cat(&[b"\x1b]4;", &n, b";?;", &tok, b";?\x07"]);
                (request, vec![tok], vec![n])
            }
            Query::DefaultColor => {
                let request = cat(&[b"\x1b]10;?;", &tok, b"\x1b\\"]);
                (request, vec![tok], vec![])
            }
            // The number in the reply is the colour's, written again.
            Query::CursorColor => {
                let n = rng.padded(12);
                let request = cat(&[b"\x1b]", &n, b";?;", &tok, b"\x07"]);
                (request, vec![tok], vec![n])
            }
            Query::PointerColor => {
                let n = rng.padded(13);
                let request = cat(&[b"\x1b]", &n, b";", &tok, b";?\x07"]);
                (request, vec![tok], vec![n])
            }
            Query::SelectionColor => {
                let n = rng.padded(17);
                let request = cat(&[b"\x1b]", &n, b";?;", &tok, b";?\x1b\\"]);
                (request, vec![tok], vec![n])
            }
            // A padded index, a known key, an unknown key and a hostile
            // value, each queried.
            Query::KittyColor => {
                let n = rng.padded(7);
                let request = cat(&[
                    b"\x1b]21;",
                    &n,
                    b"=?;cursor=?;",
                    &tok,
                    b"=?;foreground=",
                    &tok,
                    b";background=?\x07",
                ]);
                (request, vec![tok], vec![n])
            }
            Query::WindowSize => {
                let n = rng.padded(18);
                (cat(&[b"\x1b[", &n, b"t"]), vec![], vec![n])
            }
            // A hostile title, then the title report.
            Query::Title => {
                let n = rng.padded(21);
                let request = cat(&[
                    b"\x1b]2;",
                    &tok,
                    b"\x07\x1b]0;",
                    &tok,
                    b"\x1b\\\x1b[",
                    &n,
                    b"t",
                ]);
                (request, vec![tok], vec![n])
            }
            Query::ModifyOtherKeys => {
                let level = rng.padded(2);
                let n = rng.padded(4);
                let request = cat(&[b"\x1b[>4;", &level, b"m\x1b[?", &n, b"m"]);
                (request, vec![], vec![level, n])
            }
            Query::TextArea => {
                let n = rng.padded(2048);
                (cat(&[b"\x1b[?", &n, b"h"]), vec![], vec![n])
            }
            Query::Visibility => {
                let n = rng.padded(998);
                (cat(&[b"\x1b[?", &n, b"n"]), vec![], vec![n])
            }
            Query::ColorScheme => {
                let n = rng.padded(996);
                (cat(&[b"\x1b[?", &n, b"n"]), vec![], vec![n])
            }
            // Pushes and pops by padded numbers, then the report with a
            // padded parameter, which it ignores.
            Query::ColorStack => {
                let entry = rng.padded(7);
                let n = rng.padded(3);
                let request = cat(&[b"\x1b[", &entry, b"#P\x1b[#Q\x1b[", &n, b"#R"]);
                (request, vec![], vec![entry, n])
            }
        };
        Probe {
            request,
            text,
            numbers,
        }
    }

    // docs/architecture.md, "Determinism", the reply rule: every query vtium
    // answers, with a random token in each field it takes, gets a reply
    // that holds neither the token nor any 4 bytes of the injected text,
    // and a number only re-encoded.
    // covers: query.decrqm, query.da1, query.da2, query.da3, query.dsr, query.xtversion,
    // covers: query.xtgettcap, query.decrqss, colour.palette.query, colour.dynamic.query,
    // covers: colour.dynamic.set, colour.pointer, colour.selection, colour.kitty-protocol,
    // covers: window.size-report, window.title-report, input.modify-other-keys, colour.stack
    #[test]
    fn no_reply_carries_the_requests_bytes() {
        for seed in 0..200 {
            let mut rng = Rng(seed);
            for &query in Query::ALL {
                let mut t = fixture();
                t.state.answered.clear();
                let p = probe(query, &mut rng);
                t.input(&p.request);
                assert!(
                    t.state.answered.contains(&query),
                    "seed {seed}: {query:?}'s probe got no {query:?} reply"
                );
                let reply = t.take_replies();
                for text in &p.text {
                    for w in text.windows(4) {
                        assert!(
                            !reply.windows(4).any(|r| r == w),
                            "seed {seed}: {query:?}'s reply {reply:?} holds {w:?} of the request"
                        );
                    }
                }
                for n in &p.numbers {
                    assert!(
                        !reply.windows(n.len()).any(|r| r == n.as_slice()),
                        "seed {seed}: {query:?}'s reply {reply:?} echoes {n:?}"
                    );
                }
            }
        }
    }

    // docs/architecture.md, "Determinism", the reply rule: each reply's
    // exact bytes for the fixture.
    #[test]
    fn each_reply_is_pinned() {
        for &query in Query::ALL {
            let (request, want): (&[u8], &[u8]) = match query {
                Query::Decrqm => (b"\x1b[?25$p", b"\x1b[?25;1$y"),
                Query::Da1 => (b"\x1b[c", b"\x1b[?62;22c"),
                Query::Da2 => (b"\x1b[>c", b"\x1b[>1;1;0c"),
                Query::Da3 => (b"\x1b[=c", b"\x1bP!|00000000\x1b\\"),
                Query::Status => (b"\x1b[5n", b"\x1b[0n"),
                Query::CursorPosition => (b"\x1b[6n", b"\x1b[3;5R"),
                Query::ExtendedCursorPosition => (b"\x1b[?6n", b"\x1b[?3;5;1R"),
                Query::Version => (b"\x1b[>q", b"\x1bP>|vtium(test)\x1b\\"),
                Query::Xtgettcap => (
                    b"\x1bP+q544e;436F;616d\x1b\\",
                    b"\x1bP1+r544E=767469756D;436F=323536;616D\x1b\\",
                ),
                Query::Decrqss => (b"\x1bP$qm\x1b\\", b"\x1bP1$r0;1;31m\x1b\\"),
                Query::PaletteColor => (b"\x1b]4;1;?\x07", b"\x1b]4;1;rgb:cdcd/0000/0000\x07"),
                Query::DefaultColor => (b"\x1b]11;?\x1b\\", b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
                Query::CursorColor => (b"\x1b]12;?\x07", b"\x1b]12;rgb:e5e5/e5e5/e5e5\x07"),
                Query::PointerColor => (b"\x1b]14;?\x07", b"\x1b]14;rgb:0000/0000/0000\x07"),
                Query::SelectionColor => (
                    b"\x1b]17;?;?;?\x07",
                    b"\x1b]17;rgb:e5e5/e5e5/e5e5\x07\x1b]19;rgb:0000/0000/0000\x07",
                ),
                Query::KittyColor => (
                    b"\x1b]21;1=?;cursor=?\x1b\\",
                    b"\x1b]21;1=rgb:cdcd/0000/0000;cursor=\x1b\\",
                ),
                Query::WindowSize => (b"\x1b[14t", b"\x1b[4;80;80t"),
                Query::Title => (b"\x1b[21t", b"\x1b]l\x1b\\"),
                Query::ModifyOtherKeys => (b"\x1b[?4m", b"\x1b[>4;0m"),
                Query::TextArea => (b"\x1b[?2048h", b"\x1b[48;5;10;80;80t"),
                Query::Visibility => (b"\x1b[?998n", b"\x1b[?999;1n"),
                Query::ColorScheme => (b"\x1b[?996n", b"\x1b[?997;1n"),
                Query::ColorStack => (b"\x1b[#P\x1b[3#P\x1b[#R", b"\x1b[?3;3#Q"),
            };
            let mut t = fixture();
            t.input(request);
            assert_eq!(
                t.take_replies(),
                want,
                "{query:?}: {:?}",
                String::from_utf8_lossy(request)
            );
        }
    }
}
