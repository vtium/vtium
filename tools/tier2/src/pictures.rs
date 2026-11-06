//! The picture screens' strings: kitty graphics commands and sixel images,
//! generated from pixel rules, which the terminal's own handlers read as a
//! program's output would reach them (ADR-1100, ADR-1303 D2).
//!
//! Each screen's pictures are drawn at their own size and at another, so
//! the GPU path's two picture steps are judged against the CPU path's
//! drawing: shrunk by 1.5, 2.5, 4 and 8, grown by 2 and 3, shrunk along one
//! axis and grown along the other, below the backgrounds, below the text
//! and above it, opaque and half transparent.

use std::fmt::Write as _;

use crate::draw::Feed;

/// A picture's pixels, premultiplied `0xAARRGGBB`, from a rule of the
/// column and row.
type Rule = fn(u32, u32) -> u32;

/// A colorful pattern with detail at every scale, opaque: diagonal bands,
/// a grid of fine lines and a slow gradient, so a filter that skips or
/// misweighs a source pixel shows.
pub fn pattern(x: u32, y: u32) -> u32 {
    let r = (x * 7 + y * 3) & 0xff;
    let g = ((x ^ y) * 5) & 0xff;
    let b = if x.is_multiple_of(5) || y.is_multiple_of(3) {
        0xf0
    } else {
        (x * y) & 0x7f
    };
    0xff00_0000 | r << 16 | g << 8 | b
}

/// A second opaque pattern, of rings and stripes.
pub fn rings(x: u32, y: u32) -> u32 {
    let (dx, dy) = (x as i32 - 40, y as i32 - 30);
    let d = ((dx * dx + dy * dy) as u32 / 23) & 0xff;
    0xff00_0000 | d << 16 | ((x * 11) & 0xff) << 8 | (255 - d)
}

/// A half-transparent pattern, premultiplied: its alpha ramps across, and
/// each channel is at most its alpha.
pub fn glass(x: u32, y: u32) -> u32 {
    let a = 40 + (x * 3 + y) % 200;
    let c = |v: u32| v * a / 255;
    a << 24 | c((x * 9) & 0xff) << 16 | c(0xc0) << 8 | c((y * 13) & 0xff)
}

/// Standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A kitty picture placement: where its top left cell is, the cells it is
/// stretched over, `(0, 0)` for its own size, and its z.
pub struct Kitty {
    pub id: u32,
    pub size: (u32, u32),
    pub rule: Rule,
    pub at: (usize, usize),
    pub cells: (u32, u32),
    pub z: i32,
}

impl Kitty {
    /// The command that sends the picture as 32-bit RGBA and places it,
    /// the cursor left where it was and no reply asked for, in chunks of
    /// at most 4096 base64 characters.
    pub fn bytes(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let p = (self.rule)(x, y);
                // the wire carries straight alpha; the store premultiplies
                let a = p >> 24;
                let un = |c: u32| (c * 255 + a / 2).checked_div(a).unwrap_or(0).min(255);
                rgba.extend_from_slice(&[
                    un(p >> 16 & 0xff) as u8,
                    un(p >> 8 & 0xff) as u8,
                    un(p & 0xff) as u8,
                    a as u8,
                ]);
            }
        }
        let mut out = format!("\x1b[{};{}H", self.at.1 + 1, self.at.0 + 1);
        let mut keys = format!("a=T,f=32,s={w},v={h},i={},C=1,q=2,z={}", self.id, self.z);
        if self.cells != (0, 0) {
            let _ = write!(keys, ",c={},r={}", self.cells.0, self.cells.1);
        }
        out.push_str(&chunked(&keys, &rgba));
        out.into_bytes()
    }
}

/// A kitty command of `keys` whose payload is `data` in base64, in chunks
/// of at most 4096 characters, `m=1` on each but the last, the keys in the
/// first.
fn chunked(keys: &str, data: &[u8]) -> String {
    let data = base64(data);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let head = if i == 0 {
            format!("{keys},")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\x1b_G{head}m={more};{}\x1b\\",
            std::str::from_utf8(chunk).unwrap_or_default()
        );
    }
    out
}

