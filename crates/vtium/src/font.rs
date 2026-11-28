//! The font: which frit or built-in font vtium draws, its faces at each
//! height, its zoom steps and the automatic size, with the lookup's
//! fallback.
//!
//! The font is `font-name`, else vconsole's (ADR-1200 D9), else the
//! default set `terminus` where it is installed (ADR-1204 D2). A name is a
//! frit's (`NAME.vtfrit` in the frits' directory), else a kbd name, a path
//! or vconsole's font becomes a one-source frit made on the fly
//! ([`frit_named`]). A frit is generated at run time (ADR-1301 D2), in
//! the cell it states ([`frit_cell`], #145), else the cell its first
//! source gives, which the helper tells at the height the console draws
//! at, before the first frame and at each zoom of an outline frit: a
//! bitmap source's own cell (the operator, 2026-10-28), or an outline
//! source's cell at that height (the maintainer, 2026-11-01); else the
//! display's: the
//! built-in font draws in that cell at once, and the helper's glyphs
//! replace it, character by character, as they arrive ([`Chosen::Frit`],
//! [`crate::deferred`], [`crate::helper`]). The built-ins are Unscii-8
//! for cells under 16 pixels and unifont from 16, but in a zh, ja or ko
//! locale, which Unscii-8 does not cover (D7). OSC 50 switches among the
//! frits, the built-ins, the head's frit made on the fly and the fonts
//! root installed and trusts, given by their paths, while vtium runs, each
//! by the name its query reports ([`by_name`], D5).
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays, so a font that cannot be found never holds the start
//! in a loop.

use vtium_font::set::builtin_name;
use vtium_font::sprite::Range;
use vtium_font::{Cell, Face, Missing, Source, SpritePolicy};

use crate::config::Config;
use crate::config::keys::{
    GLYPH_SOURCE, GLYPH_SOURCE_BLOCK, GLYPH_SOURCE_BOX, GLYPH_SOURCE_BRAILLE, GLYPH_SOURCE_LEGACY,
    GLYPH_SOURCE_OCTANTS, GLYPH_SOURCE_SEXTANTS, Key, MISSING,
};
use crate::deferred::{DeferredFace, Shared};

mod batch;
mod chosen;
mod frits;
mod named;

pub use batch::{
    Came, Found, Sources, Wanted, gate, load_batch, package_palettes, read_text, start_batch,
};
pub use chosen::{Chosen, Rung, ZOOM_IN_MAX, zoom};
pub use frits::{
    CATEGORIES, FRITS, GENERATED, frit, frit_cell, frit_line_spacing, frit_size, frit_text,
    frit_warnings, frits, layer_note, layers_dir, log_layer_note, points_to_pixels, quoted,
    refusal, trust, whole_height, with_layers,
};
pub use named::{
    FONT_DIRS, builtin_named, by_name, font_folders, frit_named, qualifying, trusted_folder, wanted,
};

/// Each sprite range and the key that sets its source alone.
pub const RANGE_KEYS: [(Range, &Key); 6] = [
    (Range::BoxDrawing, &GLYPH_SOURCE_BOX),
    (Range::BlockElements, &GLYPH_SOURCE_BLOCK),
    (Range::Braille, &GLYPH_SOURCE_BRAILLE),
    (Range::Sextants, &GLYPH_SOURCE_SEXTANTS),
    (Range::Octants, &GLYPH_SOURCE_OCTANTS),
    (Range::LegacyComputing, &GLYPH_SOURCE_LEGACY),
];

fn source(text: &str) -> Option<Source> {
    match text {
        "font" => Some(Source::Font),
        "sprites" => Some(Source::Sprites),
        "auto" => Some(Source::Auto),
        _ => None,
    }
}

/// A font's sprite policy from the configuration: `glyph-source` for all
/// six ranges, else the face's own `default`, then each range's own key
/// over it (vtium-font's README, "#21 builds a font's policy").
pub fn policy(config: &Config, default: SpritePolicy) -> SpritePolicy {
    let base = config
        .text(&GLYPH_SOURCE)
        .and_then(source)
        .map_or(default, SpritePolicy::all);
    RANGE_KEYS.iter().fold(base, |p, &(range, key)| {
        match config.text(key).and_then(source) {
            Some(s) => p.with(range, s),
            None => p,
        }
    })
}

