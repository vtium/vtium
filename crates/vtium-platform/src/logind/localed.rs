//! The system's keyboard layout from localed, the service behind
//! `localectl`, over the system bus (ADR-1400's L2 layer, card 1b's
//! question Q10). localed holds the layout as the X11 rules' names: the
//! properties `X11Layout`, `X11Model`, `X11Variant` and `X11Options` of
//! the interface `org.freedesktop.locale1` (org.freedesktop.locale1(5),
//! systemd 262; external, not re-verified). vtium reads them when it builds
//! the keymap at its start, and again each time localed announces a new
//! layout ([`super::follow`]).

use std::ffi::{CStr, c_char, c_int, c_void};
use std::io;
use std::ptr;

use super::{
    BusError, CALL_TIMEOUT_US, Error, classify, sd_bus_error_free, sd_bus_flush_close_unref,
    sd_bus_open_system, sd_bus_set_method_call_timeout,
};
use crate::input::RuleNames;

/// localed's bus name, which is also the name of its interface.
pub(super) const LOCALE1: &CStr = c"org.freedesktop.locale1";
/// The object localed serves its properties on.
pub(super) const LOCALE1_PATH: &CStr = c"/org/freedesktop/locale1";

#[expect(unsafe_code, reason = "sd-bus's C interface")]
unsafe extern "C" {
    fn sd_bus_get_property_string(
        bus: *mut c_void,
        destination: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        ret_error: *mut BusError,
        ret: *mut *mut c_char,
    ) -> c_int;
}

/// The bus that [`layout_from`] reads, so the tests can stand in for it.
pub(crate) trait Bus {
    /// The string property `member` of `interface`, on the object at
    /// `path` of the service `destination`.
    fn string_property(
        &mut self,
        destination: &CStr,
        path: &CStr,
        interface: &CStr,
        member: &CStr,
    ) -> Result<String, Error>;
}

/// The system bus, through sd-bus.
struct SystemBus(*mut c_void);

impl SystemBus {
    /// Connects to the system bus, with each call bounded by
    /// [`CALL_TIMEOUT_US`], as localed may have to start first.
    fn open() -> Result<SystemBus, Error> {
        let mut bus: *mut c_void = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus` is a valid out pointer; sd-bus sets it only on
        // success.
        let opened = unsafe { sd_bus_open_system(&mut bus) };
        if opened < 0 {
            return Err(Error::NoBus(io::Error::from_raw_os_error(
                opened.saturating_neg(),
            )));
        }
        let bus = SystemBus(bus);
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `bus.0` is the connection just opened.
        let timed = unsafe { sd_bus_set_method_call_timeout(bus.0, CALL_TIMEOUT_US) };
        if timed < 0 {
            log::warn!(target: "input", "cannot bound localed's answer: {}",
                io::Error::from_raw_os_error(timed.saturating_neg()));
        }
        Ok(bus)
    }
}

impl Drop for SystemBus {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.0` is the connection `open` made, freed once here.
        unsafe {
            sd_bus_flush_close_unref(self.0);
        }
    }
}

impl Bus for SystemBus {
    fn string_property(
        &mut self,
        destination: &CStr,
        path: &CStr,
        interface: &CStr,
        member: &CStr,
    ) -> Result<String, Error> {
        let mut error = BusError {
            name: ptr::null(),
            message: ptr::null(),
            need_free: 0,
        };
        let mut value: *mut c_char = ptr::null_mut();
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: `self.0` is the open connection; every string is
        // NUL-ended; `error` starts as SD_BUS_ERROR_NULL and `value` as
        // null, as sd-bus requires of out parameters.
        let got = unsafe {
            sd_bus_get_property_string(
                self.0,
                destination.as_ptr(),
                path.as_ptr(),
                interface.as_ptr(),
                member.as_ptr(),
                &mut error,
                &mut value,
            )
        };
        let text = |p: *const c_char| -> Option<String> {
            if p.is_null() {
                return None;
            }
            #[expect(unsafe_code, reason = "a string sd-bus owns")]
            // SAFETY: sd-bus sets the error's name and message, and the
            // value, to NUL-ended strings or leaves them null; each lives
            // until it is freed, below.
            let s = unsafe { CStr::from_ptr(p) };
            Some(s.to_string_lossy().into_owned())
        };
        let result = if got < 0 {
            Err(classify(text(error.name), text(error.message), got))
        } else {
            Ok(text(value).unwrap_or_default())
        };
        #[expect(unsafe_code, reason = "sd-bus's C interface")]
        // SAFETY: the error is freed once (a no-op while it is null), and
        // the value is the string sd-bus allocated with malloc for its
        // caller to free, or null.
        unsafe {
            sd_bus_error_free(&mut error);
            libc::free(value.cast());
        }
        result
    }
}

