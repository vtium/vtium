//! Who draws the mouse pointer on a display: its cursor plane, or the
//! renderer in software.
//!
//! The pointer is the renderer's arrow, or the glyph `pointer-char` names
//! ([`Renderer::arrow`]), sized to the display's cells and turned with it.
//! At the first draw that shows the pointer after the renderer changes (a
//! set, a zoom, a rotation), the display's [`Pointer`] puts the new arrow
//! on the display's cursor plane,
//! as the C reference set its hardware cursor up again after each change
//! (`src/terminal.c:303-338` at kmscon b1f7a7c5); a pointer that never
//! shows, as with `mouse` off, sets no plane up. A display that takes the
//! arrow shows the pointer there, moved and hidden by the next commit with
//! no cell drawn; one without a cursor plane, fbdev's or a card's on the
//! legacy calls, or whose plane refused the arrow or failed a commit, has
//! the renderer draw the same arrow over the cells. The arrow goes to the
//! plane at a draw, when the display is lent for one and so awake and
//! online: a display that is not yet tries again at the next draw, and
//! draws the pointer in software until then. Each decision is logged
//! with the display's connector name, as the C reference named the
//! cursor, `HW` or `SW`, in its display's line. A zoom or a rotation
//! makes a new renderer, and so a new decision, which is most often the
//! one before: vtium logs a decision at info level only when it differs
//! from the last one logged for the display, and a repeat at debug level.
//!
//! `soft-cursor` has the renderer draw the pointer on every display and
//! sets no cursor plane up, as the C reference's key did
//! (`src/terminal.c:1269-1270`).
//!
//! A failed renderer set keeps the renderer the display had, and with it
//! the arrow on its plane. The C reference set the hardware cursor up from
//! the failed set's text, which had no font, and so drew the pointer
//! nowhere (aaronsb/kmscon#211).

use std::io;

use vtium_platform::display::DisplayId;
use vtium_render::Arrow;
#[cfg(doc)]
use vtium_render::Renderer;

/// A display's cursor plane, as the pointer drives it.
pub trait Plane {
    /// Puts `arrow` on the display's cursor plane, hidden. An error of
    /// kind `NotConnected` that the system did not raise means the display
    /// is not online yet; any other, the card's own errors included, that
    /// it has no cursor plane or the plane refused the arrow.
    fn set_up(&mut self, id: DisplayId, arrow: &Arrow) -> io::Result<()>;
    /// Shows the arrow with its hotspot on the display's pixel `at`, from
    /// the next commit.
    fn show(&mut self, id: DisplayId, at: (i32, i32)) -> io::Result<()>;
    /// Hides it from the next commit.
    fn hide(&mut self, id: DisplayId);
    /// Whether the plane still holds the arrow set up: a display gives it
    /// up when a commit with it fails and one without it takes.
    fn holds(&self, id: DisplayId) -> bool;
    /// The display's name in the log, its connector's, such as `DP-8`.
    fn display_name(&self, id: DisplayId) -> String;
}

/// Who draws the pointer on a display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Route {
    /// The renderer changed: the next draw that shows the pointer offers
    /// its arrow to the plane.
    #[default]
    Unset,
    /// The display's cursor plane.
    Hardware,
    /// The renderer, into the frame.
    Software,
}

/// A display's pointer: where it is drawn, kept across draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pointer {
    route: Route,
    /// `soft-cursor`: the renderer draws it, whatever the display has.
    soft: bool,
    /// The route last logged at info level. A new renderer resets
    /// `route` and keeps this, so that the same decision made again is
    /// logged at debug level.
    logged: Option<Route>,
}

impl Pointer {
    /// A pointer the renderer always draws, as `soft-cursor` asks: the
    /// display's cursor plane is never set up.
    pub fn software() -> Pointer {
        Pointer {
            route: Route::Unset,
            soft: true,
            logged: None,
        }
    }

    pub fn route(&self) -> Route {
        self.route
    }

    /// The renderer changed: its arrow goes to the plane at the next draw.
    pub fn reset(&mut self) {
        self.route = Route::Unset;
    }

