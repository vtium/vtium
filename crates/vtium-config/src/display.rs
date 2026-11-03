//! The displays as the TUI and `preview` see them, read from sysfs without
//! opening a card: each connected connector's first mode and the physical
//! size its EDID states, from which the density a size's picture is drawn
//! at comes (ADR-1201 D4, in ADR-1200 D17's order: vtium.conf's
//! `display-dpi`, else this, else 96).
//!
//! The console takes a display's physical size from its DRM connector,
//! which the kernel fills from the EDID, else from the EDID in sysfs
//! (vtium-platform's `display::edid`); vtium-config opens no card, so it
//! reads the EDID's own size, bytes 21 and 22 in centimetres, which can
//! differ from the connector's millimetres by under a centimetre a side.
//! Its bounds on a believable density are vtium-platform's `DPI_MIN` and
//! `DPI_MAX`.

use std::io::Read;
use std::path::Path;

use vtium_platform::display::{DPI_MAX, DPI_MIN};

/// The bytes of an EDID's base block.
const BASE: usize = 128;

/// The eight bytes every EDID starts with.
const HEADER: [u8; 8] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

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
    /// Its density down the screen, in whole dots per inch, where its size
    /// gives a density inside the bounds the console believes on both
    /// axes, as the console takes a display's density; a display with a
    /// side out of bounds, a misreport, has none, and the console assumes
    /// 96.
    pub fn dpi(&self) -> Option<u32> {
        let (w, h) = self.mm?;
        let axis = |px: u32, mm: u32| {
            (mm > 0)
                .then(|| f64::from(px) * 25.4 / f64::from(mm))
                .filter(|dpi| (DPI_MIN..=DPI_MAX).contains(dpi))
        };
        axis(self.pixels.0, w)?;
        let down = axis(self.pixels.1, h)?;
        format!("{:.0}", down.round()).parse().ok()
    }
}

/// The physical size an EDID's base block states, in millimetres, `None`
/// for bytes that are no EDID, whose checksum fails, or that state no size.
pub fn edid_mm(bytes: &[u8]) -> Option<(u32, u32)> {
    let base = bytes.get(..BASE)?;
    if base.get(..HEADER.len())? != HEADER
        || base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != 0
    {
        return None;
    }
    let (w, h) = (*base.get(21)?, *base.get(22)?);
    (w != 0 && h != 0).then(|| (u32::from(w) * 10, u32::from(h) * 10))
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
            let mm =
                read(&dir.join("edid"), u64::try_from(BASE).unwrap_or(0)).and_then(|b| edid_mm(&b));
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
        let mut b = vec![0u8; BASE];
        b[..8].copy_from_slice(&HEADER);
        b[21] = cm.0;
        b[22] = cm.1;
        let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
        b[127] = 0u8.wrapping_sub(sum);
        b
    }

    #[test]
    fn an_edid_states_its_size_in_centimetres() {
        assert_eq!(edid_mm(&edid((60, 34))), Some((600, 340)));
        assert_eq!(edid_mm(&edid((0, 34))), None, "no size");
        let mut bad = edid((60, 34));
        bad[127] ^= 1;
        assert_eq!(edid_mm(&bad), None, "a checksum that fails");
        assert_eq!(edid_mm(&[0; 64]), None, "too short");
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

    #[test]
    fn the_first_mode_is_the_preferred_one() {
        assert_eq!(first_mode("1920x1080\n1280x720\n"), Some((1920, 1080)));
        assert_eq!(first_mode("1920x1080i\n"), Some((1920, 1080)));
        assert_eq!(first_mode(""), None);
    }
}
