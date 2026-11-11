//! Superscript and subscript glyphs from a smaller size of a bitmap source
//! (ADR-1202 D3, D12, D15), as the helper draws them for a frit: the
//! source itself at a smaller whole multiple, a CPI source's other
//! heights, and the fonts `script-source=` names, the tallest from one half
//! to three quarters of the normal glyph's height, placed whole against the
//! text band. Only the source's own font and the fonts its frit names for
//! it give its script glyphs.

use std::path::PathBuf;
use std::sync::Arc;

use vtium_config::serve::{Engine, FritText, Paths};
use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Font, Glyph, Psf, Script, Style, Unifont, Variant, Width};

/// A directory of a test's own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("vtium-script-sizes-{name}-{}", std::process::id()));
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

/// A PSF2 font of 128 glyphs 8 wide and `height` high, glyph `g`'s row `y`
/// the byte `g * 7 + y * 3 + seed`, so each font and height draws its own
/// glyphs. With `table`, each glyph maps the codepoint of its number; with
/// none the font has no Unicode table.
fn psf2(height: u32, seed: u32, table: bool) -> Vec<u8> {
    let count = 128u32;
    let mut f = Vec::new();
    for v in [
        0x864a_b572u32,
        0,
        32,
        u32::from(table),
        count,
        height,
        height,
        8,
    ] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    for g in 0..count {
        for y in 0..height {
            f.push(((g * 7 + y * 3 + seed) % 256) as u8);
        }
    }
    if table {
        for g in 0..count {
            f.push(g as u8);
            f.push(0xff);
        }
    }
    f
}

/// A CPI file as kbd ships one, a font 8 wide for each of `heights`, glyph
/// `g`'s rows each `g` plus the height.
fn cpi_file(heights: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 0x22];
    f[0] = 0x1c;
    f[6] = 1;
    f[8..16].copy_from_slice(b"EGA     ");
    f[0x10..0x12].copy_from_slice(&437u16.to_le_bytes());
    f[0x1c] = 1;
    f[0x1e] = heights.len() as u8;
    for &h in heights {
        f.extend_from_slice(&[h, 8, 0, 0, 0, 1]);
        for g in 0..256u32 {
            f.extend(std::iter::repeat_n(
                (g as u8).wrapping_add(h),
                usize::from(h),
            ));
        }
    }
    f
}

fn engine(text: &str) -> Engine {
    let frit = FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    };
    Engine::load(&frit, &Paths::default(), None).expect("the frit loads")
}

fn variant(script: Script) -> Variant {
    Variant {
        style: Style::default(),
        script,
    }
}

/// `c`'s glyph in `script` that `engine` draws at `cell`.
fn drawn(engine: &Engine, cell: Cell, script: Script, c: char) -> Glyph {
    let v = variant(script);
    let bytes = engine
        .draw_variant(cell, v, &[u32::from(c)], (&[], false))
        .expect("draws")
        .expect("a glyph");
    let set = Arc::new(Set::from_bytes(bytes).expect("a set"));
    let face = SetFace::new(set, cell.height).expect("a face");
    face.draw_variant(&[c], v).expect("the glyph")
}

fn lit(g: &Glyph, x: u32, y: u32) -> bool {
    g.pixel(x, y).is_some_and(|p| p[0] >= 128)
}

/// Asserts that `got` holds the 1x glyph `small`, `small.width()` by
/// `small.height()`, with each pixel a block `j` across and its top left
/// corner at `at`, and nothing else.
fn holds(got: &Glyph, small: &Glyph, j: u32, at: (u32, u32), what: &str) {
    for y in 0..got.height() {
        for x in 0..got.width() {
            let inside = x >= at.0
                && y >= at.1
                && (x - at.0) / j < small.width()
                && (y - at.1) / j < small.height();
            let want = inside && lit(small, (x - at.0) / j, (y - at.1) / j);
            assert_eq!(lit(got, x, y), want, "{what} at {x},{y}");
        }
    }
}

/// `font`'s own glyph for `c` at its own size.
fn psf_glyph(font: &[u8], c: char) -> Glyph {
    let psf = Psf::parse(font).expect("the font parses");
    psf.render(psf.glyph_index(c).expect("mapped"), Style::default())
        .expect("drawn")
}