    /// Before a draw: puts the renderer's `arrow` on the plane if it
    /// changed since and the pointer shows, and gives back the pointer's
    /// tip, in the grid's pixels, for the renderer to draw, which is `tip`
    /// unless the plane shows it. A pointer that never shows, as with
    /// `mouse` off, sets no plane up.
    pub fn before_draw(
        &mut self,
        plane: &mut (impl Plane + ?Sized),
        id: DisplayId,
        arrow: &Arrow,
        tip: Option<(u32, u32)>,
    ) -> Option<(u32, u32)> {
        if self.route == Route::Unset && tip.is_some() && self.soft {
            self.take(
                &*plane,
                id,
                Route::Software,
                format_args!("draws it in software: soft-cursor"),
            );
        }
        if self.route == Route::Unset && tip.is_some() {
            match plane.set_up(id, arrow) {
                Ok(()) => {
                    self.take(
                        &*plane,
                        id,
                        Route::Hardware,
                        format_args!("shows it on its cursor plane"),
                    );
                }
                Err(e) if e.kind() == io::ErrorKind::NotConnected && e.raw_os_error().is_none() => {
                }
                Err(e) => {
                    self.take(
                        &*plane,
                        id,
                        Route::Software,
                        format_args!("draws it in software: {e}"),
                    );
                }
            }
        }
        tip.filter(|_| self.route != Route::Hardware)
    }

    /// After a swap: whether the display gave the plane up, so that the
    /// pointer, drawn by the renderer from now on, is to be drawn again.
    pub fn after_swap(&mut self, plane: &(impl Plane + ?Sized), id: DisplayId) -> bool {
        if self.route != Route::Hardware || plane.holds(id) {
            return false;
        }
        self.take(
            plane,
            id,
            Route::Software,
            format_args!("draws it in software from now on"),
        );
        true
    }

    /// Sends the pointer by `route` and logs `why`, which says how: at
    /// info level when the route differs from the one last logged for the
    /// display, and at debug level when it repeats it, as a zoom or a
    /// rotation that makes the same decision again does.
    fn take(
        &mut self,
        plane: &(impl Plane + ?Sized),
        id: DisplayId,
        route: Route,
        why: std::fmt::Arguments<'_>,
    ) {
        self.route = route;
        let level = if self.logged == Some(route) {
            log::Level::Debug
        } else {
            self.logged = Some(route);
            log::Level::Info
        };
        say(
            level,
            format_args!("display {} {why}", plane.display_name(id)),
        );
    }

    /// After a draw, which placed the grid: shows the plane's arrow at
    /// `at`, the display's pixel the renderer placed the pointer's `tip`
    /// on ([`Renderer::pointer_at`]), or hides it when the pointer is
    /// hidden, as it is while a new renderer's arrow waits for the pointer
    /// to show.
    pub fn after_draw(
        &mut self,
        plane: &mut (impl Plane + ?Sized),
        id: DisplayId,
        tip: Option<(u32, u32)>,
        at: Option<(i32, i32)>,
    ) {
        if self.route == Route::Software || (self.route == Route::Unset && tip.is_some()) {
            return;
        }
        match at {
            Some(at) => {
                if let Err(e) = plane.show(id, at) {
                    log::warn!(
                        "display {}: cannot show the pointer: {e}",
                        plane.display_name(id)
                    );
                }
            }
            None => plane.hide(id),
        }
    }
}

