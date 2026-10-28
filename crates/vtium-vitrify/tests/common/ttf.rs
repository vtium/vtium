//! A TrueType font written in the test, the outline fixture: polygon glyphs
//! in `glyf`, a format 12 `cmap`, a `name` table, and for color a COLR
//! version 0 table with its CPAL palette. Big-endian throughout, tables
//! sorted by tag and padded to four bytes.

#![allow(dead_code)]

/// One glyph: its advance and its polygons, each point on the curve, in
/// font units with y up.
#[derive(Clone, Default)]
pub struct Glyph {
    pub advance: u16,
    pub contours: Vec<Vec<(i16, i16)>>,
}

impl Glyph {
    /// A glyph whose one contour is the rectangle `x0, y0` to `x1, y1`.
    pub fn rect(advance: u16, x0: i16, y0: i16, x1: i16, y1: i16) -> Glyph {
        Glyph {
            advance,
            contours: vec![vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)]],
        }
    }

    pub fn empty(advance: u16) -> Glyph {
        Glyph {
            advance,
            contours: Vec::new(),
        }
    }
}

/// A font to write.
#[derive(Clone)]
pub struct Font {
    pub family: String,
    pub upem: u16,
    pub ascent: i16,
    /// Below the baseline: negative.
    pub descent: i16,
    pub glyphs: Vec<Glyph>,
    /// Each codepoint and its glyph.
    pub cmap: Vec<(u32, u16)>,
    /// Each color glyph and its layers: a glyph and a palette entry.
    pub colr: Vec<(u16, Vec<(u16, u16)>)>,
    /// The palette, each entry red, green, blue and alpha.
    pub palette: Vec<[u8; 4]>,
}

