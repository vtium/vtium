//! A TrueType font written in the test, the outline fixture: polygon glyphs
//! in `glyf`, a format 12 `cmap`, a `name` table, for color a COLR version
//! 0 table with its CPAL palette, and for shaping a GSUB table of single
//! substitutions and ligatures, each under a feature of its own. Big-endian
//! throughout, tables sorted by tag and padded to four bytes.

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
    /// A CBDT strike of color bitmaps, as Noto Color Emoji ships its
    /// emoji: its pixels to the em, and each glyph's bitmap.
    pub cbdt: Option<(u8, Vec<Bitmap>)>,
    /// GSUB's lookups, each under its feature, applied in this order, for
    /// the scripts `DFLT` and `arab`.
    pub gsub: Vec<([u8; 4], Lookup)>,
}

/// A GSUB lookup.
#[derive(Clone)]
pub enum Lookup {
    /// Each glyph and the glyph it becomes.
    Single(Vec<(u16, u16)>),
    /// Each sequence of glyphs and the ligature it becomes.
    Ligature(Vec<(Vec<u16>, u16)>),
}

/// A CBDT glyph: its glyph, the bitmap's size, its left bearing, its top
/// above the baseline, its advance, and its pixels, straight RGBA.
#[derive(Clone)]
pub struct Bitmap {
    pub glyph: u16,
    pub width: u8,
    pub height: u8,
    pub left: i8,
    pub top: i8,
    pub advance: u8,
    pub rgba: Vec<u8>,
}

/// CRC-32, for PNG's chunks.
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &b| {
        (0..8).fold(crc ^ u32::from(b), |c, _| {
            if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            }
        })
    })
}

