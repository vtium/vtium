//! The sizes a look's fonts supply, which its card's Size line offers.
//!
//! A look's size is its text font's size in points. An outline font draws
//! at any size, so it offers vtium-config's usual sizes. A bitmap font
//! draws only at its strikes, the heights its file holds, so it offers the
//! point sizes that draw each strike at the displays' density, and no
//! other. The text font is the font set's first source of its own, found
//! among the font files that the font helper described when vtium-config
//! started, so working the sizes out reads no folder.
//!
//! References:
//! - ADR-1205 D7: a look offers the sizes its fonts supply.
//! - ADR-1200 D17: a size in points is points × DPI ÷ 72 pixels, rounded to the nearest pixel.
//! - ADR-1204 D2: each cell draws the tallest strike that fits it.

use std::path::Path;

use vtium_config_core::found::fonts::FontFile;
use vtium_config_core::ui::Shared;
use vtium_font_helper::frit::{Frit, Origin};

/// vtium-config's usual sizes, in points, which a font that draws at any
/// size offers.
pub const USUAL: [u32; 15] = [6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32];

/// The density sizes are worked out at where vtium.conf sets none: the
/// console's own default.
pub const DPI: u32 = 96;

/// The sizes a text font supplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Supplied {
    /// The font draws at any size, or vtium-config cannot tell which
    /// sizes it draws, so the usual sizes stand.
    Any,
    /// A bitmap font: the point size that draws each of its strikes, each
    /// with the strike's height in pixels, smallest first.
    Strikes(Vec<(u32, u32)>),
}

impl Supplied {
    /// The point sizes offered, smallest first.
    pub fn points(&self) -> Vec<u32> {
        match self {
            Supplied::Any => USUAL.to_vec(),
            Supplied::Strikes(s) => s.iter().map(|(p, _)| *p).collect(),
        }
    }
}

/// The density the document gives sizes at: its `display-dpi`, else
/// [`DPI`].
pub fn dpi(shared: &Shared) -> u32 {
    vtium_settings::setting(shared.document().text(), "display-dpi")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|d| *d > 0)
        .unwrap_or(DPI)
}

/// The font set `name`'s text, as the document holds it: the document's
/// own where it adds or changes the set, else the installed set unless
/// the document removes it.
pub fn font_set_text<'a>(shared: &'a Shared, name: &str) -> Option<&'a [u8]> {
    let document = shared.document();
    let sets = &document.content().font_sets;
    match sets.kept.get(name) {
        Some(text) => Some(text.as_bytes()),
        None if sets.removed.contains(name) => None,
        None => document.installed().frits.get(name).map(Vec::as_slice),
    }
}

/// The font set `name` read as the console reads it, without its layers;
/// `None` where it does not read or the document has none of that name.
pub fn font_set(shared: &Shared, name: &str) -> Option<Result<Frit, String>> {
    let text = font_set_text(shared, name)?;
    let file = Path::new(name).with_extension("vtfrit");
    Some(Frit::parse(text, &file, name).map_err(|e| e.to_string()))
}

/// The sizes the text font of the font set `name` supplies at `dpi`.
pub fn supplied(shared: &Shared, name: Option<&str>, dpi: u32) -> Supplied {
    let Some(Ok(frit)) = name.and_then(|n| font_set(shared, n)) else {
        return Supplied::Any;
    };
    let Some(text) = frit
        .sources
        .iter()
        .find(|s| s.layer.is_none() && !s.style.bold && !s.style.italic)
    else {
        return Supplied::Any;
    };
    let files: Vec<&FontFile> = match &text.origin {
        Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => {
            shared.found.fonts.file(p).into_iter().collect()
        }
        Origin::Font(family) => {
            let key = squashed(family);
            shared
                .found
                .fonts
                .files()
                .iter()
                .filter(|f| f.faces().iter().any(|face| squashed(&face.family) == key))
                .collect()
        }
        _ => Vec::new(),
    };
    let faces: Vec<_> = files.iter().flat_map(|f| f.faces()).collect();
    if faces.is_empty() || faces.iter().any(|f| f.outline) {
        return Supplied::Any;
    }
    let mut heights: Vec<u32> = faces
        .iter()
        .flat_map(|f| f.strikes.iter().map(|s| s.height))
        .collect();
    heights.sort_unstable();
    heights.dedup();
    if heights.is_empty() {
        return Supplied::Any;
    }
    Supplied::Strikes(strike_points(&heights, dpi))
}

/// A family with case, blanks, `-` and `_` ignored, as the font helper
/// compares one.
fn squashed(family: &str) -> String {
    family
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// The point size that draws each strike of `heights`, shortest first, at
/// `dpi`: the smallest whole size whose height in pixels reaches the
/// strike's and stays below the next strike's. A strike that no whole size
/// reaches before the next one is left out.
pub fn strike_points(heights: &[u32], dpi: u32) -> Vec<(u32, u32)> {
    let dpi = u64::from(dpi.max(1));
    let pixels = |points: u32| (u64::from(points) * dpi * 2 + 72) / 144;
    let most = USUAL.iter().copied().max().unwrap_or(32).saturating_mul(4);
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (i, &height) in heights.iter().enumerate() {
        let next = heights
            .get(i.saturating_add(1))
            .map_or(u64::MAX, |h| u64::from(*h));
        let points = (1..=most).find(|p| pixels(*p) >= u64::from(height));
        if let Some(points) = points
            && pixels(points) < next
            && !out.iter().any(|(p, _)| *p == points)
        {
            out.push((points, height));
        }
    }
    out
}
