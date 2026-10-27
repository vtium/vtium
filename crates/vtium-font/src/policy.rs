//! [`SpritePolicy`], which chooses between a face's glyphs and the sprites
//! for each [`Range`], and [`Coverage`], how much of each range a face
//! holds (vtium/vtium#85).

use crate::sprite::Range;

/// Where a range's characters are drawn from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Source {
    /// The face's glyph; a character the face lacks draws as any missing
    /// character does, U+25A1.
    Font,
    /// The sprite, whatever the face holds: the C reference's behaviour,
    /// and the sprites line up exactly at every cell size.
    Sprites,
    /// The face's glyph where it has one, and the sprite where it doesn't.
    Auto,
}

/// One value for each [`Range`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct PerRange<T>([T; 6]);

impl<T> PerRange<T> {
    fn get(&self, range: Range) -> &T {
        let [boxes, blocks, braille, sextants, octants, legacy] = &self.0;
        match range {
            Range::BoxDrawing => boxes,
            Range::BlockElements => blocks,
            Range::Braille => braille,
            Range::Sextants => sextants,
            Range::Octants => octants,
            Range::LegacyComputing => legacy,
        }
    }

    fn get_mut(&mut self, range: Range) -> &mut T {
        let [boxes, blocks, braille, sextants, octants, legacy] = &mut self.0;
        match range {
            Range::BoxDrawing => boxes,
            Range::BlockElements => blocks,
            Range::Braille => braille,
            Range::Sextants => sextants,
            Range::Octants => octants,
            Range::LegacyComputing => legacy,
        }
    }
}

/// What a character the face lacks draws as.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum Missing {
    /// U+25A1, else `?`, else the face's last resort, as the C reference
    /// drew.
    #[default]
    Replacement,
}

/// A [`Source`] for each [`Range`], set per font.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SpritePolicy(PerRange<Source>);

impl SpritePolicy {
    /// Sprites for every range, as the C reference drew.
    pub const SPRITES: SpritePolicy = SpritePolicy::all(Source::Sprites);

    /// The default for a font the operator names (vtium/vtium#85): the
    /// font's glyphs for a range it covers whole, and the sprites for a
    /// range it misses any character of, so no range mixes the two.
    pub fn default_for(coverage: &Coverage) -> SpritePolicy {
        SpritePolicy(PerRange(Range::ALL.map(|range| {
            if coverage.get(range).is_complete() {
                Source::Font
            } else {
                Source::Sprites
            }
        })))
    }

    /// `source` for every range.
    pub const fn all(source: Source) -> SpritePolicy {
        SpritePolicy(PerRange([source; 6]))
    }

    /// This policy with `range` drawn from `source`.
    pub fn with(mut self, range: Range, source: Source) -> SpritePolicy {
        *self.0.get_mut(range) = source;
        self
    }

    /// Where `range` is drawn from.
    pub fn source(&self, range: Range) -> Source {
        *self.0.get(range)
    }
}

/// How many of a range's characters a face holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RangeCoverage {
    pub covered: u32,
    pub total: u32,
}

impl RangeCoverage {
    /// Counts the characters of `range` that `has` holds.
    pub fn count(range: Range, mut has: impl FnMut(char) -> bool) -> RangeCoverage {
        range.chars().fold(
            RangeCoverage {
                covered: 0,
                total: 0,
            },
            |n, c| RangeCoverage {
                covered: n.covered.saturating_add(u32::from(has(c))),
                total: n.total.saturating_add(1),
            },
        )
    }

    /// Whether the face holds every character of the range.
    pub fn is_complete(&self) -> bool {
        self.covered == self.total
    }

    /// Whether the face holds none of the range.
    pub fn is_empty(&self) -> bool {
        self.covered == 0
    }

    /// Whether the face holds some of the range but not all.
    pub fn is_partial(&self) -> bool {
        !self.is_complete() && !self.is_empty()
    }
}

/// How much of each [`Range`] a face holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Coverage(PerRange<RangeCoverage>);

impl Coverage {
    /// Each range's coverage, from `count`.
    pub fn new(mut count: impl FnMut(Range) -> RangeCoverage) -> Coverage {
        Coverage(PerRange(Range::ALL.map(&mut count)))
    }

    pub fn get(&self, range: Range) -> RangeCoverage {
        *self.0.get(range)
    }

    /// The ranges the face covers only partly, in [`Range::ALL`]'s order:
    /// each one a warning for the operator (vtium/vtium#85).
    pub fn partial(&self) -> impl Iterator<Item = (Range, RangeCoverage)> + '_ {
        Range::ALL
            .into_iter()
            .map(|r| (r, self.get(r)))
            .filter(|(_, n)| n.is_partial())
    }
}
