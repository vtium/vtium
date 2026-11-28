use super::*;
use std::path::{Path, PathBuf};
use vtium_font::set::layer;
use vtium_font::spacing::Band;
use vtium_settings::cullet::{self, Rule};
use vtium_settings::fontpath::path_source;
use vtium_settings::mounts::Mounts;
use vtium_settings::{Trust, batch};

use super::batch::load_batch_with;
use super::frits::frit_struck;
use super::named::one_source;
use crate::helper::{FritText, OwnCell};
use crate::layout::Zoom;
use crate::scratch::Scratch;
use vtium_font::Width;

/// With nothing installed, no vtium.conf, no font sets, no font files
/// and no font helper, vtium draws its terminal in the compiled-in GNU
/// Unifont and Unscii-8, and its notices in its built-in font, "vtium
/// built-in", in the terminal's cell. At 16 pixels the notice's letters
/// differ from the terminal's Unifont, so the notice draws from neither
/// the terminal's font nor Unifont. The terminal moves to the built-in
/// font with card 5b of ADR-1204, which removes Unifont and Unscii-8
/// from the binary.
///
/// References:
/// - ADR-1204 D1: vtium's own messages always draw in the built-in
///   font, the acceptance check's unit test, and the note of 2026-11-15
///   on cards 5a and 5b.
#[test]
fn with_nothing_installed_a_notice_draws_in_the_built_in_font() {
    use crate::term::{Overlay, OverlayColors};
    use vtium_render::{Canvas, Config as RenderConfig, Notice, Orientation, Renderer, Surface};
    // an empty vtium.conf, an empty vconsole.conf and an empty kernel
    // command line name no font, so vtium asks for the default set;
    // with no font sets installed it draws its built-in fonts
    let config = crate::config::load(&[], &mut |_: &Path| Ok(None)).config;
    let default = wanted(&config, "", "").expect("the default set");
    let none = Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        ..Sources::default()
    };
    assert!(frit_named(&none, &default).is_err());
    let text = "Keyboard /dev/input/event0 was not found.";
    let overlay = Overlay::new(None, &[text], None, OverlayColors::default());
    for (height, grid) in [(8, (40, 30)), (16, (40, 15))] {
        let face = Chosen::BUILTINS.face(height, "C").expect("a built-in face");
        let cell = face.cell();
        let renderer = Renderer::new(
            vtium_font::Font::new(face),
            320,
            240,
            RenderConfig::default(),
        );
        let mut drawn = Canvas::new(320, 240, 0);
        overlay.draw(&renderer, grid, Orientation::Normal, &mut drawn);
        let surface = Surface {
            width: 320,
            height: 240,
            orientation: Orientation::Normal,
            depth: renderer.depth(),
        };
        let colors = OverlayColors::default().notice;
        let in_font = |font: &vtium_font::Font<DisplayFace>| {
            let mut c = Canvas::new(320, 240, 0);
            Notice::new(text).draw(font, surface, colors, None, &mut c);
            c
        };
        let own = vtium_font::Font::new(own_face(cell).expect("the built-in font"));
        assert!(
            drawn.pixels() == in_font(&own).pixels(),
            "{height}: vtium built-in"
        );
        if height == 16 {
            let terminal = vtium_font::Font::new(Chosen::BUILTINS.face(16, "C").unwrap());
            assert!(drawn.pixels() != in_font(&terminal).pixels(), "not Unifont");
        }
    }
}

// ADR-1302 D14: a glyph is squeezed when it is exactly twice its
// cells' width, or wider than them and 2 pixels more; the built-in
// GNU Unifont squeezes its 16-pixel Devanagari into one 8x16 cell,
// and neither a Latin letter nor, in tiny5's 6x9 cell, Unscii-8's
#[test]
fn a_built_in_glyph_is_squeezed_past_its_cells_and_two_pixels() {
    let c816 = Cell {
        width: 8,
        height: 16,
    };
    assert!(squeezed(16, c816, 1));
    assert!(squeezed(11, c816, 1));
    assert!(!squeezed(10, c816, 1));
    assert!(!squeezed(9, c816, 1));
    assert!(!squeezed(8, c816, 1));
    assert!(!squeezed(16, c816, 2));
    assert!(squeezed(32, c816, 2));
    assert!(builtin_squeezes('\u{915}', c816, "hi_IN"));
    assert!(!builtin_squeezes('A', c816, "hi_IN"));
    assert!(!builtin_squeezes('\u{4e00}', c816, "zh_CN"));
    let tiny5 = Cell {
        width: 6,
        height: 9,
    };
    assert!(!builtin_squeezes('A', tiny5, "C"));
}

fn config(args: &[&str]) -> Config {
    let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
    crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
}

// font-frits names the frits' directory, its default the one vtium is
// built with
#[test]
fn font_frits_names_the_frits_directory() {
    assert_eq!(Sources::configured(&config(&[])).frits, Path::new(FRITS));
    let sources = Sources::configured(&config(&["--font-frits=/srv/frits"]));
    assert_eq!(sources.frits, Path::new("/srv/frits"));
    assert!(sources.helper.is_none(), "the helper font-helper names");
}

#[test]
fn the_policy_is_the_faces_unless_a_key_sets_it() {
    let named = SpritePolicy::all(Source::Font).with(Range::Octants, Source::Sprites);
    assert_eq!(policy(&config(&[]), named), named);
    assert_eq!(
        policy(&config(&["--glyph-source=sprites"]), named),
        SpritePolicy::SPRITES
    );
    let one = policy(&config(&["--glyph-source-braille=auto"]), named);
    assert_eq!(one, named.with(Range::Braille, Source::Auto));
    // A range's own key wins over the key for all six.
    let both = policy(
        &config(&["--glyph-source=font", "--glyph-source-legacy=sprites"]),
        SpritePolicy::SPRITES,
    );
    assert_eq!(
        both,
        SpritePolicy::all(Source::Font).with(Range::LegacyComputing, Source::Sprites)
    );
}

// ADR-1200 D1: a frit's glyph-source= lines are its sprite policy, over
// the face's, the configuration's keys over both
#[test]
fn a_frits_glyph_source_is_its_policy_under_the_keys() {
    let face = SpritePolicy::SPRITES;
    assert_eq!(frit_policy("source=kbd:x\n", face), face);
    let frit = "source=kbd:x\nglyph-source=font # all six\nglyph-source-braille = auto\n";
    let own = SpritePolicy::all(Source::Font).with(Range::Braille, Source::Auto);
    assert_eq!(frit_policy(frit, face), own);
    // a value or a range vtium does not know is the helper's to refuse
    assert_eq!(
        frit_policy("glyph-source=x\nglyph-source-y=font\n", face),
        face
    );
    assert_eq!(
        policy(
            &config(&["--glyph-source-braille=sprites"]),
            frit_policy(frit, face)
        ),
        own.with(Range::Braille, Source::Sprites)
    );
}

#[test]
fn missing_is_box_or_hex() {
    assert_eq!(missing(&config(&[])), Missing::Replacement);
    assert_eq!(missing(&config(&["--missing=hex"])), Missing::Hex);
    assert_eq!(missing(&config(&["--missing=box"])), Missing::Replacement);
}

#[test]
fn the_automatic_size() {
    assert_eq!(auto_size(1366, 768, 8), 19);
    assert_eq!(auto_size(1280, 800, 8), 20);
    assert_eq!(auto_size(3840, 2160, 8), 54);
    assert_eq!(auto_size(640, 480, 8), 16);
    assert_eq!(auto_size(640, 400, 8), 16, "80 by 25 at 16 pixels");
    assert_eq!(auto_size(0, 768, 8), 16);
}

// vtium/vtium#118: where 16-pixel cells leave fewer than 80 by 25, the
// font's own smallest height.
#[test]
fn a_small_display_takes_the_fonts_own_height() {
    assert_eq!(auto_size(320, 240, 8), 8, "40 by 30 cells of 8x8");
    assert_eq!(auto_size(480, 320, 8), 8);
    assert_eq!(auto_size(632, 480, 8), 8, "79 columns at 16");
    assert_eq!(auto_size(640, 384, 8), 8, "24 rows at 16");
    assert_eq!(auto_size(320, 240, 12), 12);
    // A font no smaller than 16 stays at 16, as unifont does in a
    // CJK locale.
    assert_eq!(auto_size(320, 240, 16), 16);
    assert_eq!(auto_size(320, 240, 24), 16);
}

fn rungs(steps: &[u32]) -> Vec<Rung> {
    steps
        .iter()
        .map(|&step| Rung {
            step,
            min: step,
            end: u32::MAX,
        })
        .collect()
}

fn cell(width: u32, height: u32) -> Cell {
    Cell { width, height }
}

fn frit_font(stated: Option<Cell>) -> Chosen {
    let cell = stated.map_or_else(String::new, |c| format!("cell={}x{}\n", c.width, c.height));
    frit_of(&format!("source=kbd:x\n{cell}"), false)
}

// #145: the console reads a frit's cell as text, before its first frame
#[test]
fn a_frits_cell_is_read_from_its_text() {
    assert_eq!(frit_cell("source=kbd:x\n"), Ok(None));
    assert_eq!(
        frit_cell("# c\nsource=outline:/f.ttf\n cell = 10x20 # the cell\n"),
        Ok(Some(cell(10, 20)))
    );
    for (text, why) in [
        ("cell=10\n", "line 1: cell=10 is not WIDTHxHEIGHT"),
        ("\ncell=5x16\n", "line 2: cell=5x16 is not"),
        ("cell=8x97\n", "is not"),
        ("cell=08x16\n", "is not"),
        ("cell=+8x16\n", "is not"),
        ("cell=8x16\ncell=8x16\n", "line 2: cell= appears twice"),
    ] {
        let e = frit_cell(text).expect_err(text);
        assert!(e.contains(why), "{text:?}: {e}");
    }
    // a comment holding cell= states nothing
    assert_eq!(frit_cell("# cell=1x1\n"), Ok(None));
}

// #145: the built-in draws in the stated cell, the helper's glyphs at
// that cell, a zoom by its whole multiples
#[test]
fn a_stated_cell_is_the_faces_at_each_of_its_multiples() {
    let f = frit_font(Some(cell(10, 20)));
    let face = |h| f.face(h, "C").expect("a face");
    assert_eq!(face(20).cell(), cell(10, 20));
    assert_eq!(face(16).cell(), cell(10, 20), "never smaller than stated");
    assert_eq!(face(45).cell(), cell(20, 40));
    assert_eq!(face(20).size_step(), 20);
    assert_eq!((f.snap(16), f.snap(45)), (20, 40));
    assert_eq!(frit_font(None).snap(19), 19);
    assert_eq!(
        f.rungs("C"),
        [Rung {
            step: 20,
            min: 20,
            end: u32::MAX
        }]
    );
    assert_eq!(zoom(&f.rungs("C"), 20, Zoom::In), Some(40));
    assert_eq!(zoom(&f.rungs("C"), 20, Zoom::Out), None);
    // the display moves no stated cell
    assert_eq!(f.auto_height(3840, 2160, "C", 96.0), 20);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 20);
}

fn frit_of(text: &str, sized: bool) -> Chosen {
    let frit = FritText {
        name: "f".into(),
        text: text.into(),
    };
    Chosen::frit(frit, sized, None).expect("a frit")
}

