//! vtium-font against the C reference, glyph for glyph: each line of
//! `reference/*.tsv` is a digest of glyphs the C reference drew
//! (`reference/refdigest.c`, at the commit `reference/README.md` names),
//! and vtium must draw the same pixels. This is what lets tier 2's
//! bitmap-font goldens (vtium/vtium#17) judge vtium unchanged, and since
//! the digests were taken on one host and are checked on every other, it
//! is also the check that a glyph is the same bytes everywhere.

use vtium_font::{Face, Font, Glyph, Psf, Style, Width, sprite};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(FNV_PRIME);
    }
    h
}

/// refdigest.c's digest(): the width and height as little-endian u32, then
/// the pixels.
fn digest(h: u64, g: &Glyph) -> u64 {
    let h = fnv(h, &g.width().to_le_bytes());
    let h = fnv(h, &g.height().to_le_bytes());
    fnv(h, g.pixels())
}

fn lines(name: &str) -> Vec<Vec<String>> {
    let path = format!("{}/tests/reference/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}

fn num(s: &str) -> u32 {
    s.parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("not hex: {s}"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Checks every line, then fails listing those that differ.
fn check(what: &str, rows: &[Vec<String>], mut ours: impl FnMut(&[String]) -> Option<u64>) {
    let mut checked = 0;
    let mut bad = Vec::new();
    for row in rows {
        let Some(got) = ours(row) else { continue };
        let want = row.last().map(|s| u64::from_str_radix(s, 16));
        checked += 1;
        if want != Some(Ok(got)) {
            bad.push(row.join(" "));
        }
    }
    assert!(checked > 0, "{what}: no lines checked");
    assert!(
        bad.is_empty(),
        "{what}: {} of {checked} digests differ from the C reference's, first:\n{}",
        bad.len(),
        bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// The 256 characters of `page` drawn through the font, as refdigest.c's
/// render_page() draws them; `None` for the surrogate pages, which a
/// `char` cannot hold and the terminal never draws.
fn render_page<F: Face>(font: &Font<F>, page: u32, cells: u32, bold: bool) -> Option<u64> {
    let width = if cells == 2 {
        Width::Wide
    } else {
        Width::Narrow
    };
    let style = Style {
        bold,
        italic: false,
    };
    let mut d = FNV_OFFSET;
    for cp in page << 8..(page + 1) << 8 {
        let c = char::from_u32(cp)?;
        let g = font
            .glyph(&[c], width, style)
            .unwrap_or_else(|| panic!("U+{cp:04X} drew nothing"));
        d = digest(d, &g);
    }
    Some(d)
}

// font_box.c over 800 cell sizes and 858 line thicknesses, and in a
// 1000x2000 cell
// covers: text.sprites
#[test]
fn sprites_match_the_c_reference() {
    check("sprites", &lines("box.tsv"), |row| {
        let [_, range, w, h, t, _] = row else {
            panic!("bad line {row:?}")
        };
        let (first, last) = range.split_once('-').expect("a range");
        let mut d = FNV_OFFSET;
        for cp in hex(first)..=hex(last) {
            let c = char::from_u32(cp).expect("a char");
            let g = sprite::draw(c, num(w), num(h), num(t))
                .unwrap_or_else(|| panic!("U+{cp:04X} at {w}x{h} drew nothing"));
            d = digest(d, &g);
        }
        Some(d)
    });
}

// font_psf.c's glyphs and font.c's kmscon_font_render over the fixtures,
// at 1, 2 and 3 times their height
// covers: text.psf
#[test]
fn psf_fonts_match_the_c_reference() {
    let rows = lines("psf.tsv");
    let mut fonts = std::collections::BTreeMap::new();
    let mut font = |name: &str, height: u32| -> Font<Psf> {
        let psf = fonts
            .entry(name.to_owned())
            .or_insert_with(|| Psf::load(fixture(name)).expect("the fixture loads"));
        let mut psf = psf.clone();
        psf.set_height(height);
        Font::new(psf)
    };
    check("psf glyphs", &rows, |row| match row {
        // a build without gzip refuses the gzipped fixtures
        [_, name, ..] if name.ends_with(".gz") && !cfg!(feature = "gzip") => None,
        [kind, name, height, bold, _] if kind == "psf-glyphs" => {
            let font = font(name, num(height));
            let style = Style {
                bold: bold == "1",
                italic: false,
            };
            let mut d = FNV_OFFSET;
            for i in 0..font.face().glyph_count() {
                d = digest(d, &font.face().render(i, style).expect("a glyph"));
            }
            Some(d)
        }
        [kind, name, height, page, cells, bold, _] if kind == "render" => {
            render_page(&font(name, num(height)), hex(page), num(cells), bold == "1")
        }
        _ => panic!("bad line {row:?}"),
    });
}

// font_unifont.c's glyphs, each at its own width, and font.c's
// kmscon_font_render over all of plane 0 and a sample beyond, at 1 and 2
// times 16
#[cfg(feature = "unifont")]
// covers: text.psf
#[test]
fn unifont_matches_the_c_reference() {
    use vtium_font::Unifont;
    check("unifont", &lines("unifont.tsv"), |row| match row {
        [kind, height, bold, page, _] if kind == "unifont-glyphs" => {
            let font = Unifont::new(num(height));
            let style = Style {
                bold: bold == "1",
                italic: false,
            };
            let mut d = FNV_OFFSET;
            let page = hex(page);
            for c in (page << 8..(page + 1) << 8).filter_map(char::from_u32) {
                if font.has_glyph(c) {
                    d = digest(d, &font.render(c, style).expect("a glyph"));
                }
            }
            Some(d)
        }
        [kind, _, height, page, cells, bold, _] if kind == "render" => render_page(
            &Font::new(Unifont::new(num(height))),
            hex(page),
            num(cells),
            bold == "1",
        ),
        _ => panic!("bad line {row:?}"),
    });
}
