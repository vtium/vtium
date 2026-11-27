//! The displays as the TUI and `preview` see them, read from sysfs without
//! opening a card: each connected connector's name, its card's udev
//! `ID_PATH`, the modes it offers, and the physical size its EDID states,
//! from which the density a size's picture is drawn at comes (ADR-1201 D4,
//! in ADR-1200 D17's order: vtium.conf's `display-dpi`, else this, else
//! 96).
//!
//! A display runs at the mode that vtium.conf's `mode` lines set for it,
//! where it offers a mode of that size, and at its preferred mode, the
//! first one sysfs lists, otherwise ([`Display::runs_at`]). vtium-config
//! picks the line as vtium does: one naming the card and the connector,
//! then one naming the connector, then one naming neither. sysfs lists
//! each mode's size and not its refresh rate, so a line's rate decides
//! nothing here.
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
use vtium_settings::{key_line, split};

/// The bytes read of a connector's `edid`: its base block, all that
/// [`Edid::parse`] reads.
const EDID_READ: u64 = 128;

/// The most bytes read of a connector's `modes`.
const MODES_MAX: u64 = 4096;

/// The longest card or connector name in brackets that a `mode` line may
/// give, in bytes. vtium refuses a longer name, so vtium-config leaves
/// that line out as vtium does.
const MODE_NAME_MAX: usize = 63;

/// The largest side of a mode in a `mode` line, in pixels: the most that
/// a DRM mode's 16-bit width and height hold. vtium refuses a larger side,
/// so vtium-config leaves that line out as vtium does.
const MODE_SIDE_MAX: u32 = 65_535;

/// The length of a PCI device's address in sysfs, `DDDD:BB:DD.F`.
const PCI_ADDRESS_LEN: usize = 12;

/// A connected display: its preferred mode in pixels, its physical size in
/// millimetres where its EDID states one, its connector's name, its card's
/// udev `ID_PATH` where sysfs shows it, and the size of each mode it
/// offers, preferred first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub pixels: (u32, u32),
    pub mm: Option<(u32, u32)>,
    /// The connector's name, as `DP-1` or `HDMI-A-1`.
    pub connector: String,
    /// udev's `ID_PATH` of the card, as `pci-0000:03:00.0`.
    pub card: Option<String>,
    /// The size of each mode the display offers, in sysfs's order.
    pub modes: Vec<(u32, u32)>,
}

/// One `mode` line of vtium.conf, `[GPU][CONNECTOR]WxH[@RATE]`: the card
/// and the connector it is for, `None` for any, and the size it sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeLine {
    pub card: Option<String>,
    pub connector: Option<String>,
    pub size: (u32, u32),
}

impl Display {
    /// A display that offers only `pixels`, with no name, as the tests
    /// and an assumed display have.
    pub fn of(pixels: (u32, u32), mm: Option<(u32, u32)>) -> Display {
        Display {
            pixels,
            mm,
            connector: String::new(),
            card: None,
            modes: vec![pixels],
        }
    }

    /// Its density down the screen at `pixels`, in whole dots per inch,
    /// where the console measures one from its size ([`Density::of`]); a
    /// display with no size, or with a side whose density the console
    /// does not believe, has none, and the console assumes 96.
    pub fn dpi_at(&self, pixels: (u32, u32)) -> Option<u32> {
        let density = Density::of(pixels, self.mm?);
        let down = density.measured().then(|| density.dpi().1)?;
        format!("{:.0}", down.round()).parse().ok()
    }

    /// Its density down the screen at its preferred mode, as
    /// [`Display::dpi_at`] measures it.
    pub fn dpi(&self) -> Option<u32> {
        self.dpi_at(self.pixels)
    }