// ADR-1200 D17: at 72 DPI, where a point is a pixel, an outline frit
// zooms by 1 pixel, from its stated cell, each height but the stated
// one as wide as the built-in font there
#[test]
fn an_outline_frit_at_72_dpi_zooms_by_one_pixel() {
    for text in [
        "source=outline:/f.ttf\ncell=10x20\n",
        "source=font:DejaVu Sans Mono\ncell=10x20\n",
        "source=kbd:x\nsource=outline:/f.ttf\ncell=10x20\n",
    ] {
        let f = frit_of(text, false);
        assert_eq!(f.zoom(20, Zoom::In, "C", 72.0), Some(21), "{text}");
        assert_eq!(f.zoom(20, Zoom::Out, "C", 72.0), Some(19), "{text}");
        assert_eq!(f.snap(21), 21, "{text}");
        let cell_at = |h| f.face(h, "C").expect("a face").cell();
        assert_eq!(cell_at(20), cell(10, 20), "{text}");
        assert_eq!(cell_at(21), cell(8, 21), "{text}");
        assert_eq!(cell_at(33), cell(16, 33), "{text}");
        assert_eq!(f.face(21, "C").expect("a face").size_step(), 1);
    }
    // down to the built-in font's smallest, which it draws in meanwhile
    let f = frit_of("source=outline:/f.ttf\n", false);
    assert_eq!(f.zoom(9, Zoom::Out, "C", 72.0), Some(8));
    assert_eq!(f.zoom(8, Zoom::Out, "C", 72.0), None);
}

// the operator's decision on #145, 2026-10-28: a bitmap frit with no
// cell stated asks for its fonts' own cell, and takes it at the largest
// whole multiple not over the automatic height, at least once; it zooms
// by the cell's height
#[test]
fn a_frits_own_cell_scales_by_whole_factors_under_the_automatic_height() {
    for text in ["source=kbd:x\n", "source=kbd:x\nsize=16\n"] {
        assert!(frit_of(text, false).wants_own_cell(16, false), "{text}");
        assert!(!frit_of(text, true).wants_own_cell(16, true), "{text}");
    }
    assert!(!frit_of("source=kbd:x\ncell=8x16\n", false).wants_own_cell(16, false));
    let mut f = frit_of("source=kbd:x\n", false);
    f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
    assert!(!f.wants_own_cell(16, false));
    // 1366x768's automatic 19, 1920x1080's 27 and 3840x2160's 54
    assert_eq!(f.auto_height(1366, 768, "C", 96.0), 16);
    assert_eq!(f.auto_height(1920, 1080, "C", 96.0), 16);
    assert_eq!(f.auto_height(3840, 2160, "C", 96.0), 48);
    // at least the cell, under #118's floor too
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 16);
    assert_eq!(f.face(16, "C").expect("a face").cell(), cell(8, 16));
    assert_eq!(f.face(48, "C").expect("a face").cell(), cell(24, 48));
    assert_eq!(zoom(&f.rungs("C"), 16, Zoom::In), Some(32));
    assert_eq!(zoom(&f.rungs("C"), 16, Zoom::Out), None);
    let mut f = frit_of("source=kbd:x\n", false);
    f.set_own_cell(OwnCell::Bitmap(cell(8, 8)), false);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 8);
    assert_eq!(f.auto_height(640, 480, "C", 96.0), 16);
    assert_eq!(f.face(16, "C").expect("a face").cell(), cell(16, 16));
}

// overhang part 1 (the maintainer, 2026-11-01): an outline frit with
// no cell stated asks the helper at each height it draws at, a
// configured font-size included, and takes each answer at its own
// height alone; at a height with no answer it is as wide as the
// built-in font, and it zooms by 1 pixel
#[test]
fn an_outline_frit_takes_the_helpers_cell_at_each_height_asked() {
    let mut f = frit_of("source=outline:/f.ttf\n", true);
    assert!(f.outline());
    assert!(f.wants_own_cell(16, true), "a configured size asks too");
    f.set_own_cell(OwnCell::Outline(cell(9, 16)), true);
    assert!(!f.wants_own_cell(16, true));
    assert!(f.wants_own_cell(17, true), "another height asks again");
    assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
    assert_eq!(f.face(17, "C").expect("a face").cell(), cell(8, 17));
    f.set_own_cell(OwnCell::Outline(cell(12, 22)), true);
    assert_eq!(f.face(22, "C").expect("a face").cell(), cell(12, 22));
    assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
    assert_eq!(f.snap(17), 17, "no whole multiples");
    // an outline frit that states a cell asks nothing
    let stated = frit_of("source=outline:/f.ttf\ncell=10x20\n", false);
    assert!(!stated.wants_own_cell(20, false));
}

// A frit whose first source is a bitmap font with strikes asks the
// helper for its cell at each height, with a font size configured or
// not, and keeps each answer for the heights it holds at: the height
// asked at, the strike's own, and the whole multiple between them.
// With no font size the cell is the strike's cell or its whole
// multiple. A configured font size keeps its height, as wide as the
// strike's whole multiple, which the helper centres in the cell. The
// helper's answers below are those of Terminus's strikes.
//
// References:
// - ADR-1204 D2: the strike for the automatic height, for a configured
//   font size and for each zoom step.
#[test]
fn a_frit_with_strikes_takes_the_strike_of_each_height() {
    let text =
        "source=bdf:/16.bdf\nsha256=0\nstrike=bdf:/12.bdf\nstrike=bdf:/32.bdf\nlayer=unifont\n";
    // the tallest Terminus strike no taller than each height
    let strike = |h: u32| match h {
        12..14 => cell(6, 12),
        14..16 => cell(7, 14),
        16..18 => cell(8, 16),
        18..20 => cell(10, 18),
        20..22 => cell(10, 20),
        22..24 => cell(11, 22),
        24..28 => cell(12, 24),
        28..32 => cell(14, 28),
        _ => cell(16, 32),
    };
    for (sized, rows) in [
        (
            true,
            [
                (12, cell(6, 12)),
                (16, cell(8, 16)),
                (18, cell(10, 18)),
                (20, cell(10, 20)),
                (23, cell(11, 23)),
                (24, cell(12, 24)),
                (28, cell(14, 28)),
                (32, cell(16, 32)),
                (40, cell(16, 40)),
                (64, cell(32, 64)),
            ],
        ),
        (
            false,
            [
                (12, cell(6, 12)),
                (16, cell(8, 16)),
                (18, cell(10, 18)),
                (20, cell(10, 20)),
                (23, cell(11, 22)),
                (24, cell(12, 24)),
                (28, cell(14, 28)),
                (32, cell(16, 32)),
                (40, cell(16, 32)),
                (64, cell(32, 64)),
            ],
        ),
    ] {
        let mut f = frit_of(text, sized);
        assert!(f.struck() && !f.outline(), "{text}");
        for (height, want) in rows {
            assert!(f.wants_own_cell(height, sized), "{height}, sized {sized}");
            assert_eq!(f.strike_at(height), None);
            f.set_own_cell_at(OwnCell::Bitmap(strike(height)), height, sized);
            assert!(!f.wants_own_cell(height, sized));
            assert_eq!(f.strike_at(height), Some(strike(height)));
            let drawn = f.snap(height);
            assert_eq!(drawn, want.height, "{height}, sized {sized}");
            let face = f.face(drawn, "C").expect("a face");
            assert_eq!(face.cell(), want, "{height}, sized {sized}");
        }
    }
    // a height with no answer yet draws as wide as the built-in font
    let f = frit_of(text, true);
    assert_eq!(f.face(24, "C").expect("a face").cell(), cell(8, 24));
    // a stated cell wins, and asks nothing
    let stated = frit_of(&format!("{text}cell=10x20\n"), false);
    assert!(!stated.struck() && !stated.wants_own_cell(20, false));
}

// Only a first source that is a bitmap font followed by its own
// strike= lines makes a frit with strikes. A bitmap source with no
// strikes, a later source's strikes, and an outline source's strikes
// keep the rules they have.
#[test]
fn only_a_bitmap_first_source_with_strikes_is_struck() {
    for (text, struck) in [
        ("source=bdf:/16.bdf\nstrike=bdf:/12.bdf\n", true),
        (
            "# strike=bdf:/12.bdf\nsource=psf:/16.psf\nversion=1\nstrike=bdf:/12.bdf # a comment\n",
            true,
        ),
        ("source=kbd:x\n", false),
        (
            "source=kbd:x\nsource=bdf:/16.bdf\nstrike=bdf:/12.bdf\n",
            false,
        ),
        ("source=kbd:x\nlayer=unifont\nstrike=bdf:/12.bdf\n", false),
        ("source=outline:/f.ttf\nstrike=bdf:/12.bdf\n", false),
        ("source=font:Terminus\nstrike=bdf:/12.bdf\n", false),
        ("strike=bdf:/12.bdf\n", false),
        // the helper ignores a strike= under a built-in font or the
        // system's console font, so the set has no strikes
        ("source=builtin:unifont\nstrike=kbd:ter-v16n\n", false),
        ("source=builtin:unscii-8\nstrike=bdf:/12.bdf\n", false),
        ("source=vconsole\nstrike=kbd:ter-v16n\n", false),
        ("source=console:2\nstrike=kbd:ter-v16n\n", false),
    ] {
        assert_eq!(frit_struck(text), struck, "{text}");
    }
    // a bitmap frit with no strikes asks once, and not with a font size
    let mut f = frit_of("source=kbd:x\n", false);
    f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
    assert!(!f.struck() && !f.wants_own_cell(24, false));
    assert!(!frit_of("source=kbd:x\n", true).wants_own_cell(24, true));
}

// A layer's keys of the whole set are left blank where the layer is
// put in place, so the console reads the set's own cell=, size=,
// line-spacing= and glyph-source= lines and never a layer's, while the
// layer's source stays in the set.
//
// References:
// - ADR-1510 D2: a key of the whole set in a layer is ignored alone.
// - ADR-1204 D5: the layer's lines in place of its layer= line.
#[test]
fn a_layers_keys_of_the_whole_set_do_not_reach_the_console() {
    let sym = "role=symbols\nsource=font:S\ncell=12x24\nsize=20\nline-spacing=50\nglyph-source=font\nglyph-source-braille=auto\nstrike=kbd:y\n";
    let mut read = |_: &str| Ok(Some(sym.to_owned()));
    let spliced = layer::expand("cell=8x16\nsource=kbd:x\nlayer=sym\n", &mut read);
    assert_eq!(spliced.placed.len(), 1);
    assert_eq!(spliced.ignored.len(), 5);
    assert!(spliced.text.contains("source=font:S\n"));
    let text = spliced.text.as_str();
    assert_eq!(frit_cell(text), Ok(Some(cell(8, 16))));
    assert_eq!(frit_size(text), Ok(None));
    assert_eq!(frit_line_spacing(text), Ok(None));
    let face = SpritePolicy::SPRITES;
    assert_eq!(frit_policy(text, face), face);
    // the layer's strike= describes the layer's source, not the set's
    // first source, so the set has no strikes
    assert!(!frit_struck(text));
}

// overhang part 1: the helper's answer, not the frit's text, says what
// kind of font the first source is. A font: source the text reads as
// an outline font that resolves to a bitmap font answers with its own
// cell, and the frit then zooms by that cell's height; a source the
// text reads as a bitmap font answered as an outline font zooms by 1
#[test]
fn the_helpers_answer_decides_whether_a_frit_is_outline() {
    let mut f = frit_of("source=font:Terminus\n", false);
    assert!(f.outline(), "the text's guess");
    assert!(f.wants_own_cell(16, false));
    f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
    assert!(!f.outline());
    assert!(!f.wants_own_cell(16, false));
    assert_eq!(f.face(32, "C").expect("a face").cell(), cell(16, 32));
    assert_eq!(zoom(&f.rungs("C"), 16, Zoom::In), Some(32));
    // with font-size configured the bitmap answer's cell does not count
    let mut f = frit_of("source=font:Terminus\n", true);
    f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), true);
    assert!(!f.outline());
    assert!(!f.wants_own_cell(19, true));
    assert_eq!(f.face(19, "C").expect("a face").cell(), cell(8, 19));
    let mut f = frit_of("source=kbd:missing\nsource=outline:/f.ttf\n", false);
    assert!(f.outline(), "the text names an outline source");
    let mut f2 = frit_of("source=kbd:missing\n", false);
    f2.set_own_cell(OwnCell::Outline(cell(9, 16)), false);
    assert!(f2.outline());
    assert_eq!(f2.face(16, "C").expect("a face").cell(), cell(9, 16));
    f.set_own_cell(OwnCell::Outline(cell(9, 16)), false);
    assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
}

