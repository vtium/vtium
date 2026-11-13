//! The VT's state, read through a descriptor with the calls that only read,
//! which need no privilege beyond the open file (ADR-1400 D1's table), and
//! the switch to a VT, which needs root.

use std::ffi::c_int;

use crate::sys::{cstring, last_error};

const KDGETMODE: libc::c_ulong = 0x4B3B;
pub const KDSETMODE: libc::c_ulong = 0x4B3A;
const KDGKBMODE: libc::c_ulong = 0x4B44;
const VT_GETMODE: libc::c_ulong = 0x5601;
const VT_GETSTATE: libc::c_ulong = 0x5603;
const VT_ACTIVATE: libc::c_ulong = 0x5606;
const VT_WAITACTIVE: libc::c_ulong = 0x5607;
pub const KD_TEXT: c_int = 0;
pub const KD_GRAPHICS: c_int = 1;

#[repr(C)]
#[derive(Default)]
struct VtMode {
    mode: libc::c_char,
    waitv: libc::c_char,
    relsig: libc::c_short,
    acqsig: libc::c_short,
    frsig: libc::c_short,
}

#[repr(C)]
#[derive(Default)]
struct VtStat {
    v_active: libc::c_ushort,
    v_signal: libc::c_ushort,
    v_state: libc::c_ushort,
}

/// Opens /dev/ttyN to read its state, without making it a controlling
/// terminal.
pub fn open(n: u32) -> Result<c_int, String> {
    let path = cstring(&format!("/dev/tty{n}"));
    // SAFETY: open with a valid path.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(format!("open /dev/tty{n}: {}", last_error()))
    } else {
        Ok(fd)
    }
}

/// The VT's state through `fd`: who owns the tty's file and its mode, the
/// display mode (KDGETMODE), the keyboard mode (KDGKBMODE), the switch mode
/// (VT_GETMODE) and the VT shown.
pub fn state(fd: c_int) -> String {
    // SAFETY: fstat and the read-only ioctls fill structures of the right
    // size.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        let owner = if libc::fstat(fd, &mut st) == 0 {
            format!(
                "owner={}:{} perm={:04o}",
                st.st_uid,
                st.st_gid,
                st.st_mode & 0o7777
            )
        } else {
            format!("owner=? ({})", last_error())
        };
        let mut kd: c_int = -1;
        let kd = if libc::ioctl(fd, KDGETMODE, &mut kd) == 0 {
            match kd {
                KD_TEXT => "KD_TEXT".to_string(),
                KD_GRAPHICS => "KD_GRAPHICS".to_string(),
                n => n.to_string(),
            }
        } else {
            format!("? ({})", last_error())
        };
        let mut kb: c_int = -1;
        let kb = if libc::ioctl(fd, KDGKBMODE, &mut kb) == 0 {
            match kb {
                0 => "K_RAW".to_string(),
                1 => "K_XLATE".to_string(),
                2 => "K_MEDIUMRAW".to_string(),
                3 => "K_UNICODE".to_string(),
                4 => "K_OFF".to_string(),
                n => n.to_string(),
            }
        } else {
            format!("? ({})", last_error())
        };
        let mut vm = VtMode::default();
        let vm = if libc::ioctl(fd, VT_GETMODE, &mut vm) == 0 {
            match vm.mode {
                0 => "VT_AUTO".to_string(),
                1 => format!("VT_PROCESS relsig={} acqsig={}", vm.relsig, vm.acqsig),
                n => n.to_string(),
            }
        } else {
            format!("? ({})", last_error())
        };
        let mut vs = VtStat::default();
        let shown = if libc::ioctl(fd, VT_GETSTATE, &mut vs) == 0 {
            vs.v_active.to_string()
        } else {
            format!("? ({})", last_error())
        };
        format!("{owner} kd={kd} kb={kb} switch={vm} shown=tty{shown}")
    }
}

/// Shows VT `n` and waits until the kernel has switched to it.
pub fn activate(n: u32) -> Result<(), String> {
    let tty0 = cstring("/dev/tty0");
    // SAFETY: open, ioctl and close on a descriptor this function owns.
    unsafe {
        let fd = libc::open(
            tty0.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        );
        if fd < 0 {
            return Err(format!("open /dev/tty0: {}", last_error()));
        }
        let r = if libc::ioctl(fd, VT_ACTIVATE, n as c_int) != 0 {
            Err(format!("VT_ACTIVATE {n}: {}", last_error()))
        } else if libc::ioctl(fd, VT_WAITACTIVE, n as c_int) != 0 {
            Err(format!("VT_WAITACTIVE {n}: {}", last_error()))
        } else {
            Ok(())
        };
        libc::close(fd);
        r
    }
}

/// Sets the display mode of the VT behind `fd` (root's job).
pub fn set_mode(fd: c_int, mode: c_int) -> String {
    // SAFETY: KDSETMODE takes its argument by value.
    crate::sys::result(unsafe { libc::ioctl(fd, KDSETMODE, mode) } as i64)
}
