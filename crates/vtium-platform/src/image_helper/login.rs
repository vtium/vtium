//! sd-login's few calls the image helper's identity makes (feature
//! `logind`), from libsystemd or libelogind, whichever the build script
//! links for sd-bus; both export them with these signatures.

use std::ffi::{CStr, c_char, c_int, c_uint};

use super::identity::LoginSession;

#[expect(unsafe_code, reason = "sd-login's C interface")]
unsafe extern "C" {
    fn sd_get_seats(seats: *mut *mut *mut c_char) -> c_int;
    fn sd_pid_get_session(pid: libc::pid_t, session: *mut *mut c_char) -> c_int;
    fn sd_session_get_class(session: *const c_char, class: *mut *mut c_char) -> c_int;
    fn sd_session_get_uid(session: *const c_char, uid: *mut c_uint) -> c_int;
}

/// A string sd-login allocated, freed once.
struct Owned(*mut c_char);

impl Owned {
    fn as_c(&self) -> &CStr {
        #[expect(unsafe_code, reason = "a string sd-login returned")]
        // SAFETY: sd-login returns a NUL-terminated string on success,
        // and an `Owned` is made only from such a return.
        unsafe {
            CStr::from_ptr(self.0)
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        #[expect(
            unsafe_code,
            reason = "freeing a string sd-login allocated with malloc"
        )]
        // SAFETY: the pointer came from sd-login, which allocates with
        // malloc, and is freed only here.
        unsafe {
            libc::free(self.0.cast());
        }
    }
}

/// How many seats logind knows: 0 when it is not running, `None` on an
/// error.
pub fn seats() -> Option<u32> {
    #[expect(unsafe_code, reason = "sd_get_seats(3), counting only")]
    // SAFETY: a null array pointer asks for the count alone.
    let n = unsafe { sd_get_seats(std::ptr::null_mut()) };
    u32::try_from(n).ok()
}

/// The logind session of `pid`, or the error sd-login answered.
fn session_of(pid: libc::pid_t) -> Result<Owned, c_int> {
    let mut out: *mut c_char = std::ptr::null_mut();
    #[expect(unsafe_code, reason = "sd_pid_get_session(3)")]
    // SAFETY: `out` is a live local, which sd-login sets on success.
    let r = unsafe { sd_pid_get_session(pid, &mut out) };
    if r < 0 || out.is_null() {
        return Err(r);
    }
    Ok(Owned(out))
}

/// The login child's session: its user, its class, and whether it is
/// vtium's own. `None` when the child is in no session or any lookup
/// fails, which the rules take as before login.
pub fn session(pid: u32) -> Option<LoginSession> {
    let pid = libc::pid_t::try_from(pid).ok().filter(|&p| p > 0)?;
    let session = session_of(pid).ok()?;
    let shared = match session_of(rustix::process::getpid().as_raw_nonzero().get()) {
        Ok(own) => own.as_c() == session.as_c(),
        // vtium in no session of its own
        Err(e) if e == -libc::ENODATA || e == -libc::ENXIO => false,
        Err(_) => return None,
    };
    let mut class: *mut c_char = std::ptr::null_mut();
    #[expect(unsafe_code, reason = "sd_session_get_class(3)")]
    // SAFETY: the session name is a live NUL-terminated string and
    // `class` a live local, which sd-login sets on success.
    let r = unsafe { sd_session_get_class(session.0, &mut class) };
    if r < 0 || class.is_null() {
        return None;
    }
    let class = Owned(class);
    let mut uid: c_uint = 0;
    #[expect(unsafe_code, reason = "sd_session_get_uid(3)")]
    // SAFETY: as above; sd-login writes the uid through the pointer.
    let r = unsafe { sd_session_get_uid(session.0, &mut uid) };
    if r < 0 {
        return None;
    }
    Some(LoginSession {
        uid,
        class: class.as_c().to_string_lossy().into_owned(),
        shared,
    })
}