/// A sixel image of `w` by `h` from `rule`, its colors reduced to the
/// rule's own, at most 64, each pixel set, placed at the cell `at`.
pub fn sixel(w: u32, h: u32, rule: Rule, at: (usize, usize)) -> Vec<u8> {
    let mut colors: Vec<u32> = Vec::new();
    let mut index = vec![0usize; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let p = rule(x, y) & 0x00ff_ffff;
            let i = match colors.iter().position(|&c| c == p) {
                Some(i) => i,
                None => {
                    colors.push(p);
                    colors.len() - 1
                }
            };
            index[(y * w + x) as usize] = i;
        }
    }
    assert!(
        colors.len() <= 64,
        "a sixel rule of {} colors",
        colors.len()
    );
    let mut out = format!("\x1b[{};{}H\x1bP0;0;0q\"1;1;{w};{h}", at.1 + 1, at.0 + 1);
    for (i, c) in colors.iter().enumerate() {
        // sixel colors are percentages
        let pc = |v: u32| (v * 100 + 127) / 255;
        let _ = write!(
            out,
            "#{i};2;{};{};{}",
            pc(c >> 16 & 0xff),
            pc(c >> 8 & 0xff),
            pc(c & 0xff)
        );
    }
    for band in 0..h.div_ceil(6) {
        for (i, _) in colors.iter().enumerate() {
            let mut line = String::new();
            let mut any = false;
            for x in 0..w {
                let mut bits = 0u8;
                for k in 0..6 {
                    let y = band * 6 + k;
                    if y < h && index[(y * w + x) as usize] == i {
                        bits |= 1 << k;
                    }
                }
                any |= bits != 0;
                line.push((b'?' + bits) as char);
            }
            if any {
                let _ = write!(out, "#{i}{line}$");
            }
        }
        out.push('-');
    }
    out.push_str("\x1b\\");
    out.into_bytes()
}

/// A sixel rule of few colors: tiles of eight colors and a diagonal.
pub fn tiles(x: u32, y: u32) -> u32 {
    const C: [u32; 8] = [
        0xd02020, 0x20a040, 0x2040d0, 0xe0c020, 0x9030c0, 0x20c0c0, 0xf0f0f0, 0x303030,
    ];
    if (x + y).is_multiple_of(9) {
        return 0xff00_0000;
    }
    0xff00_0000 | C[((x / 5 + y / 3) % 8) as usize]
}

/// Concatenates each picture's command.
fn all(pictures: &[Kitty]) -> Vec<u8> {
    pictures.iter().flat_map(Kitty::bytes).collect()
}

/// Below every picture of `pictures_kitty`, the text and backgrounds they
/// stand over.
pub const KITTY_TEXT: &[u8] = concat!(
    "pictures over text: kitty, at own size and scaled\r\n",
    "\x1b[41mred\x1b[0m \x1b[42mgreen\x1b[0m \x1b[44mblue\x1b[0m text under and over pictures\r\n",
    "The quick brown fox jumps over the lazy dog. 0123456789\r\n",
    "\x1b[1mbold\x1b[0m \x1b[4munderlined\x1b[0m \x1b[9mstruck\x1b[0m \x1b[7mreversed\x1b[0m\r\n",
    "\x1b[43m                              \x1b[0m below background\r\n",
    "\x1b[45m      \x1b[0m      \x1b[46m      \x1b[0m      \x1b[4;9;33mlayers\x1b[0m\r\n",
    "lorem ipsum dolor sit amet, consectetur adipiscing elit\r\n",
    "sed do eiusmod tempor incididunt ut labore et dolore\r\n",
    "magna aliqua. Ut enim ad minim veniam, quis nostrud\r\n",
    "exercitation ullamco laboris nisi ut aliquip ex ea\r\n",
    "commodo consequat. Duis aute irure dolor in\r\n",
    "reprehenderit in voluptate velit esse cillum dolore",
)
.as_bytes();

