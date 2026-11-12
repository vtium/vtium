//! libseat itself, through the `libseat` crate.

use std::ffi::{OsString, c_char, c_int, c_uint, c_void};
use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Mutex, Once, PoisonError};

use super::{Inbox, Manager, Seat, Told};

/// A C `va_list` as a function receives one, which vtium only passes on to
/// `vsnprintf`. On each Linux target vtium builds for it is one pointer
/// wide in the arguments of a call: x86-64's `va_list` is an array of one
/// structure, which a parameter receives as a pointer to it; aarch64's is
/// a 32-byte structure, which AAPCS64 passes by a pointer to a copy; and
/// 32-bit ARM's is a structure holding one pointer, which AAPCS passes in
/// one register. So a pointer received here and handed on unchanged is
/// the `va_list` libseat gave. The test `libseats_log_reaches_vtium`
/// checks it on each target the tests run on.
type VaList = *mut c_void;

/// libseat's `libseat_log_func`: the level, a printf format and its
/// arguments.
type LogFunc = unsafe extern "C" fn(level: c_uint, format: *const c_char, args: VaList);

#[expect(unsafe_code, reason = "libseat's log handler and C's vsnprintf")]
unsafe extern "C" {
    /// libseat.h's `libseat_set_log_handler`, which the `libseat-sys`
    /// binding leaves out because its argument takes a `va_list`.
    fn libseat_set_log_handler(handler: Option<LogFunc>);
    fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, args: VaList) -> c_int;
}

/// The longest line of libseat's that vtium keeps, in bytes; a longer one
/// is cut there.
const LINE_MAX: usize = 512;

/// The errors libseat reports while vtium opens the seat, held back for
/// [`Manager::said`], or `None` while each goes to vtium's log at once.
static HELD: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// libseat's message without the place in its source that its log macros
/// put first: `[libseat/libseat.c:58] Backend 'seatd' failed ...` becomes
/// `Backend 'seatd' failed ...`.
fn without_place(text: &str) -> &str {
    text.strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .filter(|(place, _)| place.contains(':') && !place.contains(' '))
        .map_or(text, |(_, message)| message)
}

/// libseat's log handler: formats the message and holds it back while
/// vtium opens the seat, or else logs it as a warning.
#[expect(unsafe_code, reason = "a C callback that formats a va_list")]
unsafe extern "C" fn heard(_level: c_uint, format: *const c_char, args: VaList) {
    if format.is_null() {
        return;
    }
    let mut line = [0u8; LINE_MAX];
    // SAFETY: `line` has `LINE_MAX` bytes, which vsnprintf writes at most,
    // a NUL included; `format` and `args` are the format string and the
    // va_list libseat passes for it (see `VaList`), used once.
    let written = unsafe { vsnprintf(line.as_mut_ptr().cast(), line.len(), format, args) };
    let Ok(written) = usize::try_from(written) else {
        return;
    };
    let len = written.min(LINE_MAX.saturating_sub(1));
    let text = String::from_utf8_lossy(line.get(..len).unwrap_or_default());
    let text = without_place(text.trim_end()).to_owned();
    let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
    match held.as_mut() {
        Some(said) => said.push(text),
        None => {
            drop(held);
            log::warn!(target: "seat", "{}", super::libseat_error(&text));
        }
    }
}

/// Gives libseat vtium's log handler, once for the process.
fn listen() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[expect(unsafe_code, reason = "libseat's log handler")]
        // SAFETY: `heard` matches libseat_log_func and lives as long as the
        // process.
        unsafe {
            libseat_set_log_handler(Some(heard));
        }
    });
}

/// Where seatd listens when `SEATD_SOCK` names no other socket, as libseat
/// looks for it on Linux.
const SEATD_SOCKET: &str = "/run/seatd.sock";

/// The directory of logind's seats, which exists while logind runs.
const LOGIND_SEATS: &str = "/run/systemd/seats";

/// libseat, which opens the seat through the first seat manager that
/// answers: seatd, logind, or the one `LIBSEAT_BACKEND` names.
#[derive(Debug, Default)]
pub struct Libseat {
    /// The errors libseat reported while the last open ran.
    said: Vec<String>,
}

/// A seat libseat opened.
pub struct LibseatSeat {
    seat: libseat::Seat,
}

/// A device libseat opened, holding libseat's descriptor of it, which
/// libseat leaves to its caller: closed through the seat, or, when the
/// seat is gone, by its own drop.
pub struct LibseatDevice {
    device: libseat::Device,
    fd: OwnedFd,
}

impl Manager for Libseat {
    type Seat = LibseatSeat;

    /// libseat hands its errors to vtium's log handler ([`heard`]), which
    /// holds back the ones reported during the open for [`Manager::said`].
    fn open(&mut self, inbox: Inbox) -> io::Result<LibseatSeat> {
        listen();
        libseat::set_log_level(libseat::LogLevel::Error);
        *HELD.lock().unwrap_or_else(PoisonError::into_inner) = Some(Vec::new());
        let listener = Rc::clone(&inbox);
        let seat = libseat::Seat::open(move |_, event| {
            listener.borrow_mut().push_back(match event {
                libseat::SeatEvent::Enable => Told::Enable,
                libseat::SeatEvent::Disable => Told::Disable,
            });
        });
        self.said = HELD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap_or_default();
        Ok(LibseatSeat {
            seat: seat.map_err(os_error)?,
        })
    }

    fn said(&mut self) -> Vec<String> {
        std::mem::take(&mut self.said)
    }

    /// A connection to seatd's socket that is accepted. seatd adds a
    /// client to a seat only when the client asks to open one, so this
    /// connection, closed at once, takes no VT.
    fn seatd_listens(&self) -> bool {
        let path = std::env::var_os("SEATD_SOCK")
            .map_or_else(|| PathBuf::from(SEATD_SOCKET), PathBuf::from);
        UnixStream::connect(path).is_ok()
    }

