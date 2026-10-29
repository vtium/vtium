//! The fake VT, for a system without kernel VTs: always the seat's only
//! session, taken and given up on request or with Ctrl+Super+F12
//! (vt_fake.c).

use std::io;

use xkbcommon::xkb::keysyms;

use super::{Switch, Vt, VtEvent};
use crate::input::{KeyEvent, Mods};

/// A VT with no kernel behind it.
#[derive(Debug, Default)]
pub struct FakeVt {
    active: bool,
    releasing: bool,
    events: Vec<VtEvent>,
}

impl FakeVt {
    pub fn new() -> FakeVt {
        FakeVt::default()
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

    /// Its one key, Ctrl+Super+F12, gives it up or takes it back, so no
    /// key is its own.
    fn is_own_key(&self, _: &KeyEvent) -> bool {
        false
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn bell(&mut self) {}

    fn number(&self) -> Option<u16> {
        None
    }
}