/// The kitty screen's pictures: its own size, each scale, and each layer.
pub fn kitty(f: &mut Feed) -> Result<(), String> {
    f.bytes(&all(&[
        // its own size, above the text
        Kitty {
            id: 1,
            size: (32, 32),
            rule: pattern,
            at: (0, 0),
            cells: (0, 0),
            z: 1,
        },
        // shrunk by 1.5, 2.5, 4 and 8
        Kitty {
            id: 2,
            size: (48, 48),
            rule: pattern,
            at: (5, 0),
            cells: (4, 2),
            z: 1,
        },
        Kitty {
            id: 3,
            size: (80, 80),
            rule: rings,
            at: (10, 0),
            cells: (4, 2),
            z: 1,
        },
        Kitty {
            id: 4,
            size: (128, 128),
            rule: pattern,
            at: (15, 0),
            cells: (4, 2),
            z: 1,
        },
        Kitty {
            id: 5,
            size: (256, 128),
            rule: rings,
            at: (20, 0),
            cells: (4, 1),
            z: 1,
        },
        // grown by 2 and 3
        Kitty {
            id: 6,
            size: (16, 16),
            rule: pattern,
            at: (25, 0),
            cells: (4, 2),
            z: 1,
        },
        Kitty {
            id: 7,
            size: (8, 16),
            rule: rings,
            at: (30, 0),
            cells: (3, 3),
            z: 1,
        },
        // shrunk across and grown down
        Kitty {
            id: 8,
            size: (64, 8),
            rule: pattern,
            at: (34, 0),
            cells: (4, 2),
            z: 1,
        },
        // below the backgrounds, under default and colored cells
        Kitty {
            id: 9,
            size: (120, 40),
            rule: rings,
            at: (20, 3),
            cells: (16, 2),
            z: -1_073_741_825,
        },
        // below the text, under text and colored cells
        Kitty {
            id: 10,
            size: (100, 70),
            rule: pattern,
            at: (2, 5),
            cells: (14, 3),
            z: -5,
        },
        // half transparent above the text, at its own size and scaled
        Kitty {
            id: 11,
            size: (64, 48),
            rule: glass,
            at: (30, 5),
            cells: (0, 0),
            z: 2,
        },
        Kitty {
            id: 12,
            size: (96, 40),
            rule: glass,
            at: (40, 8),
            cells: (9, 3),
            z: 3,
        },
        // a half transparent picture over an opaque one
        Kitty {
            id: 13,
            size: (90, 90),
            rule: rings,
            at: (18, 8),
            cells: (8, 4),
            z: 4,
        },
        Kitty {
            id: 14,
            size: (30, 30),
            rule: glass,
            at: (20, 9),
            cells: (5, 2),
            z: 5,
        },
    ]))
}

/// Text beside the sixel screen's pictures.
pub const SIXEL_TEXT: &[u8] = concat!(
    "sixel pictures made in cells of 10x20, drawn in 8x16\r\n",
    "\r\n\r\n\r\n\r\n",
    "text below the pictures, and \x1b[44mblue\x1b[0m cells",
)
.as_bytes();

/// Two sixels made in a 10x20 cell, so the renderer's 8x16 cell shrinks
/// them, and one beside them at its own size in that cell.
pub fn sixels(f: &mut Feed) -> Result<(), String> {
    let mut out = sixel(40, 40, tiles, (0, 1));
    out.extend(sixel(70, 36, tiles, (6, 1)));
    out.extend(sixel(10, 18, tiles, (15, 2)));
    f.bytes(&out)
}

/// The budget screen's pictures, on a display of 160x64 whose bound is
/// two screens, 81,920 bytes: one whose own pixels pass the bound and whose
/// drawn copy fits, and one whose copy passes it too, scaled each frame.
pub fn budget(f: &mut Feed) -> Result<(), String> {
    f.bytes(&all(&[
        Kitty {
            id: 1,
            size: (200, 200),
            rule: pattern,
            at: (0, 0),
            cells: (8, 2),
            z: 1,
        },
        Kitty {
            id: 2,
            size: (200, 200),
            rule: rings,
            at: (10, 1),
            cells: (40, 12),
            z: 1,
        },
    ]))
}

