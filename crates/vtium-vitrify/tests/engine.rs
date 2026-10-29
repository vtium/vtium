//! The helper's engine (ADR-1301 D2) on each source form and decoder: a
//! frit's sources found as the helper finds them, in a kbd tree, font
//! directories and vconsole files made here, parsed, and drawn at a cell
//! as the glyphs the console receives.

#[path = "common/ttf.rs"]
mod ttf;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vtium_font::set::{PixelFormat, Set, SetFace};
use vtium_font::{Cell, Face, Format, Psf, Sha256, Style, Width};
use vtium_vitrify::serve::{Engine, FritText, Paths};

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
            .join("vtium-vitrify-engine")
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

#[test]
fn a_font_named_by_family_resolves_and_a_missing_one_is_left_out() {
    let w = World::new("family");
    let font = fs::read(w.path("kbd/consolefonts/ter-v16n.psf.gz")).expect("the font");
    let sha = vtium_font::sha256_hex(&Sha256::digest(&font));
    let psf = Psf::parse(&font).expect("the font parses");
    // the family matches the kbd font with case and blanks ignored
    let set = w.drawn(
        &format!("source=font:Ter-V16N\nsha256={sha}\n"),
        cell(8, 16),
    );
    draws_as(set, &psf, &chars());
    // a family not on this machine is left out, and the next source gives
    // the glyphs
    let set = w.drawn(
        "source=font:Nonesuch Mono\npackage=nonesuch-fonts\nsource=kbd:ter-v16n\n",
        cell(8, 16),
    );
    draws_as(set, &psf, &chars());
    // bytes other than the frit's sha256= are taken as missing
    let why = w
        .engine(&format!("source=kbd:ter-v16n\nsha256={}\n", "0".repeat(64)))
        .err()
        .expect("refused");
    assert!(why.contains("the font here has SHA-256"), "{why}");
    // with every source missing, the frit fails, naming each and the
    // package that installs it
    let why = w
        .engine("source=font:Nonesuch Mono\npackage=nonesuch-fonts\nsource=kbd:also-missing\n")
        .err()
        .expect("refused");
    assert!(
        why.contains("no font of family Nonesuch Mono")
            && why.contains("package nonesuch-fonts installs it")
            && why.contains("source=kbd:also-missing"),
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

#[test]
fn a_fallback_fills_what_the_first_source_lacks_on_the_bottom_rows() {
    let w = World::new("fallback");
    let set = w.drawn(
        "source=kbd:ter-v16n\nsource=builtin:unscii-8\ncover=U+2580-U+259F\n",
        cell(8, 16),
    );
    let face = SetFace::new(set, 16).expect("a face");
    // U+2588 is the first source's; U+2580 comes from Unscii-8, 8x8 into
    // 8x16 at factor 1, on the bottom rows; 'a' is outside cover=
    assert!(face.has_glyph('█') && face.has_glyph('▀') && !face.has_glyph('a'));
    let unscii = vtium_font::set::builtin("unscii-8").expect("unscii-8");
    let theirs = SetFace::new(unscii, 8)
        .expect("a face")
        .draw_unscaled(&['▀'], Style::default())
        .expect("drawn");
    let ours = face.draw_unscaled(&['▀'], Style::default()).expect("drawn");
    assert_eq!((ours.width(), ours.height()), (8, 16));
    assert!(
        ours.pixels()[..8 * 8].iter().all(|&p| p == 0),
        "the top half is empty"
    );
    assert_eq!(&ours.pixels()[8 * 8..], theirs.pixels());
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
    // a link out of it is not, by kbd name or by family, and its hash is not told
    let sha = vtium_font::sha256_hex(&Sha256::digest(&font));
    for source in ["kbd:out", "font:OUT"] {
        let why = w
            .engine(&format!("source={source}\nsha256={}\n", "0".repeat(64)))
            .err()
            .expect("refused");
        assert!(why.contains("leaves"), "{source}: {why}");
        assert!(!why.contains(&sha), "{source}: {why}");
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
    // B from unifont's 8x16 at factor 1, on the cell's bottom row and
    // centred across: its lowest ink, unifont's row 13, on row 17
    let b = face.draw(&['B'], Style::default()).expect("B");
    assert_eq!(b.format(), Format::Mask);
    let inked: Vec<usize> = b
        .pixels()
        .chunks(12)
        .enumerate()
        .filter(|(_, row)| row.iter().any(|&p| p != 0))
        .map(|(y, _)| y)
        .collect();
    assert_eq!(inked.last(), Some(&17), "{inked:?}");
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
    assert_eq!(seen, 3, "minspec-8, minspec-16 and proof");
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
