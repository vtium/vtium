//! Keyboards and pointers through evdev, keys through libxkbcommon with
//! compose.
//!
//! [`Input`] merges every keyboard and pointer of the seat, as the C
//! reference's input object did (input.c). The udev monitor adds and
//! removes devices; the VT's acquire and release wake them and put them to
//! sleep, which opens and closes them. Each device's events are read 16 at
//! a time and become [`InputEvent`]s the owner drains once a turn and
//! hands on in the key order: the VT's keys ([`crate::vt::Vt::key`]), then
//! the seat's, then the terminal's grabs, then the encoder.
//!
//! Each device has a disposition a head file's rules set (ADR-1401 D3):
//! delivered, watched for the rescue key alone, or closed and never
//! opened. Until one is set, a device is taken by the rule vtium has always
//! used ([`Input::add`]). [`Input::device_events`] drains the events with
//! the device each came from, so a head takes its own keyboards and
//! pointers, with their modifiers, LEDs and screen per set of devices.
//!
//! Each keyboard keeps its own xkb state, so modifiers held on one apply
//! to it alone. The kernel's repeat is ignored: a held key repeats from a
//! timer after `xkb-repeat-delay` and then every `xkb-repeat-rate`
//! milliseconds, for keys the keymap marks as repeating.

pub(crate) mod class;
mod evdev;
mod keyboard;
mod pointer;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;

pub use class::{Class, Classes};
pub use evdev::Evdev;

use evdev::EventDevice;
pub use keyboard::{Keymap, KeymapConfig, Leds, locale};

use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};

use evdev::{Capabilities, EVENTS_PER_READ, RawEvent};
use keyboard::{KeyState, Keyboard, RepeatChange};
use pointer::Pointer;

use crate::ioctl::{EV_ABS, EV_KEY, EV_REL, EV_SYN};

/// The modifiers held with a key, as the C reference numbered them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const SHIFT: Mods = Mods(1);
    pub const LOCK: Mods = Mods(2);
    pub const CTRL: Mods = Mods(4);
    pub const ALT: Mods = Mods(8);
    pub const LOGO: Mods = Mods(16);

    pub const fn empty() -> Mods {
        Mods(0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
}

/// A key press, or its repeat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// xkb's key code: evdev's plus 8.
    pub keycode: u32,
    /// The ASCII keysym for shortcuts, or 0 (`NoSymbol`).
    pub ascii: u32,
    pub mods: Mods,
    /// The keysyms the key makes; a compose sequence's result replaces
    /// them.
    pub keysyms: Vec<u32>,
    /// Each keysym's character, where it has one.
    pub codepoints: Vec<Option<char>>,
    /// Made by the repeat timer of a held key.
    pub repeat: bool,
}

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEvent {
    /// The pointer is at `x`, `y`, with `held` pressed.
    Moved {
        x: i32,
        y: i32,
        held: Option<Button>,
    },
    Button {
        button: Button,
        pressed: bool,
        double_click: bool,
    },
    /// Wheel steps, positive up.
    Wheel(i32),
    /// The end of one evdev frame.
    Sync,
    /// No pointer event for [`POINTER_HIDE`]: hide the pointer.
    Hide,
}

/// What the input devices tell their owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
    Pointer(PointerEvent),
}

/// An event with the device it came from, as [`Input::device_events`]
/// drains them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInput {
    /// `None` for an event of no one device, the pointer's hide.
    pub device: Option<DeviceId>,
    /// A key of a device watched for the rescue key alone, for the rescue
    /// and nothing else.
    pub rescue: bool,
    pub event: InputEvent,
}

/// What vtium does with a device, as a head file's rules place it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Opened, its events delivered, whatever it reports.
    Deliver,
    /// Opened to watch for the rescue key alone: its key presses are
    /// drained marked [`DeviceInput::rescue`], and nothing else it sends is
    /// delivered (ADR-1401 D9).
    Rescue,
    /// Never opened.
    Closed,
}

/// The pointer hides after this long without a frame, unless the
/// configuration says otherwise (input_pointer.c:9-18).
pub const POINTER_HIDE: Duration = Duration::from_secs(20);

