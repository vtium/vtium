//! A dumb buffer of any size, made and mapped on a card that the caller
//! opened, for tools that measure the display path.
//!
//! drm2d makes each display's buffers at the display's mode. The tool that
//! measures ways to scroll on the CPU path also needs a buffer taller than
//! the screen, which the plane shows a window of. That tool drives the
//! card itself through drm-ffi, which has no unsafe call, and only the
//! mapping needs this crate's one mmap. So this file makes a dumb buffer,
//! maps it and frees it, and nothing more: the framebuffer over it, the
//! plane and the commits stay the caller's.
//!
//! References:
//! - ADR-1310 D6, "The measurement": the pan over a tall buffer that the
//!   tool measures.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use drm_ffi::mode::dumbbuffer;

use crate::mmap::Mapping;

/// A dumb buffer in XRGB8888, 32 bits a pixel, mapped into memory. It
/// frees the buffer when dropped.
pub struct MappedDumb {
    /// A copy of the card's descriptor, for the free when dropped.
    fd: OwnedFd,
    handle: u32,
    pitch: u32,
    width: u32,
    height: u32,
    /// `None` only while dropping, so the unmap comes before the free.
    map: Option<Mapping>,
}

impl MappedDumb {
    /// Makes a `width` by `height` dumb buffer on the card open as `fd` and
    /// maps it. The kernel refuses a size its driver cannot make, as one
    /// taller than the driver's largest framebuffer may be.
    pub fn new(fd: BorrowedFd<'_>, width: u32, height: u32) -> io::Result<MappedDumb> {
        let info = dumbbuffer::create(fd, width, height, 32, 0)?;
        let mapped = dumbbuffer::map(fd, info.handle, 0, 0).and_then(|m| {
            let len = usize::try_from(info.size)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            Mapping::new(fd, len, m.offset)
        });
        let owned = fd.try_clone_to_owned();
        match (mapped, owned) {
            (Ok(map), Ok(fd)) => Ok(MappedDumb {
                fd,
                handle: info.handle,
                pitch: info.pitch,
                width,
                height,
                map: Some(map),
            }),
            (Err(e), _) | (_, Err(e)) => {
                let _ = dumbbuffer::destroy(fd, info.handle);
                Err(e)
            }
        }
    }

    /// The card's handle on the buffer, which a framebuffer is made over.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// The bytes from one line of pixels to the next.
    pub fn pitch(&self) -> u32 {
        self.pitch
    }

    /// The size the buffer was made for, in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl AsMut<[u8]> for MappedDumb {
    fn as_mut(&mut self) -> &mut [u8] {
        self.map.as_mut().map_or(&mut [], AsMut::as_mut)
    }
}

impl Drop for MappedDumb {
    fn drop(&mut self) {
        drop(self.map.take());
        if let Err(e) = dumbbuffer::destroy(self.fd.as_fd(), self.handle) {
            log::warn!("cannot destroy a dumb buffer: {e}");
        }
    }
}