/// Writes a line of the pointer's log at `level`. Under test it also
/// keeps the line, for the tests to read back.
fn say(level: log::Level, line: std::fmt::Arguments<'_>) {
    log::log!(target: "pointer", level, "{line}");
    #[cfg(test)]
    tests::SAID.with(|s| s.borrow_mut().push((level, line.to_string())));
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_font::{Font, Unifont};
    use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
    use vtium_term::{Age, Cursor, Damage, Frame, Line, LineId, Palette, Rgb};

    thread_local! {
        /// The lines the pointer logged on this thread, with their level.
        pub(super) static SAID: std::cell::RefCell<Vec<(log::Level, String)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    /// The lines the pointer logged on this thread since the last call.
    fn said() -> Vec<(log::Level, String)> {
        SAID.with(|s| std::mem::take(&mut *s.borrow_mut()))
    }

    /// A stand-in plane: what was set up and where it shows.
    #[derive(Default)]
    struct Fake {
        /// What `set_up` answers, once each, then succeeds.
        refuse: Vec<io::Error>,
        arrow: Option<Arrow>,
        shown: Option<(i32, i32)>,
        /// Arrows offered.
        set_ups: usize,
    }

    impl Plane for Fake {
        fn set_up(&mut self, _: DisplayId, arrow: &Arrow) -> io::Result<()> {
            self.set_ups = self.set_ups.saturating_add(1);
            if !self.refuse.is_empty() {
                return Err(self.refuse.remove(0));
            }
            self.arrow = Some(arrow.clone());
            Ok(())
        }
        fn holds(&self, _: DisplayId) -> bool {
            self.arrow.is_some()
        }
        fn show(&mut self, _: DisplayId, at: (i32, i32)) -> io::Result<()> {
            self.shown = Some(at);
            Ok(())
        }
        fn hide(&mut self, _: DisplayId) {
            self.shown = None;
        }
        fn display_name(&self, id: DisplayId) -> String {
            assert_eq!(id, ID, "the pointer names the display it was given");
            "DP-8".to_owned()
        }
    }

    const ID: DisplayId = DisplayId(7);

    /// A renderer of 8 by 16 cells on a display 4 cells by 2 with a
    /// border, turned as `o` says, that has drawn one frame.
    fn renderer(o: Orientation) -> Renderer<Unifont> {
        let config = Config {
            orientation: o,
            ..Config::default()
        };
        let (w, h) = if o == Orientation::Normal {
            (36, 34)
        } else {
            (34, 36)
        };
        let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
        let lines: Vec<Line> = (1..3).map(|i| Line::new(LineId::new(i), 4)).collect();
        let damage = Damage::new(4, 2);
        let palette = Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
        let frame = Frame::new(
            lines.iter().collect(),
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        r.draw(&frame, &mut Canvas::new(w, h, 0), 0, &DrawState::default());
        r
    }

    /// After a draw of `r`, with the pointer's tip at `tip`.
    fn after(p: &mut Pointer, plane: &mut Fake, r: &Renderer<Unifont>, tip: Option<(u32, u32)>) {
        p.after_draw(plane, ID, tip, tip.and_then(|t| r.pointer_at(t)));
    }

    // The arrow is never larger than the cursor plane drm2d sets up: the
    // two crates bound the same side, 64 pixels.
    #[test]
    fn the_arrow_fits_the_cursor_plane() {
        assert_eq!(
            u64::from(vtium_render::POINTER_SIDE_MAX),
            vtium_platform::display::CURSOR_MAX
        );
    }

    #[test]
    fn a_display_with_a_cursor_plane_shows_the_arrow_there() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        assert_eq!(p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))), None);
        assert_eq!(p.route(), Route::Hardware);
        assert_eq!(plane.arrow.as_ref(), Some(r.arrow()));
        // the grid is centred: 2,1 from the display's corner
        after(&mut p, &mut plane, &r, Some((5, 6)));
        assert_eq!(plane.shown, Some((7, 7)));
        // past the grid, the tip is kept on its last pixel
        after(&mut p, &mut plane, &r, Some((1000, 1000)));
        assert_eq!(plane.shown, Some((2 + 31, 1 + 31)));
        after(&mut p, &mut plane, &r, None);
        assert_eq!(plane.shown, None);
    }

    #[test]
    fn the_plane_shows_the_pointer_where_the_turned_grid_puts_it() {
        let r = renderer(Orientation::Right);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        // turned right, the grid is 32 across and 32 down at 1,2; the
        // grid's pixel 5,6 lands at 1 + 32 - 1 - 6, 2 + 5
        after(&mut p, &mut plane, &r, Some((5, 6)));
        assert_eq!(plane.shown, Some((26, 7)));
        let arrow = plane.arrow.unwrap();
        assert_eq!(arrow, Arrow::new(16, Orientation::Right));
    }

    #[test]
    fn a_display_without_a_cursor_plane_draws_the_arrow_in_software() {
        let r = renderer(Orientation::Normal);
        let mut p = Pointer::default();
        let mut plane = Fake {
            refuse: vec![io::ErrorKind::Unsupported.into()],
            ..Fake::default()
        };
        assert_eq!(
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))),
            Some((5, 6))
        );
        assert_eq!(p.route(), Route::Software);
        // it is not offered again until the renderer changes
        assert_eq!(p.before_draw(&mut plane, ID, r.arrow(), None), None);
        assert_eq!(plane.arrow, None);
        after(&mut p, &mut plane, &r, Some((5, 6)));
        assert_eq!(plane.shown, None);
        p.reset();
        assert_eq!(p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))), None);
        assert_eq!(p.route(), Route::Hardware);
    }

    #[test]
    fn a_display_not_yet_online_tries_again_at_the_next_draw() {
        let r = renderer(Orientation::Normal);
        let mut p = Pointer::default();
        let mut plane = Fake {
            refuse: vec![io::Error::new(io::ErrorKind::NotConnected, "not online")],
            ..Fake::default()
        };
        assert_eq!(
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))),
            Some((5, 6))
        );
        assert_eq!(p.route(), Route::Unset);
        assert_eq!(p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))), None);
        assert_eq!(p.route(), Route::Hardware);
    }

    // An error from the card, EINVAL included, is the plane refusing the
    // arrow: it is not offered again at every draw.
    #[test]
    fn an_error_from_the_card_is_a_refusal() {
        let r = renderer(Orientation::Normal);
        let mut p = Pointer::default();
        let mut plane = Fake {
            refuse: vec![io::Error::from_raw_os_error(22)],
            ..Fake::default()
        };
        assert_eq!(
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))),
            Some((5, 6))
        );
        assert_eq!(p.route(), Route::Software);
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        assert_eq!(plane.set_ups, 1);
    }

    // With no pointer shown, as with `mouse` off, no plane is set up.
    #[test]
    fn a_hidden_pointer_sets_no_plane_up() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        for _ in 0..3 {
            assert_eq!(p.before_draw(&mut plane, ID, r.arrow(), None), None);
            after(&mut p, &mut plane, &r, None);
        }
        assert_eq!((plane.set_ups, p.route()), (0, Route::Unset));
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        assert_eq!((plane.set_ups, p.route()), (1, Route::Hardware));
    }

    // A new renderer while the pointer hides: the old arrow on the plane
    // is hidden, though the new one waits until the pointer shows.
    #[test]
    fn a_new_renderer_with_the_pointer_hidden_hides_the_plane() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        after(&mut p, &mut plane, &r, Some((5, 6)));
        assert!(plane.shown.is_some());
        p.reset();
        p.before_draw(&mut plane, ID, r.arrow(), None);
        after(&mut p, &mut plane, &r, None);
        assert_eq!(plane.shown, None);
    }

    // `soft-cursor`: the renderer draws the pointer, and the plane is
    // never offered the arrow, after a new renderer too.
    #[test]
    fn a_soft_pointer_never_sets_the_plane_up() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::software(), Fake::default());
        for _ in 0..2 {
            assert_eq!(
                p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))),
                Some((5, 6))
            );
            assert_eq!(p.route(), Route::Software);
            after(&mut p, &mut plane, &r, Some((5, 6)));
            assert!(!p.after_swap(&plane, ID));
            p.reset();
        }
        assert_eq!((plane.set_ups, plane.shown), (0, None));
    }

    // A plane given up after a failed commit: the pointer goes to the
    // renderer, and the display is to draw again.
    #[test]
    fn a_plane_given_up_sends_the_pointer_to_the_renderer() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        assert!(!p.after_swap(&plane, ID));
        plane.arrow = None;
        assert!(p.after_swap(&plane, ID));
        assert_eq!(p.route(), Route::Software);
        assert!(!p.after_swap(&plane, ID));
        assert_eq!(
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6))),
            Some((5, 6))
        );
    }

    // A zoom or a rotation makes a new renderer, and View::rebuild resets
    // the pointer, which offers the new arrow to the plane again. The
    // same decision made again is logged once at info level, by the
    // display's connector name, and after that at debug level.
    #[test]
    fn a_zoom_that_makes_the_same_decision_logs_it_at_info_once() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        said();
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        for _ in 0..2 {
            p.reset();
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        }
        let line = "display DP-8 shows it on its cursor plane".to_owned();
        assert_eq!(
            said(),
            [
                (log::Level::Info, line.clone()),
                (log::Level::Debug, line.clone()),
                (log::Level::Debug, line),
            ]
        );
        assert_eq!(plane.set_ups, 3);
    }

    // A decision that differs from the last one logged is logged at info
    // level, across new renderers too; one that repeats it, though for
    // another reason, at debug level.
    #[test]
    fn a_decision_that_changes_the_route_logs_it_at_info() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::default(), Fake::default());
        said();
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        plane.arrow = None;
        assert!(p.after_swap(&plane, ID));
        p.reset();
        plane.refuse.push(io::ErrorKind::Unsupported.into());
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        p.reset();
        p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
        let refused = io::Error::from(io::ErrorKind::Unsupported);
        assert_eq!(
            said(),
            [
                (
                    log::Level::Info,
                    "display DP-8 shows it on its cursor plane".to_owned()
                ),
                (
                    log::Level::Info,
                    "display DP-8 draws it in software from now on".to_owned()
                ),
                (
                    log::Level::Debug,
                    format!("display DP-8 draws it in software: {refused}")
                ),
                (
                    log::Level::Info,
                    "display DP-8 shows it on its cursor plane".to_owned()
                ),
            ]
        );
    }

    // `soft-cursor` logs its decision once at info level, though each new
    // renderer makes it again.
    #[test]
    fn a_soft_pointer_logs_its_decision_at_info_once() {
        let r = renderer(Orientation::Normal);
        let (mut p, mut plane) = (Pointer::software(), Fake::default());
        said();
        for _ in 0..3 {
            p.before_draw(&mut plane, ID, r.arrow(), Some((5, 6)));
            p.reset();
        }
        let line = "display DP-8 draws it in software: soft-cursor".to_owned();
        assert_eq!(
            said(),
            [
                (log::Level::Info, line.clone()),
                (log::Level::Debug, line.clone()),
                (log::Level::Debug, line),
            ]
        );
    }
}
