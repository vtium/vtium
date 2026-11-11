//! Superscript and subscript asks (ADR-1202 D2, D5, D11, D15): an ask
//! names a script, and the helper answers it with strikes of that script,
//! each glyph made small by the source that draws the character's normal
//! glyph. An outline glyph is rasterised at two thirds of its size on the
//! script baseline, never scaled from its full-size raster; a color glyph
//! is scaled once from its source to two thirds of its cells, against the
//! cell's top or bottom; a bitmap glyph is made small by vtium-font's
//! script rule, as the console makes its own bitmap fonts' small.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use vtium_config::frit::Hinting;
use vtium_config::outline::{OutlineRequest, baseline, rasterise, size};
use vtium_config::serve::{
    ASK, Engine, FILL, FRAME_DONE, FRAME_GLYPHS, FritText, Paths, ServeRequest, script_baseline,
    serve,
};
use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Format, Glyph, SCRIPT_SCALE, Script, Style, Variant};
use vtium_testfont::ttf;

/// A directory of a test's own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("vtium-script-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's directory");
        Scratch(dir)
    }

    /// Writes `bytes` to `name` in the directory: its path.
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("a file");
        path.display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

fn engine(text: &str) -> Engine {
    Engine::load(&frit(text), &Paths::default(), None).expect("the frit loads")
}

fn variant(script: Script) -> Variant {
    Variant {
        style: Style::default(),
        script,
    }
}

/// `c`'s glyph in `variant` that `engine` draws at `cell`, checked to
/// come in strikes of the variant's script alone.
fn drawn(engine: &Engine, cell: Cell, v: Variant, c: char) -> Glyph {
    let bytes = engine
        .draw_variant(cell, v, &[u32::from(c)], (&[], false))
        .expect("draws")
        .expect("a glyph");
    let set = Arc::new(Set::from_bytes(bytes).expect("a set"));
    let scripts: Vec<Script> = set.strikes().map(|s| s.script).collect();
    assert!(
        !scripts.is_empty() && scripts.iter().all(|&s| s == v.script),
        "{scripts:?}"
    );
    let face = SetFace::new(set, cell.height).expect("a face");
    face.draw_variant(&[c], v).expect("the glyph")
}

/// The rows a glyph's ink covers, by its alpha or coverage: the first
/// and one past the last.
fn ink_rows(g: &Glyph) -> (u32, u32) {
    let bpp = g.format().bytes_per_pixel() as usize;
    let inked: Vec<u32> = g
        .rows()
        .zip(0u32..)
        .filter(|(row, _)| row.chunks(bpp).any(|px| px[bpp - 1] != 0))
        .map(|(_, y)| y)
        .collect();
    (inked[0], inked[inked.len() - 1] + 1)
}

fn dejavu() -> String {
    format!(
        "{}/tests/fixtures/DejaVuSansMono-ascii.ttf",
        env!("CARGO_MANIFEST_DIR")
    )
}

const CELL: Cell = Cell {
    width: 12,
    height: 24,
};

// ADR-1202 D2: the helper rasterises an outline glyph at two thirds of
// the pixels to the em its normal glyph is drawn at, on the script
// baseline, so its edges fall on the small size's grid; it is exactly the
// glyph the rasteriser draws at that size there, and not the normal
// glyph's full-size raster made small
// covers: sgr.script
#[test]
fn an_outline_script_glyph_is_rasterised_at_two_thirds_of_its_size() {
    let path = dejavu();
    let font: Arc<[u8]> = std::fs::read(&path).expect("the fixture").into();
    let e = engine(&format!("source=outline:{path}\n"));
    let normal = drawn(&e, CELL, variant(Script::Baseline), 'a');
    let full = size(&font, "", CELL.height).expect("a size");
    let base = baseline(&font, "", CELL.height, None, Hinting::Light).expect("a baseline");
    for script in [Script::Super, Script::Sub] {
        let got = drawn(&e, CELL, variant(script), 'a');
        let (num, den) = SCRIPT_SCALE;
        let small = full * num as f32 / den as f32;
        let at = script_baseline(script, base, CELL.height);
        let want = rasterise(&OutlineRequest {
            font: Arc::clone(&font),
            family: String::new(),
            cells: vec![(CELL.width, CELL.height)],
            cover: vec![(0x61, 0x61)],
            clusters: Vec::new(),
            baselines: vec![at],
            sizes: vec![small.to_bits()],
            reaches: vec![(0, 0)],
            hinting: Hinting::Light,
            fit: false,
            synth: Style::default(),
            script,
        })
        .expect("rasterised");
        let strike = &want.strikes[0];
        let (_, g) = strike.map[0];
        assert_eq!(
            got.pixels(),
            &strike.glyphs[g as usize].pixels[..],
            "{script:?}: the glyph rasterised small"
        );
        let scaled = normal.scripted(script).expect("scaled");
        assert_ne!(
            got.pixels(),
            scaled.pixels(),
            "{script:?}: not the raster made small"
        );
        // two thirds as tall as the normal glyph's ink, a row either way
        let (top, bottom) = ink_rows(&got);
        let (ntop, nbottom) = ink_rows(&normal);
        let tall = (nbottom - ntop) * num / den;
        assert!(
            (bottom - top).abs_diff(tall) <= 1,
            "{script:?}: {top}..{bottom}"
        );
        match script {
            // a superscript's x-height letter sits well above the normal
            // one, and a subscript's below it
            Script::Super => assert!(bottom < nbottom - 3, "{bottom} {nbottom}"),
            _ => assert!(bottom > nbottom, "{bottom} {nbottom}"),
        }
    }
}