/// The repeat and the pointer, as the configuration sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputConfig {
    /// `xkb-repeat-delay`.
    pub repeat_delay: Duration,
    /// `xkb-repeat-rate`: the time between repeats.
    pub repeat_rate: Duration,
    /// `mouse`: take pointers.
    pub mouse: bool,
    /// `pointer-hide-timeout`: the pointer hides after this long without
    /// a frame; `None` never hides it.
    pub pointer_hide: Option<Duration>,
}

impl Default for InputConfig {
    fn default() -> InputConfig {
        InputConfig::new(0, 0, true)
    }
}

impl InputConfig {
    /// The configuration's values in milliseconds: 0 takes the default,
    /// 250 and 50, and a value of a second or more is cut to 999
    /// (input.c:570-585).
    pub fn new(repeat_delay_ms: u32, repeat_rate_ms: u32, mouse: bool) -> InputConfig {
        let ms = |v: u32, default: u32| match v {
            0 => default,
            1000.. => 999,
            v => v,
        };
        InputConfig {
            repeat_delay: Duration::from_millis(ms(repeat_delay_ms, 250).into()),
            repeat_rate: Duration::from_millis(ms(repeat_rate_ms, 50).into()),
            mouse,
            pointer_hide: Some(POINTER_HIDE),
        }
    }

    /// This configuration with the pointer hiding after `hide`, or never.
    #[must_use]
    pub const fn with_pointer_hide(self, hide: Option<Duration>) -> InputConfig {
        InputConfig {
            pointer_hide: hide,
            ..self
        }
    }
}

/// How the devices are opened: directly, or through the seat manager
/// (vtium/vtium#34).
pub(crate) trait Open {
    type Device: EventDevice;
    fn open(&mut self, node: &Path) -> io::Result<Self::Device>;
    fn close(&mut self, device: Self::Device) {
        drop(device);
    }
}

/// Opens each device directly, read-write and non-blocking.
#[derive(Debug, Default)]
pub struct Direct;

impl Open for Direct {
    type Device = Evdev;
    fn open(&mut self, node: &Path) -> io::Result<Evdev> {
        Evdev::open(node)
    }
}

/// A device's handle in [`Input`], never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(u64);

struct Device<E> {
    id: DeviceId,
    node: PathBuf,
    /// Open while the input is awake.
    open: Option<E>,
    /// Probed once, at the first open: what it reports, its keyboard and
    /// its pointer.
    probed: Option<Probed>,
    watch: Option<RegistrationToken>,
    /// Counts the repeats started, so a timer left from an earlier one
    /// stops.
    repeat_serial: u64,
    /// What the rules set; `None` takes the device by the rule vtium has
    /// always used.
    disposition: Option<Disposition>,
    /// The screen its pointer moves within, where it is not the seat's.
    pointer_max: Option<(i32, i32)>,
}

impl<E> Device<E> {
    fn rescue(&self) -> bool {
        self.disposition == Some(Disposition::Rescue)
    }
}

/// Where a queued event came from.
#[derive(Clone, Copy, Debug)]
struct Source {
    device: Option<DeviceId>,
    rescue: bool,
}

/// The events queued since the last drain, each with its source.
#[derive(Debug, Default)]
struct Queue {
    events: Vec<InputEvent>,
    sources: Vec<Source>,
}

impl Queue {
    fn push(&mut self, device: Option<DeviceId>, rescue: bool, event: InputEvent) {
        self.events.push(event);
        self.sources.push(Source { device, rescue });
    }
}

struct Probed {
    caps: Capabilities,
    keyboard: Option<Keyboard>,
    pointer: Pointer,
}

/// What ties the input to the loop it was inserted in.
trait Link {
    fn watch(&self, id: DeviceId, fd: OwnedFd) -> Option<RegistrationToken>;
    fn unwatch(&self, token: RegistrationToken);
    fn repeat(&self, id: DeviceId, serial: u64, delay: Duration, rate: Duration);
    /// Arms the pointer's hide timer, `after` from now, if it is not.
    fn hide(&self, after: Duration);
}

