//! An evdev node: what it reports, its events and its LEDs.

use std::ffi::c_long;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{Mode, OFlags};

use super::pointer::{Kind, Range};
use crate::ioctl::{
    self, ABS_X, ABS_Y, BTN_LEFT, BTN_TOUCH, EV_ABS, EV_KEY, EV_LED, EV_REL, EV_SYN,
    INPUT_PROP_DIRECT, KEY_MIN_INTERESTING, REL_WHEEL, REL_X, REL_Y, bit_is_set,
};

/// Events one read takes (input.c:81).
pub(crate) const EVENTS_PER_READ: usize = 16;

/// `struct input_event`'s time: two `long`s.
const TIME_SIZE: usize = size_of::<[c_long; 2]>();

/// `struct input_event`: its time, then type, code and value.
const EVENT_SIZE: usize = TIME_SIZE + 2 + 2 + 4;

/// One `struct input_event`, without its time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RawEvent {
    pub(crate) kind: u16,
    pub(crate) code: u16,
    pub(crate) value: i32,
}

impl RawEvent {
    /// Reads one event from its bytes in the kernel's layout.
    fn parse(bytes: &[u8]) -> Option<RawEvent> {
        let at = TIME_SIZE;
        let field = |from: usize, len: usize| bytes.get(from..from.checked_add(len)?);
        Some(RawEvent {
            kind: u16::from_ne_bytes(field(at, 2)?.try_into().ok()?),
            code: u16::from_ne_bytes(field(at.checked_add(2)?, 2)?.try_into().ok()?),
            value: i32::from_ne_bytes(field(at.checked_add(4)?, 4)?.try_into().ok()?),
        })
    }

    /// The event's bytes in the kernel's layout, its time zero.
    fn bytes(self) -> [u8; EVENT_SIZE] {
        let mut out = [0; EVENT_SIZE];
        let at = TIME_SIZE;
        let fields = self
            .kind
            .to_ne_bytes()
            .into_iter()
            .chain(self.code.to_ne_bytes())
            .chain(self.value.to_ne_bytes());
        for (slot, byte) in out.iter_mut().skip(at).zip(fields) {
            *slot = byte;
        }
        out
    }
}

/// What a device reports (input.c:299-369).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Capabilities {
    /// A key from Escape to Mute.
    pub(crate) keys: bool,
    /// Any key, as udev's `key` class counts them.
    pub(crate) any_key: bool,
    pub(crate) leds: bool,
    pub(crate) rel: bool,
    pub(crate) abs: bool,
    pub(crate) mouse_button: bool,
    pub(crate) touch: bool,
    pub(crate) wheel: bool,
    pub(crate) direct: bool,
}

/// The bitmaps `EVIOCGBIT` and `EVIOCGPROP` fill, each `None` where the
/// device reports no such events.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Bits {
    pub(crate) ev: ioctl::EvBits,
    pub(crate) key: Option<ioctl::KeyBits>,
    pub(crate) rel: Option<ioctl::RelBits>,
    pub(crate) abs: Option<ioctl::AbsBits>,
    pub(crate) props: Option<ioctl::PropBits>,
}

impl Capabilities {
    /// A device with any key from Escape to Mute is a keyboard, since the
    /// keymap can map any key; buttons, axes and properties make the
    /// pointer.
    pub(crate) fn from_bits(bits: &Bits) -> Capabilities {
        let mut caps = Capabilities::default();
        if let Some(key) = &bits.key {
            caps.keys = (0..=KEY_MIN_INTERESTING).any(|k| bit_is_set(key, k));
            caps.any_key = super::class::any_key(key);
            caps.mouse_button = bit_is_set(key, BTN_LEFT);
            caps.touch = bit_is_set(key, BTN_TOUCH);
        }
        if let Some(rel) = &bits.rel {
            caps.rel = bit_is_set(rel, REL_X) && bit_is_set(rel, REL_Y);
            caps.wheel = bit_is_set(rel, REL_WHEEL);
        }
        if let Some(abs) = &bits.abs {
            caps.abs = bit_is_set(abs, ABS_X) && bit_is_set(abs, ABS_Y);
        }
        if let Some(props) = &bits.props {
            caps.direct = bit_is_set(props, INPUT_PROP_DIRECT);
        }
        caps.leds = bit_is_set(&bits.ev, EV_LED);
        caps
    }

    /// Whether vtium takes the device: a keyboard, or with `mouse` on a
    /// pointer with buttons or touch (input.c:377-389).
    pub(crate) fn usable(self, mouse: bool) -> bool {
        self.keys
            || (mouse
                && ((self.rel && self.mouse_button)
                    || (self.abs && self.touch)
                    || (self.abs && self.mouse_button)))
    }

    pub(crate) fn pointer_kind(self) -> Kind {
        if self.abs {
            if self.touch {
                if self.direct {
                    Kind::Touchscreen
                } else {
                    Kind::Touchpad
                }
            } else {
                Kind::VirtualMouse
            }
        } else if self.rel {
            Kind::Mouse
        } else {
            Kind::None
        }
    }
}

