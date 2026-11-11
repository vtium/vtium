//! The helper's engine (ADR-1301 D2) on each source form and decoder: a
//! frit's sources found as the helper finds them, in a kbd tree, font
//! directories and vconsole files made here, parsed, and drawn at a cell
//! as the glyphs the console receives.

use vtium_testfont::ttf;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vtium_config::serve::{Engine, FritText, OwnCell, Paths};
use vtium_font::set::{PixelFormat, Set, SetFace};
use vtium_font::{Cell, Face, Format, Psf, Style, Width};

/// A PSF2 font, `width` by `height`, glyph `g` drawn as its number's bits
/// in every row, its table mapping `table`'s codepoints.
fn psf2(width: u32, height: u32, count: u32, table: Option<&[(u32, char)]>) -> Vec<u8> {
    let row = width.div_ceil(8);
    let mut f = Vec::new();
    for v in [
        0x864a_b572u32,
        0,
        32,
        u32::from(table.is_some()),
        count,
        row * height,
        height,
        width,
    ] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    for g in 0..count {
        for y in 0..height {
            for b in 0..row {
                f.push(((g * 7 + y * 3 + b) % 256) as u8);
            }
        }
    }
    if let Some(table) = table {
        for g in 0..count {
            for &(tg, c) in table {
                if tg == g {
                    let mut buf = [0; 4];
                    f.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            f.push(0xff);
        }
    }
    f
}

/// A PSF1 font of 256 8x`height` glyphs with no table.
fn psf1(height: u8) -> Vec<u8> {
    let mut f = vec![0x36, 0x04, 0, height];
    for g in 0..256u32 {
        for y in 0..u32::from(height) {
            f.push(((g + y) % 256) as u8);
        }
    }
    f
}

/// CRC-32, for the gzip trailer.
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

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    out.extend(miniz_oxide::deflate::compress_to_vec(bytes, 6));
    out.extend_from_slice(&crc32(bytes).to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out
}

/// The table of the test fonts: ASCII letters and a few more.
fn table() -> Vec<(u32, char)> {
    let mut t: Vec<(u32, char)> = ('A'..='Z').zip(1u32..).map(|(c, g)| (g, c)).collect();
    t.push((1, 'À'));
    t.push((40, '─'));
    t.push((41, '█'));
    t
}

fn cell(width: u32, height: u32) -> Cell {
    Cell { width, height }
}

/// A test's directory: a kbd tree holding `ter-v16n` and `def.uni`, font
/// directories and vconsole files.
struct World {
    root: PathBuf,
}

impl World {
    fn new(test: &str) -> World {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("vtium-config-engine")
            .join(test);
        let _ = fs::remove_dir_all(&root);
        for d in ["kbd/consolefonts", "kbd/unimaps", "fonts"] {
            fs::create_dir_all(root.join(d)).expect("mkdir");
        }
        fs::write(
            root.join("kbd/consolefonts/ter-v16n.psf.gz"),
            gzip(&psf2(8, 16, 64, Some(&table()))),
        )
        .expect("font");
        fs::write(
            root.join("kbd/unimaps/def.uni"),
            "0x41 U+0041\n0x42-0x43 U+0042-U+0043\n",
        )
        .expect("def.uni");
        World { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn paths(&self) -> Paths {
        Paths {
            kbd_dirs: vec![self.path("kbd")],
            font_dirs: vec![self.path("fonts")],
            vconsole_conf: self.path("vconsole.conf"),
            kernel_cmdline: self.path("cmdline"),
        }
    }

    /// The engine for the frit `text`, or why it has none.
    fn engine(&self, text: &str) -> Result<Engine, String> {
        let frit = FritText {
            name: "t".to_owned(),
            text: text.to_owned(),
        };
        Engine::load(&frit, &self.paths(), None)
    }

    /// Every glyph the frit `text` gives at `at`, as the console reads it.
    fn drawn(&self, text: &str, at: Cell) -> Arc<Set> {
        let engine = self.engine(text).expect("the frit loads");
        let bytes = engine
            .draw(at, engine.coverage())
            .expect("draws")
            .expect("some glyph");
        Arc::new(Set::from_bytes(bytes).expect("the set reads"))
    }

    /// The outline fixture installed under `fonts/` as a family's regular
    /// face.
    fn test_mono(&self) {
        fs::create_dir_all(self.path("fonts/test")).expect("mkdir");
        fs::write(
            self.path("fonts/test/TestMono-Regular.ttf"),
            ttf::test_mono().build(),
        )
        .expect("the font");
    }
}

/// Every codepoint drawn from `set` at its cell equals `psf`'s own draw.
fn draws_as(set: Arc<Set>, psf: &Psf, chars: &[char]) {
    let (w, h) = psf.glyph_size();
    let face = SetFace::new(set, h).expect("a strike");
    assert_eq!((face.cell().width, face.cell().height), (w, h));
    for &c in chars {
        let ours = face
            .draw_unscaled(&[c], Style::default())
            .unwrap_or_else(|| panic!("{c:?} is drawn"));
        let theirs = psf
            .render(psf.glyph_index(c).expect("in the font"), Style::default())
            .expect("draws");
        assert_eq!(ours.pixels(), theirs.pixels(), "{c:?}");
    }
}

fn chars() -> Vec<char> {
    table().iter().map(|&(_, c)| c).collect()
}

#[test]
fn a_kbd_font_draws_as_the_font_and_the_same_bytes_each_time() {
    let w = World::new("kbd");
    let set = w.drawn("source=kbd:ter-v16n\n", cell(8, 16));
    let font = fs::read(w.path("kbd/consolefonts/ter-v16n.psf.gz")).expect("the font");
    let psf = Psf::parse(&font).expect("the font parses");
    draws_as(set, &psf, &chars());
    // the same frit and sources give the same bytes
    let engine = w.engine("source=kbd:ter-v16n\n").expect("loads");
    let again = || engine.draw(cell(8, 16), engine.coverage()).expect("draws");
    assert_eq!(again(), again());
}

#[test]
fn a_font_without_a_table_takes_its_unimap_then_def_uni() {
    let w = World::new("unimap");
    fs::write(w.path("fonts.psf"), psf1(8)).expect("font");
    fs::write(w.path("kbd/unimaps/mine.uni"), "0x61 U+0061\n").expect("unimap");
    let file = format!("source=file:{}\n", w.path("fonts.psf").display());
    let face = SetFace::new(w.drawn(&file, cell(8, 8)), 8).expect("a face");
    assert!(face.has_glyph('A') && face.has_glyph('C') && !face.has_glyph('a'));
    let mine = format!("{file}unimap=mine\n");
    let face = SetFace::new(w.drawn(&mine, cell(8, 8)), 8).expect("a face");
    assert!(face.has_glyph('a') && !face.has_glyph('A'));
    // with no def.uni, the frit must name a unimap
    fs::remove_file(w.path("kbd/unimaps/def.uni")).expect("rm");
    let why = w.engine(&file).err().expect("refused");
    assert!(why.contains("unimap="), "{why}");
}

/// A 2x3 BDF font of `A`, its left column lit, and `中`, two cells wide
/// with its right column lit.
const BDF: &str = "STARTFONT 2.1\nFONTBOUNDINGBOX 4 3 0 0\nSTARTPROPERTIES 3\n\
    FONT_ASCENT 3\nFONT_DESCENT 0\nCHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 2\n\
    STARTCHAR A\nENCODING 65\nDWIDTH 2 0\nBBX 1 3 0 0\nBITMAP\n80\n80\n80\nENDCHAR\n\
    STARTCHAR uni4E2D\nENCODING 20013\nDWIDTH 4 0\nBBX 1 3 3 0\nBITMAP\n80\n80\n80\nENDCHAR\n\
    ENDFONT\n";

// ADR-1200 D1, D14: a bdf: source draws scaled by the largest whole factor
// that fits the cell, a wide glyph across two cells
#[test]
fn a_bdf_font_draws_scaled_to_the_cell() {
    let w = World::new("bdf");
    fs::write(w.path("fonts/t.bdf.gz"), gzip(BDF.as_bytes())).expect("font");
    let text = format!("source=bdf:{}\n", w.path("fonts/t.bdf.gz").display());
    let face = SetFace::new(w.drawn(&text, cell(4, 7)), 7).expect("a face");
    let a = face.draw_unscaled(&['A'], Style::default()).expect("A");
    let lit = |g: &vtium_font::Glyph| -> Vec<String> {
        g.pixels()
            .chunks(g.width() as usize)
            .map(|r| r.iter().map(|&p| if p > 0 { '#' } else { '.' }).collect())
            .collect()
    };
    // factor 2 in 4x7: 4x6 on the bottom row
    assert_eq!(
        lit(&a),
        ["....", "##..", "##..", "##..", "##..", "##..", "##.."]
    );
    let wide = face.draw_unscaled(&['中'], Style::default()).expect("中");
    assert_eq!(wide.cells(), Width::Wide);
    assert_eq!(lit(&wide)[6], "......##");
}

// ADR-1200 D1: cp-size= picks among a CPI font's three
#[test]
fn a_cpi_font_draws_at_its_cp_size() {
    let w = World::new("cpi");
    let mut cp = vec![0u8; 0x22];
    cp[6] = 1;
    cp[0x1e] = 3;
    for h in [16u8, 14, 8] {
        cp.extend_from_slice(&[h, 8, 0, 0, 0, 1]);
        for g in 0..256u32 {
            cp.extend(std::iter::repeat_n(
                if g == 0x41 { h } else { 0 },
                usize::from(h),
            ));
        }
    }
    fs::write(w.path("kbd/consolefonts/437.cp.gz"), gzip(&cp)).expect("font");
    let why = w.engine("source=kbd:437\n").err().expect("refused");
    assert!(why.contains("cp-size="), "{why}");
    for h in [8u32, 14, 16] {
        let set = w.drawn(&format!("source=kbd:437\ncp-size={h}\n"), cell(8, h));
        let face = SetFace::new(set, h).expect("a face");
        let a = face.draw_unscaled(&['A'], Style::default()).expect("A");
        // def.uni maps glyph 0x41 to A; its rows are the height's bits
        let row: Vec<u8> = (0..8)
            .map(|x| if h & (0x80 >> x) != 0 { 255 } else { 0 })
            .collect();
        assert_eq!(a.pixels(), row.repeat(h as usize), "{h}");
    }
}

#[test]
fn a_font_named_by_family_resolves_and_a_missing_one_is_left_out() {
    let w = World::new("family");
    let font = fs::read(w.path("kbd/consolefonts/ter-v16n.psf.gz")).expect("the font");
    let psf = Psf::parse(&font).expect("the font parses");
    // the family matches the kbd font with case and blanks ignored
    let set = w.drawn("source=font:Ter-V16N\n", cell(8, 16));
    draws_as(set, &psf, &chars());
    // a family not on this machine is left out, and the next source gives
    // the glyphs
    let set = w.drawn(
        "source=font:Nonesuch Mono\nsource=kbd:ter-v16n\n",
        cell(8, 16),
    );
    draws_as(set, &psf, &chars());
    // with every source missing, the frit fails, naming each
    let why = w
        .engine("source=font:Nonesuch Mono\nsource=kbd:also-missing\n")
        .err()
        .expect("refused");
    assert!(
        why.contains("no font of family Nonesuch Mono") && why.contains("source=kbd:also-missing"),
        "{why}"
    );
}

#[test]
fn a_kbd_font_in_each_compression_draws_the_same_glyphs() {
    let w = World::new("compressions");
    let plain = psf2(8, 16, 64, Some(&table()));
    let psf = Psf::parse(&plain).expect("the font");
    let mut bz = Vec::new();
    bzip2::read::BzEncoder::new(plain.as_slice(), bzip2::Compression::best())
        .read_to_end(&mut bz)
        .expect("bzip2");
    let mut xz = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default())
        .expect("an xz writer");
    xz.write_all(&plain).expect("xz");
    let xz = xz.finish().expect("xz");
    let zst = ruzstd::encoding::compress_to_vec(
        plain.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    for (ext, bytes) in [("bz2", bz), ("xz", xz), ("zst", zst)] {
        let font = w.path(&format!("kbd/consolefonts/z.psf.{ext}"));
        fs::write(&font, bytes).expect("font");
        draws_as(w.drawn("source=kbd:z\n", cell(8, 16)), &psf, &chars());
        fs::remove_file(&font).expect("rm");
    }
}

// ADR-1200 D17: a later bitmap source's glyphs stand on the first
// source's baseline. The test ter-v16n's H inks every row, so its baseline
// is the cell's bottom edge, row 16; Unscii-8's is its row 7, so its 8x8
// glyphs at factor 1 start at row 9 of the 8x16 cell; its row 7, past the
// cell's bottom, holds no ink of ▀, so ▀ needs no ink box
#[test]
fn a_fallback_fills_what_the_first_source_lacks_on_its_baseline() {
    let w = World::new("fallback");
    let set = w.drawn(
        "source=kbd:ter-v16n\nsource=builtin:unscii-8\ncover=U+2580-U+259F\n",
        cell(8, 16),
    );
    let face = SetFace::new(set, 16).expect("a face");
    // U+2588 is the first source's; U+2580 comes from Unscii-8; 'a' is
    // outside cover=
    assert!(face.has_glyph('█') && face.has_glyph('▀') && !face.has_glyph('a'));
    let unscii = vtium_font::set::builtin("unscii-8").expect("unscii-8");
    let theirs = SetFace::new(unscii, 8)
        .expect("a face")
        .draw_unscaled(&['▀'], Style::default())
        .expect("drawn");
    let ours = face.draw_unscaled(&['▀'], Style::default()).expect("drawn");
    assert_eq!((ours.width(), ours.height()), (8, 16));
    assert!(
        ours.pixels()[..9 * 8].iter().all(|&p| p == 0),
        "the rows above the glyph are empty"
    );
    assert_eq!(&ours.pixels()[9 * 8..], &theirs.pixels()[..7 * 8]);
    assert!(theirs.pixels()[7 * 8..].iter().all(|&p| p == 0));
}

#[test]
fn vconsole_draws_the_font_vconsole_conf_names() {
    let w = World::new("vconsole");
    fs::write(w.path("vconsole.conf"), "KEYMAP=us\nFONT=ter-v16n\n").expect("conf");
    let font = fs::read(w.path("kbd/consolefonts/ter-v16n.psf.gz")).expect("the font");
    let psf = Psf::parse(&font).expect("the font parses");
    draws_as(w.drawn("source=vconsole\n", cell(8, 16)), &psf, &chars());
}

#[test]
fn a_fifo_is_refused_without_blocking() {
    let w = World::new("fifo");
    let fifo = w.path("pipe.psf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let paths = w.paths();
    let frit = FritText {
        name: "t".to_owned(),
        text: format!("source=file:{}\n", fifo.display()),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(Engine::load(&frit, &paths, None).err()));
    let why = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the engine returns")
        .expect("refused");
    assert!(why.contains("not a regular file"), "{why}");
}

#[test]
fn a_kbd_name_does_not_follow_a_link_out_of_the_data_directory() {
    let w = World::new("links");
    let font = gzip(&psf2(8, 16, 64, Some(&table())));
    fs::write(w.path("secret.psf.gz"), &font).expect("font");
    std::os::unix::fs::symlink(
        w.path("secret.psf.gz"),
        w.path("kbd/consolefonts/out.psf.gz"),
    )
    .expect("link");
    std::os::unix::fs::symlink("ter-v16n.psf.gz", w.path("kbd/consolefonts/in.psf.gz"))
        .expect("link");
    // a link inside the directory is followed
    let face = SetFace::new(w.drawn("source=kbd:in\n", cell(8, 16)), 16).expect("a face");
    assert!(face.has_glyph('A'));
    // a link out of it is not, by kbd name or by family
    for source in ["kbd:out", "font:OUT"] {
        let why = w
            .engine(&format!("source={source}\n"))
            .err()
            .expect("refused");
        assert!(why.contains("leads out of"), "{source}: {why}");
    }
}

/// A PSF2 font of 8x16 glyphs with a table, each glyph its character and
/// the rows it inks, every column of them, as a console font whose
/// baseline is row 12 draws `H` and `a`.
fn psf2_inked(glyphs: &[(char, std::ops::Range<u32>)]) -> Vec<u8> {
    let mut f = Vec::new();
    for v in [0x864a_b572u32, 0, 32, 1, glyphs.len() as u32, 16, 16, 8] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    for (_, rows) in glyphs {
        f.extend((0..16).map(|y| if rows.contains(&y) { 0xffu8 } else { 0 }));
    }
    for (c, _) in glyphs {
        let mut buf = [0; 4];
        f.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        f.push(0xff);
    }
    f
}

// ADR-1200 D17, ADR-1306 D1 (the maintainer, 2026-11-06): every glyph of
// a later bitmap source stands on the first source's baseline, and the ink
// that this raises past the cell goes to its ink box, which the renderer
// draws into the row above. Here the first source, a PSF font, has its
// baseline on row 12, and unifont, the later source, has its baseline on
// its row 14, so each of unifont's glyphs rises 2 rows: ư (U+01B0), whose
// ink starts at unifont's row 4, stays inside its cell, and ẳ (U+1EB3),
// whose hook starts at unifont's row 0, ends on the row of the first
// source's a with the top 2 rows of its hook, whole, in its box above the
// cell. At twice the size, each of those rows is two rows of the cell
#[test]
fn a_later_bitmap_source_stands_on_the_first_sources_baseline() {
    let w = World::new("bitmap-baseline");
    fs::write(
        w.path("kbd/consolefonts/tvn.psf.gz"),
        gzip(&psf2_inked(&[('H', 4..12), ('a', 6..12)])),
    )
    .expect("font");
    let frit = "source=kbd:tvn\nsource=builtin:unifont\n";
    let engine = w.engine(frit).expect("the frit loads");
    let rows_of = |pixels: &[u8], width: usize| -> Vec<usize> {
        (0..pixels.len() / width)
            .filter(|&y| pixels[y * width..(y + 1) * width].iter().any(|&p| p != 0))
            .collect()
    };
    let span = |rows: Vec<usize>| (rows.first().copied(), rows.last().copied());
    let unifont = SetFace::new(vtium_font::set::builtin("unifont").expect("unifont"), 16)
        .expect("a face")
        .draw_unscaled(&['ẳ'], Style::default())
        .expect("unifont's ẳ");
    for k in [1usize, 2] {
        let at = cell(8 * k as u32, 16 * k as u32);
        let bytes = engine
            .draw(at, &[0x61, 0x1b0, 0x1eb3])
            .expect("draws")
            .expect("the glyphs");
        let face = SetFace::new(
            Arc::new(Set::from_bytes(bytes).expect("the set reads")),
            at.height,
        )
        .expect("a face");
        let drawn = |c: char| face.draw(&[c], Style::default()).expect("drawn");
        let (rows, last) = (|r: usize| r * k, |r: usize| r * k + k - 1);
        let cw = 8 * k;
        let a = drawn('a');
        assert_eq!(
            span(rows_of(a.pixels(), cw)),
            (Some(rows(6)), Some(last(11))),
            "{k}: a"
        );
        let u = drawn('ư');
        assert!(u.ink().is_none(), "{k}: ư inside its cell");
        assert_eq!(
            span(rows_of(u.pixels(), cw)),
            (Some(rows(2)), Some(last(11))),
            "{k}: ư"
        );
        let g = drawn('ẳ');
        assert_eq!(
            span(rows_of(g.pixels(), cw)),
            (Some(0), Some(last(11))),
            "{k}: ẳ on the baseline, cut to its cell"
        );
        let ink = g.ink().expect("ẳ's box");
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (0, -2 * k as i32, cw as u32, 18 * k as u32),
            "{k}: ẳ's box, its cell and the 2 rows above"
        );
        // the box's rows above the cell are unifont's top 2 rows of ẳ, its
        // hook's top, scaled
        let above: Vec<u8> = ink.pixels()[..2 * k * cw].to_vec();
        let want: Vec<u8> = (0..2 * k)
            .flat_map(|y| (0..cw).map(move |x| (y / k, x / k)))
            .map(|(y, x)| unifont.pixels()[y * 8 + x])
            .collect();
        assert!(
            want.iter().any(|&p| p != 0),
            "unifont's ẳ has ink in rows 0 and 1"
        );
        assert_eq!(above, want, "{k}: ẳ's hook whole above the cell");
    }
}

// covers: text.emoji
#[test]
fn an_outline_font_with_color_glyphs_and_a_bitmap_fallback_draws_at_the_cell() {
    let w = World::new("outline");
    w.test_mono();
    let set = w.drawn(
        "source=font:Test Mono\nsource=builtin:unifont\ncover=U+0042\nlicense=OFL-1.1\n",
        cell(12, 20),
    );
    let strikes: Vec<_> = set
        .strikes()
        .map(|s| (s.cell.width, s.cell.height, s.format))
        .collect();
    use PixelFormat::{Bits, R8, Rgba};
    assert_eq!(strikes, [(12, 20, Bits), (12, 20, R8), (12, 20, Rgba)]);
    let face = SetFace::new(set, 20).expect("a face");
    let a = face.draw(&['A'], Style::default()).expect("A");
    assert_eq!(a.format(), Format::Mask);
    assert!(a.pixels().iter().any(|&p| p > 0 && p < 255), "R8 coverage");
    let smile = face
        .draw(&['\u{1f600}'], Style::default())
        .expect("a smile");
    assert_eq!(
        (smile.format(), smile.cells(), smile.width()),
        (Format::Rgba, Width::Wide, 24)
    );
    // B from unifont's 8x16 at factor 1, centred across, with unifont's
    // baseline, its row 14, on Test Mono's, row 16: its lowest ink,
    // unifont's row 13, on row 15
    let b = face.draw(&['B'], Style::default()).expect("B");
    assert_eq!(b.format(), Format::Mask);
    let inked: Vec<usize> = b
        .pixels()
        .chunks(12)
        .enumerate()
        .filter(|(_, row)| row.iter().any(|&p| p != 0))
        .map(|(y, _)| y)
        .collect();
    assert_eq!(inked.last(), Some(&15), "{inked:?}");
    assert!(!face.has_glyph('C'), "unifont covers only B");
}

#[test]
fn color_glyphs_fill_a_bitmap_fonts_cell() {
    let w = World::new("outline-after-bitmap");
    w.test_mono();
    let set = w.drawn(
        "source=kbd:ter-v16n\nsource=font:Test Mono\ncover=U+1F600-U+1F64F\n",
        cell(8, 16),
    );
    let formats: Vec<_> = set
        .strikes()
        .map(|s| (s.cell.width, s.cell.height, s.format))
        .collect();
    assert_eq!(
        formats,
        [(8, 16, PixelFormat::Bits), (8, 16, PixelFormat::Rgba)]
    );
    let face = SetFace::new(set, 16).expect("a face");
    let smile = face
        .draw(&['\u{1f600}'], Style::default())
        .expect("a smile");
    assert_eq!((smile.width(), smile.height()), (16, 16));
    assert!(!face.has_glyph('\u{4e00}'), "outside cover=");
}

/// A shipped frit's text.
fn shipped(frit: &str) -> String {
    let path = format!("{}/frits/{frit}.vtfrit", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(path).expect("the frit")
}

// ADR-1200 D7: vtium's own frits carry no copy of unifont or Unscii-8; the
// console draws what a frit lacks in the built-in font, character by
// character.
#[test]
fn vtiums_own_frits_name_no_built_in_font() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/frits");
    let mut seen = 0;
    for entry in fs::read_dir(dir).expect("the frits") {
        let path = entry.expect("an entry").path();
        let text = fs::read_to_string(&path).expect("a frit");
        seen += 1;
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with("source=builtin:"),
                "{}: {line}",
                path.display()
            );
        }
    }
    assert_eq!(seen, 4, "minspec-8, minspec-16, proof and tiny5");
}

/// Each shipped minspec frit, drawn here from stand-ins of the fonts it
/// names: the outline fixture as `Noto Color Emoji`, its color square at
/// U+23F3, U+2B50 and U+1F600, and as `Symbols Nerd Font Mono`, its bar
/// at U+23FB, U+2B58 and U+E0B0. Each of those codepoints draws from the
/// font the frit takes it from, in the frit's cell.
#[test]
fn the_minspec_frits_take_each_chosen_symbol_from_its_font() {
    for (frit, at) in [("minspec-16", cell(8, 16)), ("minspec-8", cell(8, 8))] {
        let w = World::new(&format!("shipped-{frit}"));
        let mut emoji = ttf::test_mono();
        emoji.family = "Noto Color Emoji".to_owned();
        emoji.cmap = vec![(0x23f3, 5), (0x2b50, 5), (0x1f600, 5)];
        fs::write(w.path("fonts/NotoColorEmoji.ttf"), emoji.build()).expect("font");
        let mut nerd = ttf::test_mono();
        nerd.family = "Symbols Nerd Font Mono".to_owned();
        nerd.cmap = vec![(0x23fb, 2), (0x2b58, 2), (0xe0b0, 2)];
        nerd.colr.clear();
        fs::write(
            w.path("fonts/SymbolsNerdFontMono-Regular.ttf"),
            nerd.build(),
        )
        .expect("font");
        let text = shipped(frit);
        assert!(
            text.lines()
                .any(|l| l == format!("cell={}x{}", at.width, at.height)),
            "{frit} states its cell"
        );
        let face = SetFace::new(w.drawn(&text, at), at.height).expect("a face");
        for c in ['\u{23f3}', '\u{2b50}', '\u{1f600}'] {
            let g = face.draw_unscaled(&[c], Style::default()).expect("drawn");
            assert_eq!(g.format(), Format::Rgba, "{frit} {c:?}");
        }
        for c in ['\u{23fb}', '\u{2b58}', '\u{e0b0}'] {
            let g = face.draw_unscaled(&[c], Style::default()).expect("drawn");
            // the bar's edges are partly covered: an outline, not a bitmap
            assert!(
                g.pixels().iter().any(|&p| p > 0 && p < 255),
                "{frit} {c:?} is not the Nerd Font's"
            );
        }
    }
}

/// The shipped proof frit, drawn from stand-ins of its three fonts, takes
/// Nerd Fonts' private-use symbols but not Font Logos (U+F300-U+F385),
/// which carries no licence grant.
#[test]
fn the_proof_frit_leaves_font_logos_out() {
    let w = World::new("shipped-proof");
    let mut text = ttf::test_mono();
    text.family = "JetBrains Mono".to_owned();
    text.colr.clear();
    fs::write(w.path("fonts/JetBrainsMono-Regular.ttf"), text.build()).expect("font");
    let mut nerd = ttf::test_mono();
    nerd.family = "Symbols Nerd Font Mono".to_owned();
    nerd.cmap = vec![
        (0xe0b0, 2),
        (0xf2ff, 2),
        (0xf300, 2),
        (0xf385, 2),
        (0xf386, 2),
    ];
    nerd.colr.clear();
    fs::write(
        w.path("fonts/SymbolsNerdFontMono-Regular.ttf"),
        nerd.build(),
    )
    .expect("font");
    let face = SetFace::new(w.drawn(&shipped("proof"), cell(8, 16)), 16).expect("a face");
    for (c, held) in [
        (0xe0b0, true),
        (0xf2ff, true),
        (0xf300, false),
        (0xf385, false),
        (0xf386, true),
    ] {
        let c = char::from_u32(c).expect("a char");
        assert_eq!(face.has_glyph(c), held, "{c:?}");
    }
}

/// A BDF font in the shape the published Tiny5Mono.bdf has: a 6x9 cell, 7
/// rows above the baseline, its character set only in its XLFD name, and
/// a CHARS line that leaves out its small capital of ENCODING -1.
const TINY5_LIKE: &str = "STARTFONT 2.1
FONT -Stefan Schmidt Art-Tiny5 Mono-Regular-R-Regular--9-9-75-75-c-80-iso10646-1
SIZE 9 75 75
FONTBOUNDINGBOX 12 14 -6 -3
STARTPROPERTIES 3
FAMILY_NAME \"Tiny5 Mono\"
FONT_ASCENT 7
FONT_DESCENT 2
ENDPROPERTIES
CHARS 2
STARTCHAR U+0041
ENCODING 65
SWIDTH 500 0
DWIDTH 6 0
BBX 5 5 0 0
BITMAP
70
88
F8
88
88
ENDCHAR
STARTCHAR a.sc
ENCODING -1
SWIDTH 500 0
DWIDTH 6 0
BBX 5 4 0 0
BITMAP
70
88
F8
88
ENDCHAR
STARTCHAR U+0067
ENCODING 103
SWIDTH 500 0
DWIDTH 6 0
BBX 4 5 0 -2
BITMAP
70
90
70
10
60
ENDCHAR
ENDFONT
";

// The shipped tiny5 frit states its cell, its licence and one bdf:
// source pinned to a commit and a SHA-256, and reads a BDF file shaped as
// the published Tiny5Mono.bdf is: its own 6x9 cell, A on the baseline
// and g's tail below it, the small capital left out, and no warning.
#[test]
fn the_tiny5_frit_reads_its_bdf_as_published() {
    let w = World::new("shipped-tiny5");
    let text = shipped("tiny5");
    for line in ["name=tiny5", "cell=6x9", "license=OFL-1.1"] {
        assert!(text.lines().any(|l| l == line), "tiny5 has {line}");
    }
    let sources: Vec<&str> = text.lines().filter(|l| l.starts_with("source=")).collect();
    let published = "source=bdf:/usr/local/share/fonts/misc/Tiny5Mono.bdf";
    assert_eq!(sources, [published]);
    assert!(text.contains("7d30f9e887fdb29e0b951e92493822afba5f70a7"));
    assert!(text.contains("33a935687277817534ddcc1decf586d977cee211550f611ba4bb4365e99755d3"));
    assert!(!text.lines().any(|l| l.starts_with("cover=")));
    let bdf = w.path("fonts/Tiny5Mono.bdf");
    fs::write(&bdf, TINY5_LIKE).expect("the font");
    let here = text.replace(published, &format!("source=bdf:{}", bdf.display()));
    let engine = w.engine(&here).expect("the frit loads");
    assert_eq!(engine.warnings(), [] as [String; 0]);
    assert_eq!(engine.coverage(), [0x41, 0x67]);
    assert!(matches!(engine.own_cell(0), Ok(OwnCell::Bitmap(c)) if c == cell(6, 9)));
    let face = SetFace::new(w.drawn(&here, cell(6, 9)), 9).expect("a face");
    let rows = |c| -> Vec<String> {
        let g = face.draw_unscaled(&[c], Style::default()).expect("drawn");
        assert_eq!((g.width(), g.height()), (6, 9));
        g.rows()
            .map(|r| r.iter().map(|&p| if p > 0 { '#' } else { '.' }).collect())
            .collect()
    };
    assert_eq!(
        rows('A'),
        [
            "......", "......", ".###..", "#...#.", "#####.", "#...#.", "#...#.", "......",
            "......"
        ]
    );
    assert_eq!(
        rows('g'),
        [
            "......", "......", "......", "......", ".###..", "#..#..", ".###..", "...#..",
            ".##..."
        ]
    );
}

/// A text font and a color font for the emoji-presentation rule (ADR-1200
/// D21): `Test Text` holds U+2764 HEAVY BLACK HEART and U+231A WATCH as
/// plain bars, and draws the heart with U+FE0F as a ligature of its own, a
/// plain bar too, as a font of black-and-white emoji may; `Test Color`
/// holds both as color squares and maps U+FE0F to an empty glyph, as Noto
/// Color Emoji draws a character and the character with U+FE0F alike.
/// U+2764 has Emoji_Presentation=No and U+231A Emoji_Presentation=Yes.
fn text_and_color(w: &World) {
    let mut text = ttf::test_mono();
    text.family = "Test Text".to_owned();
    text.colr.clear();
    text.glyphs.push(ttf::Glyph::empty(0));
    text.glyphs.push(ttf::Glyph::rect(600, 200, 0, 400, 300));
    let (vs16, ligature) = (
        (text.glyphs.len() - 2) as u16,
        (text.glyphs.len() - 1) as u16,
    );
    text.cmap.extend([(0x2764, 2), (0x231a, 2), (0xfe0f, vs16)]);
    text.gsub = vec![(
        *b"ccmp",
        ttf::Lookup::Ligature(vec![(vec![2, vs16], ligature)]),
    )];
    fs::write(w.path("fonts/TestText-Regular.ttf"), text.build()).expect("font");
    let mut color = ttf::test_mono();
    color.family = "Test Color".to_owned();
    color.glyphs.push(ttf::Glyph::empty(0));
    let vs16 = (color.glyphs.len() - 1) as u16;
    color.cmap = vec![(0x2764, 5), (0x231a, 5), (0xfe0f, vs16)];
    fs::write(w.path("fonts/TestColor-Regular.ttf"), color.build()).expect("font");
}

const TEXT_THEN_COLOR: &str = "source=font:Test Text\nsource=font:Test Color\n";

/// The glyphs `text` draws at a 12x20 cell for the codepoints `want` and the
/// clusters `clusters`, as the console reads them.
fn drawn_for(w: &World, text: &str, want: &[u32], clusters: &[&[char]]) -> SetFace {
    let engine = w.engine(text).expect("the frit loads");
    let keys: Vec<Vec<u32>> = clusters
        .iter()
        .map(|k| k.iter().map(|&c| u32::from(c)).collect())
        .collect();
    let bytes = if keys.is_empty() {
        engine.draw(cell(12, 20), want)
    } else {
        engine.draw_clusters(cell(12, 20), &keys)
    }
    .expect("draws")
    .expect("some glyph");
    SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 20).expect("a face")
}

// ADR-1200 D21, UTS #51 ED-9: U+FE0F asks for the emoji presentation of the
// character before it, so the heart with U+FE0F draws from the first source
// with a color glyph for it, though the text source before it covers the
// heart. The color source's glyph for the heart alone counts.
// covers: text.emoji
#[test]
fn a_character_with_vs16_draws_from_the_first_color_source() {
    let w = World::new("vs16");
    text_and_color(&w);
    let heart = ['\u{2764}', '\u{fe0f}'];
    let face = drawn_for(&w, TEXT_THEN_COLOR, &[], &[&heart]);
    let g = face
        .cluster(&heart, Style::default())
        .expect("the heart with VS16");
    assert_eq!((g.format(), g.cells()), (Format::Rgba, Width::Wide));
}

// ADR-1200 D21, UTS #51 ED-7: U+2764 alone has Emoji_Presentation=No, so it
// takes its text presentation and draws from the first source in the
// frit's order that has it
#[test]
fn a_character_of_text_presentation_alone_keeps_the_frits_order() {
    let w = World::new("heart-alone");
    text_and_color(&w);
    let face = drawn_for(&w, TEXT_THEN_COLOR, &[0x2764], &[]);
    let g = face
        .draw(&['\u{2764}'], Style::default())
        .expect("the heart");
    assert_eq!((g.format(), g.cells()), (Format::Mask, Width::Narrow));
}

// ADR-1200 D21, UTS #51 ED-8: U+FE0E asks for the text presentation, so the
// heart with U+FE0E keeps the frit's order, and the text source draws it
#[test]
fn a_character_with_vs15_keeps_the_frits_order() {
    let w = World::new("vs15");
    text_and_color(&w);
    let heart = ['\u{2764}', '\u{fe0e}'];
    let face = drawn_for(&w, TEXT_THEN_COLOR, &[], &[&heart]);
    let g = face
        .cluster(&heart, Style::default())
        .expect("the heart with VS15");
    assert_eq!((g.format(), g.cells()), (Format::Mask, Width::Narrow));
}

// ADR-1200 D21, UTS #51 ED-6: U+231A has Emoji_Presentation=Yes, so alone it
// draws from the first source with a color glyph for it; with no color
// source in the frit, it falls back to the frit's order
#[test]
fn a_character_of_emoji_presentation_draws_from_a_color_source_where_there_is_one() {
    let w = World::new("watch");
    text_and_color(&w);
    let face = drawn_for(&w, TEXT_THEN_COLOR, &[0x231a], &[]);
    let g = face
        .draw(&['\u{231a}'], Style::default())
        .expect("the watch");
    assert_eq!(g.format(), Format::Rgba);
    let face = drawn_for(&w, "source=font:Test Text\n", &[0x231a], &[]);
    let g = face
        .draw(&['\u{231a}'], Style::default())
        .expect("the watch");
    assert_eq!(g.format(), Format::Mask);
}

/// A Latin font and a CJK font behind it: `Test Latin`, the outline
/// fixture without U+4E00, ascent 800 and descent 200 to the 1000-unit em,
/// and `Test CJK`, whose U+4E00 is a bar 900 units across and 500 high,
/// advancing 1000, under an ascent of 1160 and a descent of 288, as Noto
/// Sans CJK's reach further than DejaVu Sans Mono's.
fn latin_and_cjk(w: &World) {
    let mut latin = ttf::test_mono();
    latin.family = "Test Latin".to_owned();
    latin.colr.clear();
    latin.cmap.retain(|&(c, _)| c != 0x4e00 && c != 0x1f600);
    fs::write(w.path("fonts/TestLatin-Regular.ttf"), latin.build()).expect("font");
    let mut cjk = ttf::test_mono();
    cjk.family = "Test CJK".to_owned();
    cjk.colr.clear();
    (cjk.ascent, cjk.descent) = (1160, -288);
    cjk.cmap = vec![(0x4e00, 4)];
    fs::write(w.path("fonts/TestCJK-Regular.ttf"), cjk.build()).expect("font");
}

const LATIN_THEN_CJK: &str = "source=font:Test Latin\nsource=font:Test CJK\n";

// ADR-1200 D21: the terminal gives U+4E00 two cells, so the helper draws
// it across both, its advance centred in them, and its ink reaches into
// the second cell. Sized by its own ascent and descent, the CJK font's
// 1000-unit advance comes to about 10.7 pixels in a 9x16 cell, under one
// and a half cells, and the glyph still spans both
#[test]
fn an_ideograph_behind_a_latin_source_spans_both_its_cells() {
    let w = World::new("cjk-behind-latin");
    latin_and_cjk(&w);
    let engine = w.engine(LATIN_THEN_CJK).expect("the frit loads");
    let bytes = engine
        .draw(cell(9, 16), &[0x4e00])
        .expect("draws")
        .expect("a glyph");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let g = face
        .draw(&['\u{4e00}'], Style::default())
        .expect("the ideograph");
    let spans_both = |g: &vtium_font::Glyph, what: &str| {
        assert_eq!((g.cells(), g.width()), (Width::Wide, 18), "{what}");
        let inked: Vec<usize> = (0..18)
            .filter(|&x| g.rows().any(|row| row.get(x).is_some_and(|&p| p != 0)))
            .collect();
        assert!(
            inked.first() < Some(&9) && inked.last() >= Some(&9),
            "{what}: ink in both cells: {inked:?}"
        );
    };
    spans_both(&g, "the ideograph");
    // the ideograph with an ideographic variation selector, U+E0100, which
    // the helper shapes as a cluster, is placed the same way
    let ivs = ['\u{4e00}', '\u{e0100}'];
    let key: Vec<u32> = ivs.iter().map(|&c| u32::from(c)).collect();
    let bytes = engine
        .draw_clusters(cell(9, 16), &[key])
        .expect("draws")
        .expect("a glyph");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let g = face
        .cluster(&ivs, Style::default())
        .expect("the ideograph with its selector");
    spans_both(&g, "the ideograph with its selector");
}

// ADR-1200 D17 (the maintainer, 2026-11-06): an outline source behind an
// outline first source draws at the first source's size. Test Latin's
// ascent and descent span 1000 units, so in a 16-pixel cell it draws at
// 15.5 pixels to the em, and U+4E00 from Test CJK, 900 units across and 500
// high, 14 by 7.75 pixels, inks 15 columns and 9 rows with its partly
// covered edges. Sized by its own ascent and descent, 1448 units, it drew
// at 10.7 and inked 11 columns and 6 rows
#[test]
fn a_fallback_outline_font_draws_at_the_first_fonts_size() {
    let w = World::new("cjk-at-latin-size");
    latin_and_cjk(&w);
    let engine = w.engine(LATIN_THEN_CJK).expect("the frit loads");
    let bytes = engine
        .draw(cell(9, 16), &[0x4e00])
        .expect("draws")
        .expect("a glyph");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let g = face
        .draw(&['\u{4e00}'], Style::default())
        .expect("the ideograph");
    let rows: Vec<&[u8]> = g.rows().collect();
    let columns = (0..g.width() as usize)
        .filter(|&x| rows.iter().any(|r| r.get(x).is_some_and(|&p| p != 0)))
        .count();
    let inked_rows = rows.iter().filter(|r| r.iter().any(|&p| p != 0)).count();
    assert_eq!((columns, inked_rows), (15, 9));
}

// ADR-1200 D17 (the maintainer, 2026-11-06): a glyph from an outline source
// behind an outline first source whose ink would pass its cells at the
// first source's size is drawn smaller until it fits. Test Icons' U+E0B0
// is a bar 1200 units across and 1300 high, from 300 below the baseline,
// which at Test Latin's 15.5 pixels to the em spans 18.6 by 20.2 pixels,
// more than its 9x16 cell. Drawn smaller, its ink lies in the cell with
// blank rows above and below it, about as wide as the cell, and no ink
// past the cell; cut at the cell, as before, it inked every row and
// carried an ink box
#[test]
fn a_fallback_glyph_too_big_for_its_cell_is_drawn_smaller_to_fit() {
    let w = World::new("icon-too-big");
    latin_and_cjk(&w);
    let mut icons = ttf::test_mono();
    icons.family = "Test Icons".to_owned();
    icons.colr.clear();
    icons
        .glyphs
        .push(ttf::Glyph::rect(600, -300, -300, 900, 1000));
    icons.cmap = vec![(0xe0b0, (icons.glyphs.len() - 1) as u16)];
    fs::write(w.path("fonts/TestIcons-Regular.ttf"), icons.build()).expect("font");
    let engine = w
        .engine("source=font:Test Latin\nsource=font:Test Icons\n")
        .expect("the frit loads");
    let bytes = engine
        .draw(cell(9, 16), &[0xe0b0])
        .expect("draws")
        .expect("a glyph");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let g = face
        .draw(&['\u{e0b0}'], Style::default())
        .expect("the icon");
    assert_eq!((g.cells(), g.width(), g.height()), (Width::Narrow, 9, 16));
    assert!(g.ink().is_none(), "no ink past the cell");
    let rows: Vec<&[u8]> = g.rows().collect();
    let inked: Vec<usize> = (0..rows.len())
        .filter(|&y| rows[y].iter().any(|&p| p != 0))
        .collect();
    assert!(
        inked.first() > Some(&0) && inked.last() < Some(&15),
        "blank rows above and below: {inked:?}"
    );
    let columns: Vec<usize> = (0..9).filter(|&x| rows.iter().any(|r| r[x] != 0)).collect();
    // the width sets the size, 9 of 18.6 pixels: the bar inks 8 or 9
    // columns as its edges fall on the pixel grid
    assert!(columns.len() >= 8, "drawn as large as fits: {columns:?}");
}

// ADR-1306 D12 and ADR-1200 D17 (the maintainer, 2026-11-06): a fallback
// glyph of a joining script uses the reach and is not drawn smaller, where
// any other fallback glyph that passes its cells still is. Test Arabic's
// beh (U+0628, Joining_Type D) and its U+E0B0 are the same bar, 600 units
// across, from 300 below the baseline to 700 above it; at Test Latin's
// 15.5 pixels to the em in the 9x16 cell, baseline 13, it reaches 4.7
// pixels below the baseline, past the cell's bottom, 3 below. The icon is
// drawn smaller to fit its cell; the beh keeps its size, stretched to its
// cell, and its ink below the cell goes to its box, in the row below
// covers: text.overhang
#[test]
fn a_fallback_joining_letter_uses_the_reach_and_an_icon_is_drawn_smaller() {
    let w = World::new("joining-fallback");
    latin_and_cjk(&w);
    let mut arabic = ttf::test_mono();
    arabic.family = "Test Arabic".to_owned();
    arabic.colr.clear();
    arabic.glyphs.push(ttf::Glyph::rect(600, 0, -300, 600, 700));
    let bar = (arabic.glyphs.len() - 1) as u16;
    arabic.cmap = vec![(0x628, bar), (0xe0b0, bar)];
    fs::write(w.path("fonts/TestArabic-Regular.ttf"), arabic.build()).expect("font");
    let engine = w
        .engine("source=font:Test Latin\nsource=font:Test Arabic\n")
        .expect("the frit loads");
    let bytes = engine
        .draw(cell(9, 16), &[0x628, 0xe0b0])
        .expect("draws")
        .expect("the glyphs");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let inked_rows = |g: &vtium_font::Glyph| -> Vec<usize> {
        let rows: Vec<&[u8]> = g.rows().collect();
        (0..rows.len())
            .filter(|&y| rows[y].iter().any(|&p| p != 0))
            .collect()
    };
    let icon = face
        .draw(&['\u{e0b0}'], Style::default())
        .expect("the icon");
    assert!(
        icon.ink().is_none(),
        "the icon drawn smaller, inside its cell"
    );
    let beh = face.draw(&['\u{628}'], Style::default()).expect("the beh");
    assert_eq!(
        inked_rows(&beh).last(),
        Some(&15),
        "the beh fills its cell to the bottom row"
    );
    let ink = beh.ink().expect("the beh's ink below its cell");
    assert_eq!((ink.left(), ink.top(), ink.width()), (0, 0, 9));
    assert!(ink.height() > 16 && ink.height() <= 32, "{}", ink.height());
    let below = &ink.pixels()[16 * 9..17 * 9];
    assert!(
        below.iter().any(|&p| p != 0),
        "ink in the row below: {below:?}"
    );
}

// ADR-1200 D17: a fallback glyph drawn smaller keeps the first source's
// baseline. A fallback source's ẳ (U+1EB3) has stacked marks that can
// rise past the room above the baseline. Test Viet's ẳ stands on the
// baseline and rises 950 units, 14.7 pixels at Test Latin's 15.5 pixels to
// the em, where the 9x16 cell has 13 rows above its baseline. The helper
// draws it smaller until its ink fits above the baseline, and does not
// move it down to the row's top, so its lowest ink is on the row of đ's,
// in the 9x16 cell and in the 28x54 cell
#[test]
fn a_fallback_glyph_drawn_smaller_keeps_the_baseline() {
    let w = World::new("fallback-baseline");
    let mut latin = ttf::test_mono();
    latin.family = "Test Latin".to_owned();
    latin.colr.clear();
    latin.cmap = vec![(0x20, 1), (0x41, 2), (0x57, 3), (0x111, 2)];
    fs::write(w.path("fonts/TestLatin-Regular.ttf"), latin.build()).expect("font");
    let mut viet = ttf::test_mono();
    viet.family = "Test Viet".to_owned();
    viet.colr.clear();
    viet.glyphs.push(ttf::Glyph::rect(600, 150, 0, 450, 950));
    viet.cmap = vec![(0x1eb3, (viet.glyphs.len() - 1) as u16)];
    fs::write(w.path("fonts/TestViet-Regular.ttf"), viet.build()).expect("font");
    let engine = w
        .engine("source=font:Test Latin\nsource=font:Test Viet\n")
        .expect("the frit loads");
    let lowest = |g: &vtium_font::Glyph| {
        let rows: Vec<&[u8]> = g.rows().collect();
        (0..rows.len())
            .rev()
            .find(|&y| rows[y].iter().any(|&p| p != 0))
    };
    for (at, baseline) in [(cell(9, 16), 12), (cell(28, 54), 42)] {
        let bytes = engine
            .draw(at, &[0x111, 0x1eb3])
            .expect("draws")
            .expect("the glyphs");
        let face = SetFace::new(
            Arc::new(Set::from_bytes(bytes).expect("the set reads")),
            at.height,
        )
        .expect("a face");
        let d = face.draw(&['đ'], Style::default()).expect("đ");
        let a = face.draw(&['ẳ'], Style::default()).expect("ẳ");
        assert!(a.ink().is_none(), "{at:?}: ẳ inside its cell");
        assert_eq!(lowest(&d), Some(baseline), "{at:?}: đ on the baseline");
        assert_eq!(lowest(&a), lowest(&d), "{at:?}: ẳ on đ's baseline");
        let highest = a.rows().position(|r| r.iter().any(|&p| p != 0));
        assert!(
            highest <= Some(1),
            "{at:?}: ẳ as large as fits: {highest:?}"
        );
    }
}

// ADR-1200 D17: every outline source after an outline first source draws on
// the first source's baseline, so Hebrew and Arabic from Noto Sans behind
// DejaVu Sans Mono stand on the Latin letters' baseline. Test Tall has
// Noto's proportions, an ascent of 1100 and a descent of 300, so at Test
// Latin's size its own baseline is the cell's bottom row in both cells.
// Its alef and its beh fit their cells, so the helper draws them at full
// size, with their lowest ink on the row of đ's in the 9x16 cell and in
// the 28x54 cell
#[test]
fn a_fallback_source_draws_on_the_first_sources_baseline() {
    let w = World::new("fallback-own-baseline");
    let mut latin = ttf::test_mono();
    latin.family = "Test Latin".to_owned();
    latin.colr.clear();
    latin.cmap = vec![(0x20, 1), (0x41, 2), (0x57, 3), (0x111, 2)];
    fs::write(w.path("fonts/TestLatin-Regular.ttf"), latin.build()).expect("font");
    let mut tall = ttf::test_mono();
    tall.family = "Test Tall".to_owned();
    tall.colr.clear();
    tall.ascent = 1100;
    tall.descent = -300;
    tall.glyphs.push(ttf::Glyph::rect(600, 100, 0, 500, 500));
    tall.glyphs.push(ttf::Glyph::rect(600, 0, 0, 600, 300));
    let n = tall.glyphs.len() as u16;
    tall.cmap = vec![(0x5d0, n - 2), (0x628, n - 1)];
    fs::write(w.path("fonts/TestTall-Regular.ttf"), tall.build()).expect("font");
    let engine = w
        .engine("source=font:Test Latin\nsource=font:Test Tall\n")
        .expect("the frit loads");
    let lowest = |g: &vtium_font::Glyph| {
        let rows: Vec<&[u8]> = g.rows().collect();
        (0..rows.len())
            .rev()
            .find(|&y| rows[y].iter().any(|&p| p != 0))
    };
    for (at, baseline) in [(cell(9, 16), 12), (cell(28, 54), 42)] {
        let bytes = engine
            .draw(at, &[0x111, 0x5d0, 0x628])
            .expect("draws")
            .expect("the glyphs");
        let face = SetFace::new(
            Arc::new(Set::from_bytes(bytes).expect("the set reads")),
            at.height,
        )
        .expect("a face");
        let d = face.draw(&['đ'], Style::default()).expect("đ");
        assert_eq!(lowest(&d), Some(baseline), "{at:?}: đ on the baseline");
        for c in ['\u{5d0}', '\u{628}'] {
            let g = face.draw(&[c], Style::default()).expect("the letter");
            assert_eq!(lowest(&g), lowest(&d), "{at:?}: {c} on đ's baseline");
        }
    }
}

// ADR-1200 D17: behind a bitmap first source, each outline source is sized
// so its own ascent and descent span the cell's height, not at the size of
// the first outline source after the bitmap one. In the frit ter-v16n,
// Test Latin, Test CJK, U+4E00 comes from Test CJK at its own 10.7 pixels to
// the em in the 8x16 cell, and inks 11 columns and 6 rows; at Test Latin's
// 15.5 it would ink 15 columns and 9 rows
#[test]
fn outline_sources_behind_a_bitmap_first_source_keep_their_own_size() {
    let w = World::new("outline-behind-bitmap");
    latin_and_cjk(&w);
    let engine = w
        .engine("source=kbd:ter-v16n\nsource=font:Test Latin\nsource=font:Test CJK\n")
        .expect("the frit loads");
    let bytes = engine
        .draw(cell(8, 16), &[0x4e00])
        .expect("draws")
        .expect("a glyph");
    let face =
        SetFace::new(Arc::new(Set::from_bytes(bytes).expect("the set reads")), 16).expect("a face");
    let g = face
        .draw(&['\u{4e00}'], Style::default())
        .expect("the ideograph");
    let rows: Vec<&[u8]> = g.rows().collect();
    let columns = (0..g.width() as usize)
        .filter(|&x| rows.iter().any(|r| r.get(x).is_some_and(|&p| p != 0)))
        .count();
    let inked_rows = rows.iter().filter(|r| r.iter().any(|&p| p != 0)).count();
    assert_eq!((columns, inked_rows), (11, 6));
}

// ADR-1200 D21, D22: when no source has a color glyph for the heart with
// U+FE0F and none ligates it, the helper draws no glyph for the cluster:
// a text font's glyph for the heart alone is the heart's own, which the
// console draws for the cluster from its store (D22, step 3), as the first
// source in the frit's order gives it. Kept as a cluster, the same glyph
// would be placed by the helper and move tier 2's emoji-mono screen's
// heart by a pixel
#[test]
fn a_character_with_vs16_and_no_color_source_draws_as_its_character() {
    let w = World::new("vs16-no-color");
    let mut plain = ttf::test_mono();
    plain.family = "Test Plain".to_owned();
    plain.colr.clear();
    plain.cmap.push((0x2764, 2));
    fs::write(w.path("fonts/TestPlain-Regular.ttf"), plain.build()).expect("font");
    let engine = w
        .engine("source=font:Test Plain\n")
        .expect("the frit loads");
    let heart: Vec<u32> = vec![0x2764, 0xfe0f];
    assert_eq!(
        engine.draw_clusters(cell(12, 20), &[heart]).expect("draws"),
        None,
        "no glyph for the cluster"
    );
    let face = drawn_for(&w, "source=font:Test Plain\n", &[0x2764], &[]);
    let g = face
        .draw(&['\u{2764}'], Style::default())
        .expect("the heart");
    assert_eq!(g.format(), Format::Mask);
}