// ADR-1200 D1, D17: an outline frit's size= alone is its starting
// size, the cell as wide as the built-in font at its height; points do
// not apply to a bitmap frit, whose size= the console leaves aside (the
// operator's decision on #145, 2026-10-28)
#[test]
fn size_alone_starts_an_outline_frit_and_cell_wins_over_it() {
    let f = frit_of("source=outline:/f.ttf\nsize=24\n", false);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 32);
    assert_eq!(f.face(32, "C").expect("a face").cell(), cell(16, 32));
    let f = frit_of("source=kbd:x\nsize=24\n", false);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 8, "the display's");
    // the cell is cell='s; size= is the glyph's size in it
    let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=16\n", false);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 20);
    assert_eq!(f.face(20, "C").expect("a face").cell(), cell(10, 20));
    // a configured font-size wins over both
    let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=16\n", true);
    assert_eq!(f.auto_height(320, 240, "C", 96.0), 8);
    for (text, why) in [
        ("size=24\nsize=32\n", "line 2: size= appears twice"),
        (
            "size=5\n",
            "line 1: size=5 is not a size in points from 6 to 96",
        ),
        ("size=016\n", "line 1: size=016 is not"),
    ] {
        let frit = FritText {
            name: "f".into(),
            text: text.into(),
        };
        let e = Chosen::frit(frit, false, None).expect_err(text);
        assert!(e.contains(why), "{text:?}: {e}");
    }
}

/// The rows of `c`'s glyph in `face`.
fn glyph_rows(face: &DisplayFace, c: char) -> Vec<Vec<u8>> {
    let g = face
        .draw(&[c], vtium_font::Style::default())
        .expect("a glyph");
    g.rows().map(<[u8]>::to_vec).collect()
}

// ADR-1302 D13: the frit's fit= reaches the glyphs the console draws
// itself in the frit's cell. Box drawing fills the taller cell by
// default; under the frit's fit=natural for U+2500 to U+257F a box line
// keeps the text band's rows, with the added rows blank
#[test]
fn a_frits_fit_places_the_built_in_glyphs_in_its_cell() {
    let natural = frit_of(
        "source=kbd:x\nsource=kbd:y\ncover=U+2500-U+257F\nfit=natural\nline-spacing=20\n",
        false,
    );
    let plain = frit_of("source=kbd:x\nline-spacing=20\n", false);
    let at = |f: &Chosen| f.face_spaced(16, "C", None).expect("a face");
    // 20% of 16 is 3.2: 3 rows, 2 above and 1 below
    let inked = |f: &Chosen| -> Vec<usize> {
        glyph_rows(&at(f), '\u{2502}')
            .iter()
            .enumerate()
            .filter(|(_, row)| row.iter().any(|&p| p != 0))
            .map(|(y, _)| y)
            .collect()
    };
    assert_eq!(inked(&plain), (0..19).collect::<Vec<_>>());
    assert_eq!(inked(&natural), (2..18).collect::<Vec<_>>());
}

// ADR-1302 D13: a frit's `line-spacing=`, and vtium.conf's
// `font-line-spacing` over it, make the cell vtium works out taller by
// a whole percentage of the text's height, split above and below the
// text band with the odd pixel above; the text band stays the cell the
// face had without spacing
#[test]
fn line_spacing_makes_the_cell_taller_split_above_and_below() {
    let plain = frit_of("source=kbd:x\n", false);
    let spaced = frit_of("source=kbd:x\nline-spacing=20\n", false);
    let at = |f: &Chosen, spacing| f.face_spaced(16, "C", spacing).expect("a face");
    assert_eq!(at(&plain, None).cell(), cell(8, 16));
    assert_eq!(at(&plain, None).band(), Band::NONE);
    // 20% of 16 is 3.2: 3 rows, 2 above and 1 below
    let face = at(&spaced, None);
    assert_eq!(face.cell(), cell(8, 19));
    assert_eq!(face.band(), Band { above: 2, below: 1 });
    // vtium.conf's font-line-spacing wins over the frit's, 0 included
    assert_eq!(at(&spaced, Some(50)).cell(), cell(8, 24));
    assert_eq!(at(&spaced, Some(0)).cell(), cell(8, 16));
    assert_eq!(at(&plain, Some(25)).band(), Band { above: 2, below: 2 });
    // the built-in fonts with no frit take it too
    let face = Chosen::BUILTINS
        .face_spaced(16, "C", Some(25))
        .expect("a face");
    assert_eq!(
        (face.cell(), face.band()),
        (cell(8, 20), Band { above: 2, below: 2 })
    );
    // a letter keeps its rows in the text band, blank above and below,
    // and a box line, drawn by the built-in font, stretches to fill
    let unspaced = Chosen::BUILTINS.face(16, "C").expect("a face");
    let a = glyph_rows(&unspaced, 'A');
    let blank = vec![0u8; a[0].len()];
    let mut want = vec![blank.clone(); 2];
    want.extend(a.iter().cloned());
    want.extend(vec![blank; 2]);
    assert_eq!(glyph_rows(&face, 'A'), want);
    let line = glyph_rows(&unspaced, '\u{2502}');
    let mut want = vec![line[0].clone(); 2];
    want.extend(line.iter().cloned());
    want.extend(vec![line[15].clone(); 2]);
    assert_eq!(glyph_rows(&face, '\u{2502}'), want);
    // the lines move down with the text band
    let (u, d) = (unspaced.decorations(), face.decorations());
    assert_eq!((d.underline, d.strike), (u.underline + 2, u.strike + 2));
    for (text, why) in [
        (
            "line-spacing=101\n",
            "line 1: line-spacing=101 is not a whole percentage from 0 to 100",
        ),
        (
            "line-spacing=5\nline-spacing=5\n",
            "line 2: line-spacing= appears twice",
        ),
    ] {
        let frit = FritText {
            name: "f".into(),
            text: text.into(),
        };
        let e = Chosen::frit(frit, false, None).expect_err(text);
        assert!(e.contains(why), "{text:?}: {e}");
    }
}

// ADR-1302 D13: a cell the frit or the look states is exact and takes
// no spacing; where a configured font-size overrides the frit's cell=,
// the cell vtium works out takes it
#[test]
fn a_stated_cell_takes_no_spacing_unless_font_size_overrides_it() {
    let text = "source=kbd:x\ncell=10x20\nline-spacing=50\n";
    let stated = frit_of(text, false);
    let face = stated.face_spaced(20, "C", Some(50)).expect("a face");
    assert_eq!((face.cell(), face.band()), (cell(10, 20), Band::NONE));
    let sized = frit_of(text, true);
    let face = sized.face_spaced(20, "C", None).expect("a face");
    assert_eq!(face.cell().height, 30);
    assert_eq!(face.band(), Band { above: 5, below: 5 });
    let look = Chosen::frit(
        FritText {
            name: "f".into(),
            text: text.into(),
        },
        true,
        Some(cell(12, 24)),
    )
    .expect("a frit");
    let face = look.face_spaced(24, "C", Some(50)).expect("a face");
    assert_eq!((face.cell(), face.band()), (cell(12, 24), Band::NONE));
    let builtins = Chosen::builtins(Some(cell(12, 24)));
    let face = builtins.face_spaced(24, "C", Some(50)).expect("a face");
    assert_eq!((face.cell(), face.band()), (cell(12, 24), Band::NONE));
}

// ADR-1403 D5, ADR-1200 D16: the look is more specific than its frit,
// so a cell the look states wins over the frit's cell= and size=, and
// the frit asks the helper for no cell of its own
#[test]
fn a_looks_cell_wins_over_its_frits_cell_and_size() {
    let look = Some(cell(12, 24));
    for text in [
        "source=outline:/f.ttf\ncell=10x20\nsize=16\n",
        "source=outline:/f.ttf\nsize=16\n",
        "source=kbd:x\ncell=10x20\n",
        "source=kbd:x\n",
    ] {
        let frit = FritText {
            name: "f".into(),
            text: text.into(),
        };
        let f = Chosen::frit(frit, false, look).expect("a frit");
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 24, "{text:?}");
        assert_eq!(
            f.face(24, "C").expect("a face").cell(),
            cell(12, 24),
            "{text:?}"
        );
        assert!(!f.wants_own_cell(24, false), "{text:?}");
    }
}

// ADR-1200 D7: a look's 6x16 cell is 16 pixels tall but narrower than
// unifont's 8 columns, so the built-ins draw Unscii-8 in it, centered
// down the cell and on it, its first column in the cell before it, and
// OSC 50's query names the strike that draws
#[test]
fn a_looks_cell_too_narrow_for_unifont_draws_and_names_unscii_8() {
    let font = Chosen::builtins(Some(cell(6, 16)));
    assert_eq!(font.name(16, "C"), "unscii-8");
    let a = vtium_font::Font::new(font.face(16, "C").expect("a face"))
        .glyph(&['A'], Width::Narrow, vtium_font::Style::default())
        .expect("a glyph");
    assert_eq!((a.width(), a.height()), (6, 16));
    let unscii = Chosen::Builtin {
        name: Some("unscii-8"),
        cell: None,
    }
    .face(8, "C")
    .expect("unscii-8")
    .draw(&['A'], vtium_font::Style::default())
    .expect("a glyph");
    for y in 0..8 {
        for x in 0..6 {
            assert_eq!(a.pixel(x, y + 4), unscii.pixel(x + 1, y), "{x},{y}");
        }
    }
    // Unscii-8's first column lies in the cell before, in the glyph's
    // ink box where it holds ink
    let first = (0..8).any(|y| unscii.pixel(0, y).is_some_and(|p| p[0] > 0));
    assert_eq!(a.ink().map(|i| i.left()), first.then_some(-1));
    // a zoom to twice the cell, 12x32, fits unifont's 8x16 once, so
    // unifont draws there, and is named
    assert_eq!(font.name(32, "C"), "unifont");
    assert_eq!(font.face(32, "C").expect("a face").cell(), cell(12, 32));
    // a zh, ja or ko locale draws unifont at every size, and names it
    assert_eq!(font.name(16, "ja_JP.UTF-8"), "unifont");
}

// ADR-1200 D7, D17, ADR-1403 D5: the built-in fonts in a look's cell
// draw in that cell whatever the display's size, fitted to it as in a
// frit's stated cell, and a zoom steps by the cell's whole multiples;
// a built-in font named keeps to its own strike in the cell
#[test]
fn the_built_in_fonts_draw_in_a_looks_cell() {
    let look = Some(cell(12, 24));
    let plain = |name| {
        Chosen::Builtin { name, cell: None }
            .face(16, "C")
            .expect("a face")
            .draw(&['A'], vtium_font::Style::default())
            .expect("a glyph")
    };
    for (font, own) in [
        (Chosen::builtins(look), plain(None)),
        (
            builtin_named("unifont", look).expect("unifont"),
            plain(Some("unifont")),
        ),
        (
            builtin_named("unscii-8", look).expect("unscii-8"),
            Chosen::Builtin {
                name: Some("unscii-8"),
                cell: None,
            }
            .face(8, "C")
            .expect("unscii-8")
            .draw(&['A'], vtium_font::Style::default())
            .expect("a glyph"),
        ),
    ] {
        let name = font.name(24, "C").to_owned();
        assert_eq!(font.auto_height(1920, 1080, "C", 96.0), 24, "{name}");
        assert_eq!(font.snap(30), 24, "{name}");
        let face = font.face(24, "C").expect("a face");
        assert_eq!(face.cell(), cell(12, 24), "{name}");
        let a = face
            .draw(&['A'], vtium_font::Style::default())
            .expect("a glyph");
        assert_eq!((a.width(), a.height()), (12, 24), "{name}");
        // the font's own glyph, centered in the larger cell
        let (dx, dy) = ((12 - own.width()) / 2, (24 - own.height()) / 2);
        for y in 0..own.height() {
            for x in 0..own.width() {
                assert_eq!(a.pixel(x + dx, y + dy), own.pixel(x, y), "{name} {x},{y}");
            }
        }
        assert_eq!(font.zoom(24, Zoom::In, "C", 96.0), Some(48), "{name}");
        assert_eq!(font.zoom(24, Zoom::Out, "C", 96.0), None, "{name}");
        assert_eq!(
            font.face(48, "C").expect("a face").cell(),
            cell(24, 48),
            "{name}"
        );
    }
}

