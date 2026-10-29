//! A display's pixel density, from the physical size its device reports:
//! a DRM connector's `mm_width` and `mm_height`, which the kernel takes
//! from the EDID, or where those are 0 the size the connector's EDID in
//! sysfs states (ADR-1301 D3), or fbdev's `width` and `height` in
//! millimetres. The console converts point sizes to pixels with it:
//! pixels = points × DPI ÷ 72 (the operator's decision on vtium/vtium#145).

use std::io::Read;
use std::path::Path;

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

/// The bytes of an EDID's base block, the most read of one.
const EDID_BASE: usize = 128;

/// The eight bytes every EDID starts with.
const EDID_HEADER: [u8; 8] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

/// The physical size in millimetres, across and down, an EDID's base
/// block states: its image size in centimetres, bytes 21 and 22. `None`
/// for bytes that are no EDID or whose base block's checksum fails, and
/// for a side of 0, as EDID 1.4 states an aspect ratio with no size.
pub(crate) fn edid_mm(edid: &[u8]) -> Option<(u32, u32)> {
    let base = edid.get(..EDID_BASE)?;
    if base.get(..EDID_HEADER.len())? != EDID_HEADER
        || base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != 0
    {
        return None;
    }
    let (&w, &h) = (base.get(21)?, base.get(22)?);
    (w != 0 && h != 0).then(|| {
        (
            u32::from(w).saturating_mul(10),
            u32::from(h).saturating_mul(10),
        )
    })
}

/// The physical size the EDID of connector `name` (`HDMI-A-1`) states,
/// read from `card`, the card's directory in sysfs, where the kernel keeps
/// it as `card<N>-<name>/edid`; `None` where there is none to read.
pub(crate) fn sysfs_mm(card: &Path, name: &str) -> Option<(u32, u32)> {
    let suffix = format!("-{name}");
    let mut dirs: Vec<String> = std::fs::read_dir(card)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|d| {
            d.strip_prefix("card")
                .and_then(|d| d.strip_suffix(&suffix))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    dirs.sort();
    let file = std::fs::File::open(card.join(dirs.first()?).join("edid")).ok()?;
    let mut edid = Vec::with_capacity(EDID_BASE);
    file.take(EDID_BASE as u64).read_to_end(&mut edid).ok()?;
    edid_mm(&edid)
}

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
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test fails by panicking"
)]
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

    /// A base block stating `cm`, its checksum made good.
    fn edid(cm: (u8, u8)) -> Vec<u8> {
        let mut b = vec![0u8; EDID_BASE];
        b[..8].copy_from_slice(&EDID_HEADER);
        b[18] = 1;
        b[19] = 4;
        (b[21], b[22]) = cm;
        let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        b[127] = 0u8.wrapping_sub(sum);
        b
    }

    #[test]
    fn an_edid_states_its_image_size_in_centimetres() {
        // a 27-inch monitor's, 60 by 34 cm; a base block with an extension
        // after it reads the same
        assert_eq!(edid_mm(&edid((60, 34))), Some((600, 340)));
        let mut extended = edid((60, 34));
        extended.extend([0x02; 128]);
        assert_eq!(edid_mm(&extended), Some((600, 340)));
    }

    #[test]
    fn an_edid_with_no_size_or_no_edid_states_none() {
        // no size, an aspect ratio alone, a block cut short, a bad header
        // and a bad checksum
        let mut header = edid((60, 34));
        header[0] = 1;
        header[127] = header[127].wrapping_sub(1);
        let mut sum = edid((60, 34));
        sum[127] ^= 1;
        for (why, b) in [
            ("none", edid((0, 0))),
            ("ratio", edid((0, 79))),
            ("short", edid((60, 34))[..127].to_vec()),
            ("header", header),
            ("checksum", sum),
        ] {
            assert_eq!(edid_mm(&b), None, "{why}");
        }
    }

    /// A stand-in card directory in sysfs for `test`, holding `edids` by
    /// connector directory.
    fn sysfs(test: &str, edids: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vtium-edid-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (conn, bytes) in edids {
            std::fs::create_dir_all(dir.join(conn)).unwrap();
            std::fs::write(dir.join(conn).join("edid"), bytes).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_connectors_edid_is_read_from_its_cards_directory_in_sysfs() {
        let dir = sysfs(
            "read",
            &[
                ("card0-HDMI-A-1", edid((60, 34))),
                ("card0-HDMI-A-11", edid((30, 17))),
                ("card0-DP-1", Vec::new()),
            ],
        );
        let read = |name| sysfs_mm(&dir, name);
        let (hdmi, hdmi11, dp, gone) = (
            read("HDMI-A-1"),
            read("HDMI-A-11"),
            read("DP-1"),
            read("eDP-1"),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(hdmi, Some((600, 340)));
        assert_eq!(hdmi11, Some((300, 170)));
        // an empty edid file, as a connector with no EDID has, and none
        assert_eq!((dp, gone), (None, None));
        assert_eq!(sysfs_mm(Path::new("/nonexistent/vtium"), "HDMI-A-1"), None);
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
