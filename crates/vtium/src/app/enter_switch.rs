//! Enter on a head's own keyboard (ADR-1401 D3, "Enter on a head's own
//! keyboard"). On a VT where a head does not run, a keyboard its rules tie
//! to it types nowhere, and its displays show a notice that names its VT.
//! When the head sets `switch-on-enter = yes`, under the open policy and
//! for a head that is not isolated, Enter on such a keyboard switches to
//! the first VT of the head's `vt` on which vtium runs, a valid VT, and
//! the notice says so. Any other key on it only wakes the notice, but its
//! VT keys switch the VT as every keyboard's do (D3, "Who switches VTs").
//!
//! vtium asks which VTs vtium runs on the way `vtium console status` does
//! ([`crate::console::running_vts`]): when it draws a notice, and again at
//! each Enter. It switches through the VT's own keys, the path
//! `Ctrl-Alt-F<n>` takes, so a valid VT is one of VTs 1 to 12, the VTs
//! those keys name. Nothing here runs from a timer or retries.

use std::collections::BTreeSet;
use std::path::Path;

use vtium_platform::input::{KeyEvent, Mods as InputMods};

use super::App;
use crate::console::Mode;

/// Which VTs vtium runs on now, and the mode the command that enables
/// another VT names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VtiumVts {
    pub running: BTreeSet<u16>,
    /// The mode of the VTs vtium is enabled on
    /// ([`crate::console::Enabled::mode`]), direct mode where systemd's
    /// links say nothing.
    pub mode: Mode,
}

/// Asks which VTs vtium runs on now, or why it cannot tell; the tests
/// stand in for it.
pub type RunningVts = Box<dyn FnMut() -> Result<VtiumVts, String>>;

/// The running system's answer: systemd's running units, as `vtium
/// console status` reads them, and the mode of its links under `/`.
pub(super) fn running_vts() -> Result<VtiumVts, String> {
    let running = crate::console::running_vts(&mut crate::console::RunningSystemd)?;
    let mode = crate::console::enabled_vts(Path::new("/")).map_or(Mode::Direct, |e| e.mode);
    Ok(VtiumVts { running, mode })
}

/// `Return` and the keypad's `Enter`.
const ENTERS: [u32; 2] = [0xff0d, 0xff8d];

/// `XF86Switch_VT_1`: the VT keys' keysym for VT 1, whose next eleven
/// name VTs 2 to 12.
const SWITCH_VT_1: u32 = 0x1008_fe01;

/// The last VT a VT key names, `Ctrl-Alt-F12` and `XF86Switch_VT_12`.
const LAST_KEYED_VT: u16 = 12;

/// Whether `key` is a press of Enter with no Ctrl, Alt or Super held.
fn is_enter(key: &KeyEvent) -> bool {
    let held = [InputMods::CTRL, InputMods::ALT, InputMods::LOGO];
    !key.repeat
        && key.keysyms.first().is_some_and(|k| ENTERS.contains(k))
        && !held.iter().any(|&m| key.mods.contains(m))
}

/// The VT key that switches to VT `n`, one of VTs 1 to 12.
fn switch_key(n: u16) -> KeyEvent {
    KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        base: None,
        mods: InputMods::empty(),
        keysyms: vec![SWITCH_VT_1 + u32::from(n.saturating_sub(1))],
        codepoints: vec![None],
        repeat: false,
    }
}

/// Where Enter on a keyboard of a head takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Reach {
    /// To this VT, the first of the head's VTs on which vtium runs.
    To(u16),
    /// Nowhere: vtium runs on none of these VTs of the head's `vt`, the
    /// ones a VT key names; the command that enables one names this mode.
    Enable(Vec<u16>, Mode),
    /// Nowhere, and the notice says nothing of it: no VT of the head's
    /// `vt` is one a VT key names, or vtium cannot tell which VTs run
    /// vtium.
    Unknown,
}

impl App {
    /// Asks `running` in place of systemd which VTs vtium runs on.
    pub fn set_running_vts(&mut self, running: RunningVts) {
        self.running_vts = running;
    }

    /// Where Enter on a keyboard of the head at `head` of the file's heads
    /// takes it, with `running` the answer to which VTs vtium runs on, or
    /// `None` to ask now.
    pub(super) fn reach(
        &mut self,
        head: usize,
        running: &mut Option<Result<VtiumVts, String>>,
    ) -> Reach {
        let keyed: Vec<u16> = self
            .rules
            .heads
            .get(head)
            .map(|h| {
                h.vts
                    .iter()
                    .copied()
                    .filter(|n| (1..=LAST_KEYED_VT).contains(n))
                    .collect()
            })
            .unwrap_or_default();
        if keyed.is_empty() {
            return Reach::Unknown;
        }
        let asked = running.get_or_insert_with(|| (self.running_vts)());
        match asked {
            Ok(vts) => keyed
                .iter()
                .find(|n| vts.running.contains(n))
                .map_or(Reach::Enable(keyed.clone(), vts.mode), |&n| Reach::To(n)),
            Err(_) => Reach::Unknown,
        }
    }

