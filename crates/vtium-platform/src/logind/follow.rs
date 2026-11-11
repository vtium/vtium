//! Following a change to the system's keyboard layout while vtium runs
//! (ADR-1400's L2 layer).
//!
//! localed announces each change to its properties with the signal
//! `PropertiesChanged` of the interface `org.freedesktop.DBus.Properties`,
//! on its object `/org/freedesktop/locale1`. The signal's arguments are the
//! interface whose properties changed, the changed properties with their
//! new values, and the names of the properties that changed without their
//! values (org.freedesktop.DBus.Properties in the D-Bus specification;
//! org.freedesktop.locale1(5), systemd 262; external, not re-verified).
//!
//! vtium asks the bus for that signal from localed's bus name, on localed's
//! object, through one connection that stays open while vtium runs. The bus
//! routes a signal by the name's current owner, so the match still holds
//! after localed exits when it is idle and starts again for its next call.
//! When a signal names one of localed's keyboard properties, vtium works out
//! the system's layout again in the order it uses at its start: the console
//! keymap, Debian's keyboard file, then localed's X11 layout
//! ([`crate::input::system`]). `localectl set-keymap` rewrites
//! `/etc/vconsole.conf` before localed announces the change, so vtium reads
//! the files again too, and not only localed's values. vtium.conf's `xkb-*`
//! keys still win. When the names that result differ from those of the
//! keymap in use, vtium builds the new keymap, gives it to every keyboard,
//! and logs one line.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::Path;
use std::ptr;

use super::localed::{self, LOCALE1, LOCALE1_PATH};
use super::{
    BusError, CALL_TIMEOUT_US, Error, sd_bus_flush_close_unref, sd_bus_open_system,
    sd_bus_set_method_call_timeout,
};
use crate::input::system::{self, Origin};
use crate::input::{Direct, Input, Keymap, KeymapConfig, Open, RuleNames};

/// The interface whose signal announces changed properties.
const PROPERTIES: &CStr = c"org.freedesktop.DBus.Properties";
/// The signal itself.
const PROPERTIES_CHANGED: &CStr = c"PropertiesChanged";

/// localed's properties that name the keyboard layout: the X11 rules' names
/// and the console keymap, whose change rewrites vconsole.conf.
const LAYOUT_PROPERTIES: [&str; 5] = [
    "X11Layout",
    "X11Model",
    "X11Variant",
    "X11Options",
    "VConsoleKeymap",
];

/// One `PropertiesChanged` signal that the bus delivered: the interface
/// whose properties changed, and the names of the properties, with or
/// without their values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    pub interface: String,
    pub properties: Vec<String>,
}

impl Changed {
    /// Whether the signal says that localed's keyboard layout changed.
    fn moves_layout(&self) -> bool {
        self.interface.as_bytes() == LOCALE1.to_bytes()
            && self
                .properties
                .iter()
                .any(|p| LAYOUT_PROPERTIES.contains(&p.as_str()))
    }
}

/// The bus's side of following localed, so the tests can stand in for it.
pub trait Signals {
    /// Asks the bus for the signals that match every given part: the
    /// sender's bus name, the object's path, the interface and the member.
    /// A part that is `None` matches any, as in sd_bus_match_signal(3).
    fn watch(
        &mut self,
        sender: Option<&CStr>,
        path: Option<&CStr>,
        interface: Option<&CStr>,
        member: Option<&CStr>,
    ) -> Result<(), Error>;
    /// The `PropertiesChanged` signals the bus delivered since the last
    /// call, in order.
    fn take(&mut self) -> Result<Vec<Changed>, Error>;
}

/// Asks `signals` for localed's announcements of changed properties.
fn watch_localed(signals: &mut impl Signals) -> Result<(), Error> {
    signals.watch(
        Some(LOCALE1),
        Some(LOCALE1_PATH),
        Some(PROPERTIES),
        Some(PROPERTIES_CHANGED),
    )
}

/// vtium's watch on the system's keyboard layout: the bus it hears localed
/// on, vtium.conf's keymap settings, and the names of the keymap in use.
pub struct LayoutFollow<S = SystemSignals> {
    signals: S,
    /// vtium.conf's own settings, before the system's names fill them.
    own: KeymapConfig,
    /// The model, layout, variant and options of the keymap in use.
    applied: RuleNames,
}

