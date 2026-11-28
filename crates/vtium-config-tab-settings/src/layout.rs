//! Where the tab's parts go in the viewer: the sections' pane at the left,
//! a rule, the keys' pane at the right, and the foot along the viewer's
//! last row, which holds the highlighted key's one-line help. The panes
//! are worked out from the viewer's size alone, so a click is read
//! against the same places the view draws.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Settings bullet: the two
//!   panes, and the key's description at the foot of the right pane.
//! - ADR-1206 D10: designed for 100×30 and still usable at 80×25.

use ratatui_core::layout::Rect;

/// The left pane's width at 100 columns and more, as the approved mock-up
/// draws it.
const LEFT_WIDE: u16 = 33;

/// The narrowest the left pane gets, which still shows a section's name
/// and its count.
const LEFT_MIN: u16 = 20;

/// The tab's regions in the viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Panes {
    /// The sections, with their title on the first row.
    pub left: Rect,
    /// The column of the rule between the panes, as tall as the panes.
    pub rule: Rect,
    /// The keys, with the section's title on the first row.
    pub right: Rect,
    /// The viewer's last row: the highlighted key's or section's line.
    pub foot: Rect,
}

impl Panes {
    /// The regions of the viewer `area`. The left pane takes a third of a
    /// screen narrower than 100 columns, and 33 columns at 100 and more.
    pub fn of(area: Rect) -> Panes {
        let foot_rows = u16::from(area.height >= 2);
        let body = area.height.saturating_sub(foot_rows);
        let wide = if area.width >= 100 {
            LEFT_WIDE
        } else {
            (area.width / 3).max(LEFT_MIN)
        };
        let left = wide.min(area.width / 2);
        let rule = Rect::new(area.x.saturating_add(left), area.y, 1, body);
        // a blank column after the rule, and one at the screen's edge
        let right_x = rule.x.saturating_add(2);
        let right_w = area.right().saturating_sub(right_x).saturating_sub(1);
        Panes {
            left: Rect::new(area.x, area.y, left, body),
            rule,
            right: Rect::new(right_x, area.y, right_w, body),
            foot: Rect::new(area.x, area.y.saturating_add(body), area.width, foot_rows),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_100_columns_the_rule_stands_at_column_33() {
        let p = Panes::of(Rect::new(0, 2, 100, 26));
        assert_eq!(p.left, Rect::new(0, 2, 33, 25));
        assert_eq!(p.rule, Rect::new(33, 2, 1, 25));
        assert_eq!(p.right, Rect::new(35, 2, 64, 25));
        assert_eq!(p.foot, Rect::new(0, 27, 100, 1));
    }

    #[test]
    fn at_80_columns_the_left_pane_takes_a_third() {
        let p = Panes::of(Rect::new(0, 2, 80, 21));
        assert_eq!(p.left.width, 26);
        assert_eq!(p.right, Rect::new(28, 2, 51, 20));
        assert_eq!(p.foot.y, 22);
    }
}
