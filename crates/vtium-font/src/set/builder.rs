//! [`SetBuilder`]: a set described with vtium-font's types, written to
//! bytes through the writer's core.

use std::fmt;

use super::format::{MAJOR, MINOR, STYLE_BOLD, STYLE_ITALIC, valid_name};
use super::read::PixelFormat;
use super::write::{RawSet, RawStrike, encode};
use crate::face::{Cell, Decorations, Style, Width};
use crate::policy::{Source, SpritePolicy};
use crate::sprite::Range;

/// Why a set could not be written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WriteError(&'static str);

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot write the set: {}", self.0)
    }
}

impl std::error::Error for WriteError {}

/// One strike: a style at one cell size in one pixel format, its glyphs
/// and the codepoints that draw them.
#[derive(Clone, Debug)]
pub struct StrikeBuilder {
    style: Style,
    format: PixelFormat,
    cell: Cell,
    baseline: u32,
    decorations: Decorations,
    compress: bool,
    glyphs: Vec<(bool, Vec<u8>)>,
    map: Vec<(u32, u32)>,
}

impl StrikeBuilder {
    /// An empty strike of `style` at `cell` in `format`, its baseline at
    /// the cell's bottom and its decorations those of a face with no
    /// metrics of its own ([`Decorations::for_height`]). Its chunks are
    /// deflated for R8 and RGBA and raw for 1-bit, as a frit's default
    /// `compress=yes` stores them (ADR-1200 D1, D13).
    pub fn new(format: PixelFormat, cell: Cell, style: Style) -> StrikeBuilder {
        StrikeBuilder {
            style,
            format,
            cell,
            baseline: cell.height,
            decorations: Decorations::for_height(cell.height),
            compress: format != PixelFormat::Bits,
            glyphs: Vec::new(),
            map: Vec::new(),
        }
    }

    pub fn baseline(mut self, row: u32) -> StrikeBuilder {
        self.baseline = row;
        self
    }

    pub fn decorations(mut self, decorations: Decorations) -> StrikeBuilder {
        self.decorations = decorations;
        self
    }

    /// Whether each chunk is stored deflated where that makes it smaller.
    pub fn compress(mut self, compress: bool) -> StrikeBuilder {
        self.compress = compress;
        self
    }

    /// Adds a glyph `width` wide, its bytes in the strike's format: 1-bit
    /// rows padded to a byte with the first pixel the high bit, a byte a
    /// pixel for R8, four premultiplied for RGBA. Returns its number for
    /// [`StrikeBuilder::map`]; a glyph no codepoint maps is not written.
    pub fn glyph(&mut self, width: Width, bytes: Vec<u8>) -> u32 {
        self.glyphs.push((width == Width::Wide, bytes));
        u32::try_from(self.glyphs.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }

    /// Maps `c` to glyph `glyph`. Several codepoints may share a glyph.
    pub fn map(&mut self, c: char, glyph: u32) {
        self.map.push((u32::from(c), glyph));
    }

    /// Whether no codepoint is mapped yet: a strike the writer would
    /// write no glyph for.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn raw(&self) -> Result<RawStrike, WriteError> {
        let narrow = |v: u32| u16::try_from(v).map_err(|_| WriteError("a metric past u16"));
        let mut style = 0;
        if self.style.bold {
            style |= STYLE_BOLD;
        }
        if self.style.italic {
            style |= STYLE_ITALIC;
        }
        let d = self.decorations;
        if d.underline_thickness == 0
            || d.strike_thickness == 0
            || d.underline.saturating_add(d.underline_thickness) > self.cell.height
            || d.strike.saturating_add(d.strike_thickness) > self.cell.height
            || self.baseline > self.cell.height
        {
            return Err(WriteError("a baseline or decoration outside the cell"));
        }
        Ok(RawStrike {
            style,
            format: self.format.code(),
            cell_width: narrow(self.cell.width)?,
            cell_height: narrow(self.cell.height)?,
            baseline: narrow(self.baseline)?,
            underline: narrow(d.underline)?,
            underline_thickness: narrow(d.underline_thickness)?,
            strike: narrow(d.strike)?,
            strike_thickness: narrow(d.strike_thickness)?,
            compress: self.compress,
            glyphs: self.glyphs.clone(),
            map: self.map.clone(),
        })
    }
}

/// A set to write: its name, provenance, sprite policy and strikes.
#[derive(Clone, Debug)]
pub struct SetBuilder {
    name: String,
    meta: Vec<(String, String)>,
    policy: SpritePolicy,
    strikes: Vec<StrikeBuilder>,
}

impl SetBuilder {
    /// A set named `name`, drawn with [`SpritePolicy::SPRITES`] until
    /// [`SetBuilder::policy`] sets another.
    pub fn new(name: &str) -> SetBuilder {
        SetBuilder {
            name: name.to_owned(),
            meta: Vec::new(),
            policy: SpritePolicy::SPRITES,
            strikes: Vec::new(),
        }
    }

    /// Adds a provenance line after `name` and `format`, which the builder
    /// writes first (ADR-1200 D2, `META`). A key may repeat; a multi-line
    /// value, such as the frit, is one line of the key for each of its
    /// lines.
    pub fn meta(mut self, key: &str, value: &str) -> SetBuilder {
        self.meta.push((key.to_owned(), value.to_owned()));
        self
    }

    pub fn policy(mut self, policy: SpritePolicy) -> SetBuilder {
        self.policy = policy;
        self
    }

    pub fn strike(mut self, strike: StrikeBuilder) -> SetBuilder {
        self.strikes.push(strike);
        self
    }

    /// The set's bytes.
    pub fn write(&self) -> Result<Vec<u8>, WriteError> {
        if !valid_name(&self.name) {
            return Err(WriteError("the name is not a set name (ADR-1200 D1)"));
        }
        if self.meta.iter().any(|(k, _)| k == "name" || k == "format") {
            return Err(WriteError("name and format are the builder's to write"));
        }
        let mut meta = vec![
            ("name".to_owned(), self.name.clone()),
            ("format".to_owned(), format!("{MAJOR}.{MINOR}")),
        ];
        meta.extend(self.meta.iter().cloned());
        let policy = Range::ALL.map(|r| match self.policy.source(r) {
            Source::Font => super::format::SOURCE_FONT,
            Source::Sprites => super::format::SOURCE_SPRITES,
            Source::Auto => super::format::SOURCE_AUTO,
        });
        let strikes = self
            .strikes
            .iter()
            .map(StrikeBuilder::raw)
            .collect::<Result<_, _>>()?;
        encode(&RawSet {
            meta,
            policy,
            strikes,
        })
        .map_err(WriteError)
    }
}
