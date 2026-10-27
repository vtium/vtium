//! Font selection with its fallback.
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays. The C reference retried at 16 for as long as the
//! lookup failed (`src/terminal.c:921-939` at kmscon b1f7a7c5,
//! aaronsb/kmscon#63).

/// The size tried once when the first lookup fails.
pub const FALLBACK_SIZE: u32 = 16;

/// Looks a font up at `size` with `find`, which gives the font at a size
/// or why there is none; `in_use` says whether a font is in use. Returns
/// the font and the size it was found at.
pub fn select<F, E>(
    size: u32,
    in_use: bool,
    mut find: impl FnMut(u32) -> Result<F, E>,
) -> Result<(F, u32), E> {
    match find(size) {
        Ok(font) => Ok((font, size)),
        Err(e) if in_use => Err(e),
        Err(_) => find(FALLBACK_SIZE).map(|f| (f, FALLBACK_SIZE)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_asked_for() {
        assert_eq!(select(24, false, |s| Ok::<_, ()>(s * 2)), Ok((48, 24)));
    }

    #[test]
    fn with_no_font_in_use_a_failure_falls_back_once() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            if s == FALLBACK_SIZE {
                Ok("font")
            } else {
                Err("no 99")
            }
        });
        assert_eq!(found, Ok(("font", FALLBACK_SIZE)));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn a_lookup_that_always_fails_is_an_error_after_the_fallback() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(FALLBACK_SIZE));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn with_a_font_in_use_a_failure_is_the_error() {
        let mut asked = Vec::new();
        let found = select(99, true, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(99));
        assert_eq!(asked, [99]);
    }
}
