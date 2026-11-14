//! A measurement, not a gate: why the shipped `indic` frit draws Tamil,
//! Malayalam and Sinhala smaller than the text around them (vtium/vtium#144).
//! It reads the Noto fonts that tier 2 pins from tier 2's font cache,
//! `$XDG_CACHE_HOME/vtium/tier2/fonts` (`make tier2-fonts` fetches them), so
//! it is ignored by default. Run it with
//! `cargo test -p vtium-config --test indic_sizes -- --ignored --nocapture`.
//!
//! For each cluster of tier 2's `frit-indic` screen, it prints the glyph's
//! ink as the helper draws it through the frit at 32 pixels of text with
//! the frit's 40 per cent of line spacing, and the same cluster drawn by
//! the rasteriser in steps that each add one of the helper's rules: the
//! font at the first source's size with no limit, then the cell's width,
//! then the text band's height and its reach, then the fallback fit
//! (ADR-1200 D17). The step at which the ink shrinks names the rule.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

use std::path::PathBuf;
use std::sync::Arc;

use vtium_config::frit::Hinting;
use vtium_config::outline::{OutlineRequest, Placed, baseline, rasterise, size};
use vtium_config::serve::{Engine, FritText, OwnCell, Paths};
use vtium_config::shaping::{Ask, kind};
use vtium_font::set::{Set, SetFace};
use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Face, Style};

const TEXT: u32 = 32;
const SPACING: u32 = 40;

fn cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .expect("a cache directory")
        .join("vtium/tier2/fonts")
}

/// The scripts, each with its font file, its family and the words of tier
/// 2's `frit-indic` screen. Devanagari is the control that draws well.
const SCRIPTS: [(&str, &str, &str, &str); 5] = [
    (
        "Devanagari",
        "NotoSansDevanagari-Regular.ttf",
        "Noto Sans Devanagari",
        "धर्म कि कु",
    ),
    (
        "Tamil",
        "NotoSansTamil-Regular.ttf",
        "Noto Sans Tamil",
        "தமிழ் க்ஷ கொ கை கூ",
    ),
    (
        "Malayalam",
        "NotoSansMalayalam-Regular.ttf",
        "Noto Sans Malayalam",
        "മലയാളം ക്ഷ്മ സ്ത്രീ",
    ),
    (
        "Sinhala",
        "NotoSansSinhala-Regular.ttf",
        "Noto Sans Sinhala",
        "සිංහල භාෂාව ලංකා",
    ),
    (
        "Kannada",
        "NotoSansKannada-Regular.ttf",
        "Noto Sans Kannada",
        "ಕನ್ನಡ ಕ್ಷ್ಮ ಕೃ",
    ),
];

const FRIT: &str = "source=font:Noto Sans Mono\n\
    source=font:Noto Sans Devanagari\nfit=room\n\
    source=font:Noto Sans Tamil\nfit=room\n\
    source=font:Noto Sans Kannada\nfit=room\n\
    source=font:Noto Sans Malayalam\nfit=room\n\
    source=font:Noto Sans Sinhala\nfit=room\n\
    line-spacing=40\n";

/// The clusters a vtium terminal cuts `text` into.
fn clusters(text: &str) -> Vec<Vec<u32>> {
    let mut t = vtium_term::Terminal::new(80, 2, 0);
    let _ = t.input(text.as_bytes());
    let screen = t.screen();
    let line = screen.line(0).expect("a row");
    line.cells()
        .iter()
        .filter_map(|&c| match line.glyph(c) {
            vtium_term::Glyph::Char(' ') | vtium_term::Glyph::Empty => None,
            vtium_term::Glyph::Char(ch) => Some(vec![u32::from(ch)]),
            vtium_term::Glyph::Cluster(s) => Some(s.chars().map(u32::from).collect()),
        })
        .collect()
}

