//! The font vtium chose, its faces at each height, its zoom steps and
//! the zoom from one step to the next.

use vtium_font::set::{SetFace, builtin, builtin_name};
use vtium_font::spacing::Band;
use vtium_font::{Cell, Face, Unifont};

use crate::deferred::{DeferredFace, Shared};
use crate::helper::{FritText, OwnCell};
use crate::layout::Zoom;

use super::frits::{frit_cell_at, frit_name, frit_outline, frit_struck, points_under};
use super::{
    DisplayFace, FALLBACK_SIZE, GENERATED, auto_size, builtin_in, builtin_strike, frit_cell,
    frit_line_spacing, frit_size, points_to_pixels, whole_height,
};

/// The font vtium draws.
#[derive(Clone, Debug)]
pub enum Chosen {
    /// The built-in sets (ADR-1200 D7): the one `name` names, or with
    /// `None`, Unscii-8 for cells under 16 pixels and unifont from 16, but
    /// unifont alone in a zh, ja or ko locale. With a `cell`, the cell the
    /// head's look states (ADR-1403 D5), they draw in that cell, fitted to
    /// it as they draw in a frit's cell, and a zoom steps by its whole
    /// multiples (D17); with none, in their own cell at the height drawn.
    Builtin {
        name: Option<&'static str>,
        cell: Option<Cell>,
    },
    /// A frit, generated at run time (ADR-1301 D2), in the cell it
    /// states, else the display's (#145): drawn in the built-in font, each
    /// character the helper has sent drawn from `generated` in its place.
    Frit {
        /// The name the font goes by, as it was given.
        name: String,
        /// The frit as the helper is handed it, under a frit's name.
        frit: FritText,
        /// The look's `cell`, else the frit's `cell=` unless `font-size`
        /// is configured.
        cell: Option<Cell>,
        /// `size=`, unless `font-size` is configured or the look states a
        /// cell.
        size: Option<u32>,
        /// Its bitmap first source's own cell, as the helper tells it for a
        /// frit that states no cell, with no `font-size` configured.
        own: Option<Cell>,
        /// Its outline first source's cell at each height the helper was
        /// asked at, for a frit that states no cell; for a bitmap first
        /// source with strikes, the cell of the strike each height draws.
        heights: std::collections::BTreeMap<u32, Cell>,
        /// Its first source is an outline font, so a zoom steps by 1
        /// pixel: guessed from its text until the helper's answer says.
        outline: bool,
        /// Its first source is a bitmap font with strikes, read from its
        /// text, so with no cell stated it asks the helper for its cell at
        /// each height, as an outline frit does, and keeps each answer in
        /// `heights` by the height asked at.
        struck: bool,
        /// A `font-size` is configured, which sets the cell's height of a
        /// frit with strikes ([`Chosen::face_spaced`]).
        sized: bool,
        /// `line-spacing=`, a whole percentage of the text's height
        /// (ADR-1302 D13), which vtium.conf's `font-line-spacing`
        /// overrides.
        line_spacing: Option<u32>,
        /// Its `fit=` lines, which place the built-in glyphs drawn in its
        /// cell (ADR-1302 D13).
        fits: std::sync::Arc<crate::deferred::FritFits>,
        generated: Shared,
    },
}

/// Heights `step` and its whole multiples, from `min` and under `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    pub step: u32,
    pub min: u32,
    pub end: u32,
}

impl Chosen {
    /// The built-in sets in their own cell, Unscii-8 or unifont by height.
    pub const BUILTINS: Chosen = Chosen::Builtin {
        name: None,
        cell: None,
    };

    /// The built-in sets, Unscii-8 or unifont by height, in `look`, the
    /// cell the head's look states, where it states one (ADR-1403 D5).
    pub fn builtins(look: Option<Cell>) -> Chosen {
        Chosen::Builtin {
            name: None,
            cell: look,
        }
    }

