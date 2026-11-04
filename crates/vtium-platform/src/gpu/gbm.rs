//! libgbm, opened when the GPU path starts (ADR-1303 D8).
//!
//! vtium links no GBM library. It opens `libgbm.so.1` with libloading and
//! takes the eleven calls it makes, so a build with the `gpu` feature starts
//! on a machine without libgbm and reports the step that failed. A [`Gbm`]
//! device is made over a DRM device's descriptor, a card or its render node.
//! The head's GL context makes one on the render node as EGL's native
//! display; a display makes one on its card and allocates the buffers it
//! shows from it ([`Gbm::allocate`]), each exported as a dma-buf
//! ([`Bo::export`]) for the head's context to import.
//!
//! The C reference made its GBM device on the card's descriptor and drew
//! through a GBM surface that EGL swapped (`kmscon/src/video/drm3d_video.c`,
//! `drm3d_video_init` and `display_allocfb`). vtium allocates buffer objects
//! itself and imports them, because a display's buffers live with the
//! display on the event loop and the head's render thread draws into them
//! (ADR-1401 D2).

use std::ffi::{c_int, c_void};
use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::NonNull;
use std::rc::Rc;

use libloading::Library;

use crate::display::{DmaBuf, Plane};

/// The library vtium opens.
const LIBRARY: &str = "libgbm.so.1";

/// `GBM_BO_USE_SCANOUT`: a buffer a display controller can show.
const USE_SCANOUT: u32 = 1;
/// `GBM_BO_USE_RENDERING`: a buffer the GPU draws into.
const USE_RENDERING: u32 = 1 << 2;

/// Most planes a buffer has, as a DRM framebuffer allows.
const PLANES_MAX: c_int = 4;

/// What a buffer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// The GPU draws into it and a display shows it.
    Scanout,
    /// The GPU draws into it only, as a test's buffer on a render node.
    Rendering,
}

/// `struct gbm_device`, which libgbm keeps opaque.
#[repr(C)]
struct RawDevice {
    _opaque: [u8; 0],
}

/// `struct gbm_bo`, which libgbm keeps opaque.
#[repr(C)]
struct RawBo {
    _opaque: [u8; 0],
}

/// `union gbm_bo_handle`, which `gbm_bo_get_handle` returns by value. It is
/// declared whole, so the call returns it as the C ABI returns the union
/// on every target, armv7's included.
#[repr(C)]
#[derive(Clone, Copy)]
union RawHandle {
    ptr: *mut c_void,
    s32: i32,
    u32: u32,
    s64: i64,
    u64: u64,
}

/// The calls vtium makes, taken from the opened library, which the value
/// keeps open for as long as the pointers are used.
struct Api {
    create_device: unsafe extern "C" fn(c_int) -> *mut RawDevice,
    device_destroy: unsafe extern "C" fn(*mut RawDevice),
    bo_create: unsafe extern "C" fn(*mut RawDevice, u32, u32, u32, u32) -> *mut RawBo,
    bo_destroy: unsafe extern "C" fn(*mut RawBo),
    bo_get_plane_count: unsafe extern "C" fn(*mut RawBo) -> c_int,
    bo_get_fd_for_plane: unsafe extern "C" fn(*mut RawBo, c_int) -> c_int,
    bo_get_stride_for_plane: unsafe extern "C" fn(*mut RawBo, c_int) -> u32,
    bo_get_offset: unsafe extern "C" fn(*mut RawBo, c_int) -> u32,
    bo_get_modifier: unsafe extern "C" fn(*mut RawBo) -> u64,
    bo_get_format: unsafe extern "C" fn(*mut RawBo) -> u32,
    bo_get_handle: unsafe extern "C" fn(*mut RawBo) -> RawHandle,
    /// Last, so the pointers above are never used past it.
    _library: Library,
}

impl Api {
    /// Opens libgbm and takes each call, or says which is missing.
    fn load() -> Result<Api, String> {
        // SAFETY: libgbm's constructors only set up its own state, and
        // every symbol below is declared with the signature gbm.h gives it.
        #[expect(unsafe_code, reason = "opening libgbm and taking its calls")]
        unsafe {
            let library =
                Library::new(LIBRARY).map_err(|e| format!("{LIBRARY} cannot be opened: {e}"))?;
            macro_rules! call {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|e| format!("{LIBRARY} lacks {}: {e}", $name))?
                };
            }
            Ok(Api {
                create_device: call!("gbm_create_device"),
                device_destroy: call!("gbm_device_destroy"),
                bo_create: call!("gbm_bo_create"),
                bo_destroy: call!("gbm_bo_destroy"),
                bo_get_plane_count: call!("gbm_bo_get_plane_count"),
                bo_get_fd_for_plane: call!("gbm_bo_get_fd_for_plane"),
                bo_get_stride_for_plane: call!("gbm_bo_get_stride_for_plane"),
                bo_get_offset: call!("gbm_bo_get_offset"),
                bo_get_modifier: call!("gbm_bo_get_modifier"),
                bo_get_format: call!("gbm_bo_get_format"),
                bo_get_handle: call!("gbm_bo_get_handle"),
                _library: library,
            })
        }
    }
}