fn p16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn pi16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn p32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn glyf_entry(g: &Glyph) -> Vec<u8> {
    if g.contours.is_empty() {
        return Vec::new();
    }
    let points: Vec<(i16, i16)> = g.contours.iter().flatten().copied().collect();
    let (mut x0, mut y0, mut x1, mut y1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    for &(x, y) in &points {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let mut out = Vec::new();
    pi16(&mut out, g.contours.len() as i16);
    for v in [x0, y0, x1, y1] {
        pi16(&mut out, v);
    }
    let mut end = 0u16;
    for c in &g.contours {
        end += c.len() as u16;
        p16(&mut out, end - 1);
    }
    p16(&mut out, 0); // no instructions
    out.extend(std::iter::repeat_n(1u8, points.len())); // on the curve, long x and y
    let (mut px, mut py) = (0i16, 0i16);
    for &(x, _) in &points {
        pi16(&mut out, x - px);
        px = x;
    }
    for &(_, y) in &points {
        pi16(&mut out, y - py);
        py = y;
    }
    out
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

impl Font {
    /// The font's bytes.
    pub fn build(&self) -> Vec<u8> {
        let n = self.glyphs.len() as u16;
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

        let mut head = Vec::new();
        p32(&mut head, 0x0001_0000);
        p32(&mut head, 0x0001_0000);
        p32(&mut head, 0);
        p32(&mut head, 0x5f0f_3cf5);
        p16(&mut head, 0x000b);
        p16(&mut head, self.upem);
        head.extend([0u8; 16]); // created, modified
        for v in [0i16, self.descent, self.upem as i16, self.ascent] {
            pi16(&mut head, v);
        }
        p16(&mut head, 0); // macStyle
        p16(&mut head, 8); // lowestRecPPEM
        pi16(&mut head, 2);
        pi16(&mut head, 1); // long loca
        pi16(&mut head, 0);
        tables.push((*b"head", head));

        let mut hhea = Vec::new();
        p32(&mut hhea, 0x0001_0000);
        pi16(&mut hhea, self.ascent);
        pi16(&mut hhea, self.descent);
        pi16(&mut hhea, 0);
        p16(
            &mut hhea,
            self.glyphs.iter().map(|g| g.advance).max().unwrap_or(0),
        );
        for v in [0i16, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0] {
            pi16(&mut hhea, v);
        }
        p16(&mut hhea, n);
        tables.push((*b"hhea", hhea));

        let mut maxp = Vec::new();
        p32(&mut maxp, 0x0001_0000);
        p16(&mut maxp, n);
        for v in [64u16, 8, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
            p16(&mut maxp, v);
        }
        tables.push((*b"maxp", maxp));

        let mut hmtx = Vec::new();
        for g in &self.glyphs {
            p16(&mut hmtx, g.advance);
            // the left side bearing is the glyph's leftmost x
            let lsb = g.contours.iter().flatten().map(|p| p.0).min().unwrap_or(0);
            pi16(&mut hmtx, lsb);
        }
        tables.push((*b"hmtx", hmtx));

        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for g in &self.glyphs {
            p32(&mut loca, glyf.len() as u32);
            glyf.extend(glyf_entry(g));
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        p32(&mut loca, glyf.len() as u32);
        tables.push((*b"glyf", glyf));
        tables.push((*b"loca", loca));

        let mut map = self.cmap.clone();
        map.sort();
        let mut sub = Vec::new();
        p16(&mut sub, 12);
        p16(&mut sub, 0);
        p32(&mut sub, 16 + 12 * map.len() as u32);
        p32(&mut sub, 0);
        p32(&mut sub, map.len() as u32);
        for &(c, g) in &map {
            p32(&mut sub, c);
            p32(&mut sub, c);
            p32(&mut sub, u32::from(g));
        }
        let mut cmap = Vec::new();
        p16(&mut cmap, 0);
        p16(&mut cmap, 1);
        p16(&mut cmap, 3);
        p16(&mut cmap, 10);
        p32(&mut cmap, 12);
        cmap.extend(sub);
        tables.push((*b"cmap", cmap));

        let strings = [
            (1u16, utf16(&self.family)),
            (2, utf16("Regular")),
            (13, utf16("Free for testing,\n  no warranty.")),
        ];
        let mut name = Vec::new();
        p16(&mut name, 0);
        p16(&mut name, strings.len() as u16);
        p16(&mut name, 6 + 12 * strings.len() as u16);
        let mut data: Vec<u8> = Vec::new();
        for (id, s) in &strings {
            for v in [3u16, 1, 0x409, *id, s.len() as u16, data.len() as u16] {
                p16(&mut name, v);
            }
            data.extend(s);
        }
        name.extend(data);
        tables.push((*b"name", name));

        if !self.colr.is_empty() {
            let mut colr = Vec::new();
            let layers: Vec<(u16, u16)> = self
                .colr
                .iter()
                .flat_map(|(_, l)| l.iter().copied())
                .collect();
            p16(&mut colr, 0);
            p16(&mut colr, self.colr.len() as u16);
            p32(&mut colr, 14);
            p32(&mut colr, 14 + 6 * self.colr.len() as u32);
            p16(&mut colr, layers.len() as u16);
            let mut first = 0u16;
            for (base, l) in &self.colr {
                p16(&mut colr, *base);
                p16(&mut colr, first);
                p16(&mut colr, l.len() as u16);
                first += l.len() as u16;
            }
            for (g, p) in layers {
                p16(&mut colr, g);
                p16(&mut colr, p);
            }
            tables.push((*b"COLR", colr));
            let mut cpal = Vec::new();
            p16(&mut cpal, 0);
            p16(&mut cpal, self.palette.len() as u16);
            p16(&mut cpal, 1);
            p16(&mut cpal, self.palette.len() as u16);
            p32(&mut cpal, 14);
            p16(&mut cpal, 0);
            for &[r, g, b, a] in &self.palette {
                cpal.extend([b, g, r, a]);
            }
            tables.push((*b"CPAL", cpal));
        }

        tables.sort_by_key(|(t, _)| *t);
        let mut out = Vec::new();
        p32(&mut out, 0x0001_0000);
        p16(&mut out, tables.len() as u16);
        for v in [0u16, 0, 0] {
            p16(&mut out, v);
        }
        let mut offset = 12 + 16 * tables.len() as u32;
        for (tag, bytes) in &tables {
            out.extend(tag);
            let sum = bytes
                .chunks(4)
                .map(|c| {
                    let mut w = [0u8; 4];
                    w[..c.len()].copy_from_slice(c);
                    u32::from_be_bytes(w)
                })
                .fold(0u32, u32::wrapping_add);
            p32(&mut out, sum);
            p32(&mut out, offset);
            p32(&mut out, bytes.len() as u32);
            offset += (bytes.len() as u32).next_multiple_of(4);
        }
        for (_, bytes) in &tables {
            out.extend(bytes);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }
        out
    }
}

/// The fixture: `Test Mono`, 1000 units to the em, ascent 800 and
/// descent 200. `A` is a bar 400 units wide, `W` fills its advance, U+4E00
/// advances a whole em, and U+1F600 is a color glyph of a red square under
/// a half-transparent blue one, advancing 1200.
pub fn test_mono() -> Font {
    Font {
        family: "Test Mono".to_owned(),
        upem: 1000,
        ascent: 800,
        descent: -200,
        glyphs: vec![
            Glyph::empty(600),
            Glyph::empty(600),
            Glyph::rect(600, 100, 0, 500, 700),
            Glyph::rect(600, 0, 0, 600, 700),
            Glyph::rect(1000, 50, 100, 950, 600),
            Glyph::rect(1200, 100, -100, 1100, 700),
            Glyph::rect(1200, 100, -100, 1100, 700),
            Glyph::rect(1200, 400, 100, 800, 500),
        ],
        cmap: vec![
            (0x20, 1),
            (0x41, 2),
            (0x57, 3),
            (0x4e00, 4),
            (0xe0a0, 2),
            (0x1f600, 5),
        ],
        colr: vec![(5, vec![(6, 0), (7, 1)])],
        palette: vec![[255, 0, 0, 255], [0, 0, 255, 128]],
    }
}