impl LayoutFollow {
    /// Starts watching localed for a new layout. `own` is vtium.conf's
    /// keymap settings, and `system` the system's layout that vtium read at
    /// its start, which together gave the keymap in use.
    pub fn open(own: KeymapConfig, system: &RuleNames) -> Result<LayoutFollow, Error> {
        LayoutFollow::with(SystemSignals::open()?, own, system)
    }

    /// The bus connection's descriptor, which turns readable when the bus
    /// has sent something. Its owner calls [`LayoutFollow::dispatch`] then.
    pub fn fd(&self) -> io::Result<OwnedFd> {
        self.signals.fd()
    }

    /// Reads what the bus sent. When localed announced a new layout, vtium
    /// works out the system's layout again and gives `input` the keymap it
    /// names, if it differs from the keymap in use. `Err` when the bus
    /// connection fails, after which nothing more arrives on it.
    pub fn dispatch(&mut self, input: Option<&mut Input<Direct>>) -> Result<(), Error> {
        self.dispatch_in(Path::new("/"), localed::read, input)
            .map(|_| ())
    }
}

impl<S: Signals> LayoutFollow<S> {
    fn with(mut signals: S, own: KeymapConfig, system: &RuleNames) -> Result<Self, Error> {
        watch_localed(&mut signals)?;
        Ok(LayoutFollow {
            signals,
            applied: names_of(&own.clone().follow(system)),
            own,
        })
    }

    /// [`LayoutFollow::dispatch`], with the system's files under `root` and
    /// localed read through `localed`. It returns whether `input` took a
    /// new keymap.
    pub(crate) fn dispatch_in<O: Open + 'static>(
        &mut self,
        root: &Path,
        localed: impl FnOnce() -> Result<RuleNames, Error>,
        input: Option<&mut Input<O>>,
    ) -> Result<bool, Error> {
        let changes = self.signals.take()?;
        if !changes.iter().any(Changed::moves_layout) {
            return Ok(false);
        }
        // a whole keymap in vtium.conf leaves the system's names unused
        if self.own.keymap_file.is_some() {
            return Ok(false);
        }
        let found = match system::find(root, Some(localed)) {
            Ok(found) => found,
            Err(e) => {
                log::warn!(target: "input", "The keyboard layout stays as it is. localed announced a new layout, but vtium cannot read it: {e}.");
                return Ok(false);
            }
        };
        let config = self.own.clone().follow(&found.names);
        let names = names_of(&config);
        if names == self.applied {
            return Ok(false);
        }
        let Some(keymap) = Keymap::new(&config) else {
            log::warn!(target: "input", "The keyboard layout stays as it is. The system's new layout {:?} gives no keymap that vtium can build.", names.layout);
            return Ok(false);
        };
        log::info!(
            target: "input",
            "The system's keyboard layout changed, so vtium now uses layout {:?}, model {:?}, variant {:?} and options {:?} on every keyboard. The layout comes from {}.",
            names.layout,
            names.model,
            names.variant,
            names.options,
            origin(&found.origin)
        );
        self.applied = names;
        if let Some(input) = input {
            input.set_keymap(keymap);
        }
        Ok(true)
    }
}

/// The four names a keymap configuration builds its keymap from.
fn names_of(config: &KeymapConfig) -> RuleNames {
    RuleNames {
        model: config.model.clone(),
        layout: config.layout.clone(),
        variant: config.variant.clone(),
        options: config.options.clone(),
    }
}

/// Where a layout came from, in the words of the log line.
fn origin(origin: &Origin) -> String {
    match origin {
        Origin::ConsoleKeymap(keymap) => {
            format!("the console keymap {keymap} in /etc/vconsole.conf")
        }
        Origin::KeyboardFile => "/etc/default/keyboard".to_owned(),
        Origin::Localed => "localed".to_owned(),
        Origin::Default => "xkb's default, because the system sets none".to_owned(),
    }
}