// the operator's decision on #145, 2026-10-28: an outline frit's size=
// is points, pixels = points x DPI / 72; a 27-inch 2560x1440 monitor is
// about 109 DPI, and a display that measures none is assumed 96
#[test]
fn an_outline_frits_size_is_points_at_the_displays_density() {
    let f = frit_of("source=outline:/f.ttf\nsize=12\n", false);
    assert_eq!(f.auto_height(2560, 1440, "C", 108.86), 18);
    assert_eq!(f.auto_height(2560, 1440, "C", 96.0), 16);
    assert_eq!(f.auto_height(320, 240, "C", 300.0), 50);
    // cell= stays in pixels, whatever the density
    let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=12\n", false);
    assert_eq!(f.auto_height(2560, 1440, "C", 300.0), 20);
}

// the operator's decision on #145, 2026-10-28: an outline frit zooms 1
// pt at a time, converted to pixels, to the next point whose pixels
// differ
#[test]
fn an_outline_frit_zooms_by_one_point() {
    let f = frit_of("source=outline:/f.ttf\nsize=12\n", false);
    // 12 pt at 96 DPI is 16 px; 13 pt is 17.3, 14 pt 18.7 and 11 pt 14.7
    assert_eq!(f.zoom(16, Zoom::In, "C", 96.0), Some(17));
    assert_eq!(f.zoom(17, Zoom::In, "C", 96.0), Some(19));
    assert_eq!(f.zoom(16, Zoom::Out, "C", 96.0), Some(15));
    // at 300 DPI a point is over 4 pixels: 12 pt is 50, 13 pt 54.2
    assert_eq!(f.zoom(50, Zoom::In, "C", 300.0), Some(54));
    assert_eq!(f.zoom(50, Zoom::Out, "C", 300.0), Some(46));
    // at 50 DPI 15 pt is 10.4 px, so in from 10 is 16 pt's 11
    assert_eq!(f.zoom(10, Zoom::In, "C", 50.0), Some(11));
    // down to the built-in font's smallest, and not in past 150
    assert_eq!(f.zoom(9, Zoom::Out, "C", 96.0), Some(8));
    assert_eq!(f.zoom(8, Zoom::Out, "C", 96.0), None);
    assert_eq!(f.zoom(150, Zoom::In, "C", 96.0), Some(151));
    assert_eq!(f.zoom(151, Zoom::In, "C", 96.0), None);
    // a bitmap frit and the built-ins step by their cells, at any density
    let b = Chosen::BUILTINS;
    assert_eq!(b.zoom(16, Zoom::In, "C", 300.0), Some(32));
    let mut k = frit_of("source=kbd:x\n", false);
    k.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
    assert_eq!(k.zoom(16, Zoom::In, "C", 300.0), Some(32));
}

// #145: under 16 pixels the smaller strike, Unscii-8, draws
#[test]
fn a_small_stated_cell_draws_unscii_8_in_it() {
    let f = frit_font(Some(cell(7, 12)));
    let face = f.face(12, "C").expect("a face");
    assert_eq!(face.cell(), cell(7, 12));
    let a = vtium_font::Font::new(face)
        .glyph(&['A'], Width::Narrow, vtium_font::Style::default())
        .expect("a glyph");
    assert_eq!((a.width(), a.height()), (7, 12));
    // the 8th column, past the cell's right edge, is blank
    assert!(a.ink().is_none());
    let unscii = Chosen::Builtin {
        name: Some("unscii-8"),
        cell: None,
    }
    .face(8, "C")
    .expect("unscii-8")
    .draw(&['A'], vtium_font::Style::default())
    .expect("a glyph");
    // Unscii-8's 8x8, two rows down, its first 7 columns in the cell
    for y in 0..8 {
        for x in 0..7 {
            assert_eq!(a.pixel(x, y + 2), unscii.pixel(x, y), "{x},{y}");
        }
    }
}

// covers: text.overhang
// ADR-1200 D7, ADR-1302 D14: Tiny5 Mono's 6x9 cell is narrower than
// Unscii-8's 8x8, so the built-in glyph keeps all 8 of its columns,
// centred on the cell: its first column lies in the cell before it and
// its 8th, which Unscii-8 leaves blank, in the cell after it. It is
// never squeezed, since it is at most 2 pixels wider than the cell. It
// sits in the top 8 rows, its baseline on Tiny5 Mono's, and the 9th row
// is blank.
#[test]
fn unscii_8_in_a_narrower_cell_keeps_every_column() {
    let f = frit_font(Some(cell(6, 9)));
    let face = vtium_font::Font::new(f.face(9, "C").expect("a face"));
    assert_eq!(face.cell(), cell(6, 9));
    let builtin = Chosen::Builtin {
        name: Some("unscii-8"),
        cell: None,
    }
    .face(8, "C")
    .expect("unscii-8");
    let rows = |px: &[u8], w: usize| -> Vec<String> {
        px.chunks_exact(w)
            .map(|r| r.iter().map(|&b| if b > 0 { '#' } else { '.' }).collect())
            .collect()
    };
    let draw = |c| {
        face.glyph(&[c], Width::Narrow, vtium_font::Style::default())
            .expect("a glyph")
    };
    let m = draw('M');
    assert_eq!((m.width(), m.height()), (6, 9));
    let ink = m.ink().expect("an ink box");
    assert_eq!((ink.left(), ink.top(), ink.width()), (-1, 0, 7));
    assert_eq!(
        rows(ink.pixels(), 7),
        [
            "##...##", "###.###", "#######", "##.#.##", "##...##", "##...##", "##...##", ".......",
            "......."
        ],
        "Unscii-8's M, its first column in the cell before"
    );
    for c in ['A', 'M', 'g', 'y', '#', '|'] {
        let (g, own) = (draw(c), builtin.draw(&[c], vtium_font::Style::default()));
        let own = own.expect("a glyph");
        for y in 0..8 {
            for x in 0..6 {
                assert_eq!(g.pixel(x, y), own.pixel(x + 1, y), "{c} at {x},{y}");
            }
        }
        assert!(
            (0..6).all(|x| g.pixel(x, 8) == Some(&[0][..])),
            "{c}'s 9th row"
        );
        assert!(g.pixels().iter().all(|&p| p == 0 || p == 255), "{c}");
    }
}

// ADR-1200 D17: with no stated cell, the display's: the automatic
// height, #118's floor first, as wide as the built-in font
// is at that height
#[test]
fn with_no_stated_cell_the_cell_comes_from_the_display() {
    let f = frit_font(None);
    let at = |w, h| {
        let height = f.auto_height(w, h, "C", 96.0);
        f.face(height, "C").expect("a face").cell()
    };
    assert_eq!(at(320, 240), cell(8, 8), "40 by 30");
    assert_eq!(at(640, 480), cell(8, 16), "80 by 30");
    assert_eq!(at(1366, 768), cell(8, 19), "unifont's 8 at 19");
    assert_eq!(at(1920, 1080), cell(8, 27), "unifont's 8 at 27");
    assert_eq!(at(3840, 2160), cell(24, 54), "unifont's 24 at 54");
    assert_eq!(f.face(27, "C").expect("a face").size_step(), 16);
    // unifont alone in a zh, ja or ko locale: 16 at the floor
    let height = f.auto_height(320, 240, "ja_JP.UTF-8", 96.0);
    assert_eq!(
        f.face(height, "ja_JP.UTF-8").expect("a face").cell(),
        cell(8, 16)
    );
}

// A font set's `size=` is read within the size bounds that vtium-config
// reads it within, never the character cell's, at both ends.
#[test]
fn a_font_sets_size_is_read_within_the_size_bounds() {
    use crate::config::keys::{FRIT_SIZE_MAX, FRIT_SIZE_MIN};
    for (size, read) in [
        (FRIT_SIZE_MIN - 1, None),
        (FRIT_SIZE_MIN, Some(FRIT_SIZE_MIN)),
        (FRIT_SIZE_MAX, Some(FRIT_SIZE_MAX)),
        (FRIT_SIZE_MAX + 1, None),
    ] {
        let got = frit_size(&format!("size={size}\n"));
        match read {
            Some(points) => assert_eq!(got, Ok(Some(points)), "{size}"),
            None => assert_eq!(
                got,
                Err(format!(
                    "line 1: size={size} is not a size in points from {FRIT_SIZE_MIN} to {FRIT_SIZE_MAX}"
                )),
                "{size}"
            ),
        }
    }
}

// ADR-1302 D13: check-config names a frit's line-spacing= that
// vtium-config refuses, for which vtium draws its built-in font
#[test]
fn check_config_names_a_line_spacing_the_frit_reader_refuses() {
    assert!(frit_warnings("f", "source=kbd:x\nline-spacing=0\n").is_empty());
    assert!(frit_warnings("f", "source=kbd:x\nline-spacing=100\n").is_empty());
    assert_eq!(
        frit_warnings("f", "source=kbd:x\nline-spacing=120\n"),
        [
            "font set f: line 2: line-spacing=120 is not a whole percentage from 0 to 100, so vtium uses its built-in font"
        ]
    );
    for bad in ["010", "+5", "5%", "-1", "1.5"] {
        assert_eq!(
            frit_warnings("f", &format!("line-spacing={bad}\n")).len(),
            1,
            "{bad}"
        );
    }
}

#[test]
fn check_config_names_a_cell_that_does_not_suit_its_frit() {
    assert!(frit_warnings("f", "source=kbd:x\n").is_empty());
    assert!(frit_warnings("f", "source=builtin:unifont\ncell=8x16\n").is_empty());
    assert_eq!(
        frit_warnings("f", "cell=8\n"),
        [
            "font set f: line 1: cell=8 is not WIDTHxHEIGHT in pixels, each from 6 to 96, so vtium uses its built-in font"
        ]
    );
    assert_eq!(
        frit_warnings(
            "f",
            "source=outline:/f.ttf\nsource=builtin:unifont\nsource=builtin:unscii-8\ncell=6x12\n"
        ),
        [
            "font set f: its 6x12 character cell is smaller than the built-in font's 8x8, so the built-in font is cut to fit until the font set's characters are ready",
            "font set f: line 2: the characters of source=builtin:unifont (8x16) do not fit the 6x12 character cell, so vtium does not use that source",
            "font set f: line 3: the characters of source=builtin:unscii-8 (8x8) do not fit the 6x12 character cell, so vtium does not use that source",
        ]
    );
}

#[test]
fn zoom_steps_through_the_strikes_and_their_multiples() {
    let one = rungs(&[16]);
    assert_eq!(zoom(&one, 16, Zoom::In), Some(32));
    assert_eq!(zoom(&one, 32, Zoom::Out), Some(16));
    assert_eq!(zoom(&one, 19, Zoom::In), Some(32));
    assert_eq!(zoom(&one, 19, Zoom::Out), Some(16));
    // Two strikes: 12 and 16 and their multiples.
    let two = rungs(&[12, 16]);
    let mut up = vec![12];
    while let Some(next) = zoom(&two, *up.last().unwrap(), Zoom::In) {
        up.push(next);
    }
    assert_eq!(
        up[..9],
        [12, 16, 24, 32, 36, 48, 60, 64, 72],
        "each strike's multiples, in order"
    );
    assert_eq!(zoom(&two, 24, Zoom::Out), Some(16));
    assert_eq!(zoom(&two, 16, Zoom::Out), Some(12));
}

#[test]
fn zoom_stops_at_the_smallest_and_past_150() {
    let two = rungs(&[12, 16]);
    assert_eq!(zoom(&two, 12, Zoom::Out), None);
    assert_eq!(zoom(&two, 8, Zoom::Out), None);
    assert_eq!(zoom(&rungs(&[16]), 150, Zoom::In), Some(160));
    assert_eq!(zoom(&rungs(&[16]), 151, Zoom::In), None);
}