/// The tiled screen's pictures, each wider and taller than the 64 pixels
/// a side its textures are held to, grown by about 2, shrunk by 1.5 and by
/// 4.
pub fn tiled(f: &mut Feed) -> Result<(), String> {
    f.bytes(&all(&[
        Kitty {
            id: 1,
            size: (150, 100),
            rule: pattern,
            at: (0, 1),
            cells: (38, 13),
            z: 1,
        },
        Kitty {
            id: 2,
            size: (192, 192),
            rule: rings,
            at: (40, 1),
            cells: (16, 8),
            z: 1,
        },
        Kitty {
            id: 3,
            size: (256, 256),
            rule: pattern,
            at: (58, 1),
            cells: (8, 4),
            z: -5,
        },
    ]))
}

// The outline picture screens' pictures (vtium/vtium#24): four pictures
// at any size, sent as kitty commands and sixel strings at the cells the
// screens name, over text in the test font set.

/// The z that puts a kitty picture below the cell backgrounds.
pub const BELOW_BACKGROUNDS: i32 = -1_073_741_825;
/// The z that puts a kitty picture below the text and over the
/// backgrounds.
pub const BELOW_TEXT: i32 = -1;
/// The z that puts a kitty picture above the text.
pub const ABOVE_TEXT: i32 = 0;

/// The patterns of the outline picture screens' pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Opaque: red rising across, green rising down, and a blue the seed
    /// picks.
    Gradient,
    /// The gradient with its right half at half alpha.
    Half,
    /// The gradient inside the ellipse the picture's edges touch, and
    /// clear outside it.
    Disc,
    /// Opaque stripes a pixel wide, alternating red and green on the
    /// diagonal, which a filter that shrinks them must mix to an even
    /// olive.
    Stripes,
}

/// A picture of `kind`, `width` by `height` pixels, whose blue `seed`
/// picks.
#[derive(Clone, Copy, Debug)]
pub struct Pic {
    pub kind: Kind,
    pub width: u32,
    pub height: u32,
    pub seed: u32,
}

impl Pic {
    pub const fn new(kind: Kind, (width, height): (u32, u32), seed: u32) -> Pic {
        Pic {
            kind,
            width,
            height,
            seed,
        }
    }

    /// The pixel at `x`, `y`: red, green, blue and straight alpha.
    pub fn rgba(&self, x: u32, y: u32) -> [u8; 4] {
        let (w, h) = (self.width, self.height);
        let ramp = |v: u32, n: u32| if n > 1 { v * 255 / (n - 1) } else { 0 };
        let (mut r, mut g, mut b, mut a) =
            (ramp(x, w), ramp(y, h), (self.seed * 97 + 64) & 0xff, 255);
        match self.kind {
            Kind::Gradient => {}
            Kind::Half => {
                if x >= w / 2 {
                    a = 128;
                }
            }
            Kind::Disc => {
                // outside the ellipse through the edges' midpoints, with
                // the pixel's centre as its point
                let (w, h) = (i64::from(w), i64::from(h));
                let dx = 2 * i64::from(x) + 1 - w;
                let dy = 2 * i64::from(y) + 1 - h;
                if dx * dx * h * h + dy * dy * w * w > w * w * h * h {
                    a = 0;
                }
            }
            Kind::Stripes => {
                r = if (x + y) % 2 == 1 { 255 } else { 0 };
                g = 255 - r;
                b = 64;
            }
        }
        [r as u8, g as u8, b as u8, a]
    }
}

/// How a kitty command sends a picture's pixels: `f=32`, RGBA with
/// straight alpha, or `f=24`, RGB, opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Rgba,
    Rgb,
}