    /// The frit `frit`, its cell and size read from its text before the
    /// first frame (#145), unless `sized`, a configured `font-size`, wins
    /// over them (ADR-1200 D16, D17), or `look`, the cell the head's look
    /// states, which is more specific than the frit and wins over its
    /// `cell=` and `size=` alike (ADR-1403 D5). Its `size=` counts for an
    /// outline frit only: points do not apply to bitmap fonts (the
    /// operator's decision on vtium/vtium#145). A name that is not a
    /// frit's goes to the helper as [`GENERATED`]. The error is why they
    /// cannot be read.
    pub fn frit(mut frit: FritText, sized: bool, look: Option<Cell>) -> Result<Chosen, String> {
        let cell = frit_cell(&frit.text)?;
        let size = frit_size(&frit.text)?;
        let line_spacing = frit_line_spacing(&frit.text)?;
        let fits = std::sync::Arc::new(crate::deferred::FritFits::of(&frit.text));
        let outline = frit_outline(&frit.text);
        let struck = frit_struck(&frit.text);
        let name = frit.name.clone();
        if !frit_name(&name) {
            GENERATED.clone_into(&mut frit.name);
        }
        Ok(Chosen::Frit {
            name,
            outline,
            frit,
            cell: look.or(cell.filter(|_| !sized)),
            size: size.filter(|_| !sized && outline && look.is_none()),
            own: None,
            heights: std::collections::BTreeMap::new(),
            struck,
            sized,
            line_spacing,
            fits,
            generated: Shared::default(),
        })
    }