/// Every keyboard and pointer of the seat.
#[expect(private_bounds, reason = "the opener is Direct outside the crate")]
pub struct Input<O: Open = Direct> {
    opener: O,
    keymap: Keymap,
    config: InputConfig,
    /// Wake-ups minus sleeps: the devices are open while it is above 0.
    awake: u32,
    devices: Vec<Device<O::Device>>,
    next_id: u64,
    pointer_max: (i32, i32),
    /// When the pointer hides, while a frame arrived within its hide
    /// timeout.
    hide_at: Option<Instant>,
    queue: Queue,
    link: Option<Box<dyn Link>>,
}

impl<O: Open> std::fmt::Debug for Input<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("awake", &self.awake)
            .field("devices", &self.devices.len())
            .finish_non_exhaustive()
    }
}

#[expect(private_bounds, reason = "the opener is Direct outside the crate")]
impl<O: Open + 'static> Input<O> {
    /// The seat's input, asleep and with no devices.
    pub fn new(opener: O, keymap: Keymap, config: InputConfig) -> Input<O> {
        Input {
            opener,
            keymap,
            config,
            awake: 0,
            devices: Vec::new(),
            next_id: 0,
            pointer_max: (0, 0),
            hide_at: None,
            queue: Queue::default(),
            link: None,
        }
    }

    /// What happened since the last drain, in order, but for the keys of
    /// devices watched for the rescue key.
    pub fn events(&mut self) -> std::vec::Drain<'_, InputEvent> {
        let Queue { events, sources } = &mut self.queue;
        let mut from = sources.drain(..);
        events.retain(|_| from.next().is_none_or(|s| !s.rescue));
        drop(from);
        events.drain(..)
    }

    /// What happened since the last drain, in order, each with the device
    /// it came from.
    pub fn device_events(&mut self) -> impl Iterator<Item = DeviceInput> + '_ {
        let Queue { events, sources } = &mut self.queue;
        sources
            .drain(..)
            .zip(events.drain(..))
            .map(|(s, event)| DeviceInput {
                device: s.device,
                rescue: s.rescue,
                event,
            })
    }

    pub fn is_awake(&self) -> bool {
        self.awake > 0
    }

    /// The pointer's last pixel: the screen's width and height.
    pub fn set_pointer_max(&mut self, width: u32, height: u32) {
        self.pointer_max = pixel(width, height);
    }

    /// The last pixel of the screen the pointers of `devices` move within,
    /// in place of the seat's.
    pub fn set_pointer_max_of(&mut self, devices: &[DeviceId], width: u32, height: u32) {
        for device in self.devices.iter_mut().filter(|d| devices.contains(&d.id)) {
            device.pointer_max = Some(pixel(width, height));
        }
    }

    /// A device udev reported, opened and probed at once while awake, and
    /// dropped when it is no keyboard or pointer vtium takes
    /// (input.c:620-637).
    pub fn add(&mut self, node: &Path) -> Option<DeviceId> {
        self.add_device(node, None)
    }

    /// A device udev reported, with the disposition its rule gives it: a
    /// closed one is never opened, and a delivered one is kept whatever it
    /// reports, with a keyboard for any key.
    pub fn add_with(&mut self, node: &Path, disposition: Disposition) -> Option<DeviceId> {
        self.add_device(node, Some(disposition))
    }

    /// A device's disposition, `None` while it is taken by the rule vtium
    /// has always used.
    pub fn disposition(&self, id: DeviceId) -> Option<Disposition> {
        self.devices.iter().find(|d| d.id == id)?.disposition
    }

    /// Sets a device's disposition: closing it, or opening it while awake.
    pub fn set_disposition(&mut self, id: DeviceId, disposition: Disposition) {
        let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
            return;
        };
        device.disposition = Some(disposition);
        if disposition == Disposition::Closed {
            self.close(id);
        } else if self.awake > 0 {
            self.wake_device(id);
        }
    }

    fn add_device(&mut self, node: &Path, disposition: Option<Disposition>) -> Option<DeviceId> {
        let id = DeviceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.devices.push(Device {
            id,
            node: node.to_owned(),
            open: None,
            probed: None,
            watch: None,
            repeat_serial: 0,
            disposition,
            pointer_max: None,
        });
        if self.awake > 0 && !self.wake_device(id) {
            return None;
        }
        Some(id)
    }

    /// A device udev reported gone.
    pub fn remove(&mut self, id: DeviceId) {
        self.close(id);
        self.devices.retain(|d| d.id != id);
    }

    /// The device opened from `node`.
    pub fn find(&self, node: &Path) -> Option<DeviceId> {
        self.devices.iter().find(|d| d.node == node).map(|d| d.id)
    }

    /// Opens every device, probing those not probed yet; a device that
    /// cannot be opened or is not one vtium takes is dropped. Wake-ups and
    /// sleeps are counted, so the devices open at the first wake-up and
    /// close at the matching sleep (input.c:721-770).
    pub fn wake_up(&mut self) {
        self.awake = self.awake.saturating_add(1);
        if self.awake != 1 {
            return;
        }
        log::debug!(target: "input", "waking up");
        let ids: Vec<DeviceId> = self.devices.iter().map(|d| d.id).collect();
        for id in ids {
            self.wake_device(id);
        }
    }

    /// Closes every device and stops the repeat.
    pub fn sleep(&mut self) {
        if self.awake == 0 {
            return;
        }
        self.awake = self.awake.saturating_sub(1);
        if self.awake != 0 {
            return;
        }
        log::debug!(target: "input", "going to sleep");
        let ids: Vec<DeviceId> = self.devices.iter().map(|d| d.id).collect();
        for id in ids {
            self.close(id);
        }
    }

    /// Sets the keyboards' LEDs, as a program asks with DECLL, but for
    /// those watched for the rescue key.
    pub fn set_leds(&mut self, leds: Leds) {
        self.write_leds_where(leds, |d| !d.rescue());
    }

    /// Sets the LEDs of the keyboards of `devices`.
    pub fn set_leds_of(&mut self, devices: &[DeviceId], leds: Leds) {
        self.write_leds_where(leds, |d| devices.contains(&d.id));
    }

    fn write_leds_where(&mut self, leds: Leds, wanted: impl Fn(&Device<O::Device>) -> bool) {
        for device in self.devices.iter_mut().filter(|d| wanted(d)) {
            let has_leds = device.probed.as_ref().is_some_and(|p| p.caps.leds);
            if let (Some(dev), true) = (device.open.as_mut(), has_leds) {
                write_leds(dev, &device.node, leds);
            }
        }
    }

    /// Shift, Lock, Control, Alt and Super, held on any keyboard but those
    /// watched for the rescue key.
    pub fn mods(&self) -> Mods {
        self.mods_where(|d| !d.rescue())
    }

    /// The modifiers held on the keyboards of `devices`.
    pub fn mods_of(&self, devices: &[DeviceId]) -> Mods {
        self.mods_where(|d| devices.contains(&d.id))
    }

    fn mods_where(&self, wanted: impl Fn(&Device<O::Device>) -> bool) -> Mods {
        self.devices
            .iter()
            .filter(|d| wanted(d))
            .filter_map(|d| d.probed.as_ref()?.keyboard.as_ref())
            .fold(Mods::empty(), |mods, k| mods.union(k.mods()))
    }

    /// Opens a device, probing it the first time. Returns whether it is
    /// kept.
    fn wake_device(&mut self, id: DeviceId) -> bool {
        let Some(index) = self.devices.iter().position(|d| d.id == id) else {
            return false;
        };
        let opened = self.open(index);
        if !opened {
            self.devices.retain(|d| d.id != id);
        }
        opened
    }

    fn open(&mut self, index: usize) -> bool {
        let Some(device) = self.devices.get_mut(index) else {
            return false;
        };
        if device.open.is_some() || device.disposition == Some(Disposition::Closed) {
            return true;
        }
        let mut dev = match self.opener.open(&device.node) {
            Ok(dev) => dev,
            Err(e) => {
                log::warn!(target: "input", "cannot open {}: {e}", device.node.display());
                return false;
            }
        };
        let first = device.probed.is_none();
        if first {
            let name = dev.name();
            let caps = match dev.bits() {
                Ok(bits) => Capabilities::from_bits(&bits),
                Err(e) => {
                    log::warn!(target: "input", "cannot probe {name}: {e}");
                    Capabilities::default()
                }
            };
            let placed = device.disposition.is_some();
            if !placed && !caps.usable(self.config.mouse) {
                log::debug!(target: "input", "ignoring {} [{name}]", device.node.display());
                self.opener.close(dev);
                return false;
            }
            let keys = caps.keys || (placed && caps.any_key);
            let kind = caps.pointer_kind();
            device.probed = Some(Probed {
                caps,
                keyboard: keys.then(|| Keyboard::new(&self.keymap)),
                pointer: Pointer::new(kind),
            });
            let id = dev
                .id()
                .map(|(vendor, product)| format!(" {vendor:04x}:{product:04x}"))
                .unwrap_or_default();
            if device.rescue() {
                log::info!(
                    target: "input",
                    "watching {} [{name}]{id} for the rescue key",
                    device.node.display()
                );
            } else {
                log::info!(
                    target: "input",
                    "using {} [{name}]{id} as {}{}",
                    device.node.display(),
                    if keys { "keyboard" } else { "" },
                    kind.name()
                );
            }
        }
        let Some(probed) = device.probed.as_mut() else {
            return false;
        };
        if probed.caps.abs {
            match dev.abs_range() {
                Ok((x, y)) => {
                    probed.pointer.range_x = x;
                    probed.pointer.range_y = y;
                }
                Err(e) => log::debug!(target: "input", "no axis ranges: {e}"),
            }
        }
        if !first {
            if let Some(keyboard) = probed.keyboard.as_mut() {
                let leds = keyboard.wake_up(&self.keymap);
                if probed.caps.leds {
                    write_leds(&mut dev, &device.node, leds);
                }
            }
        }
        device.watch = match (&self.link, dev.watch()) {
            (Some(link), Some(Ok(fd))) => link.watch(device.id, fd),
            (Some(_), Some(Err(e))) => {
                log::warn!(target: "input", "cannot watch {}: {e}", device.node.display());
                None
            }
            _ => None,
        };
        device.open = Some(dev);
        true
    }

    /// Closes a device, keeping it to open again (input.c:193-203), with
    /// its pointer's buttons let go.
    fn close(&mut self, id: DeviceId) {
        let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
            return;
        };
        if let Some(probed) = device.probed.as_mut() {
            if let Some(keyboard) = probed.keyboard.as_mut() {
                keyboard.stop_repeat();
            }
            probed.pointer.let_go();
        }
        if let (Some(token), Some(link)) = (device.watch.take(), &self.link) {
            link.unwatch(token);
        }
        if let Some(dev) = device.open.take() {
            self.opener.close(dev);
        }
    }

    /// Reads a device's events at `now` until none are waiting, at most
    /// [`crate::INPUT_READS_PER_TURN`] reads; the device stays readable for
    /// the next turn (input.c:78-113). An end or a read error closes it,
    /// kept to open at the next wake-up, as a seat manager revokes devices
    /// on a switch; udev reports a device unplugged.
    pub fn dispatch(&mut self, id: DeviceId, now: Instant) {
        let mut buf = [RawEvent::default(); EVENTS_PER_READ];
        for _ in 0..crate::INPUT_READS_PER_TURN {
            let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
                return;
            };
            let Some(dev) = device.open.as_mut() else {
                return;
            };
            let n = match dev.read(&mut buf) {
                Ok(Some(n)) => n,
                Ok(None) => {
                    log::debug!(target: "input", "end of {}", device.node.display());
                    self.close(id);
                    return;
                }
                Err(e) => {
                    log::warn!(target: "input", "cannot read {}: {e}", device.node.display());
                    self.close(id);
                    return;
                }
            };
            for ev in buf.iter().take(n) {
                self.event(id, *ev, now);
            }
            if n < EVENTS_PER_READ {
                return;
            }
        }
    }

    /// A hangup or error on a device: closed, kept to open again.
    pub fn hangup(&mut self, id: DeviceId) {
        log::debug!(target: "input", "hangup on device {id:?}, closing it");
        self.close(id);
    }

    /// One evdev event, by type (input.c:56-74).
    fn event(&mut self, id: DeviceId, ev: RawEvent, now: Instant) {
        let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
            return;
        };
        let max = device.pointer_max.unwrap_or(self.pointer_max);
        let rescue = device.disposition == Some(Disposition::Rescue);
        let Some(probed) = device.probed.as_mut() else {
            return;
        };
        let from = Some(id);
        // A device watched for the rescue key sends its keys alone.
        let pointer = !rescue;
        match ev.kind {
            EV_KEY => {
                if let (Some(keyboard), Some(state)) =
                    (probed.keyboard.as_mut(), KeyState::from_value(ev.value))
                {
                    let processed = keyboard.process(state, ev.code);
                    if let (Some(leds), true) = (processed.leds, probed.caps.leds) {
                        if let Some(dev) = device.open.as_mut() {
                            write_leds(dev, &device.node, leds);
                        }
                    }
                    match processed.repeat {
                        Some(RepeatChange::Start) => {
                            device.repeat_serial = device.repeat_serial.wrapping_add(1);
                            if let Some(link) = &self.link {
                                link.repeat(
                                    id,
                                    device.repeat_serial,
                                    self.config.repeat_delay,
                                    self.config.repeat_rate,
                                );
                            }
                        }
                        Some(RepeatChange::Stop) => {
                            device.repeat_serial = device.repeat_serial.wrapping_add(1);
                        }
                        None => {}
                    }
                    if let Some(key) = processed.key {
                        self.queue.push(from, rescue, InputEvent::Key(key));
                    }
                }
                if let (Some(p), true) = (probed.pointer.button(ev.code, ev.value, now), pointer) {
                    self.queue.push(from, false, InputEvent::Pointer(p));
                }
            }
            EV_REL => {
                if let (Some(p), true) = (probed.pointer.rel(ev.code, ev.value, max), pointer) {
                    self.queue.push(from, false, InputEvent::Pointer(p));
                }
            }
            EV_ABS => {
                if let (Some(p), true) = (probed.pointer.abs(ev.code, ev.value, max), pointer) {
                    self.queue.push(from, false, InputEvent::Pointer(p));
                }
            }
            // A frame of a device with no pointer, a keyboard's, is no
            // pointer event.
            EV_SYN if pointer && probed.pointer.kind != pointer::Kind::None => {
                self.queue
                    .push(from, false, InputEvent::Pointer(PointerEvent::Sync));
                if let Some(after) = self.config.pointer_hide {
                    self.hide_at = now.checked_add(after);
                    if let Some(link) = &self.link {
                        link.hide(after);
                    }
                }
            }
            _ => {}
        }
    }

    /// The repeat timer of a device fired: sends its held key again.
    /// Returns whether the repeat `serial` still runs.
    pub(crate) fn repeat(&mut self, id: DeviceId, serial: u64) -> bool {
        let Some(device) = self.devices.iter().find(|d| d.id == id) else {
            return false;
        };
        if device.repeat_serial != serial || device.open.is_none() {
            return false;
        }
        let key = device
            .probed
            .as_ref()
            .and_then(|p| p.keyboard.as_ref())
            .and_then(Keyboard::repeated);
        let rescue = device.rescue();
        match key {
            Some(key) => {
                self.queue.push(Some(id), rescue, InputEvent::Key(key));
                true
            }
            None => false,
        }
    }

    /// The pointer's hide timer fired at `now`: `Some` with the time left
    /// when a frame came since it was armed.
    pub(crate) fn hide(&mut self, now: Instant) -> Option<Duration> {
        let at = self.hide_at?;
        if let Some(left) = at.checked_duration_since(now).filter(|d| !d.is_zero()) {
            return Some(left);
        }
        self.hide_at = None;
        self.queue
            .push(None, false, InputEvent::Pointer(PointerEvent::Hide));
        None
    }

    /// Whether a device's key repeats now.
    #[cfg(test)]
    pub(crate) fn repeating(&self, id: DeviceId) -> bool {
        self.devices
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.probed.as_ref()?.keyboard.as_ref())
            .is_some_and(|k| k.repeated().is_some())
    }

    /// The repeat serial a device's timer runs with.
    #[cfg(test)]
    pub(crate) fn repeat_serial(&self, id: DeviceId) -> Option<u64> {
        self.devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.repeat_serial)
    }

    /// Puts the input in the loop behind `handle`: each open device's
    /// descriptor, the repeat timers and the pointer's hide timer. `access`
    /// finds the input in the loop's data, or `None` once it is gone.
    pub fn insert<D, A>(&mut self, handle: &LoopHandle<'static, D>, access: A)
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Input<O>> + Clone + 'static,
    {
        self.link = Some(Box::new(LoopLink {
            handle: handle.clone(),
            access,
            hide_armed: std::rc::Rc::new(std::cell::Cell::new(false)),
        }));
        let ids: Vec<DeviceId> = self.devices.iter().map(|d| d.id).collect();
        for id in ids {
            let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
                continue;
            };
            if let (Some(dev), Some(link)) = (&device.open, &self.link) {
                if let Some(Ok(fd)) = dev.watch() {
                    device.watch = link.watch(id, fd);
                }
            }
        }
    }
}