// ADR-1200 D7: Unscii-8 under 16 pixels and unifont from 16, but in a
// zh, ja or ko locale.
#[test]
fn the_builtins_step_from_unscii_8_to_unifont() {
    let b = Chosen::BUILTINS;
    let r = b.rungs("en_US.UTF-8");
    assert_eq!(zoom(&r, 8, Zoom::In), Some(16));
    assert_eq!(zoom(&r, 16, Zoom::In), Some(32));
    assert_eq!(zoom(&r, 16, Zoom::Out), Some(8));
    assert_eq!(zoom(&r, 12, Zoom::Out), Some(8));
    assert_eq!(zoom(&r, 8, Zoom::Out), None);
    assert_eq!(b.smallest("en_US.UTF-8"), 8);
    assert_eq!((b.name(8, "C"), b.name(16, "C")), ("unscii-8", "unifont"));
    let cell = |h: u32, locale: &str| {
        let c = b.face(h, locale).expect("a face").cell();
        (c.width, c.height)
    };
    assert_eq!(cell(8, "C"), (8, 8));
    assert_eq!(cell(16, "C"), (8, 16));
    assert_eq!(cell(24, "C"), (8, 16));
    assert_eq!(cell(32, "C"), (16, 32));
    // In a CJK locale, unifont at every size.
    let r = b.rungs("ja_JP.UTF-8");
    assert_eq!(zoom(&r, 16, Zoom::Out), None);
    assert_eq!(b.smallest("zh_CN.UTF-8"), 16);
    assert_eq!(cell(8, "ko_KR.UTF-8"), (8, 16));
    assert_eq!(b.name(8, "ko_KR.UTF-8"), "unifont");
    // One built-in named keeps to its own steps.
    let unscii = Chosen::Builtin {
        name: Some("unscii-8"),
        cell: None,
    };
    assert_eq!(zoom(&unscii.rungs("C"), 8, Zoom::In), Some(16));
    assert_eq!(unscii.face(16, "C").expect("a face").cell().width, 16);
    let unifont = Chosen::Builtin {
        name: Some("unifont"),
        cell: None,
    };
    assert_eq!(unifont.smallest("C"), 16);
}

/// Each font file the shipped font set or layer `rel`, under
/// vtium-config's `frits` folder, names by its path.
fn shipped_paths(rel: &str) -> Vec<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vtium-config/frits")
        .join(rel);
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter_map(|l| {
            l.strip_prefix("source=")
                .or_else(|| l.strip_prefix("strike="))
        })
        .filter_map(|v| v.split_once(':').map(|(_, p)| PathBuf::from(p)))
        .collect()
}

/// The batch [`batch::Batch::system`] builds from the batch file
/// `file`, vconsole.conf at `vconsole` and the kernel command line at
/// `cmdline`, read under `trust`.
fn system_batch(file: &Path, vconsole: &Path, cmdline: &Path, trust: &Trust) -> batch::Read {
    let places = batch::Places {
        file: file.to_path_buf(),
        vconsole_conf: vconsole.to_path_buf(),
        cmdline: cmdline.to_path_buf(),
        ..batch::Places::system()
    };
    batch::Batch::system(&places, trust)
}

// With no batch file, the
// batch is the list of a fresh install. It holds every font file the
// default set terminus and its fallback layer name, and the console
// font vconsole.conf names, so the default set draws with no setup. A
// batch file that root wrote replaces the list.
//
// References:
// - ADR-1205 D4, its note of 2026-11-18: with no batch file, the batch is the list of a fresh install.
#[test]
fn with_no_batch_file_the_default_set_and_the_console_font_are_active() {
    let d = Scratch::new("fresh-batch");
    let vconsole = d.join("vconsole.conf");
    std::fs::write(
        &vconsole,
        "FONT=/usr/share/kbd/consolefonts/ter-v16n.psf.gz\n",
    )
    .unwrap();
    let file = d.join("batch.conf");
    let read = system_batch(&file, &vconsole, &d.join("cmdline"), &trust());
    assert_eq!(
        read.findings,
        [batch::Finding::Missing { path: file.clone() }]
    );
    let default_set = shipped_paths("console/terminus.vtfrit");
    let fallback = shipped_paths("layers/unifont.vtlayer");
    assert_eq!((default_set.len(), fallback.len()), (18, 1));
    for path in default_set.iter().chain(&fallback) {
        assert!(read.batch.holds(path), "{} is not active", path.display());
    }
    assert!(
        read.batch
            .holds(Path::new("/usr/share/kbd/consolefonts/ter-v16n.psf.gz"))
    );
    assert!(
        !read
            .batch
            .holds(Path::new("/usr/share/fonts/TTF/DejaVuSansMono.ttf"))
    );
    // a batch file replaces the fresh list
    std::fs::write(&file, "/usr/share/fonts/TTF/DejaVuSansMono.ttf\n").unwrap();
    let me = rustix::process::geteuid().as_raw();
    let read = system_batch(
        &file,
        &vconsole,
        &d.join("cmdline"),
        &Trust::running_as(me).below(&d),
    );
    assert_eq!(read.findings, []);
    assert!(
        read.batch
            .holds(Path::new("/usr/share/fonts/TTF/DejaVuSansMono.ttf"))
    );
    assert!(!read.batch.holds(&default_set[0]));
}

#[test]
fn the_font_named_is_the_configurations_then_vconsoles() {
    let conf = "FONT=ter-v16n\n";
    assert_eq!(
        wanted(&config(&[]), conf, ""),
        Some(Wanted {
            name: "ter-v16n".into(),
            came: Came::Vconsole
        })
    );
    assert_eq!(
        wanted(&config(&[]), "", ""),
        Some(Wanted {
            name: "terminus".into(),
            came: Came::Default
        }),
        "with no font named, the default set"
    );
    let path = "/usr/share/kbd/consolefonts/ter-v16n.psf.gz";
    let named = wanted(
        &config(&["--font-engine=psf", &format!("--font-name={path}")]),
        conf,
        "",
    )
    .expect("named");
    assert_eq!((named.name.as_str(), named.came), (path, Came::Config));
    let family = wanted(
        &config(&["--font-engine=pango", "--font-name=DejaVu Sans Mono"]),
        "",
        "",
    )
    .expect("named");
    assert_eq!(family.came, Came::Family);
    // `font-engine=unifont` or `8x16` draws the built-ins.
    for engine in ["unifont", "8x16"] {
        let c = config(&[&format!("--font-engine={engine}"), "--font-name=x"]);
        assert_eq!(wanted(&c, conf, ""), None, "{engine}");
    }
}

// ADR-1200 D9: a name no frit bears is a one-source frit made on the
// fly, named as it was named: kbd:, font: or vconsole, and a path by
// the kind of font its extension names, whatever font-engine says
#[test]
fn a_name_no_frit_bears_is_a_one_source_frit() {
    let none = Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        ..Sources::default()
    };
    let made = |name: &str, came| {
        frit_named(
            &none,
            &Wanted {
                name: name.into(),
                came,
            },
        )
    };
    let text = |name: &str, came| made(name, came).map(|f| (f.name, f.text));
    assert_eq!(
        text("Lat2-Terminus16", Came::Config),
        Ok((
            "Lat2-Terminus16".into(),
            "source=kbd:Lat2-Terminus16\n".into()
        ))
    );
    assert_eq!(
        text("/fonts/my font.psf", Came::Config),
        Ok((
            "/fonts/my font.psf".into(),
            "source=file:/fonts/my font.psf\n".into()
        ))
    );
    for came in [Came::Config, Came::Family] {
        assert_eq!(
            text("/fonts/DejaVu Sans Mono.ttf", came),
            Ok((
                "/fonts/DejaVu Sans Mono.ttf".into(),
                "source=outline:/fonts/DejaVu Sans Mono.ttf\n".into()
            )),
            "{came:?}"
        );
    }
    assert_eq!(
        text("lat9w-16", Came::Vconsole),
        Ok(("lat9w-16".into(), "source=vconsole\n".into()))
    );
    assert_eq!(
        text("DejaVu Sans Mono", Came::Family),
        Ok((
            "DejaVu Sans Mono".into(),
            "source=font:DejaVu Sans Mono\n".into()
        ))
    );
}

/// With no font named, vtium draws the default set `terminus` from the
/// catalog's console folder. Where no set of that name is installed,
/// vtium makes no set on the fly for the name, as it does for a kbd
/// font's name: the read is refused with a line that names the set,
/// and vtium draws its built-in font.
///
/// References:
/// - ADR-1204 D2: the order in which vtium chooses a font, with
///   `terminus` after vconsole's font and before the built-in font.
#[test]
fn with_no_font_named_vtium_draws_terminus_where_it_is_installed() {
    let dir = Scratch::new("font-default");
    let default = wanted(&config(&[]), "", "").expect("the default set");
    assert_eq!(default.name, DEFAULT_SET);
    let sources = Sources {
        frits: dir.to_path_buf(),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    let why = frit_named(&sources, &default).expect_err("no terminus is installed");
    assert_eq!(
        why,
        "No font is named, and the default font set terminus is not installed."
    );
    std::fs::create_dir_all(dir.join("console")).expect("the folder");
    let text = "source=bdf:/usr/share/vtium/fonts/ter-u16n.bdf.gz\n";
    std::fs::write(dir.join("console/terminus.vtfrit"), text).expect("the set");
    let frit = frit_named(&sources, &default).expect("terminus");
    assert_eq!((frit.name.as_str(), frit.text.as_str()), ("terminus", text));
}

/// A display where 16-pixel rows leave fewer than 80 columns by 25 rows
/// is small. On a small display vtium draws the built-in font's 8×8
/// strike in place of the default set, so a 320×240 panel keeps 40
/// columns by 30 rows.
///
/// References:
/// - ADR-1204 D2: an 8-pixel cell draws the built-in font's 8×8 strike.
/// - ADR-1200 D17: the small-display rule.
#[test]
fn a_small_display_keeps_the_built_in_font_with_no_font_named() {
    for (w, h) in [(320, 240), (480, 320), (632, 480), (640, 384)] {
        assert!(small_display(w, h), "{w}x{h}");
        assert_eq!(auto_size(w, h, 8), 8, "{w}x{h}");
        assert!(!default_set_fits([(w, h)]), "{w}x{h}");
    }
    for (w, h) in [(640, 400), (640, 480), (1920, 1080), (0, 0)] {
        assert!(!small_display(w, h), "{w}x{h}");
        assert!(default_set_fits([(w, h)]), "{w}x{h}");
    }
    // a head's displays count by the smallest width and the smallest
    // height among them, as the automatic size takes them
    assert!(!default_set_fits([(1920, 1080), (320, 240)]));
    assert!(!default_set_fits([(1920, 240), (320, 1080)]));
    assert!(default_set_fits([(1920, 1080), (640, 480)]));
    // with no display known yet, the default set draws
    assert!(default_set_fits([]));
}

// A set is found by its name in the frits' directory itself, where a
// person's own sets live, and then in each category folder of the
// catalog. A name in the directory itself wins over a category folder,
// two category folders are searched in the catalog's order, and a name
// in a folder that is not a category, or in the layers' folder, is not
// a set.
//
// References:
// - ADR-1204 D9: the category folders and the order vtium searches.
#[test]
fn a_set_is_found_in_the_frits_directory_and_then_in_its_category_folder() {
    let dir = Scratch::new("font-catalog");
    for (folder, name) in [
        ("console", "terminus"),
        ("code", "both"),
        ("small", "both"),
        ("code", "mine"),
        ("", "mine"),
        ("elsewhere", "stray"),
        ("layers", "layered"),
    ] {
        std::fs::create_dir_all(dir.join(folder)).expect("the folder");
        std::fs::write(
            dir.join(folder).join(format!("{name}.vtfrit")),
            "source=kbd:x\n",
        )
        .expect("the set");
    }
    assert_eq!(
        frit(&dir, "terminus"),
        Some(dir.join("console/terminus.vtfrit"))
    );
    assert_eq!(frit(&dir, "mine"), Some(dir.join("mine.vtfrit")));
    assert_eq!(frit(&dir, "both"), Some(dir.join("code/both.vtfrit")));
    assert_eq!(frit(&dir, "stray"), None);
    assert_eq!(frit(&dir, "layered"), None);
    assert_eq!(frit(&dir, "absent"), None);
    assert_eq!(frit(&dir, "../console/terminus"), None);
}

// ADR-1401 D10, ADR-1403 D3: vtium reads a frit, at the start and for
// OSC 50, only once the ownership check vtium.conf passes trusts it. A
// frit its group can write, and a frit in a directory its group can
// write, are refused, each with the path that fails, its mode and the
// command that mends it.
#[test]
fn a_frit_is_read_only_when_the_ownership_check_trusts_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("font-trust");
    let open = dir.join("open");
    std::fs::create_dir_all(&open).expect("the directories");
    let mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("the mode");
    };
    for path in [
        dir.join("t.vtfrit"),
        dir.join("g.vtfrit"),
        open.join("t.vtfrit"),
    ] {
        std::fs::write(&path, "source=kbd:x\n").expect("the frit");
        mode(&path, 0o644);
    }
    mode(&dir, 0o755);
    mode(&dir.join("g.vtfrit"), 0o664);
    mode(&open, 0o775);
    let sources = |frits: &Path| Sources {
        frits: frits.to_path_buf(),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    let at_start = |frits: &Path, name: &str| {
        let wanted = Wanted {
            name: name.into(),
            came: Came::Config,
        };
        frit_named(&sources(frits), &wanted).map(|frit| frit.text)
    };
    let by_osc_50 = |frits: &Path, name: &str| {
        by_name(&sources(frits), name, false, None, None, &[], &no_paths()).map(|chosen| {
            match chosen {
                Chosen::Frit { frit, .. } => frit.text,
                Chosen::Builtin { .. } => String::new(),
            }
        })
    };
    let results = [
        at_start(&dir, "t"),
        at_start(&dir, "g"),
        at_start(&open, "t"),
        by_osc_50(&dir, "t"),
        by_osc_50(&dir, "g"),
        by_osc_50(&open, "t"),
    ];
    let read = Ok("source=kbd:x\n".to_owned());
    let refused = |name: &str, file: &Path, failed: &Path, mode: &str| {
        Err(format!(
            "vtium does not read the font set {name} from \"{file}\", because other users can change \"{failed}\" (mode {mode}). Run `chmod go-w {failed}` so that only its owner can change it.",
            file = file.display(),
            failed = failed.display(),
        ))
    };
    let writable = refused("g", &dir.join("g.vtfrit"), &dir.join("g.vtfrit"), "664");
    let in_open = refused("t", &open.join("t.vtfrit"), &open, "775");
    assert_eq!(
        results,
        [
            read.clone(),
            writable.clone(),
            in_open.clone(),
            read,
            writable,
            in_open
        ]
    );
}