#[expect(unsafe_code, reason = "sd-bus's C interface")]
unsafe extern "C" {
    fn sd_bus_match_signal(
        bus: *mut c_void,
        slot: *mut *mut c_void,
        sender: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut BusError) -> c_int,
        userdata: *mut c_void,
    ) -> c_int;
    fn sd_bus_get_fd(bus: *mut c_void) -> c_int;
    fn sd_bus_process(bus: *mut c_void, r: *mut *mut c_void) -> c_int;
    fn sd_bus_slot_unref(slot: *mut c_void) -> *mut c_void;
    fn sd_bus_message_read(m: *mut c_void, types: *const c_char, ...) -> c_int;
    fn sd_bus_message_enter_container(
        m: *mut c_void,
        kind: c_char,
        contents: *const c_char,
    ) -> c_int;
    fn sd_bus_message_exit_container(m: *mut c_void) -> c_int;
    fn sd_bus_message_skip(m: *mut c_void, types: *const c_char) -> c_int;
}

/// The signals that the match delivered and [`Signals::take`] has not
/// taken yet.
type Delivered = Vec<Changed>;

/// The system bus, through sd-bus, on a connection of its own that stays
/// open while vtium runs.
pub struct SystemSignals {
    bus: *mut c_void,
    slot: *mut c_void,
    delivered: Box<Delivered>,
}

