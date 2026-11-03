//! The displays as the TUI and `preview` see them, read from sysfs without
//! opening a card: each connected connector's first mode and the physical
//! size its EDID states, from which the density a size's picture is drawn
//! at comes (ADR-1201 D4, in ADR-1200 D17's order: vtium.conf's
//! `display-dpi`, else this, else 96).
//!
//! The console takes a display's physical size from its DRM connector,
//! which the kernel fills from the EDID, else from the EDID in sysfs.
//! vtium-config opens no card, so it reads the EDID in sysfs, through
//! vtium-platform's reader (`display::Edid`), and works out the density
//! with the console's own `Density::of`. The EDID's size is in
//! centimetres, so it can differ from the connector's millimetres by under
//! a centimetre a side.

use std::io::Read;
use std::path::Path;

use vtium_platform::display::{Density, Edid};

/// The bytes read of a connector's `edid`: its base block, all that
/// [`Edid::parse`] reads.
const EDID_READ: u64 = 128;

/// The most bytes read of a connector's `modes`.
const MODES_MAX: u64 = 4096;

/// A connected display: its first mode in pixels, and its physical size in
/// millimetres where its EDID states one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    pub pixels: (u32, u32),
    pub mm: Option<(u32, u32)>,
}

impl Display {
    /// Its density down the screen, in whole dots per inch, where the
    /// console measures one from its size ([`Density::of`]); a display
    /// with no size, or with a side whose density the console does not
    /// believe, has none, and the console assumes 96.
    pub fn dpi(&self) -> Option<u32> {
        let density = Density::of(self.pixels, self.mm?);
        let down = density.measured().then(|| density.dpi().1)?;
        format!("{:.0}", down.round()).parse().ok()
    }
}

/// The first mode a connector's `modes` lists, `WIDTHxHEIGHT`, an
/// interlaced mode's `i` after it ignored.
fn first_mode(text: &str) -> Option<(u32, u32)> {
    let line = text.lines().next()?.trim().trim_end_matches('i');
    let (w, h) = line.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// At most `max` bytes of the file at `path`.
fn read(path: &Path, max: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(max)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// The connected displays under `sys`, sysfs's root, in the order of their
/// connectors' names: each `class/drm/cardN-CONNECTOR` whose `status` is
/// `connected` and whose `modes` lists a mode.
pub fn displays(sys: &Path) -> Vec<Display> {
    let class = sys.join("class/drm");
    let Ok(entries) = std::fs::read_dir(&class) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("card") && n.contains('-'))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let dir = class.join(name);
            let status = read(&dir.join("status"), 64)?;
            if String::from_utf8_lossy(&status).trim() != "connected" {
                return None;
            }
            let modes = read(&dir.join("modes"), MODES_MAX)?;
            let pixels = first_mode(&String::from_utf8_lossy(&modes))?;
            let mm = read(&dir.join("edid"), EDID_READ)
                .and_then(|b| Edid::parse(&b))
                .and_then(|e| e.mm());
            Some(Display { pixels, mm })
        })
        .collect()
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// An EDID's base block stating a size of `cm`, its checksum right.
    pub fn edid(cm: (u8, u8)) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        b[21] = cm.0;
        b[22] = cm.1;
        let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        b[127] = 0u8.wrapping_sub(sum);
        b
    }

    #[test]
    fn a_27_inch_1440p_monitor_is_about_108_dpi_down() {
        let d = Display {
            pixels: (2560, 1440),
            mm: Some((600, 340)),
        };
        assert_eq!(d.dpi(), Some(108));
        let none = Display {
            pixels: (2560, 1440),
            mm: None,
        };
        assert_eq!(none.dpi(), None);
        let projector = Display {
            pixels: (1920, 1080),
            mm: Some((10, 10)),
        };
        assert_eq!(projector.dpi(), None, "past DPI_MAX");
        // one side out of bounds is a misreport, as the console takes it
        let one_side = Display {
            pixels: (2560, 1440),
            mm: Some((10, 340)),
        };
        assert_eq!(one_side.dpi(), None, "across past DPI_MAX");
    }

    /// The tool's density and the console's are one for the same EDID
    /// bytes: the size read by vtium-platform's reader, the density by its
    /// `Density::of`, rounded down the screen. A 41 cm high panel of 5125
    /// rows is 317.5 dpi down, which a reader of its own once took as
    /// 317.49999 and rounded to 317.
    #[test]
    fn the_tool_s_density_is_the_console_s_for_the_same_edid() {
        use vtium_platform::display::{Density, Edid};
        for (cm, pixels) in [
            ((60, 34), (2560, 1440)),
            ((60, 41), (3000, 5125)),
            ((103, 127), (2575, 2575)),
            ((13, 13), (5000, 100)),
            ((1, 34), (2560, 1440)),
            ((0, 34), (2560, 1440)),
        ] {
            let bytes = edid(cm);
            let mm = Edid::parse(&bytes).and_then(|e| e.mm());
            let console = mm
                .map(|mm| Density::of(pixels, mm))
                .filter(Density::measured)
                .map(|d| d.dpi().1.round());
            let tool = Display { pixels, mm }.dpi().map(f64::from);
            assert_eq!(tool, console, "{cm:?} at {pixels:?}");
        }
        // the figure itself: 5125 rows over 410 mm is 317.5 dpi, which
        // rounds to 318
        let panel = Display {
            pixels: (3000, 5125),
            mm: Edid::parse(&edid((60, 41))).and_then(|e| e.mm()),
        };
        assert_eq!(panel.dpi(), Some(318));
    }

    #[test]
    fn the_first_mode_is_the_preferred_one() {
        assert_eq!(first_mode("1920x1080\n1280x720\n"), Some((1920, 1080)));
        assert_eq!(first_mode("1920x1080i\n"), Some((1920, 1080)));
        assert_eq!(first_mode(""), None);
    }
}