// D3, D11: unifont at 2x and 3x in a frit gives the helper's script glyphs
// as the console draws its own unifont's, from unifont at 1x and 2x
// covers: sgr.script
#[test]
fn the_helper_draws_unifonts_smaller_multiples_as_the_console_does() {
    let e = engine("source=builtin:unifont\n");
    for k in [2u32, 3] {
        let cell = Cell {
            width: 8 * k,
            height: 16 * k,
        };
        let console = Font::new(Unifont::new(16 * k));
        for c in ['e', 'B', 'g', '2'] {
            for script in [Script::Super, Script::Sub] {
                let ours = drawn(&e, cell, script, c);
                let theirs = console
                    .drawn_variant(&[c], Width::Narrow, variant(script))
                    .expect("the console's")
                    .glyph;
                assert_eq!(ours.pixels(), theirs.pixels(), "{c} {script:?} at {k}x");
            }
        }
        let normal = Unifont::new(16).render('g', Style::default()).expect("g");
        let sub = drawn(&e, cell, Script::Sub, 'g');
        holds(
            &sub,
            &normal,
            k - 1,
            (4, 16 * k - 16 * (k - 1)),
            "unifont g",
        );
    }
}

// D12: a source drawn at its own size takes its superscripts and
// subscripts from the smaller font script-source= names, placed whole
// against the top or bottom of the cell
// covers: sgr.script
#[test]
fn script_source_gives_the_smaller_fonts_glyph() {
    let s = Scratch::new("named");
    let big = s.file("big.psf", &psf2(16, 0, true));
    let small_bytes = psf2(8, 101, true);
    let small = s.file("small.psf", &small_bytes);
    let e = engine(&format!("source=file:{big}\nscript-source=file:{small}\n"));
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let want = psf_glyph(&small_bytes, 'A');
    holds(
        &drawn(&e, cell, Script::Super, 'A'),
        &want,
        1,
        (0, 0),
        "super",
    );
    holds(&drawn(&e, cell, Script::Sub, 'A'), &want, 1, (0, 8), "sub");
}

// D15: a script glyph comes only from its own source or the fonts its
// frit names for it: an 8x8 font named as a later source, and the
// built-in unifont filling what the frit lacks, never give the first
// source's script glyphs, which are its own made small by D4
// covers: sgr.script
#[test]
fn a_script_glyph_never_comes_from_a_font_not_named_for_its_source() {
    let s = Scratch::new("stranger");
    let big_bytes = psf2(16, 0, true);
    let big = s.file("big.psf", &big_bytes);
    let stranger = s.file("stranger.psf", &psf2(8, 101, true));
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let mut psf = Psf::parse(&big_bytes).expect("parses");
    psf.set_height(16);
    let console = Font::new(psf);
    for text in [
        format!("source=file:{big}\nsource=file:{stranger}\n"),
        format!("source=file:{big}\nsource=builtin:unifont\n"),
        format!("source=file:{big}\nsource=builtin:unscii-8\n"),
    ] {
        let e = engine(&text);
        for script in [Script::Super, Script::Sub] {
            let ours = drawn(&e, cell, script, 'A');
            let d4 = console
                .drawn_variant(&['A'], Width::Narrow, variant(script))
                .expect("the console's")
                .glyph;
            assert_eq!(ours.pixels(), d4.pixels(), "{text} {script:?}");
        }
    }
    // a script-source= of another source is not this one's
    let small = s.file("small.psf", &psf2(8, 101, true));
    let e = engine(&format!(
        "source=file:{big}\nsource=file:{stranger}\nscript-source=file:{small}\n"
    ));
    let d4 = console
        .drawn_variant(&['A'], Width::Narrow, variant(Script::Super))
        .expect("the console's")
        .glyph;
    assert_eq!(drawn(&e, cell, Script::Super, 'A').pixels(), d4.pixels());
}

// D3: a CPI source takes its superscripts from another of its heights in
// the range, here 8 for the 16 that cp-size= takes, with the source's
// unimap=
// covers: sgr.script
#[test]
fn a_cpi_source_takes_another_of_its_heights() {
    let s = Scratch::new("cpi");
    let cp = s.file("f.cp", &cpi_file(&[16, 14, 8]));
    let map = s.file("f.uni", b"0x41 U+0041\n");
    let e = engine(&format!("source=file:{cp}\ncp-size=16\nunimap={map}\n"));
    let cell = Cell {
        width: 8,
        height: 16,
    };
    // the 8-pixel font's glyph 0x41: each row 0x41 + 8
    let row = 0x41u8 + 8;
    let sup = drawn(&e, cell, Script::Super, 'A');
    for y in 0..16 {
        for x in 0..8 {
            let want = y < 8 && row & (0x80 >> x) != 0;
            assert_eq!(lit(&sup, x, y), want, "{x},{y}");
        }
    }
    // 14 is past three quarters of 16, so with no 8 the glyph is scaled
    let only = s.file("g.cp", &cpi_file(&[16, 14]));
    let e = engine(&format!("source=file:{only}\ncp-size=16\nunimap={map}\n"));
    let sup = drawn(&e, cell, Script::Super, 'A');
    assert!((0..8).any(|x| lit(&sup, x, 10)), "a glyph two thirds high");
}