/// localed's names of the system's keyboard layout, which
/// [`crate::input::system_layout`] takes when the console keymap gives none. A name the system
/// does not set is empty. When localed cannot be asked, the names are
/// empty and vtium logs why: xkb's default layout then applies, unless
/// vtium.conf sets one.
pub fn layout() -> RuleNames {
    names_or_default(read())
}

/// localed's names of the system's keyboard layout, read now over a
/// connection of their own, or why localed cannot be asked.
pub(crate) fn read() -> Result<RuleNames, Error> {
    SystemBus::open().and_then(|mut bus| layout_from(&mut bus))
}

/// The names `read` gave, or none with the reason logged.
fn names_or_default(read: Result<RuleNames, Error>) -> RuleNames {
    match read {
        Ok(names) => {
            log::info!(
                target: "input",
                "the system's keyboard layout from localed: layout {:?}, model {:?}, variant {:?}, options {:?}",
                names.layout, names.model, names.variant, names.options
            );
            names
        }
        Err(Error::NoBus(e)) => {
            log::info!(target: "input", "The system's keyboard layout is not read, because there is no system bus: {e}. vtium uses xkb's default layout unless vtium.conf sets one.");
            RuleNames::default()
        }
        Err(Error::NotRunning(name)) => {
            log::info!(target: "input", "The system's keyboard layout is not read, because localed is not on the system bus ({name}). vtium uses xkb's default layout unless vtium.conf sets one.");
            RuleNames::default()
        }
        Err(Error::Refused { name, message }) => {
            log::warn!(target: "input", "The system's keyboard layout is not read, because localed refused: {name}: {message}. vtium uses xkb's default layout unless vtium.conf sets one.");
            RuleNames::default()
        }
    }
}