    fn backend_named(&self) -> Option<OsString> {
        std::env::var_os("LIBSEAT_BACKEND").filter(|name| !name.is_empty())
    }

    /// logind keeps a file for each seat in this directory while it runs,
    /// which sd-login's `sd_get_seats` lists (systemd at 78ab4b10,
    /// src/libsystemd/sd-login/sd-login.c:1111).
    fn logind_runs(&self) -> bool {
        Path::new(LOGIND_SEATS).is_dir()
    }

    /// libseat's logind backend takes the session `XDG_SESSION_ID` names
    /// first, then the calling process's own (libseat/backend/logind.c,
    /// `get_display_session`), which sd-login finds from the process's
    /// control group ([`super::in_session_scope`]).
    fn own_session(&self) -> bool {
        std::env::var_os("XDG_SESSION_ID").is_some_and(|id| !id.is_empty())
            || std::fs::read_to_string("/proc/self/cgroup")
                .is_ok_and(|cgroup| super::in_session_scope(&cgroup))
    }
}

impl Seat for LibseatSeat {
    type Device = LibseatDevice;

    /// A second descriptor for libseat's connection, which the loop owns.
    fn fd(&mut self) -> io::Result<OwnedFd> {
        let fd = self.seat.get_fd().map_err(os_error)?;
        fd.try_clone_to_owned()
    }

    fn dispatch(&mut self) -> io::Result<()> {
        self.seat.dispatch(0).map(|_| ()).map_err(os_error)
    }

    fn disable(&mut self) -> io::Result<()> {
        self.seat.disable().map_err(os_error)
    }

    /// The device and a second descriptor of it for the owner, which shares
    /// its open file: the seat manager's revoke and its DRM master reach
    /// both. A path that is not UTF-8 or holds a NUL is refused here, as
    /// the crate would panic on it.
    fn open_device(&mut self, path: &Path) -> io::Result<(LibseatDevice, OwnedFd)> {
        let text = path.to_str().filter(|t| !t.contains('\0'));
        if text.is_none() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let device = self.seat.open_device(&path).map_err(os_error)?;
        let raw = device.as_fd().as_raw_fd();
        #[expect(
            unsafe_code,
            reason = "libseat hands its caller the device's descriptor to close"
        )]
        // SAFETY: `raw` is the descriptor libseat_open_device placed in its
        // out parameter, which libseat.h gives to the caller and which
        // libseat_close_device does not close. The `Device` holds only its
        // number and closes nothing, so this `OwnedFd` is its one owner.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let device = LibseatDevice { device, fd };
        match device.fd.try_clone() {
            Ok(owner) => Ok((device, owner)),
            Err(e) => {
                // The device is closed again; the dup's error is the one
                // reported.
                let _ = self.close_device(device);
                Err(e)
            }
        }
    }

    /// The device closed through libseat, then libseat's descriptor of it.
    fn close_device(&mut self, device: LibseatDevice) -> io::Result<()> {
        let LibseatDevice { device, fd } = device;
        let closed = self.seat.close_device(device).map_err(os_error);
        drop(fd);
        closed
    }

    fn switch_session(&mut self, session: u16) -> io::Result<()> {
        self.seat
            .switch_session(i32::from(session))
            .map_err(os_error)
    }

    fn name(&mut self) -> String {
        self.seat.name().to_owned()
    }
}

/// libseat's errno as an error.
fn os_error(e: impl Into<i32>) -> io::Error {
    io::Error::from_raw_os_error(e.into())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// Set in the child that opens the seat through libseat itself.
    const CHILD: &str = "VTIUM_TEST_LIBSEAT_LOG";

    /// A backend libseat does not have, so that it opens nothing and
    /// reports `No backend matched name '...'` through its log handler.
    const NO_BACKEND: &str = "vtium-test-no-such-backend";

    // libseat formats its message from a va_list that vtium's handler
    // passes on to vsnprintf, so this checks the handler's ABI on the
    // target the test runs on. The open runs in a child, with
    // LIBSEAT_BACKEND naming no backend libseat has, so that it opens no
    // seat and no other test sees the variable.
    #[test]
    fn libseats_log_reaches_vtium() {
        if std::env::var_os(CHILD).is_some() {
            let mut libseat = Libseat::default();
            let opened = libseat.open(Inbox::default());
            assert_eq!(
                opened.err().map(|e| e.kind()),
                Some(io::ErrorKind::InvalidInput)
            );
            assert_eq!(
                libseat.said(),
                [format!("No backend matched name '{NO_BACKEND}'")]
            );
            assert!(libseat.said().is_empty(), "the lines are taken once");
            return;
        }
        let name = format!(
            "{}::libseats_log_reaches_vtium",
            module_path!().split_once("::").expect("a crate path").1
        );
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env("LIBSEAT_BACKEND", NO_BACKEND)
            .output()
            .expect("the test binary runs again");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stdout}{stderr}");
        assert!(
            stdout.contains("1 passed"),
            "the child ran the test: {stdout}"
        );
        // libseat wrote nothing to stderr itself.
        assert!(!stderr.contains("No backend matched"), "{stderr}");
    }

    #[test]
    fn libseats_place_in_its_source_is_left_out() {
        assert_eq!(
            without_place(
                "[libseat/libseat.c:58] Backend 'seatd' failed to open seat: No such file or directory"
            ),
            "Backend 'seatd' failed to open seat: No such file or directory"
        );
        assert_eq!(
            without_place("No backend was able to open a seat"),
            "No backend was able to open a seat"
        );
        assert_eq!(without_place("[a b] c"), "[a b] c");
    }
}
