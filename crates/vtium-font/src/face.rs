//! [`Face`], what a glyph source provides, and [`Font`], which draws a
//! cluster into its cells from a face and the sprites.

use crate::glyph::Glyph;
use crate::hex;
use crate::policy::{Coverage, Missing, RangeCoverage, Source, SpritePolicy};
use crate::script::{Script, Variant};
use crate::spacing::Band;
use crate::sprite::{self, Range};

/// How a glyph is drawn. Bitmap faces draw bold by widening each row by a
/// pixel, and draw italic by shearing the glyph about its baseline
/// ([`crate::italic`], ADR-1302 D4). For bold italic the widening comes
/// first and the shear moves the widened rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
}

/// How many cells a glyph spans.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Width {
    #[default]
    Narrow,
    Wide,
}

impl Width {
    /// 1 or 2.
    pub fn cells(self) -> u32 {
        match self {
            Width::Narrow => 1,
            Width::Wide => 2,
        }
    }
}

/// A cell's size in pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub width: u32,
    pub height: u32,
}

/// Where underline and strike-through go in a cell, in pixel rows from its
/// top, each inside the cell and at least a row thick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Decorations {
    pub underline: u32,
    pub underline_thickness: u32,
    pub strike: u32,
    pub strike_thickness: u32,
}

impl Decorations {
    /// The lines a cell `height` tall gets from a face with no metrics of
    /// its own: a sixteenth of the cell thick, between 1 and a quarter of
    /// the cell, the underline on the bottom rows and the strike from the
    /// middle.
    pub fn for_height(height: u32) -> Decorations {
        let h = height.max(1);
        let thick = (h / 16).clamp(1, (h / 4).max(1));
        let bottom = h.saturating_sub(thick);
        Decorations {
            underline: bottom,
            underline_thickness: thick,
            strike: (h / 2).min(bottom),
            strike_thickness: thick,
        }
    }
}

/// A glyph source: a font file, the built-in unifont, or later an outline
/// font.
pub trait Face {
    /// The cell this face draws into.
    fn cell(&self) -> Cell;

    /// How far one zoom step moves the height asked for: the font's own
    /// height for a bitmap font, which scales by whole factors.
    fn size_step(&self) -> u32;

    /// Whether the face holds a glyph for `c`.
    fn has_glyph(&self, c: char) -> bool;

    /// How many of `range`'s characters the face holds, from
    /// [`Face::has_glyph`]. A face that reads a character map may count
    /// it faster.
    fn coverage(&self, range: Range) -> RangeCoverage {
        RangeCoverage::count(range, |c| self.has_glyph(c))
    }

    /// The policy a [`Font`] of this face draws with until it is given
    /// one: [`SpritePolicy::default_for`] the face's coverage.
    fn sprite_policy(&self) -> SpritePolicy {
        SpritePolicy::default_for(&Coverage::new(|range| self.coverage(range)))
    }

    /// The glyph for `cluster` at the face's own width for it, marked with
    /// the cells that width spans, or `None`
    /// when the face lacks it. A face that cannot shape draws the first
    /// character.
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph>;

    /// The glyph drawn when the face lacks the character, U+25A1 and `?`.
    fn last_resort(&self, _style: Style) -> Option<Glyph> {
        None
    }

    /// Where underline and strike-through go.
    fn decorations(&self) -> Decorations {
        Decorations::for_height(self.cell().height)
    }

    /// The whole factor a bitmap face scales its glyphs by, each pixel a
    /// square; 1 for a face that draws at its own size.
    fn scale(&self) -> u32 {
        1
    }

    /// [`Face::draw`] at scale 1, for a renderer that stores a bitmap
    /// glyph small and scales it while drawing (ADR-1300 D2). `None` from
    /// a face that cannot.
    fn draw_unscaled(&self, _cluster: &[char], _style: Style) -> Option<Glyph> {
        None
    }

    /// [`Face::last_resort`] at scale 1.
    fn last_resort_unscaled(&self, _style: Style) -> Option<Glyph> {
        None
    }

    /// The rows the cell adds above and below its text band for line
    /// spacing (ADR-1302 D13): none for a face whose cell is the text's
    /// own height.
    fn band(&self) -> Band {
        Band::NONE
    }

