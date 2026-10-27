//! Who draws the mouse pointer on a display: its hardware cursor, or the
//! renderer in software.
//!
//! After each change of a display's renderer font (a renderer set, a zoom,
//! a rotation), [`hardware`] decides whether the hardware cursor holds the
//! pointer: only when it is wanted (`mouse` on, `soft-cursor` off), the
//! renderer has a font to size it by, and the display took it. Otherwise
//! the renderer draws the pointer. A failed renderer set therefore leaves
//! the pointer drawn in software, and the next set that succeeds gives it
//! back to the hardware. The C reference kept the hardware flag set when
//! the failed set left no font, so neither drew it
//! (`src/terminal.c:192-207, 303-338` at kmscon b1f7a7c5,
//! aaronsb/kmscon#211).

/// Who draws the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The pointer is hidden.
    Hidden,
    /// The display's hardware cursor.
    Hardware,
    /// The renderer, into the frame.
    Software,
}

/// Whether the hardware cursor holds the pointer after a font change:
/// `wanted`, the renderer `has_font`, and `set_up`, which gives the cursor
/// to the display, succeeded. `set_up` runs only when the first two hold.
pub fn hardware(wanted: bool, has_font: bool, set_up: impl FnOnce() -> bool) -> bool {
    wanted && has_font && set_up()
}

/// Who draws a `visible` pointer on a display whose hardware cursor
/// holds it or not.
pub fn route(visible: bool, hardware: bool) -> Route {
    match (visible, hardware) {
        (false, _) => Route::Hidden,
        (true, true) => Route::Hardware,
        (true, false) => Route::Software,
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a set-up that must not run fails the test")]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_cursor_needs_a_font_and_the_display() {
        assert!(hardware(true, true, || true));
        assert!(!hardware(true, true, || false));
        assert!(!hardware(false, true, || panic!("not wanted, not set up")));
        assert!(!hardware(true, false, || panic!("no font to size it by")));
    }

    #[test]
    fn a_pointer_with_no_hardware_cursor_is_drawn_in_software() {
        assert_eq!(route(true, true), Route::Hardware);
        assert_eq!(route(true, false), Route::Software);
        assert_eq!(route(false, true), Route::Hidden);
        assert_eq!(route(false, false), Route::Hidden);
    }

    #[test]
    fn a_failed_set_leaves_the_pointer_drawn() {
        // The set fails: the renderer has no font.
        let hw = hardware(true, false, || true);
        assert_eq!(route(true, hw), Route::Software);
        // A later set succeeds: the hardware cursor takes it back.
        let hw = hardware(true, true, || true);
        assert_eq!(route(true, hw), Route::Hardware);
    }
}
