//! A set's superscript and subscript strikes (ADR-1202 D11): a strike
//! says its script in the record's 16 bits after the format, which format
//! 2.2 gives it and formats 2.0 and 2.1 reserved as 0; a script strike's
//! glyph draws only for a cell in that script; a set of an older minor
//! version reads as normal strikes; and a set holding a script strike
//! puts a field that is not 0 where an older reader refuses one.

mod common;

use std::sync::Arc;

use common::{assemble, sections, with_section};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{Cell, Face, Script, Style, Variant, Width};

const CELL: Cell = Cell {
    width: 4,
    height: 4,
};

/// A strike of `script` mapping `A` to a glyph of `value` and the cluster
/// e and its acute accent to one of `value + 1`.
fn strike(script: Script, value: u8) -> StrikeBuilder {
    let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default())
        .script(script)
        .compress(false);
    let a = s.glyph(Width::Narrow, vec![value; 16]);
    let e = s.glyph(Width::Narrow, vec![value + 1; 16]);
    s.map('A', a);
    s.cluster(&['e', '\u{301}'], e);
    s
}

/// A set of normal glyphs of 10, superscript glyphs of 20 and subscript
/// glyphs of 30, its strikes given in another order than the one written.
fn scripted() -> Vec<u8> {
    SetBuilder::new("scripted")
        .strike(strike(Script::Sub, 30))
        .strike(strike(Script::Baseline, 10))
        .strike(strike(Script::Super, 20))
        .write()
        .expect("the set writes")
}

fn face(bytes: Vec<u8>) -> SetFace {
    SetFace::new(Arc::new(Set::from_bytes(bytes).expect("read")), CELL.height).expect("a face")
}

fn variant(script: Script) -> Variant {
    Variant {
        style: Style::default(),
        script,
    }
}

/// The script field of each strike record, in the order written: the
/// 16 bits after the style and the format.
fn script_fields(set: &[u8]) -> Vec<u16> {
    let (_, _, secs) = sections(set);
    let strk = &secs.iter().find(|(t, _)| t == b"STRK").expect("STRK").1;
    let count = u32::from_le_bytes(strk[..4].try_into().unwrap()) as usize;
    let size = u32::from_le_bytes(strk[4..8].try_into().unwrap()) as usize;
    (0..count)
        .map(|i| u16::from_le_bytes(strk[8 + i * size + 2..8 + i * size + 4].try_into().unwrap()))
        .collect()
}

// covers: sgr.script
#[test]
fn a_strike_says_its_script_and_draws_only_for_a_cell_in_it() {
    let bytes = scripted();
    // written sorted by script after the cell: normal, superscript,
    // subscript, each in the record's script field
    assert_eq!(script_fields(&bytes), [0, 1, 2]);
    let f = face(bytes);
    let scripts: Vec<Script> = f.set().strikes().map(|s| s.script).collect();
    assert_eq!(scripts, [Script::Baseline, Script::Super, Script::Sub]);
    let value = |g: Option<vtium_font::Glyph>| g.map(|g| g.pixels()[0]);
    // a normal cell draws the normal glyph, never a script one
    assert_eq!(value(f.draw(&['A'], Style::default())), Some(10));
    assert_eq!(
        value(f.cluster(&['e', '\u{301}'], Style::default())),
        Some(11)
    );
    // each script draws its own strike's glyph, a codepoint and a cluster
    for (script, v) in [
        (Script::Baseline, 10),
        (Script::Super, 20),
        (Script::Sub, 30),
    ] {
        assert_eq!(
            value(f.draw_variant(&['A'], variant(script))),
            Some(v),
            "{script:?}"
        );
        assert_eq!(
            value(f.draw_variant(&['e', '\u{301}'], variant(script))),
            Some(v + 1),
            "{script:?}"
        );
    }
}

// a set of normal glyphs gives no superscript glyph, and a set of
// superscript glyphs alone, as the helper answers a superscript ask with,
// gives no normal glyph, no coverage and no subscript glyph
#[test]
fn a_set_of_one_script_draws_nothing_for_another() {
    let normal = face(
        SetBuilder::new("normal")
            .strike(strike(Script::Baseline, 10))
            .write()
            .expect("writes"),
    );
    assert_eq!(normal.draw_variant(&['A'], variant(Script::Super)), None);
    let sup = face(
        SetBuilder::new("sup")
            .strike(strike(Script::Super, 20))
            .write()
            .expect("writes"),
    );
    assert!(sup.draw_variant(&['A'], variant(Script::Super)).is_some());
    assert_eq!(sup.draw(&['A'], Style::default()), None);
    assert_eq!(sup.cluster(&['e', '\u{301}'], Style::default()), None);
    assert!(!sup.has_glyph('A'), "a script strike is no coverage");
    assert_eq!(sup.draw_variant(&['A'], variant(Script::Sub)), None);
}

// a style a script strike lacks is drawn from that script's regular
// strike, as a normal glyph's is (ADR-1302 D4), never from a normal strike
#[test]
fn a_script_cell_in_a_style_takes_the_regular_strike_of_its_script() {
    let f = face(scripted());
    let bold_super = Variant {
        style: Style {
            bold: true,
            italic: false,
        },
        script: Script::Super,
    };
    let g = f.draw_variant(&['A'], bold_super).expect("drawn");
    assert_eq!(g.pixels()[0], 20, "the superscript strike's glyph");
}

