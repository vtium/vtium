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

/// The fewest color registers XTSMGRAPHICS may set, as vtium-image's
/// decoder holds it.
#[cfg(feature = "images")]
const REGISTERS_MIN: u32 = 2;
use crate::screen::Extent;
use crate::style::{Attrs, Color, Rgb, UnderlineStyle};

macro_rules! queries {
    (
        $($(#[$doc:meta])* $name:ident,)*
        ; images: $($(#[$idoc:meta])* $iname:ident,)*
    ) => {
        /// Every query vtium answers.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(super) enum Query {
            $($(#[$doc])* $name,)*
            $(#[cfg(feature = "images")] $(#[$idoc])* $iname,)*
        }

        impl Query {
            /// Every query, for the reply rule's test.
            #[cfg(test)]
            pub(super) const ALL: &[Query] = &[
                $(Query::$name,)*
                $(#[cfg(feature = "images")] Query::$iname,)*
            ];
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
    /// OSC 4's query of a palette color.
    PaletteColor,
    /// OSC 10's and 11's queries of the default colors.
    DefaultColor,
    /// OSC 12's query of the cursor color.
    CursorColor,
    /// OSC 13's and 14's queries of the pointer's colors.
    PointerColor,
    /// OSC 17's and 19's queries of the selection's colors.
    SelectionColor,
    /// OSC 21's queries, kitty's color protocol.
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
    /// OSC 50's query of the font set in use.
    Font,
    /// XTSMGRAPHICS, `CSI ? Pi ; Pa ; Pv S`: the sixel graphics settings,
    /// answered in every build.
    Xtsmgraphics,
    ; images:
    /// The image protocols' replies (ADR-1100, D6): each `ReplyKind` a
    /// handler writes, in a build with images.
    ImageGraphics,
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

/// An X11 color specification, `rgb:rrrr/gggg/bbbb`, each channel's byte
/// written twice, as xterm answers.
pub(super) fn xcolor(rgb: Rgb) -> String {
    format!(
        "rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
        rgb.r, rgb.g, rgb.b
    )
}

/// A color as SGR sets it, for DECRQSS: `base` plus the index for the
/// eight colors, `bright` plus it for the next eight, else `ext` with the
/// colon forms. A `base` or `bright` of 0 is a color with no short form.
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

    /// DA1: a VT220 (62), one of xterm's defined levels, with sixel
    /// graphics (4) while sixel is offered, and ANSI color (22).
    pub(super) fn reply_da1(&mut self) {
        let reply: &[u8] = if self.sixel_offered() {
            b"\x1b[?62;4;22c"
        } else {
            b"\x1b[?62;22c"
        };
        self.reply(Query::Da1, reply);
    }

    /// Whether sixel is offered: a build with images, sixel switched on,
    /// its handler installed and a budget above 0 (ADR-1100, D10).
    fn sixel_offered(&self) -> bool {
        #[cfg(feature = "images")]
        return self.image_offered(super::images::ImageString::Sixel);
        #[cfg(not(feature = "images"))]
        false
    }

    /// XTSMGRAPHICS, `CSI ? Pi ; Pa ; Pv S`: item Pi, action Pa, value Pv.
    /// Item 1 is the sixel color registers: read (1) the count in force,
    /// reset (2) it to `num-color-registers`, set (3) it to Pv from 2 up to
    /// `num-color-registers`, or read the most (4); a changed count takes
    /// effect at the next picture. Item 2 is the sixel geometry: read (1)
    /// and read the most (4) both answer `max-graphic-size`, and reset and
    /// set are refused, as xterm refuses them. The reply is `CSI ? Pi ; Ps
    /// S` followed by the value or values: status 0 success, 1 an item it
    /// does not know, 2 an action it does not know, 3 a value it cannot
    /// take. Every other item, and every item while sixel is not offered or
    /// in a build without images, answers status 1 with no value (ADR-1100,
    /// D10). An item the program left out is written as 0.
    pub(super) fn reply_xtsmgraphics(&mut self, params: &Params) {
        let item = params.get(0);
        let action = params.get(1);
        let value = params.get(2);
        let (status, values) = self.xtsmgraphics(item, action, value);
        let mut reply = format!("\x1b[?{};{status}", item.unwrap_or(0));
        for v in values.into_iter().flatten() {
            reply += &format!(";{v}");
        }
        reply += "S";
        self.reply(Query::Xtsmgraphics, reply.as_bytes());
    }

    /// XTSMGRAPHICS's status and values.
    #[cfg(not(feature = "images"))]
    fn xtsmgraphics(
        &mut self,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u32>,
    ) -> (u8, [Option<u32>; 2]) {
        (1, [None, None])
    }

    /// XTSMGRAPHICS's status and values, with images.
    #[cfg(feature = "images")]
    fn xtsmgraphics(
        &mut self,
        item: Option<u32>,
        action: Option<u32>,
        value: Option<u32>,
    ) -> (u8, [Option<u32>; 2]) {
        const NONE: [Option<u32>; 2] = [None, None];
        if !self.sixel_offered() {
            return (1, NONE);
        }
        let limits = self.image_limits();
        match (item, action) {
            (Some(1), Some(1)) => (0, [Some(self.sixel_state().registers), None]),
            (Some(1), Some(2)) => {
                self.set_sixel_registers(None);
                (0, [Some(limits.registers), None])
            }
            (Some(1), Some(3)) => match value {
                Some(n) if (REGISTERS_MIN..=limits.registers).contains(&n) => {
                    self.set_sixel_registers(Some(n));
                    (0, [Some(n), None])
                }
                _ => (3, NONE),
            },
            (Some(1), Some(4)) => (0, [Some(limits.registers), None]),
            (Some(2), Some(1 | 4)) => (0, [Some(limits.graphic.0), Some(limits.graphic.1)]),
            (Some(2), Some(2 | 3)) => (3, NONE),
            (Some(1 | 2), _) => (2, NONE),
            _ => (1, NONE),
        }
    }

    /// DA2: a VT220, firmware 1 (ADR-1000, query.da2).
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
        let (_, y) = self.screen.cursor();
        // From the left margin in origin mode, as xterm's CPR counts it.
        let col = self.screen.origin_col().saturating_add(1);
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
    /// DECSLRM, DECSCUSR, DECSCL, DECSCA and DECSACE; anything else, and a request
    /// longer than any of these, the error form `DCS 0 $ r ST`, with no
    /// echo (ADR-1000).
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
            // The left and right margins, the whole width while DECLRMM
            // is reset, as xterm reports them.
            Some(b"s") => {
                let (left, right) = self.screen.lr_margins();
                Some(format!(
                    "{};{}s",
                    left.saturating_add(1),
                    right.saturating_add(1)
                ))
            }
            Some(b" q") => Some(format!("{} q", self.cursor_style.param())),
            // Pc 1: the replies stay 7-bit whatever DECSCL asked.
            Some(b"\"p") => Some(format!("{};1\"p", self.conformance)),
            // DECSCA 1 or 0, and DECSACE 2 or 0, as xterm reports them.
            Some(b"\"q") => {
                let protected = self.screen.pen().attrs.contains(Attrs::PROTECTED);
                Some(format!("{}\"q", u8::from(protected)))
            }
            Some(b"*x") => {
                let rectangle = self.screen.attribute_extent() == Extent::Rectangle;
                Some(format!("{}*x", if rectangle { 2 } else { 0 }))
            }
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
    /// A stand-in kitty handler that answers each string with its image
    /// number, read and written again, as the kitty handler's replies do.
    #[cfg(feature = "images")]
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Answer {
        keys: Vec<u8>,
        done: bool,
    }

    #[cfg(feature = "images")]
    impl vtium_image::Handler for Answer {
        fn start(&mut self, _: &mut dyn vtium_image::Context, _: vtium_image::Intro<'_>) {
            *self = Self::default();
        }
        fn feed(&mut self, _: &mut dyn vtium_image::Context, data: vtium_parse::Chunk<'_>) {
            if let vtium_parse::Chunk::Bytes(bytes) = data {
                for &b in bytes {
                    self.done |= b == b';';
                    if !self.done {
                        self.keys.push(b);
                    }
                }
            }
        }
        fn finish(&mut self, ctx: &mut dyn vtium_image::Context) {
            let id = self
                .keys
                .split(|&b| b == b',')
                .find_map(|k| k.strip_prefix(b"i="))
                .and_then(|v| core::str::from_utf8(v).ok()?.parse::<u32>().ok());
            if let Some(id) = id {
                let reply = format!("\x1b_Gi={id};OK\x1b\\");
                ctx.reply(vtium_image::ReplyKind::KittyGraphics, reply.as_bytes());
            }
        }
        fn cancel(&mut self, _: &mut dyn vtium_image::Context) {}
        fn decoded(&mut self, _: &mut dyn vtium_image::Context, _: vtium_image::decode::Reply) {}
        fn clone_box(&self) -> Box<dyn vtium_image::Handler> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn same(&self, other: &dyn vtium_image::Handler) -> bool {
            other.as_any().downcast_ref::<Self>() == Some(self)
        }
    }

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
        t.set_font_ops(true);
        assert!(t.set_font_name("unifont"));
        t.input(b"\x1b[3;5H\x1b[1;31m");
        t.take_replies();
        #[cfg(feature = "images")]
        {
            t.screen_mut().set_image_limit(1 << 20);
            t.set_image_switches(vtium_image::Switches {
                sixel: false,
                kitty: true,
                iterm2: false,
            });
            t.set_image_handler(crate::ImageString::Kitty, Some(Box::new(Answer::default())));
        }
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
            #[cfg(feature = "images")]
            Query::ImageGraphics => {
                // a kitty string to the stand-in handler, which re-encodes
                // its image number
                let id = u32::try_from(rng.next() % 70_000).expect("small");
                let n = rng.padded(id);
                (
                    cat(&[b"\x1b_Gi=", &n, b",x=", &tok, b";", &tok, b"\x1b\\"]),
                    vec![tok],
                    vec![n],
                )
            }
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
            // the item is written again; the action and value never are
            Query::Xtsmgraphics => {
                let item = u32::try_from(rng.next() % 70_000).expect("small");
                let n = rng.padded(item);
                let action = u32::try_from(rng.next() % 5).expect("small");
                let action = rng.padded(action);
                let value = u32::try_from(rng.next() % 70_000).expect("small");
                let value = rng.padded(value);
                (
                    cat(&[b"\x1b[?", &n, b";", &action, b";", &value, b"S"]),
                    vec![],
                    vec![n, action, value],
                )
            }
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
            // Hostile text half the time, else a setting vtium reports.
            Query::Decrqss => {
                let known: [&[u8]; 7] = [b"m", b"r", b"s", b" q", b"\"p", b"\"q", b"*x"];
                let pick = rng.next();
                match known.get(usize::try_from(pick % 14).unwrap_or(0)) {
                    Some(&setting) => (cat(&[b"\x1bP$q", setting, b"\x1b\\"]), vec![], vec![]),
                    None => (cat(&[b"\x1bP$q", &tok, b"\x1b\\"]), vec![tok], vec![]),
                }
            }
            Query::PaletteColor => {
                let n = rng.padded(1);
                let request = cat(&[b"\x1b]4;", &n, b";?;", &tok, b";?\x07"]);
                (request, vec![tok], vec![n])
            }
            Query::DefaultColor => {
                let request = cat(&[b"\x1b]10;?;", &tok, b"\x1b\\"]);
                (request, vec![tok], vec![])
            }
            // The number in the reply is the color's, written again.
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
            // A set asked for by a hostile name, then the query: the
            // reply names the host's set, never the one asked for.
            Query::Font => {
                let request = cat(&[b"\x1b]50;", &tok, b"\x07\x1b]50;?\x1b\\"]);
                (request, vec![tok], vec![])
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
    // covers: query.xtgettcap, query.decrqss, color.palette.query, color.dynamic.query,
    // covers: color.dynamic.set, color.pointer, color.selection, color.kitty-protocol,
    // covers: window.size-report, window.title-report, input.modify-other-keys, color.stack,
    // covers: query.xtsmgraphics
    #[test]
    fn no_reply_carries_the_requests_bytes() {
        for seed in 0..200 {
            let mut rng = Rng(seed);
            for &query in Query::ALL {
                let mut t = fixture();
                t.state.answered.clear();
                let p = probe(query, &mut rng);
                // fed whole, as the host feeds what an input call left
                let mut rest = p.request.as_slice();
                while let Some((_, more)) = rest.split_at_checked(t.input(rest)) {
                    if more.len() == rest.len() {
                        break;
                    }
                    rest = more;
                }
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
                #[cfg(feature = "images")]
                Query::ImageGraphics => (b"\x1b_Gi=31;AAAA\x1b\\", b"\x1b_Gi=31;OK\x1b\\"),
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
                Query::Font => (b"\x1b]50;?\x07", b"\x1b]50;unifont\x07"),
                Query::Xtsmgraphics => (b"\x1b[?1;1S", b"\x1b[?1;1S"),
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
