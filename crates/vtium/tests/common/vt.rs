//! The fake VT, counting each release vtium acknowledges. The fake VT's
//! `is_active` is false from the moment a release is asked for, so it
//! cannot tell whether vtium has acknowledged the release yet; a key that
//! switches the VT back before the acknowledgment finds the release still
//! pending, and the fake VT drops it. A test that switches away and back
//! waits on the count, so the switch back finds the VT let go.

use std::cell::Cell;
use std::io;
use std::rc::Rc;

use vtium_platform::input::KeyEvent;
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};

pub struct CountingVt {
    vt: FakeVt,
    /// The events `events` hands out, taken from the fake VT.
    events: Vec<VtEvent>,
    /// A release asked for and neither acknowledged nor refused.
    pending: bool,
    released: Rc<Cell<u32>>,
}

impl CountingVt {
    /// `vt`, counting, and the count of the releases vtium acknowledged.
    pub fn new(vt: FakeVt) -> (CountingVt, Rc<Cell<u32>>) {
        let released = Rc::new(Cell::new(0));
        let counting = CountingVt {
            vt,
            events: Vec::new(),
            pending: false,
            released: Rc::clone(&released),
        };
        (counting, released)
    }
}

impl Vt for CountingVt {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.extend(self.vt.events());
        if self.events.contains(&VtEvent::Release) {
            self.pending = true;
        }
        self.events.drain(..)
    }

    fn acknowledge_release(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.pending) {
            self.released.set(self.released.get() + 1);
        }
        self.vt.acknowledge_release()
    }

    fn refuse_release(&mut self) -> io::Result<()> {
        self.pending = false;
        self.vt.refuse_release()
    }

    fn activate(&mut self) -> io::Result<Switch> {
        self.vt.activate()
    }

    fn leave(&mut self) -> io::Result<Switch> {
        self.vt.leave()
    }

    fn wait(&mut self) -> io::Result<Switch> {
        self.vt.wait()
    }

    fn is_active(&self) -> bool {
        self.vt.is_active()
    }

    fn key(&mut self, key: &KeyEvent) -> bool {
        self.vt.key(key)
    }

    fn is_own_key(&self, key: &KeyEvent) -> bool {
        self.vt.is_own_key(key)
    }

    fn restore(&mut self) -> io::Result<()> {
        self.vt.restore()
    }

    fn seat_manager(&self) -> bool {
        self.vt.seat_manager()
    }

    fn logind_seat(&self) -> bool {
        self.vt.logind_seat()
    }

    fn bell(&mut self) {
        self.vt.bell();
    }

    fn number(&self) -> Option<u16> {
        self.vt.number()
    }
}
