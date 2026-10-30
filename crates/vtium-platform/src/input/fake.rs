//! Stand-in input devices: events the test queues, the bitmaps it sets,
//! and the LEDs written to them.

#![expect(
    clippy::arithmetic_side_effects,
    reason = "test arithmetic on bounded values"
)]

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::Open;
use super::evdev::{Bits, EVENTS_PER_READ, EventDevice, RawEvent};
use super::pointer::Range;
use crate::ioctl::{
    ABS_X, ABS_Y, AbsBits, BTN_LEFT, BTN_RIGHT, BTN_TOUCH, EV_ABS, EV_KEY, EV_LED, EV_REL, EV_SYN,
    INPUT_PROP_DIRECT, KeyBits, PropBits, REL_WHEEL, REL_X, REL_Y, RelBits,
};

fn set(bits: &mut [usize], bit: u16) {
    let bit = usize::from(bit);
    if let Some(word) = bits.get_mut(bit / usize::BITS as usize) {
        *word |= 1 << (bit % usize::BITS as usize);
    }
}

/// A keyboard with the keys from Escape to Mute and LEDs.
pub(crate) fn keyboard() -> Bits {
    let mut bits = Bits::default();
    for ev in [EV_SYN, EV_KEY, EV_LED] {
        set(&mut bits.ev, ev);
    }
    let mut key = KeyBits::default();
    for code in 1..=113 {
        set(&mut key, code);
    }
    bits.key = Some(key);
    bits
}

/// A media-key device: Volume Down and Volume Up, above Mute.
pub(crate) fn volume_keys() -> Bits {
    let mut bits = Bits::default();
    for ev in [EV_SYN, EV_KEY] {
        set(&mut bits.ev, ev);
    }
    let mut key = KeyBits::default();
    for code in [114, 115] {
        set(&mut key, code);
    }
    bits.key = Some(key);
    bits
}

/// A mouse: relative motion, a wheel, left and right buttons.
pub(crate) fn mouse() -> Bits {
    let mut bits = Bits::default();
    for ev in [EV_SYN, EV_KEY, EV_REL] {
        set(&mut bits.ev, ev);
    }
    let mut key = KeyBits::default();
    set(&mut key, BTN_LEFT);
    set(&mut key, BTN_RIGHT);
    bits.key = Some(key);
    let mut rel = RelBits::default();
    for code in [REL_X, REL_Y, REL_WHEEL] {
        set(&mut rel, code);
    }
    bits.rel = Some(rel);
    bits
}

/// Absolute axes with touch, a touchscreen when `direct`.
pub(crate) fn touch(direct: bool) -> Bits {
    let mut bits = Bits::default();
    for ev in [EV_SYN, EV_KEY, EV_ABS] {
        set(&mut bits.ev, ev);
    }
    let mut key = KeyBits::default();
    set(&mut key, BTN_TOUCH);
    bits.key = Some(key);
    let mut abs = AbsBits::default();
    set(&mut abs, ABS_X);
    set(&mut abs, ABS_Y);
    bits.abs = Some(abs);
    let mut props = PropBits::default();
    if direct {
        set(&mut props, INPUT_PROP_DIRECT);
    }
    bits.props = Some(props);
    bits
}

/// One stand-in device's state.
#[derive(Debug, Default)]
pub(crate) struct Node {
    pub(crate) bits: Bits,
    pub(crate) range: (Range, Range),
    pub(crate) queue: VecDeque<RawEvent>,
    /// The next read finds the device's end.
    pub(crate) end: bool,
    /// Every LED write, in order.
    pub(crate) leds: Vec<Vec<(u16, bool)>>,
    pub(crate) open: bool,
    pub(crate) opens: u32,
    /// The bitmaps cannot be read.
    pub(crate) unprobeable: bool,
}

/// The stand-in devices by node, shared with the test.
#[derive(Clone, Debug, Default)]
pub(crate) struct Nodes(pub(crate) Rc<RefCell<BTreeMap<PathBuf, Rc<RefCell<Node>>>>>);

impl Nodes {
    pub(crate) fn add(&self, path: &str, bits: Bits) -> Rc<RefCell<Node>> {
        let node = Rc::new(RefCell::new(Node {
            bits,
            ..Node::default()
        }));
        self.0
            .borrow_mut()
            .insert(PathBuf::from(path), Rc::clone(&node));
        node
    }
}

impl Open for Nodes {
    type Device = FakeDevice;

    fn open(&mut self, node: &Path) -> io::Result<FakeDevice> {
        let found = self
            .0
            .borrow()
            .get(node)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        {
            let mut n = found.borrow_mut();
            n.open = true;
            n.opens += 1;
        }
        Ok(FakeDevice(found))
    }

    fn close(&mut self, device: FakeDevice) {
        device.0.borrow_mut().open = false;
    }
}

/// An open stand-in device.
#[derive(Debug)]
pub(crate) struct FakeDevice(Rc<RefCell<Node>>);

impl EventDevice for FakeDevice {
    fn name(&self) -> String {
        String::from("stand-in")
    }

    fn bits(&self) -> io::Result<Bits> {
        let n = self.0.borrow();
        if n.unprobeable {
            return Err(io::Error::from(rustix::io::Errno::NOTTY));
        }
        Ok(n.bits)
    }

    fn id(&self) -> io::Result<(u16, u16)> {
        Ok((0x1d6b, 0x0104))
    }

    fn abs_range(&self) -> io::Result<(Range, Range)> {
        Ok(self.0.borrow().range)
    }

    fn read(&mut self, buf: &mut [RawEvent; EVENTS_PER_READ]) -> io::Result<Option<usize>> {
        let mut n = self.0.borrow_mut();
        if n.queue.is_empty() && std::mem::take(&mut n.end) {
            return Ok(None);
        }
        let mut count = 0;
        for slot in buf.iter_mut() {
            let Some(ev) = n.queue.pop_front() else { break };
            *slot = ev;
            count += 1;
        }
        Ok(Some(count))
    }

    fn write_leds(&mut self, leds: &[(u16, bool)]) -> io::Result<()> {
        self.0.borrow_mut().leds.push(leds.to_vec());
        Ok(())
    }

    fn watch(&self) -> Option<io::Result<OwnedFd>> {
        None
    }
}
