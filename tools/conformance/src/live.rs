//! The live path: a replay's terminal at one block, written as the bytes
//! that make another terminal show the same screen.
//!
//! `make tier1-play` and `conformance scenarios --play` write these bytes
//! to the terminal they run in, so a vtium console draws each screen of a
//! case itself, its pictures included. `make tier1-shots` draws the same
//! terminal at the same block as a picture, and checks that the bytes,
//! read back through a terminal of the replay's size, draw that picture.
//!
//! The bytes do not replay the case's own stream. A test command, a
//! fixture host's answer or a picture made in another cell has no
//! sequence a terminal reads, so the bytes describe the screen as the
//! replay left it:
//!
//! - Room for the screen is made first, and the cursor's place at its top
//!   left is saved (DECSC). Each row is reached from that place, so the
//!   bytes draw the screen wherever the cursor stands at the start of a
//!   line.
//! - Each cell is written with its resolved colors as direct color (SGR 38
//!   and 48), its underline style and color, and the attributes that are
//!   not already in its colors: bold, italic, blink, conceal, strike,
//!   overline and the script. A selected cell and a hovered link are
//!   written in the colors they draw in. Autowrap is off while the rows are
//!   written, and the rows go in the order they draw, in explicit
//!   bidirectional mode (BDSM reset), with each mirrored character written
//!   as its mirror.
//! - With images, each picture the frame places is sent through the kitty
//!   graphics protocol as the renderer draws it: the part drawn, with the
//!   pixels the renderer draws there, scaled and cut already, at its place
//!   by cell and pixel offset, and its layer as a z index. A picture made
//!   in another cell, or drawn at another size, then reads back at its own
//!   size with the renderer's pixels.
//! - The cursor goes to its cell in its shape, blink and color, shown or
//!   hidden as the replay's cursor is.

use core::fmt::Write as _;

use vtium_term::{Age, CursorShape, FrameCell, Glyph, Rgb, Script, Terminal, UnderlineStyle};
use vtium_term::{Attrs, Width, cluster_width};

/// The cell size the reference host makes its pictures in, and the cell of
/// the terminal that reads the bytes back.
pub const CELL: (u32, u32) = (8, 16);