// The console puts each layer's sources in place of the set's layer=
// line, reads each layer from the layers' folder of the frits'
// directory under the ownership check the frit passes, and draws the
// set without a layer it does not read, with one sentence for each.
//
// References:
// - ADR-1204 D5: the splice in place and the ownership check.
#[test]
fn a_frits_layers_go_in_place_under_the_frits_ownership_check() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("font-layers");
    let layers = dir.join("layers");
    std::fs::create_dir_all(&layers).expect("the layers' folder");
    let mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("the mode");
    };
    let write = |path: &Path, text: &str| {
        std::fs::write(path, text).expect("the file");
        mode(path, 0o644);
    };
    write(
        &dir.join("set.vtfrit"),
        "cell=8x16\nsource=kbd:text\nlayer=symbols\nlayer=theirs\nlayer=gone\nlayer=emoji\nline-spacing=10\n",
    );
    write(
        &layers.join("symbols.vtlayer"),
        "role=symbols\nsource=font:Symbols\ncover=U+E000-U+F8FF\n",
    );
    write(
        &layers.join("emoji.vtlayer"),
        "role=emoji\nsource=font:Emoji\n",
    );
    write(
        &layers.join("theirs.vtlayer"),
        "role=script\nsource=font:Theirs\n",
    );
    mode(&layers.join("theirs.vtlayer"), 0o664);
    mode(&dir, 0o755);
    mode(&layers, 0o755);
    let sources = Sources {
        frits: dir.to_path_buf(),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    let wanted = Wanted {
        name: "set".into(),
        came: Came::Config,
    };
    let frit = frit_named(&sources, &wanted).expect("the frit");
    assert_eq!(
        frit.text,
        "cell=8x16\nsource=kbd:text\nlayer=symbols\n\nsource=font:Symbols\ncover=U+E000-U+F8FF\nlayer=\nlayer=\nlayer=\nlayer=emoji\n\nsource=font:Emoji\nlayer=\nline-spacing=10\n"
    );
    // a key of the set after a layer is named by the frit's own line
    assert_eq!(frit_line_spacing(&frit.text), Ok(Some(10)));
    let after = with_layers(
        "set",
        &dir,
        "source=kbd:text\nlayer=symbols\ncell=8\n",
        &sources.trust,
    )
    .0;
    assert!(
        frit_cell(&after).unwrap_err().starts_with("line 3: "),
        "{after}"
    );
    let by_osc_50 =
        match by_name(&sources, "set", false, None, None, &[], &no_paths()).expect("the frit") {
            Chosen::Frit { frit, .. } => frit.text,
            Chosen::Builtin { .. } => String::new(),
        };
    assert_eq!(by_osc_50, frit.text);
    let (_, notes) =
        FritText::read_noting(&dir.join("set.vtfrit"), &dir, &sources.trust).expect("the frit");
    // a frit elsewhere takes its layers from the frits' directory, not
    // from a layers/ folder beside it
    let elsewhere = Scratch::new("font-layers-elsewhere");
    mode(&elsewhere, 0o755);
    write(
        &elsewhere.join("draft.vtfrit"),
        "source=kbd:text\nlayer=emoji\n",
    );
    let (draft, missing) = FritText::read_noting(
        &elsewhere.join("draft.vtfrit"),
        &dir,
        &trust().below(&elsewhere),
    )
    .expect("the draft");
    assert!(missing.is_empty(), "{missing:?}");
    assert!(draft.text.contains("source=font:Emoji\n"), "{}", draft.text);
    let theirs = layers.join("theirs.vtlayer");
    assert_eq!(
        notes,
        [
            format!(
                "font set set: line 4: vtium does not read the layer theirs from {}, because other users can change {} (mode 664). Run `chmod go-w {}` so that only its owner can change it. vtium draws the font set without that layer.",
                quoted(&theirs),
                quoted(&theirs),
                theirs.display()
            ),
            format!(
                "font set set: line 5: the layer gone is missing, because there is no file {}. vtium draws the font set without that layer.",
                quoted(&layers.join("gone.vtlayer"))
            ),
        ]
    );
}

// A line that a layer cannot hold, such as a key vtium does not know,
// does not drop the layer: vtium leaves that line out, keeps the
// layer's sources, and says which line of which layer file it left
// out. The start and OSC 50 log the sentence, and `vtium check-config`
// prints it.
//
// References:
// - ADR-1510 D2: an unknown or misplaced key is reported with its file
//   and line, and ignored alone.
#[test]
fn a_layers_ignored_line_is_named_with_its_file_and_line() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("font-layer-ignored");
    let layers = dir.join("layers");
    std::fs::create_dir_all(&layers).expect("the layers' folder");
    let sym = layers.join("sym.vtlayer");
    std::fs::write(&sym, "role=symbols\nsource=font:S\nfrob=1\n").expect("the layer");
    for path in [&*dir, &*layers] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("the mode");
    }
    std::fs::set_permissions(&sym, std::fs::Permissions::from_mode(0o644)).expect("the mode");
    let (text, notes) = with_layers(
        "set",
        &dir,
        "source=kbd:x\nlayer=sym\n",
        &trust().below(&dir),
    );
    assert_eq!(text, "source=kbd:x\nlayer=sym\n\nsource=font:S\n\nlayer=\n");
    assert_eq!(
        notes,
        [format!(
            "font set set: line 3 of the layer sym in {}: frob= is not a key vtium knows. vtium ignores this line and reads the rest of the font set.",
            quoted(&sym)
        )]
    );
}

// OSC 50 finds a name as the start does: a built-in font, then a frit
// in the frits' directory, then the frit made on the fly for the head's
// configured font under its own name. A frit in the directory wins over
// a frit made on the fly under the same name. Any other name is
// refused, and so is a path in no folder vtium searches for fonts.
//
// References:
// - ADR-1200 D5: the names a switch takes, and their order.
// - ADR-1200 D9: the frit made on the fly for the configured font.
#[test]
fn by_name_finds_a_frit_made_on_the_fly_after_the_frits_directory() {
    let dir = Scratch::new("font-by-name");
    std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").expect("the frit");
    let sources = Sources {
        frits: dir.to_path_buf(),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    let made = |name: &str| FritText {
        name: name.to_owned(),
        text: "source=vconsole\n".to_owned(),
    };
    let found = |name: &str, made: Option<&FritText>| {
        by_name(&sources, name, false, None, made, &[], &no_paths()).map(|chosen| match chosen {
            Chosen::Frit { name, frit, .. } => (name, frit.text),
            Chosen::Builtin { name, .. } => (name.unwrap_or_default().to_owned(), String::new()),
        })
    };
    let vconsole = made("ter-124n");
    let named = |name: &str, text: &str| Ok((name.to_owned(), text.to_owned()));
    let refused = Err("there is no font set of that name".to_owned());
    let results = [
        found("ter-124n", Some(&vconsole)),
        found("ter-124n", None),
        found("ter-v16n", Some(&vconsole)),
        found("t", Some(&made("t"))),
        found("unifont", Some(&made("unifont"))),
        found("/etc/passwd", None),
    ];
    assert_eq!(
        results,
        [
            named("ter-124n", "source=vconsole\n"),
            refused.clone(),
            refused.clone(),
            named("t", "source=kbd:x\n"),
            named("unifont", ""),
            Err(OUTSIDE.to_owned()),
        ]
    );
}

/// Sets the permission bits of `path` to `bits`.
fn chmod(path: &Path, bits: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).expect("the mode");
}

/// Writes `text` to the file at `path`.
fn put(path: &Path, text: &str) {
    std::fs::write(path, text).expect("the file");
}

/// A gate whose batch lists nothing, under the rule of the account the
/// test runs as.
fn no_paths() -> cullet::Gate {
    cullet::Gate::new(Rule::new(trust()), None, batch::Batch::default())
}

/// Why vtium refuses a font path at each of the four checks. The
/// fourth's reason leaves out the folder and the trust rule's own words.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": the four checks.
const NOT_PLAIN: &str = "a font's path must be absolute and plain: no `.`, `..` or empty component, no `/` at its end, no control character, no `#`, and no space at either end";
const OUTSIDE: &str = "the file is not in a folder that vtium searches for fonts: those of font-paths, the package's font folder and kbd's console-font folders";
fn inactive() -> String {
    format!(
        "the font is not active, because {} does not list it",
        batch::BATCH_FILE
    )
}
fn untrusted(dir: &Path) -> String {
    format!(
        "the folder {} fails the trust rule, because ",
        dir.display()
    )
}

/// A font file `fonts/sub/a.ttf` in a scratch folder `name`, the
/// folders vtium searches for fonts with `fonts` among them, and a gate
/// whose batch lists the file under the rule of the account the test
/// runs as, below the scratch folder.
fn font_path(name: &str) -> (Scratch, PathBuf, Vec<PathBuf>, cullet::Gate) {
    let dir = Scratch::new(name);
    let sub = dir.join("fonts/sub");
    std::fs::create_dir_all(&sub).expect("the folders");
    chmod(&dir, 0o755);
    chmod(&dir.join("fonts"), 0o755);
    chmod(&sub, 0o755);
    let file = sub.join("a.ttf");
    put(&file, "a font");
    chmod(&file, 0o644);
    let folders = vec![PathBuf::from("/usr/share/fonts"), dir.join("fonts")];
    let gate = listing(&dir, &[&file]);
    (dir, file, folders, gate)
}

