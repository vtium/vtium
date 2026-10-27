//! The in-band reports a program opts into, so it hears of a change
//! without polling: the text area's size (mode 2048, rockorager's in-band
//! resize notifications), the console's visibility (mode 2033 and
//! `CSI ? 998 n`, as foot(7) documents rockorager's spec) and its colour
//! scheme (mode 2031 and `CSI ? 996 n`, contour's colour palette update
//! notifications).
//!
//! A report is sent while its mode is set and only on a change; modes 2048
//! and 2033 also report as they are set. The size and the visibility come
//! from the host ([`Terminal::resize`], [`Terminal::set_visible`]); the
//! scheme follows the default background, set by the program or the host.
//! Each report follows the reply rule (`reply.rs`): numbers vtium holds and
//! fixed strings.

use super::reply::Query;
use super::{State, Terminal};
use crate::style::Rgb;

/// The text area as mode 2048 reports it: in cells, and in pixels, 0 while
/// the cell size is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextArea {
    rows: usize,
    cols: usize,
    height: u64,
    width: u64,
}

/// What the reports hold between events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Inband {
    /// The text area last reported or resized to, which a resize compares
    /// with.
    area: TextArea,
    /// The host's word: the console is potentially visible.
    visible: bool,
}

impl Inband {
    /// A visible console of `cols` by `rows`, its cell size unknown.
    pub(super) fn new(cols: usize, rows: usize) -> Self {
        Self {
            area: TextArea {
                rows,
                cols,
                height: 0,
                width: 0,
            },
            visible: true,
        }
    }
}

/// The colour scheme, as `CSI ? 997 ; Ps n` numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scheme {
    Dark = 1,
    Light = 2,
}

impl Scheme {
    /// A background's scheme: dark when its luma is below half, with
    /// Y = 0.299 R + 0.587 G + 0.114 B on the stored channels (ITU-R
    /// BT.601's weights), the rule neovim applies to OSC 11's answer
    /// (`runtime/lua/vim/_core/defaults.lua`), so a program that asks
    /// `CSI ? 996 n` and one that reads OSC 11 agree. Neither contour's
    /// spec nor foot(7) gives a rule.
    fn of(bg: Rgb) -> Self {
        let y = u32::from(bg.r)
            .saturating_mul(299)
            .saturating_add(u32::from(bg.g).saturating_mul(587))
            .saturating_add(u32::from(bg.b).saturating_mul(114));
        // Half of 255 * 1000.
        if y < 127_500 { Self::Dark } else { Self::Light }
    }
}

impl Terminal {
    /// Resizes the screen to `cols` by `rows`, as [`Screen::resize`]
    /// does, after the host has set the cell size for it
    /// ([`set_cell_size`](Self::set_cell_size)); while mode 2048 is set, a
    /// text area that changed, in cells or pixels, is reported. The host
    /// calls it once a resize is complete, the pty's size included.
    ///
    /// [`Screen::resize`]: crate::Screen::resize
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.state.screen.resize(cols, rows);
        let area = self.state.text_area();
        if area != self.state.inband.area {
            self.state.inband.area = area;
            if self.state.modes.resize_report() {
                self.state.report_size();
            }
        }
    }

    /// Whether the console is potentially visible: not when the VT is
    /// switched away or the display is blanked. While mode 2033 is set, a
    /// change is reported.
    pub fn set_visible(&mut self, visible: bool) {
        if self.state.inband.visible != visible {
            self.state.inband.visible = visible;
            if self.state.modes.visibility_report() {
                self.state.reply_visibility();
            }
        }
    }
}

impl State {
    fn text_area(&self) -> TextArea {
        let (rows, cols) = (self.screen.rows(), self.screen.cols());
        let px = |n: usize, cell: u32| u64::try_from(n).unwrap_or(0).saturating_mul(cell.into());
        let (w, h) = self.host.cell.unwrap_or((0, 0));
        TextArea {
            rows,
            cols,
            height: px(rows, h),
            width: px(cols, w),
        }
    }

    /// Mode 2048's report, `CSI 48 ; rows ; cols ; height ; width t`.
    pub(super) fn report_size(&mut self) {
        let a = self.text_area();
        self.inband.area = a;
        let reply = format!("\x1b[48;{};{};{};{}t", a.rows, a.cols, a.height, a.width);
        self.reply(Query::TextArea, reply.as_bytes());
    }

    /// `CSI ? 999 ; Ps n`: 1 potentially visible, 2 not.
    pub(super) fn reply_visibility(&mut self) {
        let ps = if self.inband.visible { 1 } else { 2 };
        let reply = format!("\x1b[?999;{ps}n");
        self.reply(Query::Visibility, reply.as_bytes());
    }

    /// The scheme of the default background drawn.
    pub(super) fn scheme(&self) -> Scheme {
        Scheme::of(self.colors.bg())
    }

    /// `CSI ? 997 ; Ps n`: 1 dark, 2 light.
    pub(super) fn reply_scheme(&mut self) {
        let reply = format!("\x1b[?997;{}n", self.scheme() as u8);
        self.reply(Query::ColorScheme, reply.as_bytes());
    }

    /// After a change of colours: while mode 2031 is set, a scheme that
    /// differs from `before` is reported.
    pub(super) fn report_scheme_change(&mut self, before: Scheme) {
        if self.modes.scheme_report() && self.scheme() != before {
            self.reply_scheme();
        }
    }
}
