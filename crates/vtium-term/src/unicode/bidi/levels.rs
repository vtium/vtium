//! The one module that calls the unicode-bidi crate (ADR-1004 D13).
//!
//! It builds a paragraph with `ParagraphBidiInfo::new_with_data_source`,
//! passing the crate vtium-term's own Bidi_Class and bracket tables, so the
//! crate's tables are never linked. The crate runs rules X1 to I2 over
//! the whole paragraph once. vtium-term then reads the paragraph's level
//! and each character's level from the result's public `paragraph_level`
//! and `levels` fields, and applies rule L1 to each row itself, so drawing
//! a paragraph of many rows reads each character a fixed number of times:
//! the crate's `reordered_levels_per_char` copies and walks the whole
//! paragraph's levels for every row it is asked about. It orders a row
//! with the crate's `reorder_visual`, and it never hands the crate an empty
//! text. In 0.3.18 the crate's `visual_runs`,
//! which `reorder_line` calls too, panics on an input its maintainers were
//! sent and on any empty line, and vtium's builds abort on a panic. Nothing
//! outside this module names the crate.

use core::ops::Range;

use unicode_bidi::data_source::BidiMatchedOpeningBracket;
use unicode_bidi::{BidiClass, BidiDataSource, Level, ParagraphBidiInfo};

use super::{Class, Direction, bracket, class, first_strong};

/// vtium-term's tables, as the crate reads them.
struct Tables;

impl BidiDataSource for Tables {
    fn bidi_class(&self, c: char) -> BidiClass {
        match class(c) {
            Class::L => BidiClass::L,
            Class::R => BidiClass::R,
            Class::Al => BidiClass::AL,
            Class::En => BidiClass::EN,
            Class::Es => BidiClass::ES,
            Class::Et => BidiClass::ET,
            Class::An => BidiClass::AN,
            Class::Cs => BidiClass::CS,
            Class::Nsm => BidiClass::NSM,
            Class::Bn => BidiClass::BN,
            Class::B => BidiClass::B,
            Class::S => BidiClass::S,
            Class::Ws => BidiClass::WS,
            Class::On => BidiClass::ON,
            Class::Lre => BidiClass::LRE,
            Class::Lro => BidiClass::LRO,
            Class::Rle => BidiClass::RLE,
            Class::Rlo => BidiClass::RLO,
            Class::Pdf => BidiClass::PDF,
            Class::Lri => BidiClass::LRI,
            Class::Rli => BidiClass::RLI,
            Class::Fsi => BidiClass::FSI,
            Class::Pdi => BidiClass::PDI,
        }
    }

    fn bidi_matched_opening_bracket(&self, c: char) -> Option<BidiMatchedOpeningBracket> {
        bracket(c).map(|(opening, is_open)| BidiMatchedOpeningBracket { opening, is_open })
    }
}

/// A paragraph after the bidirectional algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// The paragraph's embedding level: 0 when it runs left to right, 1
    /// when it runs right to left.
    pub(crate) level: u8,
    /// Each character's embedding level, with rule L1 applied a row at a
    /// time; an odd level runs right to left.
    pub(crate) levels: Vec<u8>,
}