// ADR-1202 D1, D2: the script baseline shrinks the rows above and below
// the normal baseline to two thirds, against the band's top or bottom
#[test]
fn the_script_baseline_keeps_two_thirds_of_each_side() {
    // a 24-row band with the baseline under row 19: 19 rows above it and
    // 5 below; two thirds of 19 is 12.67, of 5 is 3.33
    assert_eq!(script_baseline(Script::Super, 19, 24), 13);
    assert_eq!(script_baseline(Script::Sub, 19, 24), 21);
    assert_eq!(script_baseline(Script::Baseline, 19, 24), 19);
    // a 16-row band, baseline under row 14
    assert_eq!(script_baseline(Script::Super, 14, 16), 9);
    assert_eq!(script_baseline(Script::Sub, 14, 16), 15);
}

// ADR-1202 D5: a color glyph is scaled from its source once, fitted to
// two thirds of its cells, and stands against the cell's top for a
// superscript and its bottom for a subscript
// covers: sgr.script
#[test]
fn a_color_script_glyph_fits_two_thirds_of_its_cells_against_an_edge() {
    let dir = Scratch::new("color");
    let path = dir.file("e.ttf", &ttf::test_emoji().build());
    let e = engine(&format!("source=outline:{path}\n"));
    let normal = drawn(&e, CELL, variant(Script::Baseline), '\u{1f525}');
    assert_eq!(normal.format(), Format::Rgba);
    let (ntop, nbottom) = ink_rows(&normal);
    for script in [Script::Super, Script::Sub] {
        let g = drawn(&e, CELL, variant(script), '\u{1f525}');
        assert_eq!(g.format(), Format::Rgba, "{script:?}");
        let (top, bottom) = ink_rows(&g);
        // the source's 36-pixel square at 40 pixels to the em draws 21
        // rows high at the normal glyph's 23.5 pixels to the em, and two
        // thirds of that, 14 rows, at the script glyph's 15.7
        assert_eq!(nbottom - ntop, 21, "the normal glyph");
        assert_eq!(bottom - top, 14, "{script:?}");
        match script {
            Script::Super => assert_eq!(top, 0),
            _ => assert_eq!(bottom, CELL.height),
        }
    }
}

/// A 256-glyph 8x16 PSF1 font whose table maps only `glyphs`' characters,
/// each to the glyph of its index, holding those rows.
fn psf(glyphs: &[(char, [u8; 16])]) -> Vec<u8> {
    let mut b = vec![0x36, 0x04, 0x02, 16];
    b.resize(4 + 256 * 16, 0);
    for &(c, rows) in glyphs {
        let at = 4 + (c as usize) * 16;
        b[at..at + 16].copy_from_slice(&rows);
    }
    for glyph in 0..256u16 {
        if glyphs.iter().any(|&(c, _)| c as u16 == glyph) {
            b.extend(glyph.to_le_bytes());
        }
        b.extend([0xff, 0xff]);
    }
    b
}

/// Rows of `byte` from row 2 to row 13.
fn bar(byte: u8) -> [u8; 16] {
    let mut rows = [0u8; 16];
    rows[2..14].fill(byte);
    rows
}

const BITMAP: Cell = Cell {
    width: 8,
    height: 16,
};

// a frit's bitmap source's script glyph is its normal glyph made small by
// vtium-font's script rule, Glyph::scripted, the rule the console's own
// bitmap fonts take (ADR-1202 D4, D11)
// covers: sgr.script
#[test]
fn a_bitmap_script_glyph_is_its_normal_glyph_made_small_by_vtium_font() {
    let dir = Scratch::new("bitmap");
    let path = dir.file("f.psf", &psf(&[('H', bar(0b1100_0110))]));
    let e = engine(&format!("source=file:{path}\n"));
    let normal = drawn(&e, BITMAP, variant(Script::Baseline), 'H');
    for script in [Script::Super, Script::Sub] {
        let got = drawn(&e, BITMAP, variant(script), 'H');
        let want = normal.scripted(script).expect("scaled");
        assert_eq!(got.pixels(), want.pixels(), "{script:?}");
        assert_ne!(got.pixels(), normal.pixels(), "{script:?}");
    }
}

