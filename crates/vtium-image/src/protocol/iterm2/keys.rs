//! An iTerm2 string's keys, and the size they give a picture (ADR-1103, D1
//! and D3).
//!
//! The keys of `File=` and `MultipartFile=` run up to the first `:`, each
//! a name, `=` and a value, separated by `;`. The handler reads four of
//! them: `width`, `height`, `preserveAspectRatio` and `inline`. It reads
//! every other key, `name` and `size` among them, and ignores it, and so
//! does it ignore a key with no `=`.
//!
//! `width` and `height` each give a side of the picture's drawn size: a
//! whole number of cells (`20`), a whole number of pixels (`300px`), a
//! whole percentage of the screen's width or height (`50%`), or `auto`,
//! the picture's own size. A value left out, a value of 0 and a value the
//! handler cannot read each count as `auto`. With `preserveAspectRatio` at
//! 1, its default, a side given alone sets the other in the picture's own
//! proportion, and two sides make a box the picture is scaled into as
//! large as it fits with its shape kept. With it at 0, each side takes the
//! size given, or the picture's own where it is `auto`.

/// One of `width` and `height`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Side {
    /// The picture's own size, or the size its proportion gives.
    #[default]
    Auto,
    Cells(u32),
    Pixels(u32),
    /// A percentage of the screen's width or height in pixels.
    Percent(u32),
}

/// The keys the handler reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Keys {
    pub(super) width: Side,
    pub(super) height: Side,
    /// `preserveAspectRatio`: 1 unless the string gives `0`.
    pub(super) preserve: bool,
    /// `inline=1`: the picture shows. Without it iTerm2 would save the file,
    /// and vtium saves nothing, so the handler drops the picture.
    pub(super) inline: bool,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            width: Side::Auto,
            height: Side::Auto,
            preserve: true,
            inline: false,
        }
    }
}

impl Keys {
    /// The keys `text` holds, the bytes between `File=` and the `:`.
    pub(super) fn parse(text: &[u8]) -> Self {
        let mut keys = Self::default();
        for pair in text.split(|&b| b == b';') {
            let Some(eq) = pair.iter().position(|&b| b == b'=') else {
                continue;
            };
            let (name, value) = (
                pair.get(..eq).unwrap_or_default(),
                pair.get(eq.saturating_add(1)..).unwrap_or_default(),
            );
            match name {
                b"width" => keys.width = side(value),
                b"height" => keys.height = side(value),
                b"preserveAspectRatio" => keys.preserve = value != b"0",
                b"inline" => keys.inline = value == b"1",
                _ => {}
            }
        }
        keys
    }
}

/// A whole number of ASCII digits, at least one, that fits a `u32`.
fn number(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |n, &b| {
        let d = b.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(u32::from(d))
    })
}

/// A `width` or `height` value; 0 and what cannot be read are `auto`.
fn side(value: &[u8]) -> Side {
    let (digits, make): (&[u8], fn(u32) -> Side) = if let Some(d) = value.strip_suffix(b"px") {
        (d, Side::Pixels)
    } else if let Some(d) = value.strip_suffix(b"%") {
        (d, Side::Percent)
    } else {
        (value, Side::Cells)
    };
    match number(digits) {
        Some(n) if n > 0 => make(n),
        _ => Side::Auto,
    }
}

/// `a` × `num` / `den`, rounded to the nearest whole number; 0 for a `den`
/// of 0.
fn scale(a: u64, num: u64, den: u64) -> u64 {
    a.saturating_mul(num)
        .saturating_add(den / 2)
        .checked_div(den)
        .unwrap_or(0)
}

/// The pixels `side` asks for on a screen side of `cells` cells, each
/// `cell` pixels; `None` for `auto`.
fn pixels(side: Side, cell: u32, cells: usize) -> Option<u64> {
    let screen = u64::try_from(cells)
        .unwrap_or(u64::MAX)
        .saturating_mul(u64::from(cell));
    match side {
        Side::Auto => None,
        Side::Cells(n) => Some(u64::from(n).saturating_mul(u64::from(cell))),
        Side::Pixels(n) => Some(u64::from(n)),
        Side::Percent(p) => Some(scale(screen, u64::from(p), 100)),
    }
}