    /// The built-in fonts fitted to this face's cell, which the console
    /// draws its own notices and focus mark with (ADR-1401 D5, D7), for a
    /// face that draws a font set's glyphs over the built-in fonts. `None`
    /// for any other face, which draws the notices itself: the built-in
    /// fonts' own faces, and every face that is not the console's.
    fn builtins(&self) -> Option<Box<dyn Face + '_>> {
        None
    }

    /// Where the superscript or subscript glyph for `cluster` in
    /// `variant` comes from (ADR-1202 D11). A face whose source makes its
    /// own script glyphs gives the one it holds; one whose source's glyph
    /// is still on its way has the built-in fonts draw the cell meanwhile;
    /// every other face's script glyph is made from its normal glyph, as
    /// [`Font::drawn_variant`] makes it.
    fn script(&self, _cluster: &[char], _variant: Variant) -> ScriptFrom {
        ScriptFrom::Normal
    }
}

/// Where a face's superscript or subscript glyph comes from
/// ([`Face::script`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptFrom {
    /// The glyph the face's source made, small and placed in the whole
    /// cell, as the glyph helper sends it.
    Made(Glyph),
    /// The glyph made from the face's normal glyph.
    Normal,
    /// The source's glyph is on its way, and the built-in fonts
    /// ([`Face::builtins`]) draw the cell until it arrives, as they draw
    /// any glyph on its way (ADR-1200 D7, D22).
    Builtins,
}

impl<F: Face + ?Sized> Face for Box<F> {
    fn cell(&self) -> Cell {
        (**self).cell()
    }
    fn size_step(&self) -> u32 {
        (**self).size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        (**self).has_glyph(c)
    }
    fn coverage(&self, range: Range) -> RangeCoverage {
        (**self).coverage(range)
    }
    fn sprite_policy(&self) -> SpritePolicy {
        (**self).sprite_policy()
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        (**self).draw(cluster, style)
    }
    fn last_resort(&self, style: Style) -> Option<Glyph> {
        (**self).last_resort(style)
    }
    fn decorations(&self) -> Decorations {
        (**self).decorations()
    }
    fn scale(&self) -> u32 {
        (**self).scale()
    }
    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        (**self).draw_unscaled(cluster, style)
    }
    fn last_resort_unscaled(&self, style: Style) -> Option<Glyph> {
        (**self).last_resort_unscaled(style)
    }
    fn band(&self) -> Band {
        (**self).band()
    }
    fn builtins(&self) -> Option<Box<dyn Face + '_>> {
        (**self).builtins()
    }
    fn script(&self, cluster: &[char], variant: Variant) -> ScriptFrom {
        (**self).script(cluster, variant)
    }
}

/// What the renderer draws text with: a face, and the sprites its
/// [`SpritePolicy`] draws over it for the characters that fill their cell.
#[derive(Clone, Debug)]
pub struct Font<F> {
    face: F,
    policy: SpritePolicy,
    missing: Missing,
}

/// Drawn for a character the face lacks under [`Missing::Replacement`].
pub const REPLACEMENT: char = '\u{25a1}';

/// A glyph as [`Font::drawn`] gives it, with what a cache needs to know of
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub glyph: Glyph,
    /// The size of each of the glyph's pixels, as a square this many
    /// pixels across: the face's scale from [`Font::glyph_unscaled`], else
    /// 1. Drawn at that size, the glyph is [`Font::glyph`]'s.
    pub scale: u32,
    /// Whether the glyph is the font's missing glyph, because the face drew
    /// nothing for the cluster. That glyph comes from the fallback chain,
    /// U+25A1, `?` and the last resort, so it can change whenever the face
    /// gains glyphs, though the cluster's own characters did not arrive.
    pub missing: bool,
}

impl Drawn {
    fn new(glyph: Glyph, scale: u32, missing: bool) -> Drawn {
        Drawn {
            glyph,
            scale,
            missing,
        }
    }
}

impl<F: Face> Font<F> {
    /// `face`, drawn with its own [`Face::sprite_policy`] and
    /// [`Missing::Replacement`] for a character it lacks.
    pub fn new(face: F) -> Font<F> {
        Font {
            policy: face.sprite_policy(),
            missing: Missing::default(),
            face,
        }
    }

