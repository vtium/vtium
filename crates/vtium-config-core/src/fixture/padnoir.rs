//! The screen map's machine, as the old TUI's fixture holds it: the
//! installed vtium.conf of the goldens that open a draft, with two heads,
//! a look and three rules; the configuration workstation, which adds one
//! line to that file and is not saved; a second configuration, spare, the
//! same as the installed file; one version in the installed history; five
//! installed font sets, one of which is not UTF-8 text; and the devices of
//! ADR-1403 D7's figure, two displays, a keyboard and a power button.
//!
//! References:
//! - ADR-1403 D7: the figure's machine, from D4's example.

use std::collections::BTreeMap;

use vtium_settings::{Class, Edid, Identity};

use crate::document::{Base, Content, InMemory, Version};
use crate::flowmodel::Present;
use crate::ui::Shared;

/// The installed vtium.conf: two heads, a look, and three rules, the
/// second naming the keyboard at `pci-0:3.3`.
pub const INSTALLED: &str = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\n[head desk]\nlook = big\n[head panel]\n[rules]\ndisplay * -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";

/// The open configuration's name.
pub const NAME: &str = "workstation";

/// The open configuration's vtium.conf: the installed file with one line
/// more, a size for the look big, which is the one change not saved.
pub const DRAFT: &str = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\nfont-size = 11\n[head desk]\nlook = big\n[head panel]\n[rules]\ndisplay * -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";

/// The version in the installed history, which the save before this one
/// replaced.
pub const VERSION: &str = "# mine\nfont-name=unifont\nversion = 1\n[look proof]\nfont-name = proof\nvtlook = sepia\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n";

/// When that version was replaced, in seconds since 1970: 2026-11-27 at
/// 09:00 UTC.
pub const VERSION_WHEN: u64 = 1_795_770_000;

/// The installed font sets, by their names, as their bytes: one that does
/// not read, three that read, and one that is not UTF-8 text.
pub fn frits() -> BTreeMap<String, Vec<u8>> {
    [
        ("broken", &b"source=builtin:unifont\nantialias=red\n"[..]),
        ("minspec-16", b"source=builtin:unifont\n"),
        ("proof", b"source=font:JetBrains Mono\n"),
        ("tiny5", b"source=bdf:/x/Tiny5Mono.bdf\n"),
        ("latin1", b"source=kbd:lat1-16 # \xe9\n"),
    ]
    .into_iter()
    .map(|(n, b)| (n.to_owned(), b.to_vec()))
    .collect()
}

/// The devices present: the displays DP-8 and HDMI-A-2, the keyboard on
/// USB port 3.3, and the power button.
pub fn present() -> Present {
    let monitor = |connector: &str, maker: &str, product: u16, serial: &str| {
        let mut d = Identity::display();
        d.connector = Some(connector.to_owned());
        d.card = Some("pci-0000:01:00.0".to_owned());
        d.edid = Some(Edid {
            maker: maker.to_owned(),
            product,
            serial: serial.to_owned(),
        });
        d
    };
    let mut keyboard = Identity::input().with_class(Class::Keyboard);
    keyboard.name = Some("keyboard".to_owned());
    keyboard.path = Some("pci-0000:15:00.0-usb-0:3.3".to_owned());
    keyboard.id = Some((0x046d, 0xc31c));
    let mut power = Identity::input().with_class(Class::Key);
    power.name = Some("Power Button".to_owned());
    power.path = Some("LNXPWRBN/button/input0".to_owned());
    Present {
        devices: vec![
            monitor("DP-8", "AAA", 1, "LARGE00001"),
            monitor("HDMI-A-2", "BBB", 2, "SMALL00001"),
            keyboard,
            power,
        ],
        udev: true,
    }
}

/// The screen map's machine, with workstation open.
pub fn shared() -> Shared {
    let spare = Content {
        text: INSTALLED.to_owned(),
        base: Base::of(Some(INSTALLED)),
        ..Content::default()
    };
    let desk = InMemory::new()
        .with_draft("spare", spare)
        .with_version(Version {
            draft: NAME.to_owned(),
            number: 1,
            when: VERSION_WHEN,
            text: VERSION.to_owned(),
        });
    let mut shared = super::machine(NAME, DRAFT, INSTALLED, frits(), desk, present());
    shared.found = super::found::padnoir();
    shared
}