/// The size in pixels a picture of `size` pixels draws at, at a `cell`
/// sized cell on a screen of `grid` columns and rows, as `keys` give it.
/// Each side is at least 1 pixel.
pub(super) fn drawn(
    keys: &Keys,
    size: (u32, u32),
    cell: (u32, u32),
    grid: (usize, usize),
) -> (u64, u64) {
    let (pw, ph) = (u64::from(size.0).max(1), u64::from(size.1).max(1));
    let w = pixels(keys.width, cell.0, grid.0);
    let h = pixels(keys.height, cell.1, grid.1);
    let (dw, dh) = if keys.preserve {
        match (w, h) {
            (None, None) => (pw, ph),
            (Some(w), None) => (w, scale(w, ph, pw)),
            (None, Some(h)) => (scale(h, pw, ph), h),
            // the box the picture fits into, as large as fits
            (Some(w), Some(h)) if w.saturating_mul(ph) <= h.saturating_mul(pw) => {
                (w, scale(w, ph, pw))
            }
            (Some(_), Some(h)) => (scale(h, pw, ph), h),
        }
    } else {
        (w.unwrap_or(pw), h.unwrap_or(ph))
    };
    (dw.max(1), dh.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Keys {
        Keys::parse(text.as_bytes())
    }

    #[test]
    fn the_handler_reads_four_keys_and_ignores_the_rest() {
        assert_eq!(keys(""), Keys::default());
        let k = keys(
            "name=YS5wbmc=;size=1234;width=20;height=300px;preserveAspectRatio=0;inline=1;doNotMoveCursor=1;novalue;=x",
        );
        assert_eq!(k.width, Side::Cells(20));
        assert_eq!(k.height, Side::Pixels(300));
        assert!(!k.preserve);
        assert!(k.inline);
        assert_eq!(keys("width=50%").width, Side::Percent(50));
        assert_eq!(keys("height=auto").height, Side::Auto);
        // inline only with 1, and preserveAspectRatio off only with 0
        assert!(!keys("inline=0").inline);
        assert!(!keys("inline=yes").inline);
        assert!(keys("preserveAspectRatio=1").preserve);
        assert!(keys("preserveAspectRatio=no").preserve);
        // names are read as written
        assert!(!keys("Inline=1").inline);
    }

    #[test]
    fn a_side_of_0_or_one_that_cannot_be_read_is_auto() {
        for value in [
            "0",
            "0px",
            "0%",
            "",
            "px",
            "%",
            "-5",
            "+5",
            "5em",
            "5PX",
            "1.5",
            "4294967296",
            "99999999999px",
        ] {
            assert_eq!(side(value.as_bytes()), Side::Auto, "{value:?}");
        }
        assert_eq!(side(b"007"), Side::Cells(7));
        assert_eq!(side(b"4294967295px"), Side::Pixels(u32::MAX));
    }

    const CELL: (u32, u32) = (10, 20);
    const GRID: (usize, usize) = (80, 24);
    const PICTURE: (u32, u32) = (640, 480);

    fn size(text: &str) -> (u64, u64) {
        drawn(&keys(text), PICTURE, CELL, GRID)
    }

    // covers: image.iterm2
    #[test]
    fn the_records_examples_draw_at_the_sizes_it_gives() {
        // ADR-1103 D3: at a 10 by 20 pixel cell, a 640 by 480 picture
        assert_eq!(size("width=32"), (320, 240), "32 columns and 12 rows");
        assert_eq!(
            size("width=100%;height=6"),
            (160, 120),
            "inside 800 by 120, 16 columns and 6 rows"
        );
        assert_eq!(
            size("width=100%;height=6;preserveAspectRatio=0"),
            (800, 120),
            "stretched over 80 columns and 6 rows"
        );
    }

    #[test]
    fn auto_draws_at_the_pictures_own_size_or_its_proportion() {
        assert_eq!(size(""), (640, 480));
        assert_eq!(size("width=auto;height=auto"), (640, 480));
        assert_eq!(size("width=0"), (640, 480));
        assert_eq!(size("height=300px"), (400, 300));
        assert_eq!(size("height=50%"), (320, 240), "half of 24 rows of 20");
        assert_eq!(size("width=25%"), (200, 150), "a quarter of 800");
        // a box narrower than the picture's shape
        assert_eq!(size("width=100px;height=300px"), (100, 75));
        // without the proportion, an auto side keeps the picture's own size
        assert_eq!(size("width=100px;preserveAspectRatio=0"), (100, 480));
        assert_eq!(size("height=3;preserveAspectRatio=0"), (640, 60));
    }

    #[test]
    fn a_picture_scaled_to_a_sliver_keeps_a_pixel() {
        let k = keys("width=1px");
        assert_eq!(drawn(&k, (640, 4), CELL, GRID), (1, 1));
        let huge = keys("width=4294967295;height=4294967295");
        assert_eq!(
            drawn(&huge, PICTURE, CELL, GRID),
            (42_949_672_950, 32_212_254_713)
        );
    }
}