    /// The text of the notice of the display `connector` on VT `vt`, which
    /// is configured for the head at `owner` of the file's heads, if any
    /// (ADR-1401 D3): what [`crate::notice::unused_display`] says, and,
    /// while a keyboard of a head that switches on Enter is present, the
    /// line that says where Enter takes it.
    pub(super) fn unused_text(
        &mut self,
        vt: u16,
        connector: &str,
        owner: Option<usize>,
        running: &mut Option<Result<VtiumVts, String>>,
    ) -> String {
        let named = owner
            .and_then(|h| self.rules.heads.get(h))
            .map(|head| crate::notice::Owner {
                head: &head.name,
                vts: &head.vts,
            });
        let text = crate::notice::unused_display(vt, connector, named.as_ref());
        let switches =
            owner.filter(|&h| self.rules.switch_on_enter(h) && self.routes.keyboard_elsewhere(h));
        let Some(h) = switches else {
            return text;
        };
        match self.reach(h, running) {
            Reach::To(n) => format!("{text}\n{}", crate::notice::enter_switches(n)),
            Reach::Enable(vts, mode) => {
                format!("{text}\n{}", crate::notice::enable_to_switch(&vts, mode))
            }
            Reach::Unknown => text,
        }
    }

    /// A key from the keyboard at `node`, which stays with the head at
    /// `head` of the file's heads on another VT. Enter switches to that
    /// head's first valid VT where its `switch-on-enter` lets it; where
    /// vtium runs on none of its VTs, it switches nothing, and the notices
    /// say which VT to enable. Any key wakes the notices.
    pub(super) fn key_elsewhere(&mut self, head: usize, node: Option<&Path>, key: &KeyEvent) {
        if !is_enter(key) || !self.rules.switch_on_enter(head) {
            return;
        }
        let mut running = None;
        let reach = self.reach(head, &mut running);
        let name = self
            .rules
            .heads
            .get(head)
            .map_or("", |h| h.name.as_str())
            .to_owned();
        let keyboard = node.map_or_else(String::new, |n| n.display().to_string());
        match reach {
            Reach::To(n) => {
                log::info!("Enter on keyboard {keyboard} switches to VT {n}, where {name} runs.");
                if !self.vt.key(&switch_key(n)) {
                    log::warn!(
                        "Enter on keyboard {keyboard} cannot switch to VT {n}: this VT is not shown."
                    );
                }
            }
            Reach::Enable(vts, mode) => {
                log::warn!(
                    "Enter on keyboard {keyboard} switches nowhere: {}",
                    crate::notice::enable_to_switch(&vts, mode)
                );
                // The notices say which VT to enable at the guards' next
                // run, which this answer settles.
                self.guard_due = true;
            }
            Reach::Unknown => match running {
                Some(Err(why)) => log::warn!(
                    "Enter on keyboard {keyboard} switches nowhere: vtium cannot tell which VTs vtium runs on, because {why}"
                ),
                _ => log::warn!(
                    "Enter on keyboard {keyboard} switches nowhere: no VT of {name} is one Ctrl-Alt and a function key can switch to."
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(sym: u32, mods: InputMods, repeat: bool) -> KeyEvent {
        KeyEvent {
            keycode: 0,
            ascii: 0,
            unshifted: 0,
            base: None,
            mods,
            keysyms: vec![sym],
            codepoints: vec![None],
            repeat,
        }
    }

    #[test]
    fn enter_is_return_or_the_keypads_enter_pressed_without_ctrl_alt_or_super() {
        assert!(is_enter(&key(0xff0d, InputMods::empty(), false)));
        assert!(is_enter(&key(0xff8d, InputMods::SHIFT, false)));
        assert!(!is_enter(&key(0xff0d, InputMods::empty(), true)));
        assert!(!is_enter(&key(0xff0d, InputMods::CTRL, false)));
        assert!(!is_enter(&key(0xff0d, InputMods::ALT, false)));
        assert!(!is_enter(&key(0xff0d, InputMods::LOGO, false)));
        assert!(!is_enter(&key(u32::from('a'), InputMods::empty(), false)));
    }

    #[test]
    fn the_switch_key_names_its_vt() {
        assert_eq!(switch_key(1).keysyms, [0x1008_fe01]);
        assert_eq!(switch_key(12).keysyms, [0x1008_fe0c]);
    }
}