    /// The size in pixels of the mode the display runs at under the
    /// vtium.conf `conf`: the size of the `mode` line that vtium picks for
    /// it, where the display offers a mode of that size, else its
    /// preferred mode.
    pub fn runs_at(&self, conf: Option<&str>) -> (u32, u32) {
        let lines = conf.map(mode_lines).unwrap_or_default();
        let card = self.card.as_deref();
        let connector = Some(self.connector.as_str());
        let both = |m: &&ModeLine| {
            m.card.is_some() && m.card.as_deref() == card && m.connector.as_deref() == connector
        };
        let connector_only =
            |m: &&ModeLine| m.card.is_none() && m.connector.as_deref() == connector;
        let any = |m: &&ModeLine| m.card.is_none() && m.connector.is_none();
        let line = lines
            .iter()
            .find(both)
            .or_else(|| lines.iter().find(connector_only))
            .or_else(|| lines.iter().find(any));
        line.map(|l| l.size)
            .filter(|size| self.modes.contains(size))
            .unwrap_or(self.pixels)
    }
}

/// The `mode` lines at the top level of the vtium.conf `text`, in the
/// file's order. A line whose value vtium does not read as a mode is left
/// out, as vtium ignores it.
pub fn mode_lines(text: &str) -> Vec<ModeLine> {
    split(text.as_bytes())
        .top_lines()
        .filter_map(|(_, line)| {
            let read = key_line(line)?;
            if read.key != b"mode" {
                return None;
            }
            mode_line(std::str::from_utf8(&read.value?).ok()?)
        })
        .collect()
}

