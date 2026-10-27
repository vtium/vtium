//! logind over the system bus, through sd-bus from libsystemd (feature
//! `logind`, ADR-1400's L2 layer). The binding is the few calls vtium
//! makes; everything around them is Rust.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::io;
use std::ptr;

/// sd-bus's error, as `sd-bus.h` lays it out.
#[repr(C)]
struct BusError {
    name: *const c_char,
    message: *const c_char,
    need_free: c_int,
}

#[expect(unsafe_code, reason = "sd-bus's C interface")]
#[link(name = "systemd")]
unsafe extern "C" {
    fn sd_bus_open_system(bus: *mut *mut c_void) -> c_int;
    fn sd_bus_call_method(
        bus: *mut c_void,
        destination: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        ret_error: *mut BusError,
        reply: *mut *mut c_void,
        types: *const c_char,
        ...
    ) -> c_int;
    fn sd_bus_error_free(e: *mut BusError);
    fn sd_bus_message_unref(m: *mut c_void) -> *mut c_void;
    fn sd_bus_flush_close_unref(bus: *mut c_void) -> *mut c_void;
}

/// Why logind did not take a request.
#[derive(Debug)]
pub enum Error {
    /// The system bus cannot be reached.
    NoBus(io::Error),
    /// The bus has no logind on it, and cannot start one.
    NotRunning(String),
    /// logind, or the bus on its behalf, refused: an inhibitor, polkit, a
    /// sleep state the machine lacks.
    Refused { name: String, message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoBus(e) => write!(f, "no system bus: {e}"),
            Error::NotRunning(name) => write!(f, "logind is not on the bus ({name})"),
            Error::Refused { name, message } => write!(f, "{name}: {message}"),
        }
    }
}

/// The bus errors that mean nothing owns, and nothing can start,
/// `org.freedesktop.login1`.
const NOT_RUNNING: [&str; 2] = [
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
];

/// The error a failed call gives, from the bus error's name and message.
fn classify(name: Option<String>, message: Option<String>, code: c_int) -> Error {
    match name {
        Some(name) if NOT_RUNNING.contains(&name.as_str()) => Error::NotRunning(name),
        Some(name) => Error::Refused {
            name,
            message: message.unwrap_or_default(),
        },
        None => Error::Refused {
            name: "errno".to_owned(),
            message: io::Error::from_raw_os_error(code.saturating_neg()).to_string(),
        },
    }
}

/// logind's `Suspend(false)`: suspend once the inhibitors that delay
/// sleep let go, without asking polkit interactively. It returns when
/// logind has taken the request, before the machine sleeps.
pub fn suspend() -> Result<(), Error> {
    let mut bus: *mut c_void = ptr::null_mut();
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: `bus` is a valid out pointer; sd-bus sets it only on success.
    let opened = unsafe { sd_bus_open_system(&mut bus) };
    if opened < 0 {
        return Err(Error::NoBus(io::Error::from_raw_os_error(
            opened.saturating_neg(),
        )));
    }
    let mut error = BusError {
        name: ptr::null(),
        message: ptr::null(),
        need_free: 0,
    };
    let mut reply: *mut c_void = ptr::null_mut();
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: `bus` is the open connection; every string is a NUL-ended
    // literal; `error` starts as SD_BUS_ERROR_NULL and `reply` as null, as
    // sd-bus requires of out parameters; the signature "b" takes one C
    // int, which the variadic argument is.
    let called = unsafe {
        sd_bus_call_method(
            bus,
            c"org.freedesktop.login1".as_ptr(),
            c"/org/freedesktop/login1".as_ptr(),
            c"org.freedesktop.login1.Manager".as_ptr(),
            c"Suspend".as_ptr(),
            &mut error,
            &mut reply,
            c"b".as_ptr(),
            0 as c_int,
        )
    };
    let text = |p: *const c_char| -> Option<String> {
        if p.is_null() {
            return None;
        }
        #[expect(unsafe_code, reason = "a string sd-bus owns")]
        // SAFETY: sd-bus sets the error's name and message to NUL-ended
        // strings or leaves them null, and they live until the error is
        // freed, below.
        let s = unsafe { CStr::from_ptr(p) };
        Some(s.to_string_lossy().into_owned())
    };
    let result = if called < 0 {
        Err(classify(text(error.name), text(error.message), called))
    } else {
        Ok(())
    };
    #[expect(unsafe_code, reason = "sd-bus's C interface")]
    // SAFETY: each is what sd-bus gave above, freed once: the error (a
    // no-op while it is null), the reply (null-tolerant) and the bus.
    unsafe {
        sd_bus_error_free(&mut error);
        sd_bus_message_unref(reply);
        sd_bus_flush_close_unref(bus);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_logind_on_the_bus_is_told_from_a_refusal() {
        for name in NOT_RUNNING {
            assert!(matches!(
                classify(Some(name.to_owned()), None, -2),
                Error::NotRunning(_)
            ));
        }
        for name in [
            "org.freedesktop.login1.BlockedByInhibitor",
            "org.freedesktop.DBus.Error.AccessDenied",
            "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
        ] {
            assert!(matches!(
                classify(Some(name.to_owned()), Some("no".to_owned()), -1),
                Error::Refused { .. }
            ));
        }
        assert!(matches!(classify(None, None, -110), Error::Refused { .. }));
    }
}