/// `rgba`, `w` by `h`, as a PNG of 8-bit RGBA.
pub fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks((w * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunk = |out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = tag.to_vec();
        body.extend_from_slice(data);
        out.extend(&body);
        out.extend(crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(
        &mut out,
        b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6),
    );
    chunk(&mut out, b"IEND", &[]);
    out
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

/// A coverage table of format 1 over `glyphs`, sorted.
fn coverage(glyphs: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    p16(&mut out, 1);
    p16(&mut out, glyphs.len() as u16);
    for &g in glyphs {
        p16(&mut out, g);
    }
    out
}

/// A lookup's one subtable.
fn subtable(lookup: &Lookup) -> Vec<u8> {
    let mut out = Vec::new();
    match lookup {
        Lookup::Single(pairs) => {
            let mut pairs = pairs.clone();
            pairs.sort();
            let from: Vec<u16> = pairs.iter().map(|p| p.0).collect();
            p16(&mut out, 2);
            p16(&mut out, 6 + 2 * pairs.len() as u16);
            p16(&mut out, pairs.len() as u16);
            for &(_, to) in &pairs {
                p16(&mut out, to);
            }
            out.extend(coverage(&from));
        }
        Lookup::Ligature(ligs) => {
            let mut firsts: Vec<u16> = ligs.iter().map(|(s, _)| s[0]).collect();
            firsts.sort();
            firsts.dedup();
            let sets: Vec<Vec<u8>> = firsts
                .iter()
                .map(|&f| {
                    let mine: Vec<&(Vec<u16>, u16)> =
                        ligs.iter().filter(|(s, _)| s[0] == f).collect();
                    let mut set = Vec::new();
                    p16(&mut set, mine.len() as u16);
                    let mut body = Vec::new();
                    let head = 2 + 2 * mine.len();
                    for (seq, lig) in mine {
                        p16(&mut set, (head + body.len()) as u16);
                        p16(&mut body, *lig);
                        p16(&mut body, seq.len() as u16);
                        for &g in &seq[1..] {
                            p16(&mut body, g);
                        }
                    }
                    set.extend(body);
                    set
                })
                .collect();
            let head = 6 + 2 * sets.len();
            let cov = coverage(&firsts);
            p16(&mut out, 1);
            p16(&mut out, head as u16);
            p16(&mut out, sets.len() as u16);
            let mut at = head + cov.len();
            for set in &sets {
                p16(&mut out, at as u16);
                at += set.len();
            }
            out.extend(cov);
            for set in sets {
                out.extend(set);
            }
        }
    }
    out
}

/// GSUB: each lookup under a feature of its own, every feature in the
/// default language system of `DFLT` and `arab`.
fn gsub(lookups: &[([u8; 4], Lookup)]) -> Vec<u8> {
    let n = lookups.len() as u16;
    // the script list: two scripts, each with only a default LangSys
    let mut langsys = Vec::new();
    p16(&mut langsys, 0);
    p16(&mut langsys, 0xffff);
    p16(&mut langsys, n);
    for i in 0..n {
        p16(&mut langsys, i);
    }
    let mut script = Vec::new();
    p16(&mut script, 4);
    p16(&mut script, 0);
    script.extend(&langsys);
    let mut scripts = Vec::new();
    p16(&mut scripts, 2);
    for (i, tag) in [b"DFLT", b"arab"].iter().enumerate() {
        scripts.extend(*tag);
        p16(&mut scripts, (2 + 12 + i * script.len()) as u16);
    }
    scripts.extend(&script);
    scripts.extend(&script);
    // the feature list, in the lookups' order, each naming its lookup
    let mut features = Vec::new();
    p16(&mut features, n);
    for (i, (tag, _)) in lookups.iter().enumerate() {
        features.extend(tag);
        p16(&mut features, 2 + 6 * n + 6 * i as u16);
    }
    for i in 0..n {
        p16(&mut features, 0);
        p16(&mut features, 1);
        p16(&mut features, i);
    }
    // the lookup list
    let mut list = Vec::new();
    p16(&mut list, n);
    let bodies: Vec<Vec<u8>> = lookups
        .iter()
        .map(|(_, l)| {
            let kind = match l {
                Lookup::Single(_) => 1,
                Lookup::Ligature(_) => 4,
            };
            let mut body = Vec::new();
            for v in [kind, 0, 1, 8] {
                p16(&mut body, v);
            }
            body.extend(subtable(l));
            body
        })
        .collect();
    let mut at = 2 + 2 * bodies.len();
    for b in &bodies {
        p16(&mut list, at as u16);
        at += b.len();
    }
    for b in bodies {
        list.extend(b);
    }
    let mut out = Vec::new();
    p16(&mut out, 1);
    p16(&mut out, 0);
    p16(&mut out, 10);
    p16(&mut out, (10 + scripts.len()) as u16);
    p16(&mut out, (10 + scripts.len() + features.len()) as u16);
    out.extend(scripts);
    out.extend(features);
    out.extend(list);
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

        if let Some((ppem, bitmaps)) = &self.cbdt {
            let first = bitmaps.iter().map(|b| b.glyph).min().unwrap_or(0);
            let last = bitmaps.iter().map(|b| b.glyph).max().unwrap_or(0);
            // CBDT: its version, then each glyph in format 17, small
            // metrics and a PNG
            let mut cbdt = Vec::new();
            p16(&mut cbdt, 3);
            p16(&mut cbdt, 0);
            let mut offsets = Vec::new();
            for g in first..=last {
                offsets.push(cbdt.len() as u32);
                if let Some(b) = bitmaps.iter().find(|b| b.glyph == g) {
                    cbdt.extend([b.height, b.width, b.left as u8, b.top as u8, b.advance]);
                    let data = png(u32::from(b.width), u32::from(b.height), &b.rgba);
                    p32(&mut cbdt, data.len() as u32);
                    cbdt.extend(data);
                }
            }
            offsets.push(cbdt.len() as u32);
            // CBLC: one size, one index subtable of format 1 over the
            // glyphs first to last, its offsets from the start of CBDT
            let mut sub = Vec::new();
            p16(&mut sub, 1);
            p16(&mut sub, 17);
            p32(&mut sub, 0);
            for o in &offsets {
                p32(&mut sub, *o);
            }
            let mut cblc = Vec::new();
            p16(&mut cblc, 3);
            p16(&mut cblc, 0);
            p32(&mut cblc, 1);
            let array_at = 8 + 48;
            p32(&mut cblc, array_at);
            p32(&mut cblc, 8 + sub.len() as u32);
            p32(&mut cblc, 1);
            p32(&mut cblc, 0);
            for _ in 0..2 {
                // line metrics: ascender, descender, widthMax, then 9 more
                cblc.extend([*ppem, 0, *ppem, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
            }
            p16(&mut cblc, first);
            p16(&mut cblc, last);
            cblc.extend([*ppem, *ppem, 32, 1]);
            p16(&mut cblc, first);
            p16(&mut cblc, last);
            p32(&mut cblc, 8);
            cblc.extend(sub);
            tables.push((*b"CBDT", cbdt));
            tables.push((*b"CBLC", cblc));
        }

        if !self.gsub.is_empty() {
            tables.push((*b"GSUB", gsub(&self.gsub)));
        }

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
        cbdt: None,
        gsub: Vec::new(),
    }
}

/// `Test Emoji`: one CBDT glyph at 40 pixels to the em for U+1F525, a 36
/// pixel square of an opaque red left half beside a clear right half,
/// standing 30 pixels above the baseline, as Noto Color Emoji's 136 by
/// 128 bitmaps at 109 stand.
pub fn test_emoji() -> Font {
    let mut rgba = Vec::new();
    for _y in 0..36 {
        for x in 0..36 {
            rgba.extend(if x < 18 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            });
        }
    }
    Font {
        family: "Test Emoji".to_owned(),
        upem: 1000,
        ascent: 800,
        descent: -200,
        glyphs: vec![Glyph::empty(1200), Glyph::empty(1200)],
        cmap: vec![(0x1f525, 1)],
        colr: Vec::new(),
        palette: Vec::new(),
        cbdt: Some((
            40,
            vec![Bitmap {
                glyph: 1,
                width: 36,
                height: 36,
                left: 0,
                top: 30,
                advance: 48,
                rgba,
            }],
        )),
        gsub: Vec::new(),
    }
}

/// The family emoji, man ZWJ woman ZWJ girl.
pub const FAMILY: [char; 5] = [
    '\u{1f468}',
    '\u{200d}',
    '\u{1f469}',
    '\u{200d}',
    '\u{1f467}',
];
/// The flag of the United States.
pub const FLAG: [char; 2] = ['\u{1f1fa}', '\u{1f1f8}'];

/// `Test Mono` with shaping: the family emoji and the flag of the United
/// States as color ligatures under `ccmp`; beh (U+0628) with its four
/// forms under `isol`, `fina`, `medi` and `init`; lam (U+0644) and alef
/// (U+0627) with theirs, and lam-alef as a ligature of their joined forms
/// under `rlig`; and `e` with U+00E9, which `e` U+0301 composes to. Each
/// form is a bar of its own extent, so each draws apart.
pub fn test_shaping() -> Font {
    let mut f = test_mono();
    f.family = "Test Shaping".to_owned();
    let bar = |x0, x1| Glyph::rect(600, x0, 0, x1, 500);
    let at = |f: &mut Font, g: Glyph| {
        f.glyphs.push(g);
        (f.glyphs.len() - 1) as u16
    };
    let man = at(&mut f, Glyph::rect(1200, 100, 0, 1100, 300));
    let woman = at(&mut f, Glyph::rect(1200, 100, 300, 1100, 600));
    let girl = at(&mut f, Glyph::rect(1200, 100, 600, 1100, 700));
    let zwj = at(&mut f, Glyph::empty(0));
    let family = at(&mut f, Glyph::rect(1200, 100, -100, 1100, 700));
    let ri_u = at(&mut f, Glyph::rect(1200, 100, 0, 500, 700));
    let ri_s = at(&mut f, Glyph::rect(1200, 600, 0, 1100, 700));
    let flag = at(&mut f, Glyph::rect(1200, 100, 0, 1100, 700));
    let vs16 = at(&mut f, Glyph::empty(0));
    let beh = [bar(100, 500), bar(300, 600), bar(0, 600), bar(0, 300)].map(|g| at(&mut f, g));
    let lam = [bar(200, 300), bar(200, 600), bar(0, 600), bar(0, 300)].map(|g| at(&mut f, g));
    let alef = [bar(250, 350), bar(0, 350)].map(|g| at(&mut f, g));
    let lam_alef = [bar(100, 450), bar(0, 450)].map(|g| at(&mut f, g));
    let e = at(&mut f, Glyph::rect(600, 100, 0, 500, 400));
    let acute = at(&mut f, Glyph::rect(0, -400, 500, -200, 650));
    let e_acute = at(&mut f, Glyph::rect(600, 100, 0, 500, 650));
    f.cmap.extend([
        (0x1f468, man),
        (0x1f469, woman),
        (0x1f467, girl),
        (0x200d, zwj),
        (0xfe0f, vs16),
        (0x1f1fa, ri_u),
        (0x1f1f8, ri_s),
        (0x628, beh[0]),
        (0x644, lam[0]),
        (0x627, alef[0]),
        (0x65, e),
        (0x301, acute),
        (0xe9, e_acute),
    ]);
    // the family and the flag are color glyphs: a red square under a
    // half-transparent blue one, and the blue one alone
    f.colr.push((family, vec![(6, 0), (7, 1)]));
    f.colr.push((flag, vec![(7, 1)]));
    f.colr.sort();
    let [_, b_init, b_medi, b_fina] = beh;
    let [_, l_init, l_medi, l_fina] = lam;
    f.gsub = vec![
        (
            *b"ccmp",
            Lookup::Ligature(vec![
                (vec![man, zwj, woman, zwj, girl], family),
                (vec![ri_u, ri_s], flag),
            ]),
        ),
        (*b"isol", Lookup::Single(vec![])),
        (
            *b"fina",
            Lookup::Single(vec![(beh[0], b_fina), (lam[0], l_fina), (alef[0], alef[1])]),
        ),
        (
            *b"medi",
            Lookup::Single(vec![(beh[0], b_medi), (lam[0], l_medi)]),
        ),
        (
            *b"init",
            Lookup::Single(vec![(beh[0], b_init), (lam[0], l_init)]),
        ),
        (
            *b"rlig",
            Lookup::Ligature(vec![
                (vec![l_init, alef[1]], lam_alef[0]),
                (vec![l_medi, alef[1]], lam_alef[1]),
            ]),
        ),
    ];
    f
}