/// Runs the bidirectional algorithm over a paragraph, one character a cell.
///
/// The paragraph runs in `direction`, unless `detect` is set and a strong
/// character outside every isolate gives it a direction by rules P2 and P3.
/// `rows` gives the length of each of the paragraph's rows in characters,
/// in order; the characters past the last row form one more row. Rules X1
/// to I2 run over the whole paragraph once, and rule L1 over each row of
/// `wanted`, the indices of the rows the caller draws, so a row's trailing
/// spaces take the paragraph's level. A row outside `wanted` keeps the
/// levels rules X1 to I2 gave it.
pub(crate) fn resolve(
    text: &[char],
    direction: Direction,
    detect: bool,
    rows: &[usize],
    wanted: Range<usize>,
) -> Resolved {
    let direction = if detect {
        first_strong(text).unwrap_or(direction)
    } else {
        direction
    };
    let level = match direction {
        Direction::LeftToRight => Level::ltr(),
        Direction::RightToLeft => Level::rtl(),
    };
    let mut levels = Vec::with_capacity(text.len());
    if text.is_empty() {
        return Resolved {
            level: level.number(),
            levels,
        };
    }

    let string: String = text.iter().collect();
    let info = ParagraphBidiInfo::new_with_data_source(&Tables, &string, Some(level));
    let paragraph = info.paragraph_level.number();
    // each character's level, read at the byte it starts at
    let mut at: usize = 0;
    for c in text {
        levels.push(info.levels.get(at).map_or(paragraph, Level::number));
        at = at.saturating_add(c.len_utf8());
    }
    count(text.len());
    let mut start = 0;
    let lens = rows.iter().copied().chain(std::iter::once(usize::MAX));
    for (index, len) in lens.enumerate() {
        if start >= text.len() {
            break;
        }
        let end = start.saturating_add(len).min(text.len());
        if !wanted.contains(&index) {
            start = end;
            continue;
        }
        if let (Some(row), Some(row_levels)) = (text.get(start..end), levels.get_mut(start..end)) {
            line_levels(row, row_levels, paragraph);
            count(row.len());
        }
        start = end;
    }
    Resolved {
        level: paragraph,
        levels,
    }
}

/// Rule L1 over one row: `text` its characters and `levels` their levels
/// after rules X1 to I2, at the paragraph's level `paragraph`. A segment or
/// paragraph separator, the whitespace and isolate formatting characters
/// before one, and those at the row's end take the paragraph's level. A
/// character rule X9 removed (an embedding, an override, PDF or a boundary
/// neutral) takes the level of the character before it and counts as
/// whitespace here, as UAX #9's section on retaining explicit formatting
/// characters allows. This is the rule the crate's own line functions
/// apply, applied to the row's characters alone.
fn line_levels(text: &[char], levels: &mut [u8], paragraph: u8) {
    let mut reset_from: Option<usize> = Some(0);
    let mut previous = paragraph;
    for (i, &c) in text.iter().enumerate() {
        let mut reset_to = None;
        match class(c) {
            Class::B | Class::S => {
                reset_to = Some(i.saturating_add(1));
                reset_from.get_or_insert(i);
            }
            Class::Ws | Class::Fsi | Class::Lri | Class::Rli | Class::Pdi => {
                reset_from.get_or_insert(i);
            }
            Class::Rle | Class::Lre | Class::Rlo | Class::Lro | Class::Pdf | Class::Bn => {
                reset_from.get_or_insert(i);
                if let Some(level) = levels.get_mut(i) {
                    *level = previous;
                }
            }
            _ => reset_from = None,
        }
        if let (Some(from), Some(to)) = (reset_from, reset_to) {
            for level in levels.get_mut(from..to).into_iter().flatten() {
                *level = paragraph;
            }
            reset_from = None;
        }
        previous = levels.get(i).copied().unwrap_or(paragraph);
    }
    if let Some(from) = reset_from {
        for level in levels.get_mut(from..).into_iter().flatten() {
            *level = paragraph;
        }
    }
}

#[cfg(test)]
std::thread_local! {
    /// The characters this module has read on this thread: a paragraph's
    /// characters once for their levels, and each row's once more for
    /// rule L1. A test counts them to bound the work a frame does.
    static READ: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Counts `n` characters read, in a test build.
fn count(n: usize) {
    #[cfg(test)]
    READ.with(|r| r.set(r.get().saturating_add(n)));
    let _ = n;
}

/// The characters this module has read on this thread so far.
#[cfg(test)]
pub(crate) fn chars_read() -> usize {
    READ.with(core::cell::Cell::get)
}

/// The visual order of characters at `levels` by rule L2: for each place
/// from left to right, the index in `levels` of the character drawn there.
pub(crate) fn visual_order(levels: &[u8]) -> Vec<usize> {
    if levels.is_empty() {
        return Vec::new();
    }
    // every level `resolve` gives is one the crate made, so none is refused
    let levels: Vec<Level> = levels
        .iter()
        .map(|&l| Level::new(l).unwrap_or_else(|_| Level::ltr()))
        .collect();
    ParagraphBidiInfo::reorder_visual(&levels)
}

#[cfg(test)]
#[expect(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests;