/// The four names from localed on `bus`. The first that cannot be read
/// stops the rest, so a localed that cannot start costs one call's bound.
pub(crate) fn layout_from(bus: &mut impl Bus) -> Result<RuleNames, Error> {
    let mut get = |member: &CStr| bus.string_property(LOCALE1, LOCALE1_PATH, LOCALE1, member);
    Ok(RuleNames {
        layout: get(c"X11Layout")?,
        model: get(c"X11Model")?,
        variant: get(c"X11Variant")?,
        options: get(c"X11Options")?,
    })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::panic, reason = "a test fails by panicking")]

    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::Instant;

    use xkbcommon::xkb::keysyms;

    use super::*;
    use crate::input::fake::{self, Nodes};
    use crate::input::{Input, InputConfig, InputEvent, Keymap, KeymapConfig, RawEvent};
    use crate::ioctl::EV_KEY;

    /// The system bus with a localed on it, or none. It answers as the bus
    /// and sd-bus do: a service nobody owns is `ServiceUnknown`; on
    /// localed, another object is `UnknownObject`, another interface
    /// `UnknownInterface` and another property `UnknownProperty`; and every
    /// property localed holds is a string.
    #[derive(Default)]
    struct StandInBus {
        /// localed's properties by name; `None` while localed is not on
        /// the bus.
        localed: Option<BTreeMap<&'static str, &'static str>>,
        /// The properties asked, in order.
        asked: Vec<String>,
    }

    fn refused(name: &str) -> Error {
        Error::Refused {
            name: name.to_owned(),
            message: String::new(),
        }
    }

    impl Bus for StandInBus {
        fn string_property(
            &mut self,
            destination: &CStr,
            path: &CStr,
            interface: &CStr,
            member: &CStr,
        ) -> Result<String, Error> {
            let member = member.to_string_lossy().into_owned();
            self.asked.push(member.clone());
            let Some(properties) = &self.localed else {
                return Err(classify(
                    Some("org.freedesktop.DBus.Error.ServiceUnknown".to_owned()),
                    None,
                    -2,
                ));
            };
            if destination != LOCALE1 {
                return Err(classify(
                    Some("org.freedesktop.DBus.Error.ServiceUnknown".to_owned()),
                    None,
                    -2,
                ));
            }
            if path != LOCALE1_PATH {
                return Err(refused("org.freedesktop.DBus.Error.UnknownObject"));
            }
            if interface != LOCALE1 {
                return Err(refused("org.freedesktop.DBus.Error.UnknownInterface"));
            }
            properties
                .get(member.as_str())
                .map(|v| (*v).to_owned())
                .ok_or_else(|| refused("org.freedesktop.DBus.Error.UnknownProperty"))
        }
    }

    fn german() -> StandInBus {
        StandInBus {
            localed: Some(BTreeMap::from([
                ("X11Layout", "de"),
                ("X11Model", "pc105"),
                ("X11Variant", "nodeadkeys"),
                ("X11Options", "compose:ralt"),
                ("VConsoleKeymap", "de-latin1-nodeadkeys"),
            ])),
            asked: Vec::new(),
        }
    }

    /// The keysyms a press of the evdev key `code` gives with `config`'s
    /// keymap.
    fn press(config: &KeymapConfig, code: u16) -> Vec<u32> {
        let keymap = Keymap::new(config).unwrap_or_else(|| panic!("a keymap compiles"));
        let nodes = Nodes::default();
        let node = nodes.add("/dev/input/event0", fake::keyboard());
        let mut input = Input::new(nodes, keymap, InputConfig::default());
        input.wake_up();
        let id = input
            .add(Path::new("/dev/input/event0"))
            .unwrap_or_else(|| panic!("the keyboard is taken"));
        node.borrow_mut().queue.push_back(RawEvent {
            kind: EV_KEY,
            code,
            value: 1,
        });
        #[expect(
            clippy::disallowed_methods,
            reason = "the input reads its keys at a time, here the test's"
        )]
        let now = Instant::now();
        input.dispatch(id, now);
        input
            .events()
            .filter_map(|e| match e {
                InputEvent::Key(k) => Some(k.keysyms),
                _ => None,
            })
            .flatten()
            .collect()
    }

    const KEY_Y: u16 = 21;

    /// localed's four names become the layout an unconfigured vtium types
    /// with: on a German layout, the key labelled Y on a US keyboard types
    /// z.
    // covers: platform.system-layout
    #[test]
    fn the_layout_follows_localeds_names() {
        let mut bus = german();
        let names = layout_from(&mut bus).unwrap_or_else(|e| panic!("localed answers: {e}"));
        assert_eq!(
            names,
            RuleNames {
                model: "pc105".to_owned(),
                layout: "de".to_owned(),
                variant: "nodeadkeys".to_owned(),
                options: "compose:ralt".to_owned(),
            }
        );
        assert_eq!(
            bus.asked,
            ["X11Layout", "X11Model", "X11Variant", "X11Options"]
        );
        let config = KeymapConfig::default().follow(&names);
        assert_eq!(press(&config, KEY_Y), [keysyms::KEY_z]);
    }

    /// vtium.conf's `xkb-layout` wins over localed's layout, so the same key
    /// types y; localed's variant, which belongs to localed's layout, is
    /// not taken with it.
    // covers: platform.system-layout
    #[test]
    fn vtium_confs_layout_wins_over_localeds() {
        let names = layout_from(&mut german()).unwrap_or_else(|e| panic!("localed answers: {e}"));
        let config = KeymapConfig {
            layout: "us".to_owned(),
            ..KeymapConfig::default()
        }
        .follow(&names);
        assert_eq!(config.layout, "us");
        assert_eq!(config.variant, "");
        assert_eq!(press(&config, KEY_Y), [keysyms::KEY_y]);
    }

    /// Without localed on the bus, the first name asked stops the rest, and
    /// the names are empty: xkb's default layout applies.
    // covers: platform.system-layout
    #[test]
    fn without_localed_the_names_are_empty() {
        let mut bus = StandInBus::default();
        let read = layout_from(&mut bus);
        assert!(matches!(read, Err(Error::NotRunning(_))), "{read:?}");
        assert_eq!(bus.asked, ["X11Layout"]);
        assert_eq!(names_or_default(read), RuleNames::default());
    }

    /// A localed that holds no layout answers empty names, which leave
    /// vtium.conf's keys and xkb's defaults alone.
    // covers: platform.system-layout
    #[test]
    fn a_localed_with_no_layout_changes_nothing() {
        let mut bus = StandInBus {
            localed: Some(BTreeMap::from([
                ("X11Layout", ""),
                ("X11Model", ""),
                ("X11Variant", ""),
                ("X11Options", ""),
            ])),
            asked: Vec::new(),
        };
        let names = layout_from(&mut bus).unwrap_or_else(|e| panic!("localed answers: {e}"));
        assert_eq!(names, RuleNames::default());
        let config = KeymapConfig {
            options: "ctrl:nocaps".to_owned(),
            ..KeymapConfig::default()
        }
        .follow(&names);
        assert_eq!(config.options, "ctrl:nocaps");
        assert_eq!(config.layout, "");
    }
}