/// A frit's sprite policy from its `text` (ADR-1200 D1): `glyph-source=`
/// for all six ranges, else `default`, then each range's own
/// `glyph-source-<range>=` over it, read in the frit's line syntax as the
/// configuration's keys are named. A value or a range vtium does not know
/// changes nothing here; the helper refuses the frit naming its line.
pub fn frit_policy(text: &str, default: SpritePolicy) -> SpritePolicy {
    let lines = || {
        text.split('\n').filter_map(|line| {
            let (key, value) = line.split('#').next()?.split_once('=')?;
            Some((key.trim(), source(value.trim())?))
        })
    };
    let base = lines()
        .rfind(|&(key, _)| key == GLYPH_SOURCE.name)
        .map_or(default, |(_, s)| SpritePolicy::all(s));
    lines().fold(base, |p, (key, s)| {
        match RANGE_KEYS.iter().find(|(_, k)| k.name == key) {
            Some(&(range, _)) => p.with(range, s),
            None => p,
        }
    })
}

/// What a character the font lacks draws as, from `missing`.
pub fn missing(config: &Config) -> Missing {
    match config.text(&MISSING) {
        Some("hex") => Missing::Hex,
        _ => Missing::Replacement,
    }
}

/// The size tried once when the first lookup fails.
pub const FALLBACK_SIZE: u32 = 16;

/// Looks a font up at `size` with `find`, which gives the font at a size
/// or why there is none; `in_use` says whether a font is in use. Returns
/// the font and the size it was found at.
pub fn select<F, E>(
    size: u32,
    in_use: bool,
    mut find: impl FnMut(u32) -> Result<F, E>,
) -> Result<(F, u32), E> {
    match find(size) {
        Ok(font) => Ok((font, size)),
        Err(e) if in_use => Err(e),
        Err(_) => find(FALLBACK_SIZE).map(|f| (f, FALLBACK_SIZE)),
    }
}

/// The height `font-size=0` picks for a display `width` by `height`, for
/// a font whose smallest height is `smallest`: at most 120 columns,
/// taking a cell as half as wide as it is high, and at least 40 rows, and
/// never under 16; but where 16-pixel cells
/// would leave fewer than 80 by 25, the font's own smallest height, so a
/// 320x240 display draws an 8x8 font as 40 by 30 cells (vtium/vtium#118).
pub fn auto_size(width: u32, height: u32, smallest: u32) -> u32 {
    if width == 0 || height == 0 {
        return FALLBACK_SIZE;
    }
    let by_width = width.saturating_mul(2) / 120;
    let size = by_width.min(height / 40).max(16);
    if small_display(width, height) {
        size.min(smallest.max(1))
    } else {
        size
    }
}

/// Whether a display `width` by `height` is small: 16-pixel cells, 8
/// pixels wide, would leave it fewer than 80 columns or 25 rows. There
/// [`auto_size`] takes the font's own smallest height, and with no font
/// named vtium draws the built-in font's 8×8 strike in place of the
/// default set, so a 320×240 panel shows 40 columns by 30 rows. A display
/// whose size is not known yet, 0 by 0, is not small.
///
/// References:
/// - ADR-1200 D17: the small-display rule.
/// - ADR-1204 D2: an 8-pixel cell draws the built-in font's 8×8 strike,
///   because Terminus has no strike under 12 pixels.
pub fn small_display(width: u32, height: u32) -> bool {
    width != 0 && height != 0 && (width < 80 * 8 || height < 25 * 16)
}

/// Whether vtium draws the default set on a head whose displays have
/// `sizes`, each a width and height in pixels. The head counts as small
/// when its smallest width and its smallest height make a small display
/// ([`small_display`]), as the automatic size takes them, and there vtium
/// draws the built-in font's 8×8 strike instead. With no display known yet
/// the default set draws.
///
/// References:
/// - ADR-1204 D2: on a display where the small-display rule picks 8-pixel
///   rows, vtium draws the built-in font's 8×8 strike in place of
///   `terminus`, which has no strike under 12 pixels.
pub fn default_set_fits(sizes: impl IntoIterator<Item = (u32, u32)>) -> bool {
    let smallest = sizes
        .into_iter()
        .reduce(|(w, h), (w2, h2)| (w.min(w2), h.min(h2)));
    smallest.is_none_or(|(w, h)| !small_display(w, h))
}

