//! A display's pixel density, from the physical size its device reports:
//! a DRM connector's `mm_width` and `mm_height`, which the kernel takes
//! from the EDID, or fbdev's `width` and `height` in millimetres. The
//! console converts point sizes to pixels with it: pixels = points × DPI
//! ÷ 72 (the operator's decision on vtium/vtium#145).

/// The density of a display with no physical size, or with one that gives
/// a density out of bounds: a desktop monitor's.
pub const DPI_ASSUMED: f64 = 96.0;

/// The coarsest density believed. A 110-inch television at 1920x1080 is
/// 20 DPI; a coarser figure comes from a size the device misreports.
pub const DPI_MIN: f64 = 20.0;

/// The finest density believed, past the densest phone panels at about
/// 800 DPI. A finer figure comes from a size the device misreports, as a
/// projector that reports 1 mm.
pub const DPI_MAX: f64 = 1000.0;

/// An inch in tenths of a millimetre, so the bounds' own figures divide
/// exactly.
const INCH_IN_TENTH_MM: f64 = 254.0;

/// A display's density in dots per inch, across and down, and whether the
/// device measured it or vtium assumed [`DPI_ASSUMED`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    dpi: (f64, f64),
    measured: bool,
}

impl Density {
    /// The density assumed where none is measured.
    pub const ASSUMED: Density = Density {
        dpi: (DPI_ASSUMED, DPI_ASSUMED),
        measured: false,
    };

    /// The density of a mode `pixels` wide and high on a panel `mm` wide
    /// and high. A side of 0 mm is no physical size, and a side whose
    /// density falls outside [`DPI_MIN`] to [`DPI_MAX`] is a misreport:
    /// either gives [`Density::ASSUMED`].
    pub(crate) fn of(pixels: (u32, u32), mm: (u32, u32)) -> Density {
        let axis = |px: u32, mm: u32| {
            (mm != 0)
                .then(|| f64::from(px) * INCH_IN_TENTH_MM / (f64::from(mm) * 10.0))
                .filter(|dpi| (DPI_MIN..=DPI_MAX).contains(dpi))
        };
        match (axis(pixels.0, mm.0), axis(pixels.1, mm.1)) {
            (Some(x), Some(y)) => Density {
                dpi: (x, y),
                measured: true,
            },
            _ => Density::ASSUMED,
        }
    }

    /// Dots per inch, across and down.
    pub fn dpi(&self) -> (f64, f64) {
        self.dpi
    }

    /// Whether the density comes from the physical size the device
    /// reported; false where vtium assumed [`DPI_ASSUMED`].
    pub fn measured(&self) -> bool {
        self.measured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Within half a dot per inch of `dpi` on each axis.
    fn near(d: Density, dpi: (f64, f64)) -> bool {
        let (x, y) = d.dpi();
        (x - dpi.0).abs() < 0.5 && (y - dpi.1).abs() < 0.5
    }

    #[test]
    fn a_27_inch_1440p_monitor_is_about_109_dpi() {
        // 27 inches at 16:9 is 597.7 by 336.2 mm.
        let d = Density::of((2560, 1440), (597, 336));
        assert!(d.measured());
        assert!(near(d, (109.0, 109.0)), "{:?}", d.dpi());
    }

    #[test]
    fn a_3_5_inch_320x240_panel_is_about_114_dpi() {
        // 3.5 inches at 4:3 is 71.1 by 53.3 mm.
        let d = Density::of((320, 240), (71, 53));
        assert!(d.measured());
        assert!(near(d, (114.0, 115.0)), "{:?}", d.dpi());
    }

    #[test]
    fn no_physical_size_assumes_96_dpi() {
        for mm in [(0, 0), (597, 0), (0, 336)] {
            let d = Density::of((2560, 1440), mm);
            assert_eq!(d, Density::ASSUMED, "{mm:?}");
            assert_eq!(d.dpi(), (96.0, 96.0));
            assert!(!d.measured());
        }
    }

    #[test]
    fn an_implausible_density_is_assumed() {
        // A projector reporting 1 mm, a screen of 20 m, one bad side, and
        // a size of all ones.
        for mm in [(1, 1), (20_000, 11_250), (597, 1), (u32::MAX, u32::MAX)] {
            assert_eq!(Density::of((1920, 1080), mm), Density::ASSUMED, "{mm:?}");
        }
    }

    #[test]
    fn the_bounds_are_inclusive() {
        // 5000 px over 127 mm is 1000 DPI, and 100 px over 127 mm is 20.
        let edges = Density::of((5000, 100), (127, 127));
        assert!(edges.measured());
        assert_eq!(edges.dpi(), (DPI_MAX, DPI_MIN));
        assert!(!Density::of((5001, 100), (127, 127)).measured());
        assert!(!Density::of((5000, 99), (127, 127)).measured());
    }
}
