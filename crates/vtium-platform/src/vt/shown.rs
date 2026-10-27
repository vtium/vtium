//! tty0's `active` attribute: the VT the kernel shows, which every switch
//! notifies (vt.c:945-946, 3712-3717).

use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};

use rustix::fs::{Mode, OFlags};

/// Where the attribute is; it exists only on the seat with kernel VTs.
const ACTIVE: &str = "/sys/class/tty/tty0/active";

/// Opens the attribute, read-only.
pub(super) fn open() -> io::Result<OwnedFd> {
    Ok(rustix::fs::open(
        ACTIVE,
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
}

/// The VT the attribute names, read from its start as sysfs requires.
pub(crate) fn read(fd: BorrowedFd<'_>) -> io::Result<u16> {
    let mut text = [0u8; 16];
    let len = rustix::io::pread(fd, &mut text, 0)?;
    parse(text.get(..len).unwrap_or_default())
}

/// The VT number in `ttyN`, with or without its newline.
pub(crate) fn parse(text: &[u8]) -> io::Result<u16> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let digits = text
        .strip_suffix(b"\n")
        .unwrap_or(text)
        .strip_prefix(b"tty")
        .ok_or_else(invalid)?;
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(invalid());
    }
    std::str::from_utf8(digits)
        .map_err(|_| invalid())?
        .parse()
        .map_err(|_| invalid())
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use std::os::fd::AsFd;

    use super::*;

    #[test]
    fn the_attribute_names_the_vt_shown() {
        assert_eq!(parse(b"tty3\n").ok(), Some(3));
        assert_eq!(parse(b"tty12").ok(), Some(12));
        for bad in [
            &b""[..],
            b"tty",
            b"tty\n",
            b"ttyx\n",
            b"3\n",
            b"tty0x\n",
            b"tty70000\n",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_read_starts_at_the_attributes_start() {
        let file = rustix::fs::memfd_create("active", rustix::fs::MemfdFlags::CLOEXEC)
            .unwrap_or_else(|e| panic!("memfd: {e}"));
        let written = rustix::io::write(&file, b"tty7\n").unwrap_or_else(|e| panic!("write: {e}"));
        assert_eq!(written, 5);
        // The offset is past the text; the read still finds it all, twice.
        assert_eq!(read(file.as_fd()).ok(), Some(7));
        assert_eq!(read(file.as_fd()).ok(), Some(7));
    }
}