    /// The cell the font draws in, whatever the height asked: the look's
    /// `cell`, else a frit's `cell=` (ADR-1200 D17, ADR-1403 D5).
    fn stated(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { cell, .. } | Chosen::Builtin { cell, .. } => *cell,
        }
    }

    /// Whether the font is a frit that asks the helper for its first
    /// source's cell at `height`: one that states no cell, and either an
    /// outline frit or a bitmap frit with strikes with no answer at
    /// `height` yet, whether or not a `font-size` is configured, or another
    /// bitmap frit with no answer yet and no `font-size` configured, which
    /// `sized` says. Each height of a font with strikes can draw another
    /// strike, so its cell depends on the height, as an outline font's
    /// does.
    ///
    /// References:
    /// - ADR-1200 D17: an outline frit asks at each height (the maintainer,
    ///   2026-11-01), and a bitmap frit at its own size (the operator,
    ///   2026-10-28).
    /// - ADR-1204 D2: the strike for the automatic height, for a configured
    ///   font size and for each zoom step.
    pub fn wants_own_cell(&self, height: u32, sized: bool) -> bool {
        match self {
            Chosen::Frit {
                cell: None,
                outline: true,
                heights,
                ..
            }
            | Chosen::Frit {
                cell: None,
                struck: true,
                heights,
                ..
            } => !heights.contains_key(&height),
            Chosen::Frit {
                cell: None,
                own: None,
                outline: false,
                ..
            } => !sized,
            _ => false,
        }
    }

    /// Takes `answer`, the cell the helper told for the frit's first
    /// source, which also says whether that source is a bitmap or an
    /// outline font, whatever the frit's text suggested, as the answer at
    /// the cell's own height ([`Chosen::set_own_cell_at`]).
    pub fn set_own_cell(&mut self, answer: OwnCell, sized: bool) {
        let height = match answer {
            OwnCell::Bitmap(cell) | OwnCell::Outline(cell) => cell.height,
        };
        self.set_own_cell_at(answer, height, sized);
    }

    /// Takes `answer`, the cell the helper told for the frit's first
    /// source when asked at `height`, which also says whether that source
    /// is a bitmap or an outline font, whatever the frit's text suggested.
    /// A bitmap source's own cell counts only where no `font-size` is
    /// configured, as `sized` says. A bitmap source with strikes answers
    /// with the cell of the tallest strike no taller than `height`, which
    /// then holds at every height from the strike's own up to `height`.
    pub fn set_own_cell_at(&mut self, answer: OwnCell, height: u32, sized: bool) {
        if let Chosen::Frit {
            cell: stated,
            own,
            heights,
            outline,
            struck,
            ..
        } = self
        {
            match answer {
                OwnCell::Bitmap(cell) if *struck && stated.is_none() => {
                    *outline = false;
                    for at in [height, whole_height(cell, height), cell.height] {
                        heights.insert(at, cell);
                    }
                }
                OwnCell::Bitmap(cell) => {
                    *outline = false;
                    if !sized {
                        *own = Some(cell);
                    }
                }
                OwnCell::Outline(cell) => {
                    *outline = true;
                    heights.insert(cell.height, cell);
                }
            }
        }
    }

    /// The cell a bitmap frit's heights, and the built-ins' in a look's
    /// cell, are whole multiples of, and the cell an outline frit states:
    /// the one stated, else a bitmap first source's own.
    fn base_cell(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { own, .. } => self.stated().or(*own),
            Chosen::Builtin { .. } => self.stated(),
        }
    }

    /// Whether the font is a frit a zoom steps by 1 pixel, whose first
    /// source is an outline font.
    pub fn outline(&self) -> bool {
        matches!(self, Chosen::Frit { outline: true, .. })
    }

    /// Whether the font is a frit that states no cell and whose first
    /// source is a bitmap font with strikes, so each height it draws at
    /// takes the cell of the strike the helper names for that height.
    pub fn struck(&self) -> bool {
        matches!(
            self,
            Chosen::Frit {
                cell: None,
                outline: false,
                struck: true,
                ..
            }
        )
    }

    /// The cell of the strike that a frit with strikes draws at `height`,
    /// as the helper answered for that height. `None` before the answer,
    /// and for any other font.
    pub fn strike_at(&self, height: u32) -> Option<Cell> {
        match self {
            Chosen::Frit { heights, .. } if self.struck() => heights.get(&height).copied(),
            _ => None,
        }
    }

    /// The built-in font drawn at `height` in `locale`: in a look's cell,
    /// the strike that [`builtin_strike`] chooses for the cell at that
    /// height, which is what the face draws.
    fn builtin_at(&self, height: u32, locale: &str) -> &'static str {
        match self {
            Chosen::Builtin {
                name,
                cell: Some(look),
            } => builtin_strike(*name, frit_cell_at(Some(*look), height, *look), locale).0,
            Chosen::Builtin {
                name: Some(name),
                cell: None,
            } => name,
            Chosen::Builtin {
                name: None,
                cell: None,
            }
            | Chosen::Frit { .. } => builtin_name(height, locale),
        }
    }

    /// The font's name at `height` in `locale`, as OSC 50 spells it: the
    /// frit's, else the built-in's.
    pub fn name(&self, height: u32, locale: &str) -> &str {
        match self {
            Chosen::Frit { name, .. } => name,
            Chosen::Builtin { .. } => self.builtin_at(height, locale),
        }
    }

    /// The face at `height`: a frit's, in its cell at that height, drawn
    /// in the built-in font until the helper's glyphs land; the built-ins
    /// in a look's cell, fitted to that cell's whole multiple at that
    /// height; else the built-in's strike of that height, else the tallest
    /// under it scaled by the largest whole factor that fits. The built-in
    /// unifont draws through its own face. A frit's own `line-spacing=`
    /// makes the cell taller where it applies ([`Chosen::face_spaced`]).
    pub fn face(&self, height: u32, locale: &str) -> Option<DisplayFace> {
        self.face_spaced(height, locale, None)
    }

    /// The rows line spacing adds to a cell whose text is `text` pixels
    /// high (ADR-1302 D13): `spacing`, vtium.conf's `font-line-spacing`,
    /// else a frit's `line-spacing=`, else none. A cell the look or the
    /// frit states, `cell` or `cell=`, is exact and takes none; the cell
    /// vtium works out takes it: a frit's fonts' own cell, the automatic
    /// rule's, and the cell a configured `font-size` gives in place of a
    /// frit's `cell=`.
    fn band(&self, text: u32, spacing: Option<u32>) -> Band {
        if self.stated().is_some() {
            return Band::NONE;
        }
        let own = match self {
            Chosen::Frit { line_spacing, .. } => *line_spacing,
            Chosen::Builtin { .. } => None,
        };
        Band::of(text, spacing.or(own).unwrap_or(0))
    }

    /// [`Chosen::face`], with `spacing`, vtium.conf's
    /// `font-line-spacing`, over a frit's own `line-spacing=`: the cell is
    /// taller by the rows `Chosen::band` gives, split above and below
    /// the text band, and each glyph is placed in it (ADR-1302 D13). The
    /// text band is the cell [`Chosen::face`] would draw in, so `height`
    /// stays the text's height.
    pub fn face_spaced(
        &self,
        height: u32,
        locale: &str,
        spacing: Option<u32>,
    ) -> Option<DisplayFace> {
        if let Chosen::Builtin {
            name,
            cell: Some(look),
        } = self
        {
            // D7: the built-ins fitted to the look's cell, as to a frit's
            // stated cell, and to its whole multiples at a zoom
            let cell = frit_cell_at(Some(*look), height, *look);
            return Some(Box::new(DeferredFace::new(
                builtin_in(*name, cell, locale)?,
                cell,
                look.height,
                Shared::default(),
            )));
        }
        if let Chosen::Frit {
            generated,
            heights,
            fits,
            sized,
            ..
        } = self
        {
            let at = Chosen::BUILTINS.face(height, locale)?;
            let stated = self.base_cell();
            let (cell, step) = match (stated, self.outline()) {
                // A bitmap source with strikes: the cell of the strike the
                // helper names for this height, or its whole multiple
                // above the tallest strike. A configured font size keeps
                // its own height, as wide as that strike's whole multiple,
                // and the helper centres the strike in it.
                // References: ADR-1204 D2 decides the strike each height
                // draws.
                (None, false) if self.struck() => match heights.get(&height) {
                    Some(c) => {
                        let whole = frit_cell_at(Some(*c), height, *c);
                        if *sized {
                            (
                                Cell {
                                    width: whole.width,
                                    height: height.max(whole.height),
                                },
                                1,
                            )
                        } else {
                            (whole, c.height)
                        }
                    }
                    None => (frit_cell_at(None, height, at.cell()), at.size_step()),
                },
                // D17, an outline frit steps by 1: the stated cell at its
                // own height, else the helper's answer
                // at this height, else the built-in font's width
                (Some(c), true) if c.height == height => (c, 1),
                (_, true) => match heights.get(&height) {
                    Some(c) => (*c, 1),
                    None => (frit_cell_at(None, height, at.cell()), 1),
                },
                // D17, a bitmap frit steps by the font's height: the
                // stated or own cell's whole multiples
                (Some(c), false) => (frit_cell_at(Some(c), height, at.cell()), c.height),
                // D17: with no cell stated, the display's
                (None, false) => (frit_cell_at(None, height, at.cell()), at.size_step()),
            };
            let builtin = builtin_in(None, cell, locale)?;
            let band = self.band(cell.height, spacing);
            return Some(Box::new(
                DeferredFace::spaced(builtin, cell, band, step, std::sync::Arc::clone(generated))
                    .with_fits(std::sync::Arc::clone(fits)),
            ));
        }
        let face: DisplayFace = match self.builtin_at(height, locale) {
            "unifont" => Box::new(Unifont::new(height)),
            name => Box::new(SetFace::new(builtin(name)?, height)?),
        };
        let band = self.band(face.cell().height, spacing);
        // with right-to-left text the built-in font draws through the
        // lookup that draws a joined letter's presentation form (ADR-1004
        // D6), as it does in a look's cell
        if band.is_none() && !cfg!(feature = "bidi") {
            return Some(face);
        }
        // D13: the built-in font in a taller cell, placed by the face that
        // places a frit's built-in glyphs, with a store that stays empty
        let (cell, step) = (face.cell(), face.size_step());
        Some(Box::new(DeferredFace::spaced(
            face,
            cell,
            band,
            step,
            Shared::default(),
        )))
    }

    /// The heights a zoom steps through (ADR-1200 D17): an outline frit's
    /// every height from the built-in's smallest, a bitmap frit's stated
    /// or own cell's height and its whole multiples, and the built-ins' in
    /// a look's cell the same. The built-ins in their own cell, and a
    /// bitmap frit with no cell, step through Unscii-8's 8 under 16, where
    /// it is drawn, and unifont's multiples of 16 from 16.
    pub(super) fn rungs(&self, locale: &str) -> Vec<Rung> {
        let all = |step| Rung {
            step,
            min: step,
            end: u32::MAX,
        };
        // D17, an outline frit steps by 1
        if self.outline() {
            return vec![Rung {
                step: 1,
                min: Chosen::BUILTINS.smallest(locale),
                end: u32::MAX,
            }];
        }
        // D17, a bitmap frit steps by the font's height
        if let Some(cell) = self.base_cell() {
            return vec![all(cell.height)];
        }
        match self {
            Chosen::Builtin {
                name: Some("unscii-8"),
                ..
            } => vec![all(8)],
            Chosen::Builtin { name: Some(_), .. } => vec![all(16)],
            Chosen::Builtin { name: None, .. } | Chosen::Frit { .. }
                if builtin_name(8, locale) == "unscii-8" =>
            {
                vec![
                    Rung {
                        step: 8,
                        min: 8,
                        end: 16,
                    },
                    all(16),
                ]
            }
            Chosen::Builtin { name: None, .. } | Chosen::Frit { .. } => vec![all(16)],
        }
    }

    /// The height `font-size=0` picks for a display `width` by `height` at
    /// `dpi`: the height of the cell a frit states, or an outline frit's
    /// `size=` in points at `dpi`, which no display's size moves (#145,
    /// ADR-1200 D17); else [`auto_size`] for the font's smallest height,
    /// which a frit's own cell takes at the largest whole multiple not
    /// over it, at least the cell itself, so a bitmap font keeps its own
    /// pixels whole (the operator, 2026-10-28).
    pub fn auto_height(&self, width: u32, height: u32, locale: &str, dpi: f64) -> u32 {
        if let Some(cell) = self.stated() {
            return cell.height;
        }
        match self {
            Chosen::Frit {
                size: Some(points), ..
            } => points_to_pixels(*points, dpi),
            _ => self.snap(auto_size(width, height, self.smallest(locale))),
        }
    }

    /// The height the font draws when `height` is asked for: a bitmap
    /// frit's stated or own cell's whole multiple at or under it, at least
    /// the cell itself (#145, ADR-1200 D17); with strikes and no
    /// `font-size` configured, the whole multiple of the strike the helper
    /// named for `height` (ADR-1204 D2); else `height`.
    pub fn snap(&self, height: u32) -> u32 {
        if let Chosen::Frit { sized: false, .. } = self
            && let Some(strike) = self.strike_at(height)
        {
            return whole_height(strike, height);
        }
        match self.base_cell() {
            Some(cell) if !self.outline() => frit_cell_at(Some(cell), height, cell).height,
            _ => height,
        }
    }

    /// The height a zoom asks for from `size` at `dpi` (ADR-1200 D17): an
    /// outline frit's next whole point in its direction whose pixels
    /// differ from `size` (the operator's decision on vtium/vtium#145);
    /// else the next of its rungs. `None` past [`ZOOM_IN_MAX`]
    /// in, or below the smallest out.
    pub fn zoom(&self, size: u32, dir: Zoom, locale: &str, dpi: f64) -> Option<u32> {
        if !self.outline() {
            return zoom(&self.rungs(locale), size, dir);
        }
        let under = points_under(size, dpi);
        match dir {
            Zoom::In if size > ZOOM_IN_MAX => None,
            Zoom::In => (under.saturating_add(1)..=under.saturating_add(POINTS_SCAN))
                .map(|p| points_to_pixels(p, dpi))
                .find(|&h| h > size),
            Zoom::Out => (under.saturating_sub(POINTS_SCAN)..=under)
                .rev()
                .map(|p| points_to_pixels(p, dpi))
                .find(|&h| h < size)
                .filter(|&h| h >= self.smallest(locale)),
        }
    }

    /// The smallest height the font draws.
    pub fn smallest(&self, locale: &str) -> u32 {
        self.rungs(locale)
            .iter()
            .map(|r| r.min)
            .min()
            .unwrap_or(FALLBACK_SIZE)
    }
}