/// A gate whose batch lists `files`, under the rule of the account the
/// test runs as, with the folders above `dir` left unchecked.
fn listing(dir: &Path, files: &[&Path]) -> cullet::Gate {
    let entries = files
        .iter()
        .map(|f| batch::Entry::Path(f.to_path_buf()))
        .collect();
    cullet::Gate::new(
        Rule::new(trust()).below(dir),
        Some(Mounts::system().unwrap_or_default()),
        batch::Batch::new(entries),
    )
}

// A font path that passes the four checks is a qualifying font. OSC 50
// takes it as a one-source frit named by the path, the frit the start
// makes for a path that font-name gives.
//
// References:
// - ADR-1200 D5, "A qualifying font": the four checks and the frit
//   vtium builds.
#[test]
fn osc_50_takes_a_font_path_that_passes_every_check() {
    let (dir, file, folders, gate) = font_path("font-path-qualifies");
    let path = file.to_str().unwrap();
    assert_eq!(qualifying(path, &folders, &gate), Ok(()));
    let sources = Sources {
        frits: dir.join("frits"),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    let chosen = by_name(&sources, path, false, None, None, &folders, &gate);
    let (name, text) = match chosen.expect("the font file qualifies") {
        Chosen::Frit { name, frit, .. } => (name, frit.text),
        Chosen::Builtin { .. } => (String::new(), String::new()),
    };
    assert_eq!(name, path, "the name the query reports");
    assert_eq!(text, format!("source=outline:{path}\n"));
    let at_start = frit_named(
        &sources,
        &Wanted {
            name: path.to_owned(),
            came: Came::Config,
        },
    )
    .map(|frit| frit.text);
    assert_eq!(at_start, Ok(text), "the frit the start makes for the path");
}

// vtium makes the four checks in a fixed order. A path that fails them
// all is refused at the first, and each check put right moves the
// refusal on to the next.
//
// References:
// - ADR-1200 D5, "A qualifying font": the four checks and their order.
#[test]
fn a_font_path_is_refused_at_the_first_check_it_fails() {
    let (dir, file, folders, _) = font_path("font-path-order");
    let open = dir.join("fonts/open");
    std::fs::create_dir_all(&open).expect("the folder");
    chmod(&open, 0o777);
    let inside = open.join("b.ttf");
    put(&inside, "a font");
    let relative = inside.strip_prefix("/").unwrap().to_str().unwrap();
    let elsewhere = &[PathBuf::from("/usr/share/fonts")];
    let none = listing(&dir, &[]);
    let listed = listing(&dir, &[&inside]);
    let refusal = |path: &str, folders: &[PathBuf], gate: &cullet::Gate| {
        qualifying(path, folders, gate).unwrap_err()
    };
    let path = inside.to_str().unwrap();
    assert_eq!(refusal(relative, elsewhere, &none), NOT_PLAIN, "check 1");
    assert_eq!(refusal(path, elsewhere, &none), OUTSIDE, "check 2");
    assert_eq!(refusal(path, &folders, &none), inactive(), "check 3");
    let fourth = refusal(path, &folders, &listed);
    assert!(fourth.starts_with(&untrusted(&open)), "check 4: {fourth}");
    chmod(&open, 0o755);
    assert_eq!(qualifying(path, &folders, &listed), Ok(()), "all four");
    assert_eq!(
        qualifying(file.to_str().unwrap(), &folders, &listing(&dir, &[&file])),
        Ok(())
    );
}

// The first check refuses a path that is not plain before vtium looks
// at anything else: one that does not start with `/`, holds `..` or a
// control character, or is not in its plain form. A path in another
// form, with a `/` at its end, a `.` component or `//`, names the same
// file as the batch's line, but the query would report that other
// spelling. A path holding `#`, or with a space at either end, would
// reach the font helper as another file, because the frit reader cuts
// a line at `#` and trims the spaces around a value. Each file is
// real, and the batch lists each spelling, so only the first check
// refuses them.
//
// References:
// - ADR-1200 D5, "A qualifying font": the first check.
#[test]
fn a_font_path_that_is_not_plain_fails_the_first_check() {
    let (dir, file, folders, _) = font_path("font-path-plain");
    let path = file.to_str().unwrap();
    let relative = path.trim_start_matches('/');
    let up = format!("{}/../sub/a.ttf", dir.join("fonts/sub").display());
    let control = format!("{path}\u{7}");
    let sub = dir.join("fonts/sub");
    let hash = sub.join("a#b.ttf");
    let spaced = sub.join("c.ttf ");
    for real in [&hash, &spaced] {
        put(real, "a font");
        chmod(real, 0o644);
    }
    let slash = format!("{path}/");
    let dot = format!("{}/./sub/a.ttf", dir.join("fonts").display());
    let double = format!("{}//sub/a.ttf", dir.join("fonts").display());
    let (hash, spaced) = (hash.to_str().unwrap(), spaced.to_str().unwrap());
    let gate = listing(
        &dir,
        &[
            Path::new(relative),
            Path::new(&up),
            Path::new(&control),
            Path::new("fonts/sub/a.ttf"),
            Path::new(hash),
            Path::new(spaced),
            Path::new(&slash),
        ],
    );
    for spelling in [&slash, &dot, &double] {
        assert!(
            gate.holds(Path::new(spelling)),
            "the batch lists {spelling}"
        );
    }
    for bad in [
        relative,
        &up,
        &control,
        "fonts/sub/a.ttf",
        "./a.ttf",
        hash,
        spaced,
        &slash,
        &dot,
        &double,
    ] {
        assert_eq!(
            qualifying(bad, &folders, &gate),
            Err(NOT_PLAIN.to_owned()),
            "{bad:?}"
        );
    }
}

// vtium makes the frit for a qualifying path, then reads the frit's
// `source=` line back as the font helper's frit reader does: a line
// cut at `#`, and its value trimmed. The frit is used only when the
// line names the same path. So a path the first check let through by
// mistake, one that the reader would take for another file, is still
// refused.
//
// References:
// - ADR-1200 D5: vtium builds a frit with the one source, named by the
//   path, as it does at the start.
#[test]
fn the_frit_made_for_a_font_path_reads_back_the_same_path() {
    let made = |path: &str| one_source(path).map(|frit| (frit.name, frit.text));
    let path = "/usr/share/fonts/TTF/DejaVu Sans Mono.ttf";
    assert_eq!(
        made(path),
        Ok((path.to_owned(), format!("source=outline:{path}\n")))
    );
    for other in [
        "/usr/share/fonts/a#b.ttf",
        "/usr/share/fonts/a.ttf ",
        "/usr/share/fonts/a.ttf\t",
        "/usr/share/fonts/a.ttf\nsource=file:/etc/passwd",
    ] {
        assert_eq!(made(other), Err(READS_ANOTHER.to_owned()), "{other:?}");
    }
}

// A font file's path builds the source that reads that kind of font,
// by the file's extension with case ignored: `outline:` for an
// OpenType or TrueType file or collection, `bdf:` and `hex:` for those
// formats, and `file:` for a console font and any other name. A PSF or
// BDF font's extension may be followed by a compression's.
//
// References:
// - ADR-1200 D1: the source forms, and which kinds of font each reads.
// - ADR-1200 D5: the console decides from the path's text and opens no
//   font file.
#[test]
fn a_font_paths_source_follows_the_files_extension() {
    for (path, source) in [
        ("/f/DejaVuSansMono.ttf", "outline"),
        ("/f/A.OTF", "outline"),
        ("/f/NotoSansCJK-Regular.ttc", "outline"),
        ("/f/b.otc", "outline"),
        ("/f/ter-u16n.otb", "outline"),
        ("/f/ter-v16n.psf.gz", "file"),
        ("/f/ter-v16n.psf", "file"),
        ("/f/lat9w-16.psfu.zst", "file"),
        ("/f/ter-u16n.bdf", "bdf"),
        ("/f/ter-u16n.bdf.xz", "bdf"),
        ("/f/unifont.hex", "hex"),
        ("/f/ega.cp", "file"),
        ("/f/raw", "file"),
        ("/f/a.ttf.d/raw", "file"),
        ("/f/gz", "file"),
    ] {
        assert_eq!(path_source(path), format!("{source}:{path}"), "{path}");
    }
}

// vtium-config's Fonts tab sends OSC 50 the path of a font file that
// it lists, and draws its own preview of that file from a font set
// whose source it builds by the same rule. For each extension of a file
// the tab lists, the console builds the source that the preview builds,
// so the console draws the font the tab showed.
//
// References:
// - ADR-1200 D5, "Trying one font from vtium-config's Fonts tab".
#[test]
fn a_font_paths_source_is_the_one_the_fonts_tab_previews() {
    use vtium_config_core::discover::source_of;
    use vtium_config_core::discover::{COMPRESSED, COMPRESSIBLE_SUFFIXES, FONT_SUFFIXES};
    let mut names: Vec<String> = FONT_SUFFIXES.iter().map(|s| format!("a.{s}")).collect();
    for s in COMPRESSIBLE_SUFFIXES {
        names.extend(COMPRESSED.iter().map(|c| format!("a.{s}.{c}")));
    }
    for name in names {
        for name in [name.clone(), name.to_ascii_uppercase()] {
            let path = format!("/usr/share/fonts/{name}");
            assert_eq!(
                Some(path_source(&path)),
                source_of(Path::new(&path)),
                "{path}"
            );
        }
    }
}

/// What vtium-config's real serve mode, run in this process, answers a
/// sources query for the font set `text` named `generated`: an empty
/// text when it reads every source, one line for each source it leaves
/// out, and `Err` with its reason when it draws nothing. No batch is
/// sent, so the helper reads any file, as in vtium-config's previews.
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
fn served_sources(text: &str) -> Result<String, String> {
    use vtium_font_helper::serve;
    let request = serve::ServeRequest {
        kind: serve::SOURCES,
        frit: serve::FritText {
            name: "generated".to_owned(),
            text: text.to_owned(),
        },
        cell: Cell {
            width: 0,
            height: 0,
        },
        band: vtium_font::spacing::Band::NONE,
        style: vtium_font::Style::default(),
        script: vtium_font::Script::Baseline,
        doubled: None,
        cluster_width: Default::default(),
        codepoints: Vec::new(),
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let mut out = Vec::new();
    let status = serve::serve(
        std::io::Cursor::new(request.encode()),
        &mut out,
        &serve::Paths::default(),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(status, 0);
    let mut rest = out.as_slice();
    while let [k0, k1, k2, k3, l0, l1, l2, l3, tail @ ..] = rest {
        let kind = u32::from_le_bytes([*k0, *k1, *k2, *k3]);
        let len = u32::from_le_bytes([*l0, *l1, *l2, *l3]) as usize;
        let text = String::from_utf8(tail[..len].to_vec()).unwrap();
        rest = &tail[len..];
        match kind {
            serve::FRAME_SOURCES => return Ok(text),
            serve::FRAME_DRAWS_NOTHING => return Err(text),
            _ => {}
        }
    }
    Err("the helper sent no answer".to_owned())
}

// The font set vtium makes for a font file's path is one that the
// real font helper reads, for an outline font, a PSF console font
// compressed with gzip and a BDF font. The helper's PSF reader refuses
// an outline font, and its outline reader a console font, so a set
// whose source names the wrong kind draws nothing.
//
// References:
// - ADR-1200 D1: the source forms, and which kinds of font each reads.
// - ADR-1200 D9: the one-source frit vtium makes for a path.
#[test]
fn the_font_helper_reads_the_font_set_made_for_each_kind_of_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for file in [
        "vtium-config/tests/fixtures/DejaVuSansMono-W-M-0.ttf",
        "vtium-font/tests/fixtures/ter-v16n.psf.gz",
        "vtium-font/data/ter-u16n.bdf",
    ] {
        let path = std::fs::canonicalize(root.join(file)).expect("the fixture");
        let path = path.to_str().expect("a UTF-8 path");
        let made = one_source(path).expect("a plain path").text;
        assert_eq!(served_sources(&made), Ok(String::new()), "{made}");
    }
    let ttf =
        std::fs::canonicalize(root.join("vtium-config/tests/fixtures/DejaVuSansMono-W-M-0.ttf"))
            .expect("the fixture");
    let psf = std::fs::canonicalize(root.join("vtium-font/tests/fixtures/ter-v16n.psf.gz"))
        .expect("the fixture");
    for wrong in [
        format!("source=file:{}\n", ttf.display()),
        format!("source=outline:{}\n", psf.display()),
    ] {
        let answer = served_sources(&wrong).expect("an answer");
        assert!(answer.contains("\nrefused\t1\t"), "{wrong}: {answer}");
    }
}

// OSC 50 refuses a font file whose path holds `#` or ends in a space,
// though the file is real, sits in a searched folder and the batch
// lists it. The font helper would read the frit's line as another
// file, so vtium refuses the path at its first check, and the frit's
// read-back would refuse it too.
//
// References:
// - ADR-1200 D5, "A qualifying font": the first check.
#[test]
fn osc_50_refuses_a_font_path_the_helper_would_read_as_another_file() {
    let (dir, _, folders, _) = font_path("font-path-another");
    let sub = dir.join("fonts/sub");
    let files = [sub.join("a#b.ttf"), sub.join("c.ttf ")];
    for file in &files {
        put(file, "a font");
        chmod(file, 0o644);
    }
    let gate = listing(&dir, &[&files[0], &files[1]]);
    let sources = Sources {
        frits: dir.join("frits"),
        trust: trust().below(&dir),
        ..Sources::default()
    };
    for file in &files {
        let path = file.to_str().unwrap();
        assert!(
            by_name(&sources, path, false, None, None, &folders, &gate).is_err(),
            "{path:?}"
        );
    }
}

/// Why vtium refuses a path whose frit would name another file.
const READS_ANOTHER: &str = "the font set vtium would make for this path would name another file";

// The second check passes a file in a folder vtium searches for fonts,
// or in a folder below one, compared by whole components. The folder
// itself is no font file.
//
// References:
// - ADR-1200 D5, "A qualifying font": the second check.
#[test]
fn a_font_path_outside_the_folders_vtium_searches_fails_the_second_check() {
    let (dir, _, folders, _) = font_path("font-path-outside");
    let old = dir.join("fonts-old");
    std::fs::create_dir_all(&old).expect("the folder");
    chmod(&old, 0o755);
    let beside = old.join("a.ttf");
    put(&beside, "a font");
    let fonts = dir.join("fonts");
    let gate = listing(&dir, &[&beside, &fonts]);
    for outside in [beside.as_path(), &fonts, Path::new("/etc/passwd")] {
        assert_eq!(
            qualifying(outside.to_str().unwrap(), &folders, &gate),
            Err(OUTSIDE.to_owned()),
            "{}",
            outside.display()
        );
    }
    // a relative folder matches no absolute path
    let relative = [PathBuf::from(fonts.strip_prefix("/").unwrap())];
    let file = fonts.join("sub/a.ttf");
    assert_eq!(
        qualifying(file.to_str().unwrap(), &relative, &listing(&dir, &[&file])),
        Err(OUTSIDE.to_owned())
    );
}

// The third check refuses a font the batch does not list: the font is
// not active, whatever folder holds it.
//
// References:
// - ADR-1200 D5, "A qualifying font": the third check.
#[test]
fn a_font_path_the_batch_does_not_list_fails_the_third_check() {
    let (dir, file, folders, _) = font_path("font-path-inactive");
    let other = dir.join("fonts/sub/b.ttf");
    put(&other, "a font");
    let gate = listing(&dir, &[&other]);
    assert_eq!(
        qualifying(file.to_str().unwrap(), &folders, &gate),
        Err(inactive())
    );
}

// The fourth check refuses a path through a folder that is a symbolic
// link, though the link's target would pass.
//
// References:
// - ADR-1200 D5, "A qualifying font": the fourth check.
#[test]
fn a_font_path_through_a_symbolic_link_fails_the_fourth_check() {
    let (dir, _, folders, _) = font_path("font-path-link");
    let link = dir.join("fonts/link");
    std::os::unix::fs::symlink(dir.join("fonts/sub"), &link).expect("the link");
    let file = link.join("a.ttf");
    let why = qualifying(file.to_str().unwrap(), &folders, &listing(&dir, &[&file])).unwrap_err();
    assert_eq!(
        why,
        format!(
            "{}{} is a symbolic link, and vtium uses no resource reached through one.",
            untrusted(&link),
            link.display()
        )
    );
}

// The fourth check refuses the fonts in a folder others can write,
// because anyone could put a file in it.
//
// References:
// - ADR-1200 D5, "A qualifying font": the fourth check.
#[test]
fn a_font_in_a_folder_others_can_write_fails_the_fourth_check() {
    let (dir, file, folders, gate) = font_path("font-path-writable");
    let sub = dir.join("fonts/sub");
    chmod(&sub, 0o775);
    let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
    assert!(why.starts_with(&untrusted(&sub)), "{why}");
    assert!(why.contains("chmod go-w"), "{why}");
}

// The fourth check, judged as root, refuses a font in a folder that a
// user owns, such as one in a home folder, because of the folder's
// owner.
//
// References:
// - ADR-1200 D5, "A qualifying font": the fourth check, judged as root.
#[test]
fn a_font_in_a_folder_a_user_owns_fails_the_fourth_check() {
    let (dir, file, folders, _) = font_path("font-path-owner");
    let me = rustix::process::geteuid().as_raw();
    if me == 0 {
        return;
    }
    let gate = cullet::Gate::new(
        Rule::new(Trust::ROOT).below(&dir),
        Some(Mounts::system().unwrap_or_default()),
        batch::Batch::new(vec![batch::Entry::Path(file.clone())]),
    );
    let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
    assert!(why.starts_with(&untrusted(&dir.join("fonts/sub"))), "{why}");
    assert!(why.contains("chown root:root"), "{why}");
}

// The fourth check refuses a folder on a filesystem that vtium cannot
// show stays mounted.
//
// References:
// - ADR-1200 D5, "A qualifying font": the fourth check.
#[test]
fn a_font_on_a_filesystem_not_known_to_stay_mounted_fails_the_fourth_check() {
    let (dir, file, folders, _) = font_path("font-path-mounts");
    let gate = cullet::Gate::new(
        Rule::new(trust()).below(&dir),
        Some(Mounts::default()),
        batch::Batch::new(vec![batch::Entry::Path(file.clone())]),
    );
    let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
    assert!(why.starts_with(&untrusted(&dir.join("fonts/sub"))), "{why}");
}

// The folders of the second check are those of font-paths, else the
// build's list, which vtium-config's font helper searches too. The
// package's font folder and kbd's console-font folders follow them. The
// configuration leaves out a folder of font-paths that is not absolute.
//
// References:
// - ADR-1200 D5, "A qualifying font": the second check's folders.
// - ADR-1200 D25: font-paths replaces the build's list.
#[test]
fn the_folders_a_font_path_must_sit_in_follow_font_paths() {
    assert_eq!(FONT_DIRS, vtium_font_helper::sources::FONT_DIRS);
    let tail = [
        batch::PACKAGE_FONTS,
        "/usr/share/kbd/consolefonts",
        "/usr/lib/kbd/consolefonts",
        "/lib/kbd/consolefonts",
        "/usr/share/consolefonts",
    ];
    let folders = |args: &[&str]| -> Vec<String> {
        font_folders(&config(args))
            .iter()
            .map(|p| p.display().to_string())
            .collect()
    };
    let mut built: Vec<&str> = FONT_DIRS.to_vec();
    built.extend(tail);
    assert_eq!(folders(&[]), built);
    let mut configured = vec!["/opt/fonts", "/srv/f"];
    configured.extend(tail);
    assert_eq!(
        folders(&["--font-paths=/opt/fonts:fonts:/srv/f"]),
        configured
    );
}

#[test]
fn the_size_asked_for() {
    assert_eq!(select(24, false, |s| Ok::<_, ()>(s * 2)), Ok((48, 24)));
}

#[test]
fn with_no_font_in_use_a_failure_falls_back_once() {
    let mut asked = Vec::new();
    let found = select(99, false, |s| {
        asked.push(s);
        if s == FALLBACK_SIZE {
            Ok("font")
        } else {
            Err("no 99")
        }
    });
    assert_eq!(found, Ok(("font", FALLBACK_SIZE)));
    assert_eq!(asked, [99, FALLBACK_SIZE]);
}

#[test]
fn a_lookup_that_always_fails_is_an_error_after_the_fallback() {
    let mut asked = Vec::new();
    let found = select(99, false, |s| {
        asked.push(s);
        Err::<(), _>(s)
    });
    assert_eq!(found, Err(FALLBACK_SIZE));
    assert_eq!(asked, [99, FALLBACK_SIZE]);
}

#[test]
fn with_a_font_in_use_a_failure_is_the_error() {
    let mut asked = Vec::new();
    let found = select(99, true, |s| {
        asked.push(s);
        Err::<(), _>(s)
    });
    assert_eq!(found, Err(99));
    assert_eq!(asked, [99]);
}

// Each login reads the mount table again, though the batch is the same.
// A mount ID can be reused after an unmount, so a gate that kept the
// table of the first login would judge a filesystem mounted later under
// that ID by the old mount's line, an NFS share as ext4. The login
// that finds only the mount table changed logs nothing.
//
// References:
// - ADR-1205 D4: the trust rule judges the filesystem a resource sits on.
#[test]
fn each_login_judges_by_the_mount_table_read_again() {
    let dir = Scratch::new("batch-mounts");
    let me = rustix::process::geteuid().as_raw();
    let places = batch::Places {
        file: dir.join("batch.conf"),
        vconsole_conf: PathBuf::from("/dev/null"),
        cmdline: PathBuf::from("/dev/null"),
        palettes: dir.to_path_buf(),
        ..batch::Places::system()
    };
    let trust = Trust::owner(me).below(&dir);
    let sys = Path::new("/nonexistent-sys");
    let ext4 = Mounts::parse("40 22 8:17 / /mnt rw - ext4 /dev/sdb1 rw\n", sys);
    let nfs = Mounts::parse("40 22 0:52 / /mnt rw - nfs4 host:/share rw\n", sys);
    let rule = || Rule::new(trust.clone());
    assert!(load_batch_with(&places, &trust, rule(), ext4.clone()).is_some());
    assert_eq!(gate().mounts.as_ref(), Some(&ext4));
    assert!(
        load_batch_with(&places, &trust, rule(), nfs.clone()).is_none(),
        "the batch is the same, so there is nothing to log"
    );
    assert_eq!(gate().mounts.as_ref(), Some(&nfs));
}

// A batch file of more bad lines than vtium lists gives the first
// ERRORS_MAX as findings and the line that counts the rest, for the log.
//
// References:
// - ADR-1205 D4, its note of 2026-11-19: the console names each batch line it ignores.
#[test]
fn a_batch_past_the_error_bound_logs_the_count_of_the_rest() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("batch-more");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = dir.join("batch.conf");
    std::fs::write(&file, "relative\n".repeat(vtium_settings::ERRORS_MAX + 2)).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let me = rustix::process::geteuid().as_raw();
    let places = batch::Places {
        file: file.clone(),
        vconsole_conf: PathBuf::from("/dev/null"),
        cmdline: PathBuf::from("/dev/null"),
        palettes: dir.to_path_buf(),
        ..batch::Places::system()
    };
    let trust = Trust::owner(me).below(&dir);
    let found = load_batch_with(&places, &trust, Rule::new(trust.clone()), Mounts::default())
        .expect("the first read logs");
    assert_eq!(found.findings.len(), vtium_settings::ERRORS_MAX);
    assert_eq!(
        found.more,
        Some(format!(
            "{}: vtium also ignores 2 more lines of this file. Only the first {} are listed.",
            file.display(),
            vtium_settings::ERRORS_MAX
        ))
    );
}
