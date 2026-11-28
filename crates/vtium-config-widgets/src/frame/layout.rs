//! The frame's regions, computed once from the screen's size with
//! ratatui's `Layout`: the top row, a rule under it, the viewer, the rows
//! of an open search, a rule over the bottom row, and the action bar.
//!
//! References:
//! - ADR-1206 D4, "One frame for every screen": a row of length 1, the
//!   viewer filling the rest and a row of length 1, with the search row
//!   and Find's matches taking rows between the viewer and the bar.
//! - ADR-1403 D11, "The frame": the rules under the top row and over the
//!   bottom row, and the message on the viewer's last row.

use ratatui_core::layout::{Constraint, Layout, Rect};

/// The most rows an open search takes: its own row, and up to eight of
/// Find's matches above it.
pub const SEARCH_ROWS_MAX: u16 = 9;

/// What the frame holds besides the viewer at this moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The rows the search takes over the action bar: 0 while it is
    /// closed, 1 for the search row, and more for Find's matches above
    /// it, at most [`SEARCH_ROWS_MAX`].
    pub search_rows: u16,
    /// Whether a message shows. It takes the viewer's last row, so the
    /// open tab draws in the rows above it.
    pub message: bool,
}

/// The rectangles of the frame's regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Regions {
    /// The top row: the menus, the tabs and the document's facts.
    pub top: Rect,
    /// The rule under the top row.
    pub top_rule: Rect,
    /// The rows the open tab draws in.
    pub viewer: Rect,
    /// The viewer's last row while a message shows there.
    pub message: Option<Rect>,
    /// The rows the search component draws in while the search is open:
    /// the search row last, and Find's matches above it.
    pub search: Option<Rect>,
    /// The rule over the bottom row.
    pub bottom_rule: Rect,
    /// The bottom row: the action bar.
    pub bar: Rect,
}

impl Regions {
    /// The regions of a screen of `area`. A screen too short for a region
    /// gives that region no rows: the viewer goes first, so the top row
    /// and the action bar stay on any screen of four rows or more.
    pub fn of(area: Rect, options: Options) -> Regions {
        let search = options.search_rows.min(SEARCH_ROWS_MAX);
        let [top, top_rule, middle, search_rows, bottom_rule, bar] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(search),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let (viewer, message) = if options.message && middle.height > 0 {
            let [viewer, message] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(middle);
            (viewer, Some(message))
        } else {
            (middle, None)
        };
        Regions {
            top,
            top_rule,
            viewer,
            message,
            search: (search > 0 && search_rows.height > 0).then_some(search_rows),
            bottom_rule,
            bar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_spends_four_rows_and_the_viewer_takes_the_rest() {
        let r = Regions::of(Rect::new(0, 0, 100, 30), Options::default());
        assert_eq!(r.top, Rect::new(0, 0, 100, 1));
        assert_eq!(r.top_rule, Rect::new(0, 1, 100, 1));
        assert_eq!(r.viewer, Rect::new(0, 2, 100, 26));
        assert_eq!(r.bottom_rule, Rect::new(0, 28, 100, 1));
        assert_eq!(r.bar, Rect::new(0, 29, 100, 1));
        assert_eq!((r.message, r.search), (None, None));
    }

    #[test]
    fn the_search_row_stacks_on_the_bar_and_the_viewer_gives_it_up() {
        let r = Regions::of(
            Rect::new(0, 0, 80, 25),
            Options {
                search_rows: 1,
                message: true,
            },
        );
        assert_eq!(r.search, Some(Rect::new(0, 22, 80, 1)));
        assert_eq!(r.message, Some(Rect::new(0, 21, 80, 1)));
        assert_eq!(r.viewer, Rect::new(0, 2, 80, 19));
        assert_eq!(r.bottom_rule.y, 23);
        assert_eq!(r.bar.y, 24);
    }

    #[test]
    fn find_takes_at_most_eight_rows_of_matches_above_its_row() {
        let r = Regions::of(
            Rect::new(0, 0, 100, 30),
            Options {
                search_rows: 40,
                message: false,
            },
        );
        assert_eq!(r.search, Some(Rect::new(0, 19, 100, 9)));
        assert_eq!(r.viewer.height, 17);
    }

    #[test]
    fn a_tiny_screen_keeps_its_top_row_and_its_bar() {
        let r = Regions::of(
            Rect::new(0, 0, 10, 4),
            Options {
                search_rows: 1,
                message: true,
            },
        );
        assert_eq!(r.top.height, 1);
        assert_eq!(r.bar.height, 1);
        assert_eq!(r.viewer.height, 0);
        assert_eq!(r.message, None);
    }
}
