//! The fake VT, for a system without kernel VTs: always the seat's only
//! session, taken and given up on request or with Ctrl+Super+F12
//! (vt_fake.c). A test gives it a VT number, so its own key follows the
//! kernel VT's rule.

use std::io;

use xkbcommon::xkb::keysyms;

use super::kernel::switch_target;
use super::{Switch, Vt, VtEvent};
use crate::input::{KeyEvent, Mods};

/// A VT with no kernel behind it.
#[derive(Debug, Default)]
pub struct FakeVt {
    /// The VT number it reports, `None` as a system without kernel VTs.
    num: Option<u16>,
    active: bool,
    releasing: bool,
    events: Vec<VtEvent>,
}

impl FakeVt {
    pub fn new() -> FakeVt {
        FakeVt::default()
    }

    /// A fake VT reporting itself as VT `num`, shown while it is ours.
    pub fn with_number(num: u16) -> FakeVt {
        FakeVt {
            num: Some(num),
            ..FakeVt::default()
        }
    }

    fn take(&mut self) {
        if !self.active {
            self.active = true;
            self.events.push(VtEvent::Acquired);
        }
    }

    fn give_up(&mut self) {
        if self.active && !self.releasing {
            self.releasing = true;
            self.events.push(VtEvent::Release);
        }
    }
}

impl Vt for FakeVt {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.drain(..)
    }

    fn acknowledge_release(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.releasing) {
            self.active = false;
        }
        Ok(())
    }

    fn refuse_release(&mut self) -> io::Result<()> {
        self.releasing = false;
        Ok(())
    }

    fn activate(&mut self) -> io::Result<Switch> {
        log::debug!(target: "vt", "activating the fake VT on request");
        self.take();
        Ok(Switch::Pending)
    }

    fn leave(&mut self) -> io::Result<Switch> {
        log::debug!(target: "vt", "deactivating the fake VT on request");
        self.give_up();
        Ok(if self.active {
            Switch::Pending
        } else {
            Switch::Done
        })
    }

    fn wait(&mut self) -> io::Result<Switch> {
        Ok(Switch::Done)
    }

    fn is_active(&self) -> bool {
        self.active && !self.releasing
    }

    fn key(&mut self, key: &KeyEvent) -> bool {
        let toggle = key.mods.contains(Mods::CTRL)
            && key.mods.contains(Mods::LOGO)
            && key.keysyms.first() == Some(&keysyms::KEY_F12);
        if toggle {
            if self.active {
                self.give_up();
            } else {
                self.take();
            }
        }
        toggle
    }

    /// The kernel VT's rule for its number: `Ctrl+Alt+F<n>` or
    /// `XF86Switch_VT_<n>` while it is ours; with no number, none.
    fn is_own_key(&self, key: &KeyEvent) -> bool {
        self.num.is_some() && switch_target(key) == self.num && self.is_active()
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn bell(&mut self) {}

    fn number(&self) -> Option<u16> {
        self.num
    }
}
