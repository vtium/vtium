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

use crate::document::{Base, Content, Document, Documents, InMemory, Installed};
use crate::ui::Shared;

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