/// A kitty command of `keys` that sends `pic` as `format`, the format and
/// size keys added, in chunks of 4096 base64 characters. It acts at the
/// cursor, where `keys` place it.
pub fn kitty_send(keys: &str, format: Format, pic: Pic) -> Vec<u8> {
    let (f, n) = match format {
        Format::Rgba => (32, 4),
        Format::Rgb => (24, 3),
    };
    let mut data = Vec::with_capacity((pic.width * pic.height * n) as usize);
    for y in 0..pic.height {
        for x in 0..pic.width {
            data.extend_from_slice(&pic.rgba(x, y)[..n as usize]);
        }
    }
    let keys = format!("{keys},f={f},s={},v={}", pic.width, pic.height);
    chunked(&keys, &data).into_bytes()
}

/// A kitty command of `keys` with no payload, such as a put of an image
/// already sent.
pub fn kitty_command(keys: &str) -> Vec<u8> {
    format!("\x1b_G{keys}\x1b\\").into_bytes()
}

/// Moves the cursor to the cell at column `col` and row `row`, from 0.
pub fn at(col: u32, row: u32) -> Vec<u8> {
    format!("\x1b[{};{}H", row + 1, col + 1).into_bytes()
}

/// A sixel string of `pic` at its own size, whose top left lands at the
/// cursor. Each band of six rows defines the color registers its own
/// colors take before it draws with them, so a picture of any number of
/// colors keeps every one, at sixel's steps of a percent; a pixel with
/// no alpha is left unset, and the string asks for unset pixels to stay
/// clear. Every pixel is opaque or clear.
pub fn sixel_of(pic: Pic) -> Vec<u8> {
    let (w, h) = (pic.width, pic.height);
    let px: Vec<[u8; 4]> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| pic.rgba(x, y))
        .collect();
    assert!(
        px.iter().all(|p| p[3] == 0 || p[3] == 255),
        "a sixel picture with partial alpha"
    );
    let clear = u8::from(px.iter().any(|p| p[3] == 0));
    let pc = |v: u8| (u32::from(v) * 100 + 127) / 255;
    let mut out = format!("\x1bP0;{clear}q\"1;1;{w};{h}");
    for band in 0..h.div_ceil(6) {
        let rows = band * 6..(band * 6 + 6).min(h);
        let mut colors: Vec<[u8; 3]> = Vec::new();
        for y in rows.clone() {
            for x in 0..w {
                let p = px[(y * w + x) as usize];
                let c = [p[0], p[1], p[2]];
                if p[3] != 0 && !colors.contains(&c) {
                    colors.push(c);
                }
            }
        }
        assert!(colors.len() <= 1024, "a band of {} colors", colors.len());
        for (i, c) in colors.iter().enumerate() {
            // defining a register selects it
            let _ = write!(out, "#{i};2;{};{};{}", pc(c[0]), pc(c[1]), pc(c[2]));
            for x in 0..w {
                let mut bits = 0u8;
                for y in rows.clone() {
                    let p = px[(y * w + x) as usize];
                    if p[3] != 0 && [p[0], p[1], p[2]] == *c {
                        bits |= 1 << (y - band * 6);
                    }
                }
                out.push((b'?' + bits) as char);
            }
            out.push('$');
        }
        out.push('-');
    }
    out.push_str("\x1b\\");
    out.into_bytes()
}

/// The size in pixels of `cols` by `rows` cells of `cell`, less `less`
/// pixels across and down.
fn cells(
    (cw, ch): (u32, u32),
    (cols, rows): (u32, u32),
    (less_w, less_h): (u32, u32),
) -> (u32, u32) {
    (cols * cw - less_w, rows * ch - less_h)
}

