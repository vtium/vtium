//! libseat itself, through the `libseat` crate.

use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{Inbox, Manager, Seat, Told};

/// Where seatd listens when `SEATD_SOCK` names no other socket, as libseat
/// looks for it on Linux.
const SEATD_SOCKET: &str = "/run/seatd.sock";

/// libseat, which opens the seat through the first seat manager that
/// answers: seatd, logind, or the one `LIBSEAT_BACKEND` names.
#[derive(Debug, Default)]
pub struct Libseat;

/// A seat libseat opened.
pub struct LibseatSeat {
    seat: libseat::Seat,
}

impl Manager for Libseat {
    type Seat = LibseatSeat;

    fn open(&mut self, inbox: Inbox) -> io::Result<LibseatSeat> {
        let listener = Rc::clone(&inbox);
        let seat = libseat::Seat::open(move |_, event| {
            listener.borrow_mut().push_back(match event {
                libseat::SeatEvent::Enable => Told::Enable,
                libseat::SeatEvent::Disable => Told::Disable,
            });
        })
        .map_err(os_error)?;
        Ok(LibseatSeat { seat })
    }

    /// A connection to seatd's socket that is accepted. seatd adds a
    /// client to a seat only when the client asks to open one, so this
    /// connection, closed at once, takes no VT.
    fn seatd_listens(&self) -> bool {
        let path = std::env::var_os("SEATD_SOCK")
            .map_or_else(|| PathBuf::from(SEATD_SOCKET), PathBuf::from);
        UnixStream::connect(path).is_ok()
    }
}

impl Seat for LibseatSeat {
    type Device = libseat::Device;

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
    fn open_device(&mut self, path: &Path) -> io::Result<(libseat::Device, OwnedFd)> {
        let text = path.to_str().filter(|t| !t.contains('\0'));
        if text.is_none() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let device = self.seat.open_device(&path).map_err(os_error)?;
        match device.as_fd().try_clone_to_owned() {
            Ok(fd) => Ok((device, fd)),
            Err(e) => {
                // The device is closed again; the dup's error is the one
                // reported.
                let _ = self.close_device(device);
                Err(e)
            }
        }
    }

    /// The device closed through libseat, then libseat's descriptor of it,
    /// which libseat leaves to its caller.
    fn close_device(&mut self, device: libseat::Device) -> io::Result<()> {
        let raw = device.as_fd().as_raw_fd();
        let closed = self.seat.close_device(device).map_err(os_error);
        #[expect(
            unsafe_code,
            reason = "libseat hands its caller the device's descriptor to close"
        )]
        // SAFETY: `raw` is the descriptor libseat_open_device placed in its
        // out parameter, which libseat.h gives to the caller and which
        // libseat_close_device does not close. The `Device` it came from
        // was consumed above, and nothing else holds this number: the
        // owner holds a duplicate of its own.
        drop(unsafe { OwnedFd::from_raw_fd(raw) });
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