/// An input device as vtium uses it, so the tests can stand one in.
pub(crate) trait EventDevice {
    /// The device's name.
    fn name(&self) -> String;

    /// What it reports, from its bitmaps.
    fn bits(&self) -> io::Result<Bits>;

    /// The vendor and product it reports.
    fn id(&self) -> io::Result<(u16, u16)>;

    /// The range of its X and Y axes.
    fn abs_range(&self) -> io::Result<(Range, Range)>;

    /// Reads events into `buf`: how many, 0 when none are waiting.
    /// `Ok(None)` is the end of the device.
    fn read(&mut self, buf: &mut [RawEvent; EVENTS_PER_READ]) -> io::Result<Option<usize>>;

    /// Writes `EV_LED` events.
    fn write_leds(&mut self, leds: &[(u16, bool)]) -> io::Result<()>;

    /// A second descriptor for the loop to watch; `None` for a device
    /// without one.
    fn watch(&self) -> Option<io::Result<OwnedFd>>;
}

/// An evdev node, opened read-write and non-blocking.
#[derive(Debug)]
pub struct Evdev {
    fd: OwnedFd,
}

impl Evdev {
    pub fn new(fd: OwnedFd) -> Evdev {
        Evdev { fd }
    }

    pub fn open(path: &Path) -> io::Result<Evdev> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?;
        Ok(Evdev { fd })
    }
}

impl EventDevice for Evdev {
    fn name(&self) -> String {
        ioctl::evdev_name(self.fd.as_fd()).unwrap_or_default()
    }

    fn bits(&self) -> io::Result<Bits> {
        let fd = self.fd.as_fd();
        let ev = ioctl::evdev_ev_bits(fd)?;
        let syn = bit_is_set(&ev, EV_SYN);
        Ok(Bits {
            ev,
            key: bit_is_set(&ev, EV_KEY)
                .then(|| ioctl::evdev_key_bits(fd))
                .transpose()?,
            rel: (syn && bit_is_set(&ev, EV_REL))
                .then(|| ioctl::evdev_rel_bits(fd))
                .transpose()?,
            abs: (syn && bit_is_set(&ev, EV_ABS))
                .then(|| ioctl::evdev_abs_bits(fd))
                .transpose()?,
            props: (syn && bit_is_set(&ev, EV_ABS))
                .then(|| ioctl::evdev_props(fd))
                .transpose()?,
        })
    }

    fn id(&self) -> io::Result<(u16, u16)> {
        let id = ioctl::evdev_id(self.fd.as_fd())?;
        Ok((id.vendor, id.product))
    }

    fn abs_range(&self) -> io::Result<(Range, Range)> {
        let (x, y) = ioctl::evdev_abs_xy(self.fd.as_fd())?;
        Ok((
            Range {
                min: x.minimum,
                max: x.maximum,
            },
            Range {
                min: y.minimum,
                max: y.maximum,
            },
        ))
    }

    fn read(&mut self, buf: &mut [RawEvent; EVENTS_PER_READ]) -> io::Result<Option<usize>> {
        let mut bytes = [0u8; EVENTS_PER_READ * EVENT_SIZE];
        let len = match rustix::io::read(&self.fd, &mut bytes) {
            Ok(0) => return Ok(None),
            Ok(len) => len,
            Err(rustix::io::Errno::AGAIN) => return Ok(Some(0)),
            Err(e) => return Err(e.into()),
        };
        if len % EVENT_SIZE != 0 {
            log::warn!(target: "input", "a short input_event: {len} bytes");
            return Ok(Some(0));
        }
        let chunks = bytes
            .get(..len)
            .unwrap_or_default()
            .chunks_exact(EVENT_SIZE);
        let mut n: usize = 0;
        for (slot, chunk) in buf.iter_mut().zip(chunks) {
            *slot = RawEvent::parse(chunk).unwrap_or_default();
            n = n.saturating_add(1);
        }
        Ok(Some(n))
    }

    fn write_leds(&mut self, leds: &[(u16, bool)]) -> io::Result<()> {
        let bytes: Vec<u8> = leds
            .iter()
            .flat_map(|&(code, on)| {
                RawEvent {
                    kind: EV_LED,
                    code,
                    value: i32::from(on),
                }
                .bytes()
            })
            .collect();
        if rustix::io::write(&self.fd, &bytes)? == bytes.len() {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::WriteZero))
        }
    }

    fn watch(&self) -> Option<io::Result<OwnedFd>> {
        Some(self.fd.try_clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_reads_back_from_its_bytes() {
        let ev = RawEvent {
            kind: EV_KEY,
            code: 30,
            value: -2,
        };
        assert_eq!(RawEvent::parse(&ev.bytes()), Some(ev));
        assert_eq!(EVENT_SIZE, if size_of::<c_long>() == 8 { 24 } else { 16 });
        assert_eq!(RawEvent::parse(&ev.bytes()[..EVENT_SIZE - 1]), None);
    }
}