/// `img-bands`: one half-transparent picture in each of the three layers,
/// each image sent first and then put at its cell by a command of its
/// own, the cursor left where it was.
pub fn img_bands(f: &mut Feed) -> Result<(), String> {
    let mut out = Vec::new();
    for (b, z) in (0u32..).zip([BELOW_BACKGROUNDS, BELOW_TEXT, ABOVE_TEXT]) {
        let id = b + 1;
        let pic = Pic::new(Kind::Half, cells(f.cell, (24, 3), (0, 0)), b);
        out.extend(kitty_send(&format!("a=t,i={id},q=2"), Format::Rgba, pic));
        out.extend(at(14, 3 * b));
        out.extend(kitty_command(&format!("a=p,i={id},C=1,q=2,z={z}")));
    }
    f.bytes(&out)
}

/// `kitty-bands`: the pictures of `img-bands`, each sent and placed by one
/// command at the cursor, which DECSC and DECRC keep where it was.
pub fn kitty_bands(f: &mut Feed) -> Result<(), String> {
    let mut out = Vec::new();
    for (b, z) in (0u32..).zip([BELOW_BACKGROUNDS, BELOW_TEXT, ABOVE_TEXT]) {
        let pic = Pic::new(Kind::Half, cells(f.cell, (24, 3), (0, 0)), b);
        out.extend(b"\x1b7");
        out.extend(at(14, 3 * b));
        out.extend(kitty_send(
            &format!("a=T,i={},C=1,z={z}", b + 1),
            Format::Rgba,
            pic,
        ));
        out.extend(b"\x1b8");
    }
    f.bytes(&out)
}

/// `img-sixel`: three sixels over rows of text, an opaque one whose size
/// is not whole cells, a disc clear outside it, and an opaque one that
/// text written afterwards cuts.
pub fn img_sixel(f: &mut Feed) -> Result<(), String> {
    let mut out = at(2, 1);
    out.extend(sixel_of(Pic::new(
        Kind::Gradient,
        cells(f.cell, (10, 3), (3, 5)),
        1,
    )));
    out.extend(at(16, 1));
    out.extend(sixel_of(Pic::new(
        Kind::Disc,
        cells(f.cell, (10, 3), (0, 0)),
        2,
    )));
    out.extend(at(30, 1));
    out.extend(sixel_of(Pic::new(
        Kind::Gradient,
        cells(f.cell, (10, 3), (0, 0)),
        3,
    )));
    out.extend(at(32, 2));
    out.extend(b"TEXT");
    f.bytes(&out)
}

/// `img-scale`: a 40x20 gradient at its own size and fitted to 10 by 5
/// cells, 480x160 stripes fitted to 12 by 2 cells, and a 20x16 gradient
/// made in cells of 5x8 pixels, at its own size there.
pub fn img_scale(f: &mut Feed) -> Result<(), String> {
    let small = Pic::new(Kind::Gradient, (40, 20), 1);
    let mut out = at(1, 0);
    out.extend(kitty_send("a=T,i=1,p=1,C=1,q=2", Format::Rgba, small));
    out.extend(at(7, 0));
    out.extend(kitty_command("a=p,i=1,p=2,c=10,r=5,C=1,q=2"));
    out.extend(at(19, 0));
    out.extend(kitty_send(
        "a=T,i=2,c=12,r=2,C=1,q=2",
        Format::Rgba,
        Pic::new(Kind::Stripes, (480, 160), 0),
    ));
    f.bytes(&out)?;
    let font = f.cell;
    f.made_in((5, 8))?;
    let mut out = at(33, 0);
    out.extend(kitty_send(
        "a=T,i=3,C=1,q=2",
        Format::Rgba,
        Pic::new(Kind::Gradient, (20, 16), 2),
    ));
    f.bytes(&out)?;
    f.made_in(font)
}

