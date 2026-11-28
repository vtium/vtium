//! A large machine: ten graphics cards with four displays each, thirty
//! keyboards and thirty pointers, twelve configurations and thirty
//! versions in the installed history. Its vtium.conf gives each card a
//! head and a look and places every display and every input by a rule of
//! its own, so the Flow page, the Problems list, the list box and every
//! list have dozens of rows to scroll. Every name and number is made from
//! its place in a loop, so the machine is the same each time it is built.
//!
//! References:
//! - ADR-1206 D10, "Readable on a large machine": dozens of displays, keyboards and pointers, as ten GPUs' outputs give.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use vtium_settings::{Class, Edid, Identity};

use crate::document::{Base, Content, InMemory, Version};
use crate::flowmodel::Present;
use crate::ui::Shared;

/// How many graphics cards the machine has.
pub const CARDS: usize = 10;

/// How many displays each card drives.
pub const DISPLAYS_PER_CARD: usize = 4;

/// How many keyboards, and how many pointers, are plugged in.
pub const KEYBOARDS: usize = 30;
pub const POINTERS: usize = 30;

/// How many configurations the store keeps, the open one among them.
pub const CONFIGURATIONS: usize = 12;

/// How many versions the installed history keeps.
pub const VERSIONS: usize = 30;

/// The open configuration's name.
pub const NAME: &str = "lab";

/// The first version's time, in seconds since 1970: 2026-11-01 at 00:00
/// UTC. Each later version is an hour newer.
const FIRST_WHEN: u64 = 1_793_491_200;

/// The udev path of card `card`.
fn card_path(card: usize) -> String {
    format!("pci-0000:{:02x}:00.0", card.saturating_add(0x10))
}

/// The udev path of the input `n`'s USB port.
fn port_path(n: usize) -> String {
    format!("pci-0000:40:00.0-usb-0:{}.{}", n / 8 + 1, n % 8 + 1)
}

/// The installed vtium.conf: a head and a look for each card, a rule for
/// each display and each input, and labels for the first displays.
pub fn installed() -> String {
    let mut out = String::from("font-size=12\nmouse\nversion = 1\n");
    for card in 0..CARDS {
        let _ = write!(
            out,
            "[look card{card}]\nfont-name = proof\nfont-size = {}\n",
            10 + card % 5
        );
    }
    for card in 0..CARDS {
        let _ = write!(out, "[head gpu{card}]\nlook = card{card}\n");
    }
    out.push_str("[rules]\n");
    for card in 0..CARDS {
        for port in 1..=DISPLAYS_PER_CARD {
            let _ = writeln!(out, "display connector:card{card}/DP-{port} -> gpu{card}");
        }
    }
    for n in 0..KEYBOARDS.saturating_add(POINTERS) {
        let _ = writeln!(out, "input path:{} -> gpu{}", port_path(n), n % CARDS);
    }
    out.push_str("input * -> ignore\n[labels]\n");
    for card in 0..CARDS {
        let _ = writeln!(out, "connector:card{card}/DP-1 = Card {card}, left");
    }
    out
}

/// The installed font sets: twenty, each drawing a built-in font.
pub fn frits() -> BTreeMap<String, Vec<u8>> {
    (0..20)
        .map(|n| {
            (
                format!("set-{n:02}"),
                format!("source=builtin:unifont\n# set {n}\n").into_bytes(),
            )
        })
        .collect()
}

/// The devices present: every display of every card, each with an EDID of
/// its own, then the keyboards and the pointers, each on a USB port of its
/// own.
pub fn present() -> Present {
    let mut devices = Vec::new();
    for card in 0..CARDS {
        for port in 1..=DISPLAYS_PER_CARD {
            let mut d = Identity::display();
            d.connector = Some(format!("DP-{port}"));
            d.card = Some(card_path(card));
            d.edid = Some(Edid {
                maker: if card % 2 == 0 { "AAA" } else { "BBB" }.to_owned(),
                product: u16::try_from(card.saturating_mul(10).saturating_add(port)).unwrap_or(0),
                serial: format!("PANEL{card:02}{port:02}"),
            });
            d.card_order = u32::try_from(card).unwrap_or(0);
            d.order = u32::try_from(port).unwrap_or(0);
            devices.push(d);
        }
    }
    for n in 0..KEYBOARDS.saturating_add(POINTERS) {
        let (class, kind) = if n < KEYBOARDS {
            (Class::Keyboard, "Keyboard")
        } else {
            (Class::Mouse, "Mouse")
        };
        let mut d = Identity::input().with_class(class);
        d.name = Some(format!("{kind} {n:02}"));
        d.path = Some(port_path(n));
        d.id = Some((
            0x046d,
            u16::try_from(0xc300_usize.saturating_add(n)).unwrap_or(0),
        ));
        devices.push(d);
    }
    Present {
        devices,
        udev: true,
    }
}

/// The large machine, with lab open and one size changed in it.
pub fn shared() -> Shared {
    let installed = installed();
    let open = installed.replacen("font-size = 10", "font-size = 11", 1);
    let mut desk = InMemory::new();
    for n in 1..CONFIGURATIONS {
        let text = format!("# configuration {n}\n{installed}");
        desk = desk.with_draft(
            &format!("lab-{n:02}"),
            Content {
                text,
                base: Base::of(Some(&installed)),
                ..Content::default()
            },
        );
    }
    // newest first, as the installed history lists them
    for number in (1..=VERSIONS).rev() {
        let n = u64::try_from(number).unwrap_or(0);
        desk = desk.with_version(Version {
            draft: NAME.to_owned(),
            number: u32::try_from(number).unwrap_or(0),
            when: FIRST_WHEN.saturating_add(n.saturating_mul(3600)),
            text: format!("# version {number}\n{installed}"),
        });
    }
    let mut shared = super::machine(NAME, &open, &installed, frits(), desk, present());
    shared.found = super::found::large();
    let reads = super::reads(&shared.found);
    shared.read_fonts(reads);
    shared
}

/// Records on `shared` what `vtium check-config` finds in lab as the
/// fixture holds it: each look's font set proof needs JetBrains Mono,
/// which the machine lacks, ten warnings that leave Save working.
pub fn checked(shared: &mut Shared) {
    use vtium_settings::fonts_report::{Named, missing};
    let shown = super::shown(NAME);
    let text = shared.document().text().to_owned();
    let problems = text
        .lines()
        .enumerate()
        .filter(|(_, l)| *l == "font-name = proof")
        .map(|(i, _)| {
            format!(
                "{}:{}: {}",
                shown.display(),
                i.saturating_add(1),
                missing("proof", "JetBrains Mono", Named::ByName)
            )
        })
        .collect();
    let checked = crate::drafts::Checked {
        verdict: crate::drafts::Verdict::MissingFonts,
        problems,
        notes: Vec::new(),
    };
    shared.checked(checked, shown);
}