    /// This font with each range drawn from the source `policy` gives it.
    pub fn with_policy(self, policy: SpritePolicy) -> Font<F> {
        Font { policy, ..self }
    }

    /// This font with a character the face lacks drawn as `missing`.
    pub fn with_missing(self, missing: Missing) -> Font<F> {
        Font { missing, ..self }
    }

    pub fn policy(&self) -> SpritePolicy {
        self.policy
    }

    pub fn missing(&self) -> Missing {
        self.missing
    }

    /// How much of each sprite range the face holds.
    pub fn coverage(&self) -> Coverage {
        Coverage::new(|range| self.face.coverage(range))
    }

    pub fn face(&self) -> &F {
        &self.face
    }

    pub fn face_mut(&mut self) -> &mut F {
        &mut self.face
    }

    pub fn cell(&self) -> Cell {
        self.face.cell()
    }

    pub fn decorations(&self) -> Decorations {
        self.face.decorations()
    }

    /// Whether `c` draws as itself: a sprite always does.
    pub fn has_glyph(&self, c: char) -> bool {
        self.draws_sprite(c) || self.face.has_glyph(c)
    }

    /// Whether `c` draws as a sprite under the policy.
    fn draws_sprite(&self, c: char) -> bool {
        Range::of(c).is_some_and(|range| match self.policy.source(range) {
            Source::Sprites => true,
            Source::Font => false,
            Source::Auto => !self.face.has_glyph(c),
        })
    }

    /// The character `cluster` draws as a sprite: a single character the
    /// policy draws as one, or the first of a longer cluster when the
    /// policy draws it as one and the face lacks it. The marks are
    /// dropped, since a bitmap face draws no marks, and the character
    /// still draws as its sprite instead of as a missing glyph.
    fn sprite_base(&self, cluster: &[char]) -> Option<char> {
        match *cluster {
            [c] => self.draws_sprite(c).then_some(c),
            [c, ..] => (self.draws_sprite(c) && !self.face.has_glyph(c)).then_some(c),
            [] => None,
        }
    }

    /// The glyph for `cluster`, exactly `width` cells wide and a cell high:
    /// [`Font::drawn`]'s glyph.
    pub fn glyph(&self, cluster: &[char], width: Width, style: Style) -> Option<Glyph> {
        self.drawn(cluster, width, style).map(|d| d.glyph)
    }

    /// The glyph for `cluster`, exactly `width` cells wide and a cell high,
    /// at scale 1, and whether it is the font's missing glyph.
    /// A character the policy draws as a sprite fills the cell, alone or
    /// as the base of a cluster the face lacks, with the light lines
    /// [`sprite::light_line`] gives the cell's height. A cluster the face
    /// draws nothing for, because it lacks the first character or holds no
    /// glyph for the cluster, otherwise draws as the font's missing glyph:
    /// the [`Missing`] glyph, whose last fallback is U+25A1, else `?`, else
    /// the face's last resort. A glyph of another width is centred and cut
    /// to the cells. `None` only for an empty cluster or a face that can
    /// draw nothing.
    pub fn drawn(&self, cluster: &[char], width: Width, style: Style) -> Option<Drawn> {
        let &first = cluster.first()?;
        let cell = self.face.cell();
        let w = cell.width.saturating_mul(width.cells());
        if let Some(c) = self.sprite_base(cluster) {
            let thick = sprite::light_line(cell.height);
            if let Some(s) = sprite::draw(c, w, cell.height, thick) {
                return Some(Drawn::new(s.with_cells(width), 1, false));
            }
        }
        let (glyph, missing) = match self.own(cluster, style, Face::draw) {
            Some(g) => (g, false),
            None => {
                let g = self
                    .missing_glyph(first, width)
                    .or_else(|| self.face.draw(&[REPLACEMENT], style))
                    .or_else(|| self.face.draw(&['?'], style))
                    .or_else(|| self.face.last_resort(style))?;
                (g, true)
            }
        };
        Some(Drawn::new(glyph.fit(w, width)?, 1, missing))
    }