// ADR-1202 D15: a script glyph comes from the source that draws the
// character's normal glyph. The first source lacks B, so the second draws
// B's normal glyph and its script glyphs; the third also has B and never
// draws it. A comes from the first source in both
// covers: sgr.script
#[test]
fn a_script_glyph_comes_from_the_source_of_the_normal_glyph() {
    let dir = Scratch::new("source");
    let first = dir.file("1.psf", &psf(&[('A', bar(0b1000_0000))]));
    let second = dir.file(
        "2.psf",
        &psf(&[('A', bar(0b0000_0001)), ('B', bar(0b1100_0000))]),
    );
    let third = dir.file("3.psf", &psf(&[('B', bar(0b0000_0011))]));
    let e = engine(&format!(
        "source=file:{first}\nsource=file:{second}\nsource=file:{third}\n"
    ));
    let alone = |path: &str, c| {
        drawn(
            &engine(&format!("source=file:{path}\n")),
            BITMAP,
            variant(Script::Baseline),
            c,
        )
    };
    for (c, from, other) in [('A', &first, &second), ('B', &second, &third)] {
        let normal = drawn(&e, BITMAP, variant(Script::Baseline), c);
        assert_eq!(normal, alone(from, c), "{c}'s normal glyph");
        for script in [Script::Super, Script::Sub] {
            let got = drawn(&e, BITMAP, variant(script), c);
            let own = alone(from, c).scripted(script).expect("scaled");
            let borrowed = alone(other, c).scripted(script).expect("scaled");
            assert_eq!(got.pixels(), own.pixels(), "{c} {script:?}");
            assert_ne!(got.pixels(), borrowed.pixels(), "{c} {script:?}");
        }
    }
}

/// `requests` served: each frame's kind and payload.
fn served(requests: &[ServeRequest]) -> Vec<(u32, Vec<u8>)> {
    let input: Vec<u8> = requests.iter().flat_map(ServeRequest::encode).collect();
    let mut out = Vec::new();
    let status = serve(
        std::io::Cursor::new(input),
        &mut out,
        &Paths::default(),
        Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let mut rest = &out[..];
    let mut frames = Vec::new();
    while rest.len() >= 8 {
        let kind = u32::from_le_bytes(rest[..4].try_into().expect("4 bytes"));
        let len = u32::from_le_bytes(rest[4..8].try_into().expect("4 bytes")) as usize;
        frames.push((kind, rest[8..8 + len].to_vec()));
        rest = &rest[8 + len..];
    }
    frames
}

fn ask(text: &str, script: Script) -> ServeRequest {
    ServeRequest {
        kind: ASK,
        frit: frit(text),
        cell: CELL,
        band: vtium_font::spacing::Band::NONE,
        style: Style::default(),
        script,
        codepoints: vec![0x61],
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    }
}

// ADR-1202 D11: an ask names its script on the wire, and the helper
// answers it with strikes of that script alone
// covers: sgr.script
#[test]
fn a_script_ask_is_answered_in_strikes_of_its_script() {
    let text = format!("source=outline:{}\n", dejavu());
    let frames = served(&[
        ask(&text, Script::Super),
        ask(&text, Script::Sub),
        ask(&text, Script::Baseline),
    ]);
    let mut answers = Vec::new();
    for (kind, payload) in frames {
        match kind {
            // the band, none here, then the set
            FRAME_GLYPHS => {
                let set = Set::from_bytes(payload[8..].to_vec()).expect("a set");
                let scripts: Vec<Script> = set.strikes().map(|s| s.script).collect();
                answers.push(scripts);
            }
            FRAME_DONE => {}
            other => panic!("a frame of kind {other}"),
        }
    }
    assert_eq!(
        answers,
        [
            vec![Script::Super],
            vec![Script::Sub],
            vec![Script::Baseline]
        ]
    );
}

// the script is a field of the request after the style, read back as
// written, and only an ask may name one other than normal
#[test]
fn a_request_carries_its_script_and_only_an_ask_names_one() {
    let text = format!("source=outline:{}\n", dejavu());
    for script in [Script::Baseline, Script::Super, Script::Sub] {
        let request = ask(&text, script);
        let read = ServeRequest::read(&mut std::io::Cursor::new(request.encode()))
            .expect("read")
            .expect("a request");
        assert_eq!(read, request);
    }
    let fill = ServeRequest {
        kind: FILL,
        codepoints: Vec::new(),
        ..ask(&text, Script::Super)
    };
    let why = ServeRequest::read(&mut std::io::Cursor::new(fill.encode())).expect_err("refused");
    assert!(why.contains("superscript or subscript"), "{why}");
    // a script past subscript's 2
    let mut bytes = ask(&text, Script::Baseline).encode();
    let style_at = 8 + 4 + 4 + 1 + 4 + text.len() + 4 * 4;
    assert_eq!(&bytes[style_at..style_at + 8], &[0; 8], "style and script");
    bytes[style_at + 4] = 3;
    let why = ServeRequest::read(&mut std::io::Cursor::new(bytes)).expect_err("refused");
    assert!(why.contains("unknown script"), "{why}");
}