/// The bytes that make a terminal show `term`'s screen, drawn from the
/// start of the line the cursor is on, as the module describes.
pub fn screen(term: &Terminal) -> Vec<u8> {
    let frame = term.screen().frame(term.palette(), Age::NEVER);
    let rows = frame.rows();
    let mut out = String::from("\x1b[0m\r");
    // room for the screen, scrolled into view, then its top left saved
    for _ in 1..rows {
        out.push('\n');
    }
    up(&mut out, rows.saturating_sub(1));
    out.push_str("\x1b7\x1b[?7l\x1b[?25l");
    #[cfg(feature = "images")]
    out.push_str("\x1b_Ga=d,d=A,q=2\x1b\\");
    // grapheme clustering (mode 2027), the reading terminal's default
    out.push_str("\x1b[?2027h");
    let mut grapheme = true;
    for row in 0..rows {
        out.push_str("\x1b8");
        down(&mut out, row);
        // explicit mode marks the paragraph the cursor is in when it is set
        out.push_str("\x1b[8l\x1b[0m\x1b[2K");
        let mut last = String::new();
        for (col, cell) in frame.cells(row).enumerate() {
            if cell.width == Width::WideTail {
                continue;
            }
            let sgr = sgr(&cell);
            if sgr != last {
                out.push_str(&sgr);
                last = sgr;
            }
            // each cell from its own column, so no cell joins the one
            // before it, in the width method that gives its width
            let unicode = match cell.glyph {
                Glyph::Char(c) => (cluster_width(&[c]) == 2) == (cell.width == Width::Wide),
                Glyph::Cluster(s) => {
                    let chars: Vec<char> = s.chars().collect();
                    (cluster_width(&chars) == 2) == (cell.width == Width::Wide)
                }
                Glyph::Empty => true,
            };
            if unicode != grapheme {
                out.push_str(if unicode {
                    "\x1b[?2027h"
                } else {
                    "\x1b[?2027l"
                });
                grapheme = unicode;
            }
            let _ = write!(out, "\x1b[{}G", col + 1);
            text(&cell, &mut out);
        }
    }
    out.push_str("\x1b[0m\x1b[?2027h");
    let mut bytes = out.into_bytes();
    #[cfg(feature = "images")]
    pictures(&frame, &mut bytes);
    let mut out = String::from("\x1b8");
    let c = frame.cursor();
    down(&mut out, c.row);
    if c.col > 0 {
        let _ = write!(out, "\x1b[{}C", c.col);
    }
    let shape = match c.shape {
        CursorShape::Block => 1,
        CursorShape::Underline => 3,
        CursorShape::Bar => 5,
    } + u8::from(!c.blink);
    let _ = write!(out, "\x1b[{shape} q");
    match c.color {
        Some(rgb) => {
            let _ = write!(
                out,
                "\x1b]12;#{:02x}{:02x}{:02x}\x1b\\",
                rgb.r, rgb.g, rgb.b
            );
        }
        None => out.push_str("\x1b]112\x1b\\"),
    }
    if c.visible {
        out.push_str("\x1b[?25h");
    }
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

/// What a player writes after the screen [`screen`] drew for a terminal of
/// `rows` rows, to go on writing on the line below it: the colors, the
/// cursor, autowrap and implicit bidirectional mode put back.
pub fn below(rows: usize) -> Vec<u8> {
    let mut out = String::from("\x1b[0m\x1b8");
    down(&mut out, rows.saturating_sub(1));
    out.push_str("\r\n\x1b[?7h\x1b[8h\x1b[0 q\x1b]112\x1b\\\x1b[?25h");
    out.into_bytes()
}

fn up(out: &mut String, n: usize) {
    if n > 0 {
        let _ = write!(out, "\x1b[{n}A");
    }
}

fn down(out: &mut String, n: usize) {
    if n > 0 {
        let _ = write!(out, "\x1b[{n}B");
    }
}

/// The SGR that writes `cell` in the colors and attributes it draws in.
fn sgr(cell: &FrameCell<'_>) -> String {
    let (f, b) = (cell.fg, cell.bg);
    let mut s = format!(
        "\x1b[0;38;2;{};{};{};48;2;{};{};{}",
        f.r, f.g, f.b, b.r, b.g, b.b
    );
    let a = cell.attrs;
    for (flag, code) in [
        (Attrs::BOLD, ";1"),
        (Attrs::ITALIC, ";3"),
        (Attrs::BLINK, ";5"),
        (Attrs::RAPID_BLINK, ";6"),
        (Attrs::CONCEAL, ";8"),
        (Attrs::STRIKE, ";9"),
        (Attrs::OVERLINE, ";53"),
    ] {
        if a.contains(flag) {
            s.push_str(code);
        }
    }
    let underline = if cell.hovered {
        UnderlineStyle::Dashed
    } else {
        a.underline()
    };
    s.push_str(match underline {
        UnderlineStyle::None => "",
        UnderlineStyle::Single => ";4",
        UnderlineStyle::Double => ";4:2",
        UnderlineStyle::Curly => ";4:3",
        UnderlineStyle::Dotted => ";4:4",
        UnderlineStyle::Dashed => ";4:5",
    });
    if let Some(Rgb { r, g, b }) = cell.underline_color {
        let _ = write!(s, ";58:2::{r}:{g}:{b}");
    }
    s.push_str(match a.script() {
        Script::Baseline => "",
        Script::Super => ";73",
        Script::Sub => ";74",
    });
    s.push('m');
    s
}

/// The kitty Unicode placeholder, which a cell of a placeholder run holds;
/// the run's picture is sent as a picture of its own, so its cells are
/// written blank.
const PLACEHOLDER: char = '\u{10EEEE}';

/// Writes `cell`'s text: its mirror in place of its first character when
/// it draws mirrored, a space for an empty cell or a placeholder.
fn text(cell: &FrameCell<'_>, out: &mut String) {
    let written = match cell.glyph {
        Glyph::Empty => " ",
        Glyph::Char(c) if c == PLACEHOLDER => " ",
        Glyph::Char(_) | Glyph::Cluster(_) => "",
    };
    if !written.is_empty() {
        out.push_str(written);
        return;
    }
    let all = match cell.glyph {
        Glyph::Char(c) => c.to_string(),
        Glyph::Cluster(s) => s.to_owned(),
        Glyph::Empty => String::new(),
    };
    #[cfg(feature = "bidi")]
    if let Some(m) = cell.mirror {
        let mut chars = all.chars();
        chars.next();
        out.push(m);
        out.extend(chars);
        return;
    }
    out.push_str(&all);
}

/// Sends each picture `frame` places through the kitty graphics protocol,
/// as the renderer draws it on a display of the reference host's cells:
/// the part drawn, at its place by cell and pixel offset, with the pixels
/// the renderer draws there ([`vtium_render::placed_picture`]), scaled and
/// cut already, so the reading terminal draws them at their own size; and
/// its layer as a z index.
#[cfg(feature = "images")]
fn pictures(frame: &vtium_term::Frame<'_>, out: &mut Vec<u8>) {
    use vtium_term::images::Layer;
    let (cx, cy) = (CELL.0, CELL.1);
    let grid = (
        u32::try_from(frame.cols())
            .unwrap_or(u32::MAX)
            .saturating_mul(cx),
        u32::try_from(frame.rows())
            .unwrap_or(u32::MAX)
            .saturating_mul(cy),
    );
    for (n, p) in frame.placements().iter().enumerate() {
        let Some(drawn) = vtium_render::placed_picture(p, CELL, grid) else {
            continue;
        };
        let (Ok(x), Ok(y)) = (u32::try_from(drawn.x), u32::try_from(drawn.y)) else {
            continue;
        };
        let id = n + 1;
        transmit(id, (drawn.width, drawn.height), &drawn.pixels, out);
        let z = match p.layer {
            Layer::AboveText => p.z.max(0),
            Layer::BelowText => p.z.clamp(-1_073_741_824, -1),
            Layer::BelowBackground => p.z.min(-1_073_741_825),
        };
        let mut cmd = String::from("\x1b8");
        down(&mut cmd, usize::try_from(y / cy).unwrap_or(0));
        if x / cx > 0 {
            let _ = write!(cmd, "\x1b[{}C", x / cx);
        }
        let _ = write!(
            cmd,
            "\x1b_Ga=p,q=2,C=1,i={id},p={id},X={},Y={},z={z}\x1b\\",
            x % cx,
            y % cy
        );
        out.extend_from_slice(cmd.as_bytes());
    }
}

/// Sends `pixels`, premultiplied ARGB, `width` by `height`, as image `id`,
/// in straight RGBA in chunks.
#[cfg(feature = "images")]
fn transmit(id: usize, (width, height): (u32, u32), pixels: &[u32], out: &mut Vec<u8>) {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &px in pixels {
        let a = px >> 24;
        let un = |c: u32| -> u8 {
            (c * 255 + a / 2)
                .checked_div(a)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(if a == 0 { 0 } else { u8::MAX })
        };
        rgba.extend_from_slice(&[
            un((px >> 16) & 0xff),
            un((px >> 8) & 0xff),
            un(px & 0xff),
            u8::try_from(a).unwrap_or(u8::MAX),
        ]);
    }
    let data = base64(&rgba);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
    for (k, chunk) in chunks.iter().enumerate() {
        let more = u8::from(k + 1 < chunks.len());
        let head = if k == 0 {
            format!("\x1b_Ga=t,q=2,f=32,i={id},s={width},v={height},m={more};")
        } else {
            format!("\x1b_Gm={more};")
        };
        out.extend_from_slice(head.as_bytes());
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
}

/// `data` in base64 with padding.
#[cfg(feature = "images")]
fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A terminal of `cols` by `rows` with the reference host's cell and, with
/// images, its kitty graphics handler, after it read `bytes`: the screen a
/// terminal shows when it reads the live path's bytes.
pub fn read_back(bytes: &[u8], cols: usize, rows: usize) -> Terminal {
    let mut t = Terminal::new(cols, rows, 0);
    t.set_cell_size(CELL.0, CELL.1);
    #[cfg(feature = "images")]
    {
        use vtium_term::ImageString;
        use vtium_term::images::{Kitty, Switches};
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        t.screen_mut().set_image_limit(320_000_000);
        let _ = t.screen_mut().set_image_cell(CELL.0, CELL.1);
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
    }
    let mut rest = bytes;
    while !rest.is_empty() {
        let taken = t.input(rest);
        t.take_replies();
        if taken == 0 {
            break;
        }
        rest = rest.get(taken..).unwrap_or_default();
    }
    t
}

#[cfg(all(test, feature = "images"))]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_as_rfc_4648_gives() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }
}