/// A `mode` line's value, `[GPU][CONNECTOR]WxH[@RATE]`, read as vtium
/// reads it: each side a whole number from 1 to [`MODE_SIDE_MAX`], a rate
/// a whole number of Hz, and a name in brackets neither empty nor longer
/// than [`MODE_NAME_MAX`] bytes.
fn mode_line(text: &str) -> Option<ModeLine> {
    let name = |s: &str| (!s.is_empty() && s.len() <= MODE_NAME_MAX).then(|| s.to_owned());
    let (card, connector, dims) = match text.split_once('[') {
        Some((card, rest)) => {
            let (connector, dims) = rest.split_once(']')?;
            let card = if card.is_empty() {
                None
            } else {
                Some(name(card)?)
            };
            (card, Some(name(connector)?), dims)
        }
        None => (None, None, text),
    };
    let number = |s: &str| {
        s.parse::<u32>()
            .ok()
            .filter(|_| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    };
    let size = match dims.split_once('@') {
        Some((size, rate)) => {
            number(rate)?;
            size
        }
        None => dims,
    };
    let (w, h) = size.split_once('x')?;
    let side = |s: &str| number(s).filter(|n| (1..=MODE_SIDE_MAX).contains(n));
    Some(ModeLine {
        card,
        connector,
        size: (side(w)?, side(h)?),
    })
}

/// The size of each mode a connector's `modes` lists, one `WIDTHxHEIGHT`
/// a line, an interlaced mode's `i` after it ignored, in the order listed.
/// The kernel lists the preferred mode first.
fn all_modes(text: &str) -> Vec<(u32, u32)> {
    text.lines()
        .filter_map(|line| {
            let (w, h) = line.trim().trim_end_matches('i').split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .collect()
}

/// The first mode a connector's `modes` lists.
#[cfg(test)]
fn first_mode(text: &str) -> Option<(u32, u32)> {
    all_modes(text).first().copied()
}

/// udev's `ID_PATH` of the card whose sysfs directory is `card`, as udev's
/// `path_id` names the two kinds of card a console draws on: `pci-` and
/// the PCI device's address, or `platform-` and the platform device's
/// name. A card on any other bus has none here, so only the `mode` lines
/// that name no card apply to it.
fn card_path(card: &Path) -> Option<String> {
    let device = std::fs::canonicalize(card.join("device")).ok()?;
    let name = device.file_name()?.to_str()?;
    let pci = |n: &str| {
        let b = n.as_bytes();
        b.len() == PCI_ADDRESS_LEN
            && b.get(4) == Some(&b':')
            && b.get(7) == Some(&b':')
            && b.get(10) == Some(&b'.')
    };
    if pci(name) {
        return Some(format!("pci-{name}"));
    }
    let platform = device.components().any(|c| c.as_os_str() == "platform");
    platform.then(|| format!("platform-{name}"))
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
            let dir = class.join(&name);
            let status = read(&dir.join("status"), 64)?;
            if String::from_utf8_lossy(&status).trim() != "connected" {
                return None;
            }
            let modes = read(&dir.join("modes"), MODES_MAX)?;
            let modes = all_modes(&String::from_utf8_lossy(&modes));
            let pixels = *modes.first()?;
            let mm = read(&dir.join("edid"), EDID_READ)
                .and_then(|b| Edid::parse(&b))
                .and_then(|e| e.mm());
            let (card, connector) = name.split_once('-')?;
            Some(Display {
                pixels,
                mm,
                connector: connector.to_owned(),
                card: card_path(&class.join(card)),
                modes,
            })
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
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
        let d = Display::of((2560, 1440), Some((600, 340)));
        assert_eq!(d.dpi(), Some(108));
        let none = Display::of((2560, 1440), None);
        assert_eq!(none.dpi(), None);
        let projector = Display::of((1920, 1080), Some((10, 10)));
        assert_eq!(projector.dpi(), None, "past DPI_MAX");
        // one side out of bounds is a misreport, as the console takes it
        let one_side = Display::of((2560, 1440), Some((10, 340)));
        assert_eq!(one_side.dpi(), None, "across past DPI_MAX");
    }

    /// The tool's density and the console's are one for the same EDID
    /// bytes: the size read by vtium-platform's reader, the density by its
    /// `Density::of`, rounded down the screen. A 41 cm high panel of 5125
    /// rows is 317.5 dpi down, which a reader computing it another way can
    /// take as 317.49999 and round to 317.
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
            let tool = Display::of(pixels, mm).dpi().map(f64::from);
            assert_eq!(tool, console, "{cm:?} at {pixels:?}");
        }
        // the figure itself: 5125 rows over 410 mm is 317.5 dpi, which
        // rounds to 318
        let panel = Display::of(
            (3000, 5125),
            Edid::parse(&edid((60, 41))).and_then(|e| e.mm()),
        );
        assert_eq!(panel.dpi(), Some(318));
    }

    #[test]
    fn the_first_mode_is_the_preferred_one() {
        assert_eq!(first_mode("1920x1080\n1280x720\n"), Some((1920, 1080)));
        assert_eq!(first_mode("1920x1080i\n"), Some((1920, 1080)));
        assert_eq!(first_mode(""), None);
    }

    /// The size in pixels of a full 8K display, which the tests of large
    /// displays use.
    const EIGHT_K: (u32, u32) = (7680, 4320);

    /// The smaller mode that the tests' 8K display prefers.
    const PREFERRED: (u32, u32) = (5120, 1440);

    /// An 8K display, DP-1 on the card at `pci-0000:03:00.0`, preferring
    /// [`PREFERRED`] and offering [`EIGHT_K`] too.
    fn eight_k() -> Display {
        Display {
            pixels: PREFERRED,
            mm: None,
            connector: "DP-1".to_owned(),
            card: Some("pci-0000:03:00.0".to_owned()),
            modes: vec![PREFERRED, EIGHT_K, (1920, 1080)],
        }
    }

    #[test]
    fn a_display_runs_at_the_mode_vtium_conf_sets_for_it() {
        let d = eight_k();
        assert_eq!(d.runs_at(None), PREFERRED, "no file: preferred");
        assert_eq!(d.runs_at(Some("font-size=12\n")), PREFERRED);
        assert_eq!(d.runs_at(Some("mode=[DP-1]7680x4320@120\n")), EIGHT_K);
        assert_eq!(
            d.runs_at(Some("mode = 7680x4320\n")),
            EIGHT_K,
            "any display"
        );
        // a line for the card and the connector wins over one for the
        // connector, which wins over one for any display
        let three = "mode=1920x1080\nmode=[DP-1]7680x4320\nmode=pci-0000:03:00.0[DP-1]5120x1440\n";
        assert_eq!(d.runs_at(Some(three)), PREFERRED);
        let two = "mode=1920x1080\nmode=[DP-1]7680x4320\n";
        assert_eq!(d.runs_at(Some(two)), EIGHT_K);
        // a line for another connector or card does not apply
        assert_eq!(d.runs_at(Some("mode=[HDMI-A-1]1920x1080\n")), PREFERRED);
        assert_eq!(
            d.runs_at(Some("mode=pci-0000:04:00.0[DP-1]1920x1080\n")),
            PREFERRED
        );
        // a size the display does not offer leaves it at its preferred mode
        assert_eq!(d.runs_at(Some("mode=[DP-1]3840x2160\n")), PREFERRED);
        // a line in a section, or one vtium does not read, is no mode line
        assert_eq!(
            d.runs_at(Some("[head desk]\nmode=[DP-1]7680x4320\n")),
            PREFERRED
        );
        assert_eq!(d.runs_at(Some("mode=[DP-1]7680x4320@fast\n")), PREFERRED);
        assert_eq!(d.runs_at(Some("mode=[]7680x4320\n")), PREFERRED);
        assert_eq!(d.runs_at(Some("mode=[DP-1]7680x0\n")), PREFERRED);
    }

    #[test]
    fn mode_lines_read_as_vtium_reads_them() {
        let lines = mode_lines("mode=gpu[DP-1]7680x4320@120 # mine\nmode=640x480@0\nmode=x\n");
        assert_eq!(
            lines,
            [
                ModeLine {
                    card: Some("gpu".to_owned()),
                    connector: Some("DP-1".to_owned()),
                    size: EIGHT_K,
                },
                ModeLine {
                    card: None,
                    connector: None,
                    size: (640, 480),
                },
            ]
        );
    }

    /// A name or a side past vtium's bound leaves the line out, and one at
    /// the bound keeps it.
    #[test]
    fn mode_lines_keep_vtium_s_bounds() {
        let name = |n: usize| "c".repeat(n);
        let side = MODE_SIDE_MAX;
        let text = format!(
            "mode=[{}]640x480\nmode=[{}]800x600\nmode={side}x480\nmode={}x480\n",
            name(MODE_NAME_MAX),
            name(MODE_NAME_MAX + 1),
            side + 1
        );
        let sizes: Vec<(u32, u32)> = mode_lines(&text).iter().map(|l| l.size).collect();
        assert_eq!(sizes, [(640, 480), (side, 480)]);
    }

    #[test]
    fn displays_name_their_connector_card_and_modes() {
        let root =
            std::env::temp_dir().join(format!("vtium-config-display-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let device = root.join("devices/pci0000:00/0000:00:01.0/0000:03:00.0");
        let class = root.join("class/drm");
        let dp = class.join("card1-DP-1");
        std::fs::create_dir_all(&device).expect("device");
        std::fs::create_dir_all(class.join("card1")).expect("card");
        std::fs::create_dir_all(&dp).expect("connector");
        std::os::unix::fs::symlink(&device, class.join("card1/device")).expect("link");
        std::fs::write(dp.join("status"), "connected\n").expect("status");
        std::fs::write(dp.join("modes"), "5120x1440\n7680x4320\n1920x1080i\n").expect("modes");
        let found = displays(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            found,
            [Display {
                pixels: PREFERRED,
                mm: None,
                connector: "DP-1".to_owned(),
                card: Some("pci-0000:03:00.0".to_owned()),
                modes: vec![PREFERRED, EIGHT_K, (1920, 1080)],
            }]
        );
    }
}