/// `img-region`: a sixel and two kitty pictures, then a scroll of two
/// lines in a region from the third row to the eighth.
pub fn img_region(f: &mut Feed) -> Result<(), String> {
    let mut out = at(2, 2);
    out.extend(sixel_of(Pic::new(
        Kind::Gradient,
        cells(f.cell, (8, 4), (0, 0)),
        1,
    )));
    out.extend(at(16, 3));
    out.extend(kitty_send(
        "a=T,i=1,C=1,q=2",
        Format::Rgba,
        Pic::new(Kind::Gradient, cells(f.cell, (6, 2), (0, 0)), 2),
    ));
    out.extend(at(28, 1));
    out.extend(kitty_send(
        "a=T,i=2,C=1,q=2",
        Format::Rgba,
        Pic::new(Kind::Gradient, cells(f.cell, (6, 4), (0, 0)), 3),
    ));
    out.extend(b"\x1b[3;8r\x1b[8;1H\n\n\x1b[r");
    f.bytes(&out)
}

/// `img-rotated`: one half-transparent picture placed twice, at its own
/// size above the text and fitted to 6 by 3 cells below it.
pub fn img_rotated(f: &mut Feed) -> Result<(), String> {
    let mut out = at(2, 0);
    out.extend(kitty_send(
        "a=T,i=1,p=1,C=1,q=2",
        Format::Rgba,
        Pic::new(Kind::Half, cells(f.cell, (4, 2), (0, 0)), 1),
    ));
    out.extend(at(10, 0));
    out.extend(kitty_command(&format!(
        "a=p,i=1,p=2,c=6,r=3,C=1,q=2,z={BELOW_TEXT}"
    )));
    f.bytes(&out)
}

/// `img-moves`: before the first frame, two kitty pictures and a sixel;
/// before the third, the first kitty placement moved by a put of the
/// same placement id, the second picture's image sent again, which
/// removes its placement, the sixel's rows erased from its first column,
/// and a new picture below the text.
pub fn img_moves(f: &mut Feed) -> Result<(), String> {
    let mut out = Vec::new();
    match f.frame {
        0 => {
            out.extend(at(2, 1));
            out.extend(kitty_send(
                "a=T,i=1,p=1,C=1,q=2",
                Format::Rgba,
                Pic::new(Kind::Gradient, cells(f.cell, (8, 3), (0, 0)), 1),
            ));
            out.extend(at(20, 1));
            out.extend(sixel_of(Pic::new(
                Kind::Disc,
                cells(f.cell, (8, 2), (0, 0)),
                2,
            )));
            out.extend(at(40, 3));
            out.extend(kitty_send(
                "a=T,i=3,C=1,q=2",
                Format::Rgba,
                Pic::new(Kind::Gradient, cells(f.cell, (4, 2), (0, 0)), 4),
            ));
        }
        2 => {
            out.extend(at(6, 5));
            out.extend(kitty_command("a=p,i=1,p=1,C=1,q=2"));
            out.extend(kitty_send(
                "a=t,i=3,q=2",
                Format::Rgba,
                Pic::new(Kind::Gradient, cells(f.cell, (4, 2), (0, 0)), 5),
            ));
            out.extend(b"\x1b[2;21H\x1b[K\x1b[3;21H\x1b[K");
            out.extend(at(30, 6));
            out.extend(kitty_send(
                &format!("a=T,i=2,C=1,q=2,z={BELOW_TEXT}"),
                Format::Rgba,
                Pic::new(Kind::Gradient, cells(f.cell, (6, 2), (0, 0)), 3),
            ));
        }
        _ => {}
    }
    f.bytes(&out)
}

/// The banner line a banner screen writes at the bottom of its region
/// before frame `frame`, which scrolls the region a line.
fn banner_line(frame: u32) -> Vec<u8> {
    format!("\r\n\x1b[44;97m banner {frame} \x1b[0m text \x1b[42m over \x1b[0m video").into_bytes()
}

/// A banner screen's frame: before each of the first three frames, a
/// picture the size of the screen sent again as image 1, which removes
/// the last one's placement, and put at the top left at `z` by a command
/// of its own, then a banner line.
fn img_banner(f: &mut Feed, z: i32) -> Result<(), String> {
    if f.frame == 3 {
        return Ok(());
    }
    let pic = Pic::new(Kind::Gradient, cells(f.cell, (48, 10), (0, 0)), f.frame);
    let mut out = kitty_send("a=t,i=1,q=2", Format::Rgba, pic);
    out.extend(b"\x1b7\x1b[H");
    out.extend(kitty_command(&format!("a=p,i=1,C=1,q=2,z={z}")));
    out.extend(b"\x1b8");
    out.extend(banner_line(f.frame));
    f.bytes(&out)
}