/// A screen's width and height as the pointer's last pixel.
fn pixel(width: u32, height: u32) -> (i32, i32) {
    (
        i32::try_from(width).unwrap_or(i32::MAX),
        i32::try_from(height).unwrap_or(i32::MAX),
    )
}

fn write_leds<E: EventDevice>(dev: &mut E, node: &Path, leds: Leds) {
    if let Err(e) = dev.write_leds(&leds.codes()) {
        log::warn!(target: "input", "cannot set the LEDs of {}: {e}", node.display());
    }
}

struct LoopLink<D: 'static, A> {
    handle: LoopHandle<'static, D>,
    access: A,
    /// The hide timer is in the loop.
    hide_armed: std::rc::Rc<std::cell::Cell<bool>>,
}

impl<D, A, O> Link for LoopLink<D, A>
where
    D: 'static,
    O: Open + 'static,
    A: Fn(&mut D) -> Option<&mut Input<O>> + Clone + 'static,
{
    fn watch(&self, id: DeviceId, fd: OwnedFd) -> Option<RegistrationToken> {
        let access = self.access.clone();
        self.handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |readiness, _, data: &mut D| {
                    let Some(input) = access(data) else {
                        return Ok(PostAction::Remove);
                    };
                    if readiness.error {
                        input.hangup(id);
                        return Ok(PostAction::Continue);
                    }
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "the double click reads the clock, held in vtium-platform"
                    )]
                    let now = Instant::now();
                    input.dispatch(id, now);
                    Ok(PostAction::Continue)
                },
            )
            .inspect_err(|e| log::warn!(target: "input", "cannot watch a device: {}", e.error))
            .ok()
    }

    fn unwatch(&self, token: RegistrationToken) {
        self.handle.remove(token);
    }

    fn repeat(&self, id: DeviceId, serial: u64, delay: Duration, rate: Duration) {
        let access = self.access.clone();
        let inserted = self
            .handle
            .insert_source(Timer::from_duration(delay), move |_, _, data| {
                if access(data).is_some_and(|input| input.repeat(id, serial)) {
                    TimeoutAction::ToDuration(rate)
                } else {
                    TimeoutAction::Drop
                }
            });
        if let Err(e) = inserted {
            log::warn!(target: "input", "cannot start the key repeat: {}", e.error);
        }
    }

    fn hide(&self, after: Duration) {
        if self.hide_armed.replace(true) {
            return;
        }
        let access = self.access.clone();
        let armed = std::rc::Rc::clone(&self.hide_armed);
        let inserted = self
            .handle
            .insert_source(Timer::from_duration(after), move |_, _, data| {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "the pointer's hide timer is one of the timers vtium-platform holds"
                )]
                let now = Instant::now();
                match access(data).and_then(|input| input.hide(now)) {
                    Some(left) => TimeoutAction::ToDuration(left),
                    None => {
                        armed.set(false);
                        TimeoutAction::Drop
                    }
                }
            });
        if let Err(e) = inserted {
            self.hide_armed.set(false);
            log::warn!(target: "input", "cannot arm the pointer's hide timer: {}", e.error);
        }
    }
}