/// The font set vtium draws when no font is named: bitmap Terminus at
/// every strike from 12 to 32 pixels, then GNU Unifont, which every vtium
/// build installs. It sits in the catalog's `console` folder.
///
/// References:
/// - ADR-1204 D2: `terminus` is the default set, after the font that a
///   head's look, `font-name` or vconsole names, and before the built-in
///   font.
pub const DEFAULT_SET: &str = "terminus";

/// The smallest and largest side of a frit's `cell=` and a look's
/// `cell`, from the key table.
pub use crate::config::keys::{FRIT_CELL_MAX, FRIT_CELL_MIN};

/// A face a display draws with: [`Send`], as its head's render thread
/// draws it (ADR-1401 D2).
pub type DisplayFace = Box<dyn Face + Send>;

/// The built-in strike that draws in `cell` (#145, ADR-1200 D7), and the
/// height it draws at. With no `name`, it is unifont at the largest whole
/// factor whose cell fits in `cell`; where even unifont's 8x16 does not
/// fit, it is the smaller strike, Unscii-8 at 8x8, but unifont in a zh, ja
/// or ko locale. The built-in that `name` names keeps its own strike, at
/// the largest whole factor whose cell fits, at least once. The face cuts
/// a strike that does not fit ([`DeferredFace`]).
fn builtin_strike(name: Option<&'static str>, cell: Cell, locale: &str) -> (&'static str, u32) {
    let factor = |w: u32, h: u32| {
        let across = cell.width.checked_div(w).unwrap_or(0);
        across.min(cell.height.checked_div(h).unwrap_or(0))
    };
    let height = match name {
        Some("unscii-8") => factor(8, 8).max(1).saturating_mul(8),
        Some(_) => factor(8, 16).max(1).saturating_mul(16),
        None => match factor(8, 16) {
            0 => 8,
            k => k.saturating_mul(16),
        },
    };
    (name.unwrap_or_else(|| builtin_name(height, locale)), height)
}

/// Whether a built-in glyph `drawn` pixels wide, in a cluster of
/// `cells` cells of `cell`, is squeezed to fit them, the last step of the
/// fallback (ADR-1302 D14): when it is exactly twice their width, or
/// wider than them and 2 pixels more.
pub fn squeezed(drawn: u32, cell: Cell, cells: u32) -> bool {
    let w = cell.width.saturating_mul(cells);
    drawn == w.saturating_mul(2) || drawn > w.saturating_add(2)
}

/// Whether the built-in fonts, drawing in `cell` under `locale`, squeeze
/// `c` to fit its cells ([`squeezed`]); false where they lack it.
pub fn builtin_squeezes(c: char, cell: Cell, locale: &str) -> bool {
    let Some(face) = builtin_in(None, cell, locale) else {
        return false;
    };
    let cells = u32::from(vtium_term::cluster_width(&[c]).max(1));
    face.has_glyph(c)
        && face
            .draw(&[c], vtium_font::Style::default())
            .is_some_and(|g| squeezed(g.width(), cell, cells))
}

/// The built-in face that draws in `cell`: the strike
/// [`builtin_strike`] chooses, at its height.
fn builtin_in(name: Option<&'static str>, cell: Cell, locale: &str) -> Option<DisplayFace> {
    let (strike, height) = builtin_strike(name, cell, locale);
    Chosen::Builtin {
        name: Some(strike),
        cell: None,
    }
    .face(height, locale)
}

#[cfg(test)]
thread_local! {
    /// How many times this thread has called [`own_face`], so that a test
    /// can check which frames build the face.
    pub static OWN_FACES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The face vtium draws its own messages in for a grid of `cell`: the
/// notices, the on-screen display's line and dot, and the panic screen.
/// It is the built-in font, "vtium built-in", whose strike the cell's
/// height chooses at the largest whole multiple that fits, centred in the
/// cell where the two differ. It draws with no font file and no font
/// helper, whatever font the head draws its terminal in.
///
/// References:
/// - ADR-1204 D1: vtium's own messages always draw in the built-in font,
///   and which strike draws which cell.
pub fn own_face(cell: Cell) -> Option<DisplayFace> {
    #[cfg(test)]
    OWN_FACES.set(OWN_FACES.get().saturating_add(1));
    let face = vtium_font::set::own::face(cell)?;
    if face.cell() == cell {
        return Some(Box::new(face));
    }
    Some(Box::new(DeferredFace::new(
        Box::new(face),
        cell,
        cell.height,
        Shared::default(),
    )))
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod lists_tests;

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests;
