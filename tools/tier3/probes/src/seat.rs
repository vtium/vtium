//! A libseat client, through libseat loaded at run time
//! (include/libseat.h of seatd 0.9).

use std::ffi::{c_char, c_int, c_void};

use crate::sys::{Library, cstring, text};

#[repr(C)]
struct Listener {
    enable_seat: unsafe extern "C" fn(*mut c_void, *mut c_void),
    disable_seat: unsafe extern "C" fn(*mut c_void, *mut c_void),
}

/// What the listener heard, which the userdata pointer points at.
#[derive(Default)]
struct Heard {
    enabled: u32,
    disabled: u32,
}

unsafe extern "C" fn enable_seat(_seat: *mut c_void, data: *mut c_void) {
    // SAFETY: data is the Heard the client passed to libseat_open_seat.
    unsafe { (*data.cast::<Heard>()).enabled += 1 };
}

unsafe extern "C" fn disable_seat(_seat: *mut c_void, data: *mut c_void) {
    // SAFETY: data is the Heard the client passed to libseat_open_seat.
    unsafe { (*data.cast::<Heard>()).disabled += 1 };
}

type OpenSeat = unsafe extern "C" fn(*const Listener, *mut c_void) -> *mut c_void;
type SeatCall = unsafe extern "C" fn(*mut c_void) -> c_int;
type OpenDevice = unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_int) -> c_int;
type Dispatch = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type SeatName = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type LogLevel = unsafe extern "C" fn(c_int);

pub struct Seat {
    _lib: Library,
    seat: *mut c_void,
    heard: Box<Heard>,
    _listener: Box<Listener>,
    close_seat: SeatCall,
    open_device: OpenDevice,
    dispatch: Dispatch,
}

impl Seat {
    /// Opens the seat through the backend LIBSEAT_BACKEND names, with
    /// libseat's own log on stderr at info level.
    pub fn open() -> Result<(Seat, String), String> {
        let lib = Library::open("libseat.so.1")?;
        // SAFETY: each type matches the function's declaration in
        // libseat.h.
        let (open_seat, close_seat, open_device, dispatch, seat_name, log_level) = unsafe {
            (
                lib.get::<OpenSeat>("libseat_open_seat")?,
                lib.get::<SeatCall>("libseat_close_seat")?,
                lib.get::<OpenDevice>("libseat_open_device")?,
                lib.get::<Dispatch>("libseat_dispatch")?,
                lib.get::<SeatName>("libseat_seat_name")?,
                lib.get::<LogLevel>("libseat_set_log_level")?,
            )
        };
        // SAFETY: sets a level the enum defines (LIBSEAT_LOG_LEVEL_INFO).
        unsafe { log_level(2) };
        let listener = Box::new(Listener {
            enable_seat,
            disable_seat,
        });
        let mut heard = Box::<Heard>::default();
        // SAFETY: the listener and the userdata outlive the seat.
        let seat = unsafe { open_seat(&*listener, (&raw mut *heard).cast()) };
        if seat.is_null() {
            return Err(format!("libseat_open_seat: {}", crate::sys::last_error()));
        }
        // SAFETY: the seat is open.
        let name = text(unsafe { seat_name(seat) });
        Ok((
            Seat {
                _lib: lib,
                seat,
                heard,
                _listener: listener,
                close_seat,
                open_device,
                dispatch,
            },
            name,
        ))
    }

    /// Dispatches until the seat is enabled, for at most `ms` in steps.
    pub fn wait_enabled(&mut self, ms: i32) -> bool {
        let mut left = ms;
        while self.heard.enabled == 0 && left > 0 {
            // SAFETY: the seat is open.
            unsafe { (self.dispatch)(self.seat, 100) };
            left -= 100;
        }
        self.heard.enabled > 0
    }

    pub fn enabled(&self) -> u32 {
        self.heard.enabled
    }

    /// Opens a device through the seat: its descriptor, or the error.
    pub fn open_device(&self, path: &str) -> Result<c_int, String> {
        let p = cstring(path);
        let mut fd: c_int = -1;
        // SAFETY: the seat is open, and fd is filled.
        let id = unsafe { (self.open_device)(self.seat, p.as_ptr(), &mut fd) };
        if id < 0 {
            Err(crate::sys::last_error())
        } else {
            Ok(fd)
        }
    }

    pub fn close(self) -> String {
        // SAFETY: closes the seat once.
        crate::sys::result(i64::from(unsafe { (self.close_seat)(self.seat) }))
    }
}