impl SystemSignals {
    /// Connects to the system bus, with the match's call bounded by
    /// [`CALL_TIMEOUT_US`].
    fn open() -> Result<SystemSignals, Error> {
        let mut bus: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus` is a valid out pointer; sd-bus sets it only on
        // success.
        let opened = unsafe { sd_bus_open_system(&mut bus) };
        if opened < 0 {
            return Err(Error::NoBus(errno(opened)));
        }
        let signals = SystemSignals {
            bus,
            slot: ptr::null_mut(),
            delivered: Box::default(),
        };
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `signals.bus` is the connection just opened.
        let timed = unsafe { sd_bus_set_method_call_timeout(signals.bus, CALL_TIMEOUT_US) };
        if timed < 0 {
            log::warn!(target: "input", "cannot bound the bus's answer: {}", errno(timed));
        }
        Ok(signals)
    }

    /// A duplicate of the connection's descriptor, for the loop to watch.
    fn fd(&self) -> io::Result<OwnedFd> {
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the open connection.
        let fd = unsafe { sd_bus_get_fd(self.bus) };
        if fd < 0 {
            return Err(errno(fd));
        }
        #[expect(unsafe_code, reason = "a descriptor sd-bus owns")]
        // SAFETY: `fd` is the connection's descriptor, open for as long as
        // the connection is; it is only borrowed to duplicate it.
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        borrowed.try_clone_to_owned()
    }
}

impl Drop for SystemSignals {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: the slot goes first, so the handler never sees the queue
        // freed; each pointer is freed once (sd_bus_slot_unref takes null).
        unsafe {
            sd_bus_slot_unref(self.slot);
            sd_bus_flush_close_unref(self.bus);
        }
    }
}

fn errno(code: c_int) -> io::Error {
    io::Error::from_raw_os_error(code.saturating_neg())
}

/// A string sd-bus owns, as a Rust string.
fn text(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    #[expect(unsafe_code, reason = "a string sd-bus owns")]
    // SAFETY: sd-bus gives NUL-ended strings that live as long as the
    // message they belong to, which outlives this call.
    let s = unsafe { CStr::from_ptr(p) };
    Some(s.to_string_lossy().into_owned())
}

/// Reads one string argument of `m`: `Ok(None)` at the end of an array.
#[expect(unsafe_code, reason = "sd-bus's C interface")]
unsafe fn read_string(m: *mut c_void) -> Result<Option<String>, c_int> {
    let mut s: *const c_char = ptr::null();
    // SAFETY: the caller passes the message sd-bus is dispatching; `s` is a
    // live local of the type the signature "s" reads into.
    let r = unsafe { sd_bus_message_read(m, c"s".as_ptr(), &mut s as *mut *const c_char) };
    match r {
        r if r < 0 => Err(r),
        0 => Ok(None),
        _ => Ok(text(s)),
    }
}

/// Reads `PropertiesChanged`'s arguments, `sa{sv}as`, from `m`.
#[expect(unsafe_code, reason = "sd-bus's C interface")]
unsafe fn read_changed(m: *mut c_void) -> Result<Changed, c_int> {
    let check = |r: c_int| if r < 0 { Err(r) } else { Ok(r) };
    let mut changed = Changed {
        // SAFETY: the caller passes the message sd-bus is dispatching.
        interface: unsafe { read_string(m) }?.unwrap_or_default(),
        properties: Vec::new(),
    };
    // SAFETY: as above; each container is entered and left in turn, as the
    // signature "a{sv}" and then "as" lays them out.
    unsafe {
        check(sd_bus_message_enter_container(
            m,
            c_char::from_ne_bytes(*b"a"),
            c"{sv}".as_ptr(),
        ))?;
        while check(sd_bus_message_enter_container(
            m,
            c_char::from_ne_bytes(*b"e"),
            c"sv".as_ptr(),
        ))? > 0
        {
            if let Some(name) = read_string(m)? {
                changed.properties.push(name);
            }
            check(sd_bus_message_skip(m, c"v".as_ptr()))?;
            check(sd_bus_message_exit_container(m))?;
        }
        check(sd_bus_message_exit_container(m))?;
        check(sd_bus_message_enter_container(
            m,
            c_char::from_ne_bytes(*b"a"),
            c"s".as_ptr(),
        ))?;
        while let Some(name) = read_string(m)? {
            changed.properties.push(name);
        }
        check(sd_bus_message_exit_container(m))?;
    }
    Ok(changed)
}

/// The `PropertiesChanged` signal's handler: puts what changed in the
/// queue that `userdata` points to. A signal it cannot read is logged and
/// dropped.
#[expect(unsafe_code, reason = "sd-bus calls it with its message")]
unsafe extern "C" fn properties_changed(
    m: *mut c_void,
    userdata: *mut c_void,
    _: *mut BusError,
) -> c_int {
    // SAFETY: `m` is the signal sd-bus is dispatching.
    match unsafe { read_changed(m) } {
        Ok(changed) => {
            // SAFETY: `userdata` is the `delivered` box of the
            // `SystemSignals` whose slot this handler is on; the slot goes
            // before the box does, and sd-bus calls the handler only from
            // that bus's own process call, on this thread.
            let delivered = unsafe { &mut *userdata.cast::<Delivered>() };
            delivered.push(changed);
        }
        Err(r) => {
            log::debug!(target: "input", "cannot read localed's PropertiesChanged: {}", errno(r));
        }
    }
    0
}

impl Signals for SystemSignals {
    fn watch(
        &mut self,
        sender: Option<&CStr>,
        path: Option<&CStr>,
        interface: Option<&CStr>,
        member: Option<&CStr>,
    ) -> Result<(), Error> {
        let or_null = |s: Option<&CStr>| s.map_or(ptr::null(), CStr::as_ptr);
        let userdata: *mut Delivered = &mut *self.delivered;
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.bus` is the open connection; every string is
        // NUL-ended or null, which matches any; `userdata` is the boxed
        // queue, which outlives the slot (see Drop).
        let r = unsafe {
            sd_bus_match_signal(
                self.bus,
                &mut self.slot,
                or_null(sender),
                or_null(path),
                or_null(interface),
                or_null(member),
                properties_changed,
                userdata.cast(),
            )
        };
        if r < 0 {
            return Err(Error::NoBus(errno(r)));
        }
        Ok(())
    }

    fn take(&mut self) -> Result<Vec<Changed>, Error> {
        loop {
            #[expect(unsafe_code, reason = "sd-bus's C interface")]
            // SAFETY: `self.bus` is the open connection; a null message
            // pointer asks for none back.
            let processed = unsafe { sd_bus_process(self.bus, ptr::null_mut()) };
            if processed < 0 {
                return Err(Error::NoBus(errno(processed)));
            }
            if processed == 0 {
                return Ok(std::mem::take(&mut *self.delivered));
            }
        }
    }
}

#[cfg(test)]
mod tests;
