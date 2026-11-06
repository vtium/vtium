//! vtium-font against its glyph digests: each line of `reference/*.tsv` is
//! a digest of glyphs vtium draws (`reference/README.md`), and vtium must
//! draw the same pixels. A change to a sprite, a PSF glyph or a unifont
//! glyph fails here first. The digests were taken on x86-64 and are
//! checked on every host, so this is also the check that a glyph is the
//! same bytes everywhere.

use vtium_font::{Face, Font, Glyph, Psf, Style, Width, sprite};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(FNV_PRIME);
    }
    h
}

/// A glyph's digest: the width and height as little-endian u32, then the
/// pixels.
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

/// Checks every line of `reference/{file}`. When any differs, it writes
/// the file as vtium draws it, each checked line with vtium's digest and
/// every other line as it is, to `digests/{file}` under the test's scratch
/// directory, and fails naming that file and how many lines differ.
fn check(file: &str, mut ours: impl FnMut(&[String]) -> Option<u64>) {
    let mut checked = 0;
    let mut differ = 0;
    let mut drawn = String::new();
    for row in lines(file) {
        let mut row = row;
        if let Some(got) = ours(&row) {
            checked += 1;
            let want = row.last().map(|s| u64::from_str_radix(s, 16));
            if want != Some(Ok(got)) {
                differ += 1;
            }
            if let Some(last) = row.last_mut() {
                *last = format!("{got:016x}");
            }
        }
        drawn.push_str(&row.join("\t"));
        drawn.push('\n');
    }
    assert!(checked > 0, "{file}: no lines checked");
    if differ > 0 {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("digests");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let path = dir.join(file);
        std::fs::write(&path, drawn).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        panic!(
            "{file}: {differ} of {checked} digests differ from tests/reference/{file}; \
             vtium's digests for every line are in {}",
            path.display()
        );
    }
}

/// The Symbols for Legacy Computing characters vtium draws as sprites
/// (vtium/vtium#27), which the page digests leave out; `tests/legacy.rs`
/// holds them.
fn outside_digests(cp: u32) -> bool {
    matches!(
        cp,
        0x1cea0..=0x1ceaf
            | 0x1fb3c..=0x1fb92
            | 0x1fb94..=0x1fbaf
            | 0x1fbbd..=0x1fbbf
            | 0x1fbce..=0x1fbef
    )
}

// The skip never reaches past what vtium draws as a sprite, so a page
// digest leaves out no character the face draws
#[test]
fn the_characters_left_out_are_all_sprites() {
    let mut left_out = 0;
    for cp in (0x1ce00..0x1cf00).chain(0x1fb00..0x1fc00) {
        if outside_digests(cp) {
            left_out += 1;
            let c = char::from_u32(cp).expect("a char");
            assert!(sprite::has(c), "U+{cp:04X} is left out but is no sprite");
        }
    }
    assert_eq!(left_out, 168);
}

/// The characters of `page` drawn through the font, less those the page
/// digests leave out, as the `render` digests cover them; `None` for the
/// surrogate pages, which a `char` cannot hold and the terminal never
/// draws.
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
    for cp in (page << 8..(page + 1) << 8).filter(|&cp| !outside_digests(cp)) {
        let c = char::from_u32(cp)?;
        let g = font
            .glyph(&[c], width, style)
            .unwrap_or_else(|| panic!("U+{cp:04X} drew nothing"));
        d = digest(d, &g);
    }
    Some(d)
}

// every sprite over 800 cell sizes, the box-drawing lines at 858 more
// sizes and thicknesses, and in a 1000x2000 cell
// covers: text.sprites
#[test]
fn sprites_match_their_digests() {
    check("box.tsv", |row| {
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

// every glyph of the PSF fixtures, and pages of characters drawn through
// `Font::glyph`, at 1, 2 and 3 times their height
// covers: text.psf
#[test]
fn psf_fonts_match_their_digests() {
    let mut fonts = std::collections::BTreeMap::new();
    let mut font = |name: &str, height: u32| -> Font<Psf> {
        let psf = fonts
            .entry(name.to_owned())
            .or_insert_with(|| Psf::load(fixture(name)).expect("the fixture loads"));
        let mut psf = psf.clone();
        psf.set_height(height);
        Font::new(psf)
    };
    check("psf.tsv", |row| match row {
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

// unifont's glyphs, each at its own width, and every page of plane 0 and a
// sample beyond drawn through `Font::glyph`, at 1 and 2 times 16
#[cfg(feature = "unifont")]
// covers: text.psf
#[test]
fn unifont_matches_its_digests() {
    use vtium_font::Unifont;
    check("unifont.tsv", |row| match row {
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
