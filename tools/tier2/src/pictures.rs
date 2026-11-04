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
        let data = base64(&rgba);
        let mut out = format!("\x1b[{};{}H", self.at.1 + 1, self.at.0 + 1);
        let mut keys = format!("a=T,f=32,s={w},v={h},i={},C=1,q=2,z={}", self.id, self.z);
        if self.cells != (0, 0) {
            let _ = write!(keys, ",c={},r={}", self.cells.0, self.cells.1);
        }
        let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
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
        out.into_bytes()
    }
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
pub fn kitty() -> Vec<u8> {
    all(&[
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
    ])
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
pub fn sixels() -> Vec<u8> {
    let mut out = sixel(40, 40, tiles, (0, 1));
    out.extend(sixel(70, 36, tiles, (6, 1)));
    out.extend(sixel(10, 18, tiles, (15, 2)));
    out
}

/// The budget screen's pictures, on a display of 160x64 whose bound is
/// two screens, 81,920 bytes: one whose own pixels pass the bound and whose
/// drawn copy fits, and one whose copy passes it too, scaled each frame.
pub fn budget() -> Vec<u8> {
    all(&[
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
    ])
}

/// The tiled screen's pictures, each wider and taller than the 64 pixels
/// a side its textures are held to, grown by about 2, shrunk by 1.5 and by
/// 4.
pub fn tiled() -> Vec<u8> {
    all(&[
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
    ])
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
