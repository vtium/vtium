//! Where the tab's parts go in the viewer: the list at the left with the
//! picture of the console under it, a rule, and the highlighted font's
//! facts at the right. The parts are worked out from the viewer's size
//! alone, so a click is read against the same places the view draws.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Fonts bullet: the facts
//!   beside the list, the rule at column 62 of a 100-column screen, and
//!   the picture under the list.
//! - ADR-1206 D10: designed for 100×30 and still usable at 80×25.

use ratatui_core::layout::Rect;

/// The rows the picture takes at most, as the approved figure draws it.
const PICTURE_MAX: u16 = 13;

/// The viewer's height under which the picture gives its rows to the list.
const PICTURE_FROM: u16 = 16;

/// The narrowest the facts get, which still holds a label and a word.
const FACTS_MIN: u16 = 26;

/// The widest the facts get on a wide screen.
const FACTS_MAX: u16 = 40;

/// The tab's regions in the viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Panes {
    /// The filter line, the first item and the fonts.
    pub list: Rect,
    /// The picture of the console, under the list; empty on a viewer too
    /// short for it.
    pub picture: Rect,
    /// The column of the rule between the list and the facts.
    pub rule: Rect,
    /// The highlighted row's facts.
    pub facts: Rect,
}

impl Panes {
    /// The regions of the viewer `area`. The facts take 36 hundredths of
    /// the width, so the rule stands at column 62 of a 100-column screen
    /// and at column 50 of an 80-column one.
    pub fn of(area: Rect) -> Panes {
        let facts_w = (area.width.saturating_mul(36) / 100).clamp(FACTS_MIN, FACTS_MAX);
        let left = area.width.saturating_sub(facts_w).saturating_sub(2);
        let picture_h = if area.height >= PICTURE_FROM {
            (area.height.saturating_mul(2) / 5).min(PICTURE_MAX)
        } else {
            0
        };
        let list_h = area.height.saturating_sub(picture_h);
        let rule = Rect::new(area.x.saturating_add(left), area.y, 1, area.height);
        let facts_x = rule.x.saturating_add(2);
        Panes {
            list: Rect::new(area.x, area.y, left, list_h),
            picture: Rect::new(
                area.x,
                area.y.saturating_add(list_h),
                left.saturating_sub(1),
                picture_h,
            ),
            rule,
            facts: Rect::new(
                facts_x,
                area.y,
                area.right().saturating_sub(facts_x).saturating_sub(1),
                area.height,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_100_columns_the_rule_stands_at_column_62() {
        let p = Panes::of(Rect::new(0, 2, 100, 26));
        assert_eq!(p.rule, Rect::new(62, 2, 1, 26));
        assert_eq!(p.facts, Rect::new(64, 2, 35, 26));
        assert_eq!(p.list, Rect::new(0, 2, 62, 16));
        assert_eq!(p.picture, Rect::new(0, 18, 61, 10));
    }

    #[test]
    fn at_80_columns_the_list_keeps_50_columns() {
        let p = Panes::of(Rect::new(0, 2, 80, 21));
        assert_eq!(p.rule.x, 50);
        assert_eq!(p.facts.width, 27);
        assert_eq!(p.list.height, 13);
        assert_eq!(p.picture.height, 8);
    }

    #[test]
    fn a_short_viewer_gives_the_picture_s_rows_to_the_list() {
        let p = Panes::of(Rect::new(0, 0, 80, 12));
        assert_eq!(p.picture.height, 0);
        assert_eq!(p.list.height, 12);
    }
}