    /// [`Font::drawn`] in `variant`: the normal glyph in its style, which
    /// a superscript or subscript then scales and places in its cells
    /// ([`Glyph::scripted`]).
    ///
    /// A face whose source makes its own script glyphs ([`Face::script`])
    /// gives the glyph it holds, fitted to the cells, or has the built-in
    /// fonts draw the cell while that glyph is on its way. A sprite is
    /// always the font's own.
    pub fn drawn_variant(&self, cluster: &[char], width: Width, variant: Variant) -> Option<Drawn> {
        if variant.script != Script::Baseline && self.sprite_base(cluster).is_none() {
            match self.face.script(cluster, variant) {
                ScriptFrom::Made(glyph) => {
                    let w = self.face.cell().width.saturating_mul(width.cells());
                    return Some(Drawn::new(glyph.fit(w, width)?, 1, false));
                }
                ScriptFrom::Builtins => {
                    let builtins = Font {
                        face: self.face.builtins()?,
                        policy: self.policy,
                        missing: self.missing,
                    };
                    return builtins.drawn_variant(cluster, width, variant);
                }
                ScriptFrom::Normal => {}
            }
        }
        let drawn = self.drawn(cluster, width, variant.style)?;
        if variant.script == Script::Baseline {
            return Some(drawn);
        }
        let glyph = drawn.glyph.scripted(variant.script)?;
        Some(Drawn::new(glyph, 1, drawn.missing))
    }

    /// The face's own glyph for `cluster`, drawn by `draw`, when the face
    /// has its first character.
    fn own(
        &self,
        cluster: &[char],
        style: Style,
        draw: impl Fn(&F, &[char], Style) -> Option<Glyph>,
    ) -> Option<Glyph> {
        let &first = cluster.first()?;
        self.face
            .has_glyph(first)
            .then(|| draw(&self.face, cluster, style))
            .flatten()
    }

    /// The glyph [`Missing`] draws for `c` across `width`, or `None` for
    /// U+25A1's chain.
    fn missing_glyph(&self, c: char, width: Width) -> Option<Glyph> {
        match self.missing {
            Missing::Replacement => None,
            Missing::Hex => hex::draw(c, self.face.cell(), width),
        }
    }

    /// [`Font::glyph_unscaled`] in `variant`. A superscript or subscript
    /// glyph comes at its drawn size, with scale 1, since two thirds of a
    /// bitmap glyph is no whole number of its pixels.
    pub fn glyph_unscaled_variant(
        &self,
        cluster: &[char],
        width: Width,
        variant: Variant,
    ) -> Option<Drawn> {
        match variant.script {
            Script::Baseline => self.glyph_unscaled(cluster, width, variant.style),
            Script::Super | Script::Sub => self.drawn_variant(cluster, width, variant),
        }
    }

    /// [`Font::drawn`] for a renderer that stores bitmap glyphs small: the
    /// glyph at the face's scale 1 with that scale, which drawn with each
    /// pixel a square `scale` across is exactly [`Font::glyph`]'s. A
    /// sprite, a face that draws at its own size, and a glyph whose box
    /// to its cells does not scale (centred in an odd number of spare
    /// pixels) come at their drawn size, with scale 1.
    pub fn glyph_unscaled(&self, cluster: &[char], width: Width, style: Style) -> Option<Drawn> {
        let drawn = || self.drawn(cluster, width, style);
        let scale = self.face.scale();
        if scale <= 1 || self.sprite_base(cluster).is_some() {
            return drawn();
        }
        let w = self
            .face
            .cell()
            .width
            .checked_div(scale)?
            .saturating_mul(width.cells());
        let own = self.own(cluster, style, Face::draw_unscaled);
        if own.is_none() && self.missing == Missing::Hex {
            return drawn();
        }
        let missing = own.is_none();
        let glyph = own
            .or_else(|| self.face.draw_unscaled(&[REPLACEMENT], style))
            .or_else(|| self.face.draw_unscaled(&['?'], style))
            .or_else(|| self.face.last_resort_unscaled(style));
        match glyph {
            Some(g) if w.abs_diff(g.width()) % 2 == 0 => {
                Some(Drawn::new(g.fit(w, width)?, scale, missing))
            }
            _ => drawn(),
        }
    }
}