/// The device, its library and its descriptor, freed in that order when the
/// device and its last buffer are gone.
struct Device {
    api: Api,
    raw: NonNull<RawDevice>,
    /// The DRM device's descriptor, which outlives the GBM device made over
    /// it.
    _fd: OwnedFd,
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: `raw` came from gbm_create_device, and every buffer made
        // from it holds this value, so none is left.
        #[expect(unsafe_code, reason = "gbm_device_destroy on the device made here")]
        unsafe {
            (self.api.device_destroy)(self.raw.as_ptr());
        }
    }
}

/// A GBM device over a DRM device's descriptor.
///
/// It stays on the thread that made it: libgbm makes no promise about a
/// device used from two threads. A buffer it exports travels as a
/// [`DmaBuf`], which is [`Send`].
pub struct Gbm(Rc<Device>);

impl Gbm {
    /// Opens libgbm and makes a device over `fd`, a card or a render node.
    pub fn new(fd: OwnedFd) -> Result<Gbm, String> {
        let api = Api::load()?;
        // SAFETY: `fd` is an open descriptor this value keeps until the
        // device is destroyed.
        #[expect(unsafe_code, reason = "gbm_create_device over a descriptor kept open")]
        let raw = unsafe { (api.create_device)(fd.as_raw_fd()) };
        let raw = NonNull::new(raw).ok_or("gbm_create_device failed on the device")?;
        Ok(Gbm(Rc::new(Device { api, raw, _fd: fd })))
    }

    /// The device as EGL's GBM platform takes it.
    pub(super) fn as_ptr(&self) -> *mut c_void {
        self.0.raw.as_ptr().cast()
    }

    /// A buffer of `width` by `height` pixels in `format`, a DRM fourcc,
    /// for `usage`, its layout the driver's choice.
    pub fn allocate(&self, width: u32, height: u32, format: u32, usage: Usage) -> io::Result<Bo> {
        let flags = match usage {
            Usage::Scanout => USE_SCANOUT | USE_RENDERING,
            Usage::Rendering => USE_RENDERING,
        };
        // SAFETY: the device is live while `self` is.
        #[expect(unsafe_code, reason = "gbm_bo_create on a live device")]
        let raw =
            unsafe { (self.0.api.bo_create)(self.0.raw.as_ptr(), width, height, format, flags) };
        let raw = NonNull::new(raw).ok_or_else(io::Error::last_os_error)?;
        Ok(Bo {
            device: Rc::clone(&self.0),
            raw,
            width,
            height,
        })
    }
}

impl fmt::Debug for Gbm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gbm").finish_non_exhaustive()
    }
}

/// A GBM buffer object, freed when dropped. It keeps its device alive.
pub struct Bo {
    device: Rc<Device>,
    raw: NonNull<RawBo>,
    width: u32,
    height: u32,
}

impl Bo {
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The card's handle on the buffer, which a DRM framebuffer is made
    /// over.
    pub fn handle(&self) -> u32 {
        // SAFETY: the buffer is live while `self` is; every member of the
        // union is plain data, and a DRM device's handle is its `u32`.
        #[expect(unsafe_code, reason = "gbm_bo_get_handle on a live buffer")]
        unsafe {
            (self.device.api.bo_get_handle)(self.raw.as_ptr()).u32
        }
    }

    /// The buffer as a dma-buf: a new descriptor for each of its planes,
    /// with their offsets and strides, its format and its modifier.
    pub fn export(&self) -> io::Result<DmaBuf> {
        let api = &self.device.api;
        let bo = self.raw.as_ptr();
        // SAFETY: the buffer is live while `self` is. Each descriptor
        // gbm_bo_get_fd_for_plane returns is a new one the caller owns, or
        // -1.
        #[expect(unsafe_code, reason = "libgbm's queries on a live buffer")]
        unsafe {
            let count = (api.bo_get_plane_count)(bo);
            if !(1..=PLANES_MAX).contains(&count) {
                return Err(io::Error::other(format!("a buffer of {count} planes")));
            }
            let mut planes = Vec::new();
            for plane in 0..count {
                let fd = (api.bo_get_fd_for_plane)(bo, plane);
                if fd < 0 {
                    return Err(io::Error::last_os_error());
                }
                planes.push(Plane {
                    fd: OwnedFd::from_raw_fd(fd),
                    offset: (api.bo_get_offset)(bo, plane),
                    stride: (api.bo_get_stride_for_plane)(bo, plane),
                });
            }
            let format = (api.bo_get_format)(bo);
            let modifier = (api.bo_get_modifier)(bo);
            DmaBuf::new((self.width, self.height), format, modifier, planes)
                .ok_or_else(|| io::Error::other("libgbm described a buffer no dma-buf holds"))
        }
    }
}

impl Drop for Bo {
    fn drop(&mut self) {
        // SAFETY: `raw` came from gbm_bo_create on the device this value
        // keeps alive, and is destroyed once.
        #[expect(unsafe_code, reason = "gbm_bo_destroy on the buffer made here")]
        unsafe {
            (self.device.api.bo_destroy)(self.raw.as_ptr());
        }
    }
}

impl fmt::Debug for Bo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bo")
            .field("size", &(self.width, self.height))
            .finish_non_exhaustive()
    }
}
