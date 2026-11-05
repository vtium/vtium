//! The devices a seat holds, as vtium-platform lists them from sysfs and
//! udev's database, mapped as the start maps each device it opens, and
//! whether a udev daemon runs to fill that database.

use std::path::Path;

use vtium_platform::listing::Listing;
use vtium_settings::Identity;

use crate::identity::{Card, display, input, unnamed_display};

/// The seat whose devices are listed: the kernel VTs', which vtium runs on.
pub const SEAT: &str = "seat0";

/// The devices `listing` holds as the rules see them, through the mapping
/// the start walks them through: each card's displays with the card and
/// their place among its displays, each framebuffer's display as the start
/// names it, `fb`, then each input.
///
/// A display's place is the lister's connector-id order, which can differ
/// from the order the start registers a card's displays in, and no finding
/// of `vtium check-config` depends on it: a rule matching no device and the
/// layer 1 warnings read only which rule each device walks to, and the
/// place feeds only where the display guard puts a display, which the check
/// does not run.
pub fn devices(listing: &Listing) -> Vec<Identity> {
    let displays = listing.cards.iter().flat_map(|c| {
        let card = Card::at(&c.node, c.id_path.as_deref(), c.primary);
        (0u32..)
            .zip(&c.displays)
            .map(move |(order, d)| display(d, &card, order))
    });
    let framebuffers = listing
        .framebuffers
        .iter()
        .map(|f| unnamed_display("fb", &Card::at(&f.node, None, f.primary), 0));
    displays
        .chain(framebuffers)
        .chain(listing.inputs.iter().map(input))
        .collect()
}

/// The socket a udev daemon listens on, which `udevadm control` talks to.
pub const UDEV_CONTROL: &str = "/run/udev/control";

/// Whether a udev daemon runs: its control socket `control` is present.
/// Without one, the keys that read udev's database match nothing
/// (ADR-1401 D4).
pub fn udev_daemon_runs(control: &Path) -> bool {
    control.exists()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vtium_platform::display::{Edid, Identity as Display};
    use vtium_platform::listing::Card as Listed;
    use vtium_settings::{Place, parse};

    use super::*;

    /// An EDID base block naming maker `AAA`, product 0x0001 and the serial
    /// string `LARGE00001`, with its checksum.
    fn edid() -> Vec<u8> {
        let mut base = vec![0u8; 128];
        base.splice(..8, [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        // `A` is 1 in each five-bit letter of the maker id
        base.splice(8..12, [0x04, 0x21, 0x01, 0x00]);
        base.splice(54..59, [0, 0, 0, 0xff, 0]);
        base.splice(59..70, *b"LARGE00001\n");
        let sum = base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b));
        base.splice(127..128, [0u8.wrapping_sub(sum)]);
        base
    }

    // ADR-1401 D4: a display the lister reads with its EDID is placed by an
    // `edid:` rule naming the EDID's maker, product and serial, and by no
    // rule naming another product or serial
    #[test]
    fn a_listed_displays_edid_places_it_by_maker_product_and_serial() {
        let named = Edid::parse(&edid()).map(|e| e.id().clone());
        assert!(named.is_some(), "the fixture is an EDID");
        let listing = Listing {
            cards: vec![Listed {
                node: PathBuf::from("/dev/dri/card0"),
                id_path: None,
                primary: true,
                displays: vec![Display {
                    connector: "HDMI-A-1".into(),
                    card: None,
                    edid: named,
                    connector_path: None,
                    lit_at_start: false,
                }],
            }],
            ..Listing::default()
        };
        let devices = devices(&listing);
        let place = |rule: &str| {
            parse(&format!(
                "version = 1\n[head desk]\n[rules]\ndisplay {rule} -> desk\n"
            ))
            .map(|p| p.config)
            .unwrap_or_default()
            .walk(devices.first().unwrap_or(&Identity::display()))
            .place
        };
        assert_eq!(devices.len(), 1);
        assert_eq!(place("edid:AAA:0x0001:LARGE00001"), Place::Head(0));
        assert_eq!(place("edid:AAA:0x0001:LARGE00002"), Place::Off);
        assert_eq!(place("edid:AAA:0x0002:LARGE00001"), Place::Off);
    }
}