/// `img-banner-bg`: the banner's pictures below the backgrounds.
pub fn img_banner_bg(f: &mut Feed) -> Result<(), String> {
    img_banner(f, BELOW_BACKGROUNDS)
}

/// `img-banner-text`: the banner's pictures below the text.
pub fn img_banner_text(f: &mut Feed) -> Result<(), String> {
    img_banner(f, BELOW_TEXT)
}

/// A kitty banner screen's frame: before each of the first three frames,
/// a picture the size of the screen sent and placed as image 1 at the top
/// left at `z` by one command, as RGBA and RGB in turn, then a banner
/// line.
fn kitty_banner(f: &mut Feed, z: i32) -> Result<(), String> {
    if f.frame == 3 {
        return Ok(());
    }
    let pic = Pic::new(Kind::Gradient, cells(f.cell, (48, 10), (0, 0)), f.frame);
    let format = if f.frame % 2 == 1 {
        Format::Rgb
    } else {
        Format::Rgba
    };
    let mut out = b"\x1b7\x1b[1;1H".to_vec();
    out.extend(kitty_send(&format!("a=T,i=1,q=2,C=1,z={z}"), format, pic));
    out.extend(b"\x1b8");
    out.extend(banner_line(f.frame));
    f.bytes(&out)
}

/// `kitty-banner-bg`: the kitty banner's pictures below the backgrounds.
pub fn kitty_banner_bg(f: &mut Feed) -> Result<(), String> {
    kitty_banner(f, BELOW_BACKGROUNDS)
}

/// `kitty-banner-text`: the kitty banner's pictures below the text.
pub fn kitty_banner_text(f: &mut Feed) -> Result<(), String> {
    kitty_banner(f, BELOW_TEXT)
}

/// `img-ed2`: before the first frame, a sixel and a kitty disc below the
/// text; the screen's second part erases the screen.
pub fn img_ed2(f: &mut Feed) -> Result<(), String> {
    if f.frame != 0 {
        return Ok(());
    }
    let mut out = at(4, 1);
    out.extend(sixel_of(Pic::new(
        Kind::Gradient,
        cells(f.cell, (12, 4), (0, 0)),
        1,
    )));
    out.extend(at(24, 2));
    out.extend(kitty_send(
        &format!("a=T,i=1,C=1,q=2,z={BELOW_TEXT}"),
        Format::Rgba,
        Pic::new(Kind::Disc, cells(f.cell, (8, 4), (0, 0)), 2),
    ));
    f.bytes(&out)
}

/// `kitty-text`: an opaque RGB gradient below the text placed with the
/// cursor moving past it, and text written after it and over it; and a
/// disc above three rows of text, the middle row then erased by EL.
pub fn kitty_text(f: &mut Feed) -> Result<(), String> {
    let mut out = at(1, 2);
    out.extend(kitty_send(
        &format!("a=T,i=1,z={BELOW_TEXT}"),
        Format::Rgb,
        Pic::new(Kind::Gradient, cells(f.cell, (8, 3), (0, 0)), 1),
    ));
    out.extend(b"<after");
    out.extend(at(2, 3));
    out.extend(b"\x1b[97;41mTEXT\x1b[0m on it");
    out.extend(at(25, 2));
    out.extend(kitty_send(
        &format!("a=T,i=2,C=1,z={ABOVE_TEXT}"),
        Format::Rgba,
        Pic::new(Kind::Disc, cells(f.cell, (10, 3), (0, 0)), 2),
    ));
    out.extend(at(19, 3));
    out.extend(b"\x1b[K");
    out.extend(at(0, 7));
    f.bytes(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standards_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