/// An ink extent: its top and bottom rows from the top of the text band,
/// and its width in columns; `None` when nothing is inked.
type Extent = Option<(i32, i32, i32)>;

fn extent(rows: impl Iterator<Item = (i32, Vec<i32>)>) -> Extent {
    let mut top = i32::MAX;
    let mut bottom = i32::MIN;
    let (mut left, mut right) = (i32::MAX, i32::MIN);
    for (y, xs) in rows {
        if let (Some(&a), Some(&b)) = (xs.first(), xs.last()) {
            top = top.min(y);
            bottom = bottom.max(y);
            left = left.min(a);
            right = right.max(b);
        }
    }
    (top <= bottom).then_some((top, bottom, right - left + 1))
}

/// The inked columns of each row of an R8 image `width` wide.
fn rows_of(pixels: &[u8], width: usize, top: i32) -> Vec<(i32, Vec<i32>)> {
    pixels
        .chunks(width)
        .enumerate()
        .map(|(y, row)| {
            let xs = row
                .iter()
                .enumerate()
                .filter(|(_, p)| **p > 0)
                .map(|(x, _)| x as i32)
                .collect();
            (y as i32 + top, xs)
        })
        .collect()
}

/// A rasterised glyph's extent, its ink box when it has one, rows counted
/// from `band_top`, the text band's top row in its cell.
fn placed_extent(p: &Placed, width: u32, band_top: i32) -> Extent {
    match &p.ink {
        Some(ink) => {
            extent(rows_of(&ink.pixels, ink.width as usize, ink.top - band_top).into_iter())
        }
        None => extent(rows_of(&p.pixels, width as usize, -band_top).into_iter()),
    }
}

struct Step {
    name: &'static str,
    cell: (u32, u32),
    /// The text band's top row in the step's cell.
    band_top: u32,
    reach: (u32, u32),
    fit: bool,
}

