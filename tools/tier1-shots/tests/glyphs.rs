//! The glyphs the pictures draw: a combining mark over its base, and
//! Arabic in the fonts tier 2's `scripts-arabic` screen draws it with.
//!
//! The pictures draw text through the console's own built-in face, which
//! draws a cluster the font has no glyph for by its NFC form or its base
//! with its marks. A screen that shows Arabic draws in tier 2's scripts
//! font set: Iosevka Term in its 16×40 cell at 40 pixels, with Noto Sans
//! Arabic behind it. When those fonts are absent, the drawing fails and
//! names `make tier2-fonts`.
#![cfg(feature = "draw")]

use vtium_conformance::live;
use vtium_conformance::runner::Corpus;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::spec::Spec;
use vtium_tier1_shots::manifest::{Group, Typeface};
use vtium_tier1_shots::select;
use vtium_tier1_shots::shoot::{self, Drawn};
use vtium_tier1_shots::{CONFORMANCE, root};
use vtium_tier2::ppm::Image;

/// How many pixels of the cell at `x`, `y` and `w` by `h` pixels are not
/// black, the default background.
fn ink(img: &Image, (x, y): (u32, u32), (w, h): (u32, u32)) -> u32 {
    let mut lit = 0;
    for row in y..y + h {
        let line = img.row(row);
        for col in x..x + w {
            let at = usize::try_from(col * 3).unwrap();
            if line[at..at + 3] != [0, 0, 0] {
                lit += 1;
            }
        }
    }
    lit
}

/// vtium-replay's code as the scenarios meet it, over tier 1's interface
/// documents and fixtures.
fn spec() -> Spec {
    let tier1 = root().join(CONFORMANCE);
    let texts: Vec<String> = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"]
        .iter()
        .map(|f| std::fs::read_to_string(tier1.join(f)).unwrap())
        .collect();
    Spec::read(&texts.iter().map(String::as_str).collect::<Vec<_>>()).unwrap()
}

// select-wide writes `中文 e\u{301}x 😀!`: the e and its combining acute
// accent are one cluster in the sixth cell. The accent stands above the
// x-height, in the cell's top six rows, where a plain e has no ink.
#[test]
fn select_wide_draws_the_acute_accent_over_its_e() {
    let dir = root().join(CONFORMANCE).join(select::SCENARIOS);
    let scenarios = select::scenarios(&dir).unwrap();
    let s = scenarios.iter().find(|s| s.name == "select-wide").unwrap();
    let spec = spec();
    let runner = Runner::new(
        &spec,
        &vtium_conformance::describe(),
        root().join(CONFORMANCE).join("fixtures"),
    )
    .unwrap();
    let transcript = dir.join("select-wide.transcript");
    let shots = shoot::scenario(&runner, s, &transcript, None);
    let Drawn { shot, picture, .. } = &shots[0];
    assert_eq!(shot.error, None);
    assert!(shot.live.matches, "the live play draws another picture");
    let img = picture.as_ref().unwrap();
    assert_eq!((img.width(), img.height()), (96, 32));
    assert_eq!((shot.typeface, shot.cell), (Typeface::Unifont, (8, 16)));
    let accent = ink(img, (5 * 8, 0), (8, 6));
    assert!(accent > 0, "no ink above the e in the sixth cell");
}

/// The cases of vtium's own set that show Arabic.
const ARABIC_CASES: [&str; 4] = [
    "bidi-02-all-arabic",
    "bidi-04-prompt-arabic",
    "bidi-05-joining",
    "bidi-11-autodetect-2501",
];

// Each case is 30 by 4 cells, drawn in Iosevka Term's 16×40 cell. The
// glyph for a character no font holds is its codepoint's hex digits cut
// out of a cell filled in the text color: in GNU Unifont's 8×16 cell it
// inks 63 of the 128 pixels, 49 per cent. A letter of Noto Sans Arabic
// inks far less of its cell, and the densest glyph of these cases, the
// `$` of bidi-04, inks 36 per cent. So a cell more than 42 per cent ink
// holds a hex box, unless it is the cursor's cell, drawn in reverse.
// Without the fonts, the case is not drawn, and its error names `make
// tier2-fonts`.
#[test]
fn the_arabic_cases_draw_noto_sans_arabic_and_no_hex_box() {
    let have = vtium_tier2::fonts::check_scripts(&vtium_tier2::fonts::cache_dir()).is_ok();
    let corpus = Corpus::open(&root().join(CONFORMANCE).join("vtium")).unwrap();
    for name in ARABIC_CASES {
        let case = corpus.cases.iter().find(|c| c.name == name).unwrap();
        let Drawn { shot, picture, .. } = shoot::case(&corpus, Group::Vtium, case, None);
        if !have {
            eprintln!("{name}: the scripts fonts are absent, so only the error is checked");
            let why = shot.error.unwrap_or_default();
            assert!(why.contains("make tier2-fonts"), "{name}: {why}");
            assert!(picture.is_none(), "{name}: drawn without its fonts");
            continue;
        }
        assert_eq!(shot.error, None, "{name}");
        assert!(
            shot.live.matches,
            "{name}: the live play draws another picture"
        );
        let img = picture.unwrap();
        // the cells as drawn, so the boxes are found in a picture of any
        // cell size, and the size is checked after them
        let (w, h) = (img.width() / 30, img.height() / 4);
        let mut full = Vec::new();
        let mut inked = 0;
        for row in 0..4 {
            for col in 0..30 {
                let lit = ink(&img, (col * w, row * h), (w, h));
                if lit > 0 {
                    inked += 1;
                }
                if lit * 100 > w * h * 42 {
                    full.push((col, row));
                }
            }
        }
        assert!(full.len() <= 1, "{name}: hex boxes in the cells {full:?}");
        assert_eq!((w, h), (16, 40), "{name}: the cell");
        assert_eq!(
            (shot.typeface, shot.cell),
            (Typeface::Scripts, (16, 40)),
            "{name}"
        );
        assert!(inked >= 5, "{name}: {inked} cells hold ink");
    }
}

// A screen of Arabic drawn from a directory that lacks the scripts fonts
// fails and says which font is missing and how to fetch it, rather than
// drawing a box for each letter.
#[test]
fn arabic_without_its_fonts_fails_naming_make_tier2_fonts() {
    let empty = std::env::temp_dir().join(format!("tier1-shots-nofonts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).unwrap();
    let term = live::read_back("مرحبا".as_bytes(), 10, 2);
    let why = shoot::draw_from(&term, &empty).unwrap_err();
    assert!(why.contains("Arabic"), "{why}");
    assert!(why.contains("IosevkaTerm-Regular.ttf"), "{why}");
    assert!(why.contains("make tier2-fonts"), "{why}");
    // a screen with no Arabic draws from the built-in font alone
    let latin = live::read_back(b"abc", 10, 2);
    let img = shoot::draw_from(&latin, &empty).unwrap();
    assert_eq!((img.width(), img.height()), (80, 32));
    std::fs::remove_dir_all(&empty).unwrap();
}
