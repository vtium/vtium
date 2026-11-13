//! The few logind calls the probes make over sd-bus, through libsystemd
//! loaded at run time. A session's controller must keep its bus connection
//! open, since logind drops the controller when the connection closes, so
//! one connection serves every call of a probe.

use std::ffi::{c_char, c_int, c_void};

use crate::sys::{Library, cstring, text};

#[repr(C)]
struct BusError {
    name: *const c_char,
    message: *const c_char,
    need_free: c_int,
}

type Open = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type NewCall = unsafe extern "C" fn(
    *mut c_void,
    *mut *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
) -> c_int;
type Append = unsafe extern "C" fn(*mut c_void, c_char, *const c_void) -> c_int;
type Call =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u64, *mut BusError, *mut *mut c_void) -> c_int;
type Read = unsafe extern "C" fn(*mut c_void, c_char, *mut c_void) -> c_int;
type Unref = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type ErrorFree = unsafe extern "C" fn(*mut BusError);

/// An argument of a call.
pub enum Arg {
    Bool(bool),
    U32(u32),
}

/// What a reply holds that the probes read.
pub enum Want {
    Nothing,
    /// a unix fd and a boolean, as TakeDevice answers
    FdBool,
    /// one string, as CanPowerOff answers
    Str,
}

pub enum Reply {
    Nothing,
    FdBool(c_int, bool),
    Str(String),
}

pub struct Bus {
    _lib: Library,
    bus: *mut c_void,
    new_call: NewCall,
    append: Append,
    call: Call,
    read: Read,
    msg_unref: Unref,
    bus_close: Unref,
    error_free: ErrorFree,
}

impl Bus {
    pub fn system() -> Result<Bus, String> {
        let lib = Library::open("libsystemd.so.0")?;
        // SAFETY: each type matches the function's declaration in
        // systemd/sd-bus.h.
        let (open, new_call, append, call, read, msg_unref, bus_close, error_free) = unsafe {
            (
                lib.get::<Open>("sd_bus_open_system")?,
                lib.get::<NewCall>("sd_bus_message_new_method_call")?,
                lib.get::<Append>("sd_bus_message_append_basic")?,
                lib.get::<Call>("sd_bus_call")?,
                lib.get::<Read>("sd_bus_message_read_basic")?,
                lib.get::<Unref>("sd_bus_message_unref")?,
                lib.get::<Unref>("sd_bus_flush_close_unref")?,
                lib.get::<ErrorFree>("sd_bus_error_free")?,
            )
        };
        let mut bus = std::ptr::null_mut();
        // SAFETY: fills a bus pointer.
        let r = unsafe { open(&mut bus) };
        if r < 0 {
            return Err(format!(
                "sd_bus_open_system: {}",
                crate::sys::errno_text(-r)
            ));
        }
        Ok(Bus {
            _lib: lib,
            bus,
            new_call,
            append,
            call,
            read,
            msg_unref,
            bus_close,
            error_free,
        })
    }

    /// Calls `member` on logind's `path` and `iface`, and returns the reply,
    /// or the error's name and message.
    pub fn login1(
        &self,
        path: &str,
        iface: &str,
        member: &str,
        args: &[Arg],
        want: Want,
    ) -> Result<Reply, String> {
        self.method("org.freedesktop.login1", path, iface, member, args, want)
    }

    pub fn method(
        &self,
        dest: &str,
        path: &str,
        iface: &str,
        member: &str,
        args: &[Arg],
        want: Want,
    ) -> Result<Reply, String> {
        let (dest, path, iface, member) = (
            cstring(dest),
            cstring(path),
            cstring(iface),
            cstring(member),
        );
        let mut msg = std::ptr::null_mut();
        // SAFETY: the bus is open, the strings live through the call, and
        // each appended value has the type its code names.
        unsafe {
            let r = (self.new_call)(
                self.bus,
                &mut msg,
                dest.as_ptr(),
                path.as_ptr(),
                iface.as_ptr(),
                member.as_ptr(),
            );
            if r < 0 {
                return Err(format!("new call: {}", crate::sys::errno_text(-r)));
            }
            for a in args {
                let r = match a {
                    Arg::Bool(b) => {
                        let v: c_int = c_int::from(*b);
                        (self.append)(msg, b'b' as c_char, (&raw const v).cast())
                    }
                    Arg::U32(u) => (self.append)(msg, b'u' as c_char, (u as *const u32).cast()),
                };
                if r < 0 {
                    (self.msg_unref)(msg);
                    return Err(format!("append: {}", crate::sys::errno_text(-r)));
                }
            }
            let mut err = BusError {
                name: std::ptr::null(),
                message: std::ptr::null(),
                need_free: 0,
            };
            let mut reply = std::ptr::null_mut();
            let r = (self.call)(self.bus, msg, 25_000_000, &mut err, &mut reply);
            (self.msg_unref)(msg);
            if r < 0 {
                let e = format!("{}: {}", text(err.name), text(err.message));
                (self.error_free)(&mut err);
                return Err(e);
            }
            let out = match want {
                Want::Nothing => Ok(Reply::Nothing),
                Want::FdBool => {
                    let mut fd: c_int = -1;
                    let mut inactive: c_int = 0;
                    let r1 = (self.read)(reply, b'h' as c_char, (&raw mut fd).cast());
                    let r2 = (self.read)(reply, b'b' as c_char, (&raw mut inactive).cast());
                    if r1 < 0 || r2 < 0 {
                        Err("cannot read the reply".to_string())
                    } else {
                        // The reply owns its descriptor, so the probe keeps
                        // a copy of its own.
                        Ok(Reply::FdBool(
                            libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3),
                            inactive != 0,
                        ))
                    }
                }
                Want::Str => {
                    let mut s: *const c_char = std::ptr::null();
                    if (self.read)(reply, b's' as c_char, (&raw mut s).cast()) < 0 {
                        Err("cannot read the reply".to_string())
                    } else {
                        Ok(Reply::Str(text(s)))
                    }
                }
            };
            (self.msg_unref)(reply);
            out
        }
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        // SAFETY: closes the bus once.
        unsafe { (self.bus_close)(self.bus) };
    }
}