#[test]
#[ignore = "a measurement: reads tier 2's font cache, run by hand"]
fn measure_indic_sizes() {
    let dir = cache();
    let read = |f: &str| -> Arc<[u8]> {
        std::fs::read(dir.join(f))
            .unwrap_or_else(|e| panic!("{f} in {}: {e}; run make tier2-fonts", dir.display()))
            .into()
    };
    let mono = read("NotoSansMono-Regular.ttf");
    let ppem = size(&mono, "Noto Sans Mono", TEXT).expect("a size");
    let base = baseline(&mono, "Noto Sans Mono", TEXT, None, Hinting::Light).expect("a baseline");
    let paths = Paths {
        kbd_dirs: Vec::new(),
        font_dirs: vec![dir.clone()],
        vconsole_conf: dir.join("none"),
        kernel_cmdline: dir.join("none"),
    };
    let frit = FritText {
        name: "indic".to_owned(),
        text: FRIT.to_owned(),
    };
    let engine = Engine::load(&frit, &paths, None).expect("the frit loads");
    let cw = match engine.own_cell(TEXT).expect("a cell") {
        OwnCell::Outline(c) | OwnCell::Bitmap(c) => c.width,
    };
    let band = Band::of(TEXT, SPACING);
    let whole = Cell {
        width: cw,
        height: TEXT + band.added(),
    };
    println!(
        "Noto Sans Mono: {ppem:.2} px/em, baseline row {base}, cell {cw}x{TEXT}, band +{}/+{}",
        band.above, band.below
    );
    // the Latin text around the clusters, as the helper draws it
    let latin = engine
        .draw_style(
            SpacedCell { cell: whole, band },
            Style::default(),
            &[0x48, 0x78],
            (&[], false),
        )
        .expect("draws")
        .expect("glyphs");
    let face = SetFace::new(
        Arc::new(Set::from_bytes(latin).expect("a set")),
        whole.height,
    )
    .expect("a face");
    for c in ['H', 'x'] {
        let g = face.draw(&[c], Style::default()).expect("drawn");
        let e = extent(rows_of(g.pixels(), cw as usize, -(band.above as i32)).into_iter());
        println!("  Latin {c}: {e:?}");
    }
    let tall = 4 * TEXT;
    let steps = [
        Step {
            name: "free",
            cell: (16 * cw, 3 * tall),
            band_top: tall,
            reach: (0, 0),
            fit: false,
        },
        Step {
            name: "+width",
            cell: (cw, 3 * tall),
            band_top: tall,
            reach: (0, 0),
            fit: false,
        },
        Step {
            name: "+band",
            cell: (cw, TEXT),
            band_top: 0,
            reach: (band.above + 1, band.below + 1),
            fit: false,
        },
        Step {
            name: "+fit",
            cell: (cw, TEXT),
            band_top: 0,
            reach: (band.above + 1, band.below + 1),
            fit: true,
        },
        // a control: no reach and the fallback fit, which must shrink a
        // cluster whose ink passes the text band, so the steps can see a
        // vertical rule fire
        Step {
            name: "control",
            cell: (cw, TEXT),
            band_top: 0,
            reach: (0, 0),
            fit: true,
        },
    ];
    for (script, file, family, words) in SCRIPTS {
        let font = read(file);
        println!("{script} ({family}):");
        for key in clusters(words) {
            let text: String = key.iter().filter_map(|&c| char::from_u32(c)).collect();
            let single = key.len() == 1;
            let (want, asks): (Vec<u32>, Vec<Vec<u32>>) = if single {
                (key.clone(), Vec::new())
            } else {
                (Vec::new(), vec![key.clone()])
            };
            let bytes = engine
                .draw_style(
                    SpacedCell { cell: whole, band },
                    Style::default(),
                    &want,
                    (&asks, false),
                )
                .expect("draws");
            let helper = bytes.and_then(|b| {
                let face = SetFace::new(Arc::new(Set::from_bytes(b).ok()?), whole.height)?;
                let chars: Vec<char> = text.chars().collect();
                let g = if single {
                    face.draw(&chars, Style::default())
                } else {
                    face.cluster(&chars, Style::default())
                }?;
                let w = g.width() as usize;
                Some(match g.ink() {
                    Some(ink) => extent(
                        rows_of(
                            ink.pixels(),
                            ink.width() as usize,
                            ink.top() - band.above as i32,
                        )
                        .into_iter(),
                    ),
                    None => extent(rows_of(g.pixels(), w, -(band.above as i32)).into_iter()),
                })
            });
            let mut line = format!("  {text:<8} helper {:?}", helper.flatten());
            for s in &steps {
                let request = OutlineRequest {
                    fit: s.fit,
                    synth: Default::default(),
                    script: Default::default(),
                    font: font.clone(),
                    family: family.to_owned(),
                    cells: vec![s.cell],
                    cover: if single {
                        vec![(key[0], key[0])]
                    } else {
                        Vec::new()
                    },
                    clusters: asks
                        .iter()
                        .map(|k| Ask {
                            key: k.clone(),
                            kind: kind(k),
                        })
                        .collect(),
                    baselines: vec![s.band_top + base],
                    sizes: vec![ppem.to_bits()],
                    reaches: vec![s.reach],
                    hinting: Hinting::Light,
                };
                let o = rasterise(&request).expect("rasterised");
                let strike = &o.strikes[0];
                let g = if single {
                    strike
                        .map
                        .iter()
                        .find(|(c, _)| *c == key[0])
                        .map(|(_, g)| *g)
                } else {
                    strike
                        .clusters
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, g)| *g)
                };
                let e = g.and_then(|g| {
                    let p = &strike.glyphs[g as usize];
                    let width = if p.wide {
                        2 * strike.width
                    } else {
                        strike.width
                    };
                    placed_extent(p, width, s.band_top as i32)
                });
                line.push_str(&format!("  {} {e:?}", s.name));
            }
            println!("{line}");
        }
    }
}
