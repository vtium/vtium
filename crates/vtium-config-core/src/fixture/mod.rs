//! The machines that the TUI's crates test their screens on, each as a
//! [`crate::ui::Shared`] over a desk kept in memory, so a test opens a
//! screen with no file, no terminal and no clock. [`padnoir`] is the
//! screen map's machine: two displays, a keyboard and a power button, and
//! a configuration with one change not saved. [`large`] is a machine with
//! ten graphics cards' worth of displays and inputs, many configurations
//! and a long installed history, on which every list must scroll and stay
//! readable. Building either twice gives the same state.
//!
//! References:
//! - ADR-1206 D10, "Tests at both ends": each card that rebuilds a tab tests its screens with the screen map's fixture and with a large fixture of many displays, inputs and fonts.

pub mod found;
pub mod large;
pub mod padnoir;

use std::path::PathBuf;

use crate::batch::{Folders, Start, View};
use crate::document::{Base, Content, Document, Documents, InMemory, Installed};
use crate::ui::Shared;
use crate::ui::shared::FontReads;

/// The folder of drafts that a fixture's check names each draft in.
pub const DRAFTS: &str = "/home/user/.config/vtium/drafts";

/// The path a fixture's check names the draft `name` by.
pub fn shown(name: &str) -> PathBuf {
    PathBuf::from(format!("{DRAFTS}/{name}.conf"))
}

/// What the walk of the fonts in use reads on a fixture's machine, whose
/// scan is `found`: a batch file that activates each font the scan marks
/// as the batch file's, and no list of a package or of root's on disk, no
/// layers and no kbd folder, so the walk reads no file of the machine it
/// runs on. A font a package's list activates is not active in the walk.
fn reads(found: &crate::found::Found) -> FontReads {
    let batch: String = found
        .fonts
        .files()
        .iter()
        .filter(|f| f.resource.origin == crate::found::Origin::BatchFile)
        .map(|f| format!("{}\n", f.path().display()))
        .collect();
    let start = Start {
        fresh: vtium_settings::batch::Batch::default(),
        folders: Folders {
            package: PathBuf::from("/nonexistent/vtium-fixture/package-lists"),
            admin: PathBuf::from("/nonexistent/vtium-fixture/batch.d"),
        },
        trust: vtium_settings::Trust::running_as(0),
    };
    FontReads {
        view: View::new(Some(batch.as_bytes()), &start),
        layers: None,
        kbd_dirs: Vec::new(),
        euid: 1000,
    }
}

/// The shared state of a machine whose installed vtium.conf is
/// `installed`, with the installed font sets `frits`, where the
/// configuration `name` is open and holds `text`, on the desk `desk`, with
/// the devices `present`.
fn machine(
    name: &str,
    text: &str,
    installed: &str,
    frits: std::collections::BTreeMap<String, Vec<u8>>,
    desk: InMemory,
    present: crate::flowmodel::Present,
) -> Shared {
    let content = Content {
        text: text.to_owned(),
        base: Base::of(Some(installed)),
        ..Content::default()
    };
    let desk = desk.with_draft(name, content.clone());
    let document = Document::new(
        name,
        content,
        Installed {
            text: Some(installed.to_owned()),
            frits,
        },
    );
    Shared::new(Documents::new(document, Box::new(desk)), present)
}
