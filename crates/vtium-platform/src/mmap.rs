//! A framebuffer mapped into memory: a DRM dumb buffer or the fbdev
//! framebuffer.
//!
//! [`Mapping`] is the crate's one mmap. It owns the mapping, hands out its
//! bytes as a slice borrowed from it, and unmaps it once, when dropped.

use std::io;
use std::os::fd::BorrowedFd;
use std::ptr::{NonNull, null_mut};

use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};

/// A shared, readable and writable mapping of a device's memory.
pub(crate) struct Mapping {
    ptr: NonNull<u8>,
    len: usize,
}

impl Mapping {
    /// Maps `len` bytes of `fd` from `offset`.
    pub(crate) fn new(fd: BorrowedFd<'_>, len: usize, offset: u64) -> io::Result<Mapping> {
        if len == 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        #[expect(
            unsafe_code,
            reason = "mapping a framebuffer is one of the crate's four kinds of unsafe"
        )]
        // SAFETY: the kernel chooses the address (the hint is null), so the
        // mapping overlaps no memory Rust already owns. MAP_SHARED with
        // read and write makes every byte of it readable and writable.
        let ptr = unsafe {
            mmap(
                null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                fd,
                offset,
            )?
        };
        let ptr = NonNull::new(ptr.cast::<u8>()).ok_or(io::ErrorKind::InvalidData)?;
        Ok(Mapping { ptr, len })
    }
}

impl AsMut<[u8]> for Mapping {
    fn as_mut(&mut self) -> &mut [u8] {
        #[expect(
            unsafe_code,
            reason = "the bytes of a mapped framebuffer are reached through its pointer"
        )]
        // SAFETY: `ptr` starts a mapping of `len` readable and writable
        // bytes that stays mapped until this value drops, and the slice
        // borrows this value mutably, so no other slice of it lives.
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "unmapping the framebuffer this value mapped")]
        // SAFETY: `ptr` and `len` are the mapping `new` made, unmapped only
        // here, and no slice of it outlives this value.
        let unmapped = unsafe { munmap(self.ptr.as_ptr().cast(), self.len) };
        if let Err(e) = unmapped {
            log::warn!("cannot unmap a framebuffer: {e}");
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::os::fd::AsFd;

    use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
    use rustix::io::pread;

    use super::*;

    /// What is written through the mapping reaches the file behind it.
    #[test]
    fn writes_reach_the_mapped_file() {
        let fd = memfd_create("vtium-mapping", MemfdFlags::CLOEXEC).expect("memfd");
        ftruncate(&fd, 4096 + 8192).expect("ftruncate");
        let mut map = Mapping::new(fd.as_fd(), 8192, 4096).expect("mmap");
        assert_eq!(map.as_mut().len(), 8192);
        map.as_mut()[..4].copy_from_slice(b"vtiu");
        let mut read = [0u8; 4];
        pread(&fd, &mut read, 4096).expect("pread");
        assert_eq!(&read, b"vtiu");
    }

    #[test]
    fn an_empty_mapping_is_refused() {
        let fd = memfd_create("vtium-mapping", MemfdFlags::CLOEXEC).expect("memfd");
        assert!(Mapping::new(fd.as_fd(), 0, 0).is_err());
    }
}