// format 2.0 and 2.1 sets, whose strikes hold 0 in the field, read as
// normal strikes, and their glyphs draw for normal cells
#[test]
fn an_older_minor_versions_set_reads_as_normal_strikes() {
    let bytes = SetBuilder::new("older")
        .strike(strike(Script::Baseline, 10))
        .write()
        .expect("writes");
    let (major, minor, secs) = sections(&bytes);
    assert_eq!((major, minor), (2, 2));
    for old in [0u16, 1] {
        let set = Set::from_bytes(assemble(major, old, &secs)).expect("read");
        assert_eq!(set.version(), (2, old));
        assert!(set.strikes().all(|s| s.script == Script::Baseline));
        let f = SetFace::new(Arc::new(set), CELL.height).expect("a face");
        assert_eq!(
            f.draw(&['A'], Style::default()).map(|g| g.pixels()[0]),
            Some(10)
        );
        assert_eq!(f.draw_variant(&['A'], variant(Script::Super)), None);
    }
}

// A reader of formats 2.0 and 2.1 refuses a strike record whose 16 bits
// after the format are not 0 ("a reserved field that is not 0",
// set/read/parse.rs at integration 7caec0fc). A set of normal glyphs keeps
// them 0, so such a reader reads it as its own; a set holding a script
// strike puts its script there, so such a reader refuses it whole and the
// console draws in the built-in font, rather than drawing a small glyph
// for a normal cell
#[test]
fn an_older_reader_reads_a_set_of_normal_glyphs_and_refuses_a_script_strike() {
    let normal = SetBuilder::new("normal")
        .strike(strike(Script::Baseline, 10))
        .write()
        .expect("writes");
    assert_eq!(script_fields(&normal), [0]);
    let sup = SetBuilder::new("sup")
        .strike(strike(Script::Super, 20))
        .write()
        .expect("writes");
    assert_eq!(script_fields(&sup), [1]);
    let sub = SetBuilder::new("sub")
        .strike(strike(Script::Sub, 30))
        .write()
        .expect("writes");
    assert_eq!(script_fields(&sub), [2]);
}

// a script past subscript's 2 is refused, and two strikes alike but for
// their script are two strikes, not one repeated
#[test]
fn an_unknown_script_is_refused() {
    let bytes = scripted();
    let unknown = with_section(&bytes, b"STRK", |s| s[8 + 2] = 3);
    assert!(matches!(
        Set::from_bytes(unknown),
        Err(SetError::Malformed("an unknown script"))
    ));
    // the superscript strike given the normal one's script repeats it
    let repeated = with_section(&bytes, b"STRK", |s| s[8 + 24 + 2] = 0);
    assert!(matches!(
        Set::from_bytes(repeated),
        Err(SetError::Malformed(_))
    ));
}

/// A 4x4 mask every pixel of which is `v`.
fn solid(v: u8) -> vtium_font::Glyph {
    vtium_font::Glyph::from_pixels(
        CELL.width,
        CELL.height,
        vtium_font::Format::Mask,
        vec![v; 16],
    )
    .expect("a glyph")
}

/// A face of 4x4 cells that draws every glyph full, holds a superscript
/// glyph of 7 its source made, has its subscript glyph on its way, and
/// has the built-in fonts' face for that meanwhile: one that draws every
/// glyph at 200.
struct Sourced;

struct Builtins;

impl Face for Builtins {
    fn cell(&self) -> Cell {
        CELL
    }
    fn size_step(&self) -> u32 {
        CELL.height
    }
    fn has_glyph(&self, _c: char) -> bool {
        true
    }
    fn draw(&self, _cluster: &[char], _style: Style) -> Option<vtium_font::Glyph> {
        Some(solid(200))
    }
}

impl Face for Sourced {
    fn cell(&self) -> Cell {
        CELL
    }
    fn size_step(&self) -> u32 {
        CELL.height
    }
    fn has_glyph(&self, _c: char) -> bool {
        true
    }
    fn draw(&self, _cluster: &[char], _style: Style) -> Option<vtium_font::Glyph> {
        Some(solid(255))
    }
    fn builtins(&self) -> Option<Box<dyn Face + '_>> {
        Some(Box::new(Builtins))
    }
    fn script(&self, _cluster: &[char], variant: Variant) -> vtium_font::ScriptFrom {
        match variant.script {
            Script::Super => vtium_font::ScriptFrom::Made(solid(7)),
            _ => vtium_font::ScriptFrom::Builtins,
        }
    }
}

// The font draws a script cell from the glyph its face's source made, as
// it is, and while that glyph is on its way from the built-in fonts'
// script glyph, never from the face's own normal glyph made small
// (ADR-1202 D11)
// covers: sgr.script
#[test]
fn a_script_cell_takes_its_sources_glyph_else_the_built_in_fonts() {
    let font = vtium_font::Font::new(Sourced);
    let sup = font
        .drawn_variant(&['A'], Width::Narrow, variant(Script::Super))
        .expect("drawn")
        .glyph;
    assert!(sup.pixels().iter().all(|&p| p == 7), "the source's glyph");
    let sub = font
        .drawn_variant(&['A'], Width::Narrow, variant(Script::Sub))
        .expect("drawn")
        .glyph;
    let builtin = vtium_font::Font::new(Builtins)
        .drawn_variant(&['A'], Width::Narrow, variant(Script::Sub))
        .expect("drawn")
        .glyph;
    assert_eq!(sub, builtin, "the built-in fonts' subscript glyph");
    assert!(sub.pixels().iter().all(|&p| p != 255), "not the face's own");
    // a normal cell is the face's own glyph
    let normal = font
        .drawn_variant(&['A'], Width::Narrow, variant(Script::Baseline))
        .expect("drawn")
        .glyph;
    assert!(normal.pixels().iter().all(|&p| p == 255));
}
