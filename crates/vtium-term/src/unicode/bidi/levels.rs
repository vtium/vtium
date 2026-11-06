//! The one module that calls the unicode-bidi crate (ADR-1004 D13).
//!
//! It builds a paragraph with `ParagraphBidiInfo::new_with_data_source`,
//! passing the crate vtium-term's own Bidi_Class and bracket tables, so the
//! crate's tables are never linked. It reads the result only through
//! `reordered_levels_per_char` and `reorder_visual`, and it never hands the
//! crate an empty text or an empty line. In 0.3.18 the crate's `visual_runs`,
//! which `reorder_line` calls too, panics on an input its maintainers were
//! sent and on any empty line, and vtium's builds abort on a panic. Nothing
//! outside this module names the crate.

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
/// to I2 run over the whole paragraph and rule L1 over each row, so a row's
/// trailing spaces take the paragraph's level.
pub(crate) fn resolve(
    text: &[char],
    direction: Direction,
    detect: bool,
    rows: &[usize],
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
    #[cfg(test)]
    RESOLVED.with(|n| n.set(n.get().saturating_add(text.len())));
    let mut levels = Vec::with_capacity(text.len());
    if text.is_empty() {
        return Resolved {
            level: level.number(),
            levels,
        };
    }

    let string: String = text.iter().collect();
    // the byte offset where each character starts, and the text's end
    let mut offsets = Vec::with_capacity(text.len().saturating_add(1));
    let mut at: usize = 0;
    for c in text {
        offsets.push(at);
        at = at.saturating_add(c.len_utf8());
    }
    offsets.push(at);

    let info = ParagraphBidiInfo::new_with_data_source(&Tables, &string, Some(level));
    let mut start = 0;
    for len in rows.iter().copied().chain(std::iter::once(usize::MAX)) {
        if start >= text.len() {
            break;
        }
        let end = start.saturating_add(len).min(text.len());
        // an empty row never reaches the crate
        if end > start
            && let (Some(&from), Some(&to)) = (offsets.get(start), offsets.get(end))
        {
            let per_char = info.reordered_levels_per_char(from..to);
            let row = per_char.get(start..end).unwrap_or_default();
            levels.extend(row.iter().map(Level::number));
        }
        start = end;
    }
    Resolved {
        level: info.paragraph_level.number(),
        levels,
    }
}

#[cfg(test)]
std::thread_local! {
    /// The characters [`resolve`] has read on this thread, which a test
    /// counts to bound the work a frame does.
    static RESOLVED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// The characters [`resolve`] has read on this thread so far.
#[cfg(test)]
pub(crate) fn resolved_chars() -> usize {
    RESOLVED.with(core::cell::Cell::get)
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
