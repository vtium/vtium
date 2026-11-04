//! A display's buffer as a dma-buf, which the GPU path's token carries
//! (ADR-1303 D7, ADR-1401 D2).
//!
//! A dma-buf is the kernel's way to share a buffer between devices: each of
//! the buffer's planes is a file descriptor, an offset into it and a stride,
//! and the buffer has a format, a DRM fourcc, and a modifier, the driver's
//! name for how its pixels are laid out in memory. A display exports each
//! GBM buffer it shows this way, and the head's GL context imports it as an
//! EGL image to draw into.

use std::fmt;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

/// The modifier of a buffer whose layout its driver did not say:
/// `DRM_FORMAT_MOD_INVALID`. The importer takes the driver's implicit
/// layout then.
pub const MODIFIER_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// One plane of a dma-buf.
#[derive(Debug)]
pub struct Plane {
    /// A descriptor of the buffer's memory.
    pub fd: OwnedFd,
    /// Where the plane starts in that memory, in bytes.
    pub offset: u32,
    /// The bytes from one line of the plane to the next.
    pub stride: u32,
}

/// A buffer as a dma-buf: one to four planes, its size, format and modifier.
pub struct DmaBuf {
    width: u32,
    height: u32,
    format: u32,
    modifier: u64,
    planes: Vec<Plane>,
}

impl DmaBuf {
    /// A `width` by `height` buffer in `format` with `modifier`. `None`
    /// unless it has one to four planes and a size of at least one pixel.
    pub fn new(
        (width, height): (u32, u32),
        format: u32,
        modifier: u64,
        planes: Vec<Plane>,
    ) -> Option<DmaBuf> {
        ((1..=4).contains(&planes.len()) && width > 0 && height > 0).then_some(DmaBuf {
            width,
            height,
            format,
            modifier,
            planes,
        })
    }

    /// Its size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Its DRM fourcc.
    pub fn format(&self) -> u32 {
        self.format
    }

    /// Its layout's modifier, [`MODIFIER_INVALID`] where the driver gave
    /// none.
    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    /// Its planes, from the first: each descriptor, offset and stride.
    pub fn planes(&self) -> impl Iterator<Item = (BorrowedFd<'_>, u32, u32)> {
        self.planes
            .iter()
            .map(|p| (p.fd.as_fd(), p.offset, p.stride))
    }
}

impl fmt::Debug for DmaBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DmaBuf")
            .field("size", &(self.width, self.height))
            .field("format", &format_args!("{:#010x}", self.format))
            .field("modifier", &format_args!("{:#x}", self.modifier))
            .field("planes", &self.planes.len())
            .finish()
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn plane() -> Plane {
        Plane {
            fd: std::fs::File::open("/dev/null").unwrap().into(),
            offset: 0,
            stride: 16,
        }
    }

    #[test]
    fn a_dma_buf_has_one_to_four_planes_and_a_size() {
        assert!(DmaBuf::new((4, 4), 0, 0, Vec::new()).is_none());
        assert!(DmaBuf::new((4, 4), 0, 0, (0..5).map(|_| plane()).collect()).is_none());
        assert!(DmaBuf::new((0, 4), 0, 0, vec![plane()]).is_none());
        let four = DmaBuf::new((4, 4), 0, 0, (0..4).map(|_| plane()).collect()).unwrap();
        assert_eq!(four.planes().count(), 4);
    }
}