// D12: a script-source= font with no Unicode table takes the source's
// unimap=, and one with its own table keeps it
// covers: sgr.script
#[test]
fn script_source_takes_the_sources_unimap_only_without_a_table_of_its_own() {
    let s = Scratch::new("unimap");
    let map = s.file("f.uni", b"0x41 U+0041\n0x42 U+0042\n");
    let big = s.file("big.psf", &psf2(16, 0, false));
    let bare_bytes = psf2(8, 101, false);
    let bare = s.file("bare.psf", &bare_bytes);
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let e = engine(&format!(
        "source=file:{big}\nunimap={map}\nscript-source=file:{bare}\n"
    ));
    let glyph_a = Psf::parse(&bare_bytes)
        .expect("parses")
        .render(0x41, Style::default())
        .expect("glyph 0x41");
    holds(
        &drawn(&e, cell, Script::Super, 'A'),
        &glyph_a,
        1,
        (0, 0),
        "bare",
    );
    // its own table maps A to glyph 0x41 too, but B to glyph 0x42; swap
    // them by a table where glyph 0x41 is B
    let mut own = psf2(8, 101, true);
    let at = own.len() - 2 * 128 + 2 * 0x41;
    own[at] = b'B';
    let at = own.len() - 2 * 128 + 2 * 0x42;
    own[at] = b'A';
    let mine = s.file("own.psf", &own);
    let e = engine(&format!(
        "source=file:{big}\nunimap={map}\nscript-source=file:{mine}\n"
    ));
    let glyph_b = Psf::parse(&own)
        .expect("parses")
        .render(0x42, Style::default())
        .expect("glyph 0x42");
    holds(
        &drawn(&e, cell, Script::Super, 'A'),
        &glyph_b,
        1,
        (0, 0),
        "own table",
    );
}

// D12: a script-source= font this machine lacks is left out quietly; one
// it has and refuses is left out with a warning naming its line
#[test]
fn a_missing_script_source_is_quiet_and_a_refused_one_warns() {
    let s = Scratch::new("warn");
    let big = s.file("big.psf", &psf2(16, 0, true));
    let bad = s.file("bad.psf", b"not a font at all");
    let gone = s.0.join("gone.psf").display().to_string();
    let e = engine(&format!(
        "source=file:{big}\nscript-source=file:{gone}\nscript-source=file:{bad}\n"
    ));
    let warnings = e.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("t:3: script-source=file:"),
        "{warnings:?}"
    );
}

/// The lines of the strike in `script` that `engine` answers at `cell`.
fn strike_lines(engine: &Engine, cell: Cell, script: Script) -> vtium_font::Decorations {
    let bytes = engine
        .draw_variant(cell, variant(script), &[u32::from('A')], (&[], false))
        .expect("draws")
        .expect("a glyph");
    let set = Set::from_bytes(bytes).expect("a set");
    let strike = set.strikes().next().expect("a strike");
    assert_eq!(strike.script, script);
    strike.decorations
}

// ADR-1202 D8 step 3: the helper's script strikes report the lines the
// console's own fonts take for the same font: unifont at 1x by D4's map,
// and at 2x and 3x by D3, the lines of unifont at 1x and 2x placed with
// the strike
// covers: sgr.script
#[test]
fn the_helpers_script_lines_are_the_consoles_for_unifont() {
    let e = engine("source=builtin:unifont\n");
    for k in [1u32, 2, 3] {
        let cell = Cell {
            width: 8 * k,
            height: 16 * k,
        };
        let console = Font::new(Unifont::new(16 * k));
        for script in [Script::Super, Script::Sub] {
            assert_eq!(
                strike_lines(&e, cell, script),
                console.script_decorations(script),
                "{script:?} at {k}x"
            );
        }
        assert_eq!(
            strike_lines(&e, cell, Script::Baseline),
            console.decorations(),
            "normal at {k}x"
        );
    }
}

// ADR-1202 D8 step 3: a source drawn at its own size whose script glyphs
// come from the font script-source= names takes that font's lines, the
// lines of its height, placed with its glyph: an 8-pixel font against
// the top of a 16-pixel cell for a superscript and its bottom for a
// subscript
// covers: sgr.script
#[test]
fn script_source_gives_the_smaller_fonts_lines() {
    let s = Scratch::new("lines");
    let big = s.file("big.psf", &psf2(16, 0, true));
    let small = s.file("small.psf", &psf2(8, 101, true));
    let e = engine(&format!("source=file:{big}\nscript-source=file:{small}\n"));
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let lines = |underline, strike, overline| vtium_font::Decorations {
        underline,
        underline_thickness: 1,
        strike,
        strike_thickness: 1,
        overline,
    };
    assert_eq!(strike_lines(&e, cell, Script::Super), lines(7, 4, 0));
    assert_eq!(strike_lines(&e, cell, Script::Sub), lines(15, 12, 8));
}