/// The largest height a zoom in starts from.
pub const ZOOM_IN_MAX: u32 = 150;

/// The points an outline frit's zoom looks through for a height that
/// differs: a pixel is at most 3.6 points at vtium-platform's coarsest
/// density believed, 20 DPI, and rounding moves it by under one more.
const POINTS_SCAN: u32 = 5;

/// The height a zoom asks for from `size`, the next of `rungs` in its
/// direction; `None` past [`ZOOM_IN_MAX`] in, or below the smallest out.
pub fn zoom(rungs: &[Rung], size: u32, dir: Zoom) -> Option<u32> {
    let heights = rungs.iter().filter(|r| r.step > 0);
    match dir {
        Zoom::In if size > ZOOM_IN_MAX => None,
        Zoom::In => heights
            .filter_map(|r| {
                let next = size
                    .checked_div(r.step)?
                    .checked_add(1)?
                    .checked_mul(r.step)?;
                let next = next.max(r.min);
                (next < r.end).then_some(next)
            })
            .min(),
        Zoom::Out => heights
            .filter_map(|r| {
                // the multiple under `size`, and the last under `end`
                let floor = |n: u32| n.checked_div(r.step)?.checked_mul(r.step);
                let below = floor(size.checked_sub(1)?)?.min(floor(r.end.saturating_sub(1))?);
                (below >= r.min).then_some(below)
            })
            .max(),
    }
}
