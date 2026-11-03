//! EGL: the display, the context and the import of a dma-buf as an EGL
//! image (ADR-1303 D7, D8, D9).
//!
//! vtium loads libEGL when a context is made, through khronos-egl's
//! `dynamic` feature, and needs EGL 1.5 for `eglGetPlatformDisplay` and
//! `eglCreateImage`. A head's context lives on a GBM device over its card's
//! render node, which needs no DRM master. A test's context lives on Mesa's
//! software device, llvmpipe, through the device platform: Mesa's
//! surfaceless platform picks the machine's GPU where it has one, so only
//! the software device draws the same pixels on every machine.
//!
//! The context is OpenGL ES 3.0 or later with no surface, current on the
//! thread that made it.
//!
//! The C reference made its display with `eglGetDisplay` over its GBM
//! device, required `EGL_KHR_surfaceless_context`, chose a config whose
//! native visual was XRGB8888 or ARGB8888, asked for OpenGL ES 2.0 and drew
//! through a GBM surface (`kmscon/src/video/drm3d_video.c`,
//! `drm3d_video_init`). vtium asks for 3.0, needs no config where the
//! driver has `EGL_KHR_no_config_context`, draws into imported buffers and
//! asks for a context that reports a GPU reset where the driver offers it.

use std::ffi::{CStr, c_char, c_void};
use std::os::fd::AsRawFd;

use khronos_egl as egl;

use super::gbm::Gbm;
use super::{SetupError, Step};
use crate::display::{DmaBuf, MODIFIER_INVALID};

type Instance = egl::DynamicInstance<egl::EGL1_5>;

const PLATFORM_GBM_KHR: egl::Enum = 0x31D7;
const PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;
const CONTEXT_MAJOR_VERSION: egl::Int = 0x3098;
const CONTEXT_MINOR_VERSION: egl::Int = 0x30FB;
const OPENGL_ES3_BIT: egl::Int = 0x40;
const CONTEXT_OPENGL_ROBUST_ACCESS_EXT: egl::Int = 0x30BF;
const CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT: egl::Int = 0x3138;
const LOSE_CONTEXT_ON_RESET_EXT: egl::Int = 0x31BF;
/// `EGL_TRUE` as an attribute's value.
const ATTRIB_TRUE: egl::Int = 1;
const LINUX_DMA_BUF_EXT: egl::Enum = 0x3270;
const LINUX_DRM_FOURCC_EXT: egl::Attrib = 0x3271;
/// Each plane's descriptor, offset, pitch, and modifier's low and high
/// halves.
const PLANE_ATTRIBS: [[egl::Attrib; 5]; 4] = [
    [0x3272, 0x3273, 0x3274, 0x3443, 0x3444],
    [0x3275, 0x3276, 0x3277, 0x3445, 0x3446],
    [0x3278, 0x3279, 0x327A, 0x3447, 0x3448],
    [0x3440, 0x3441, 0x3442, 0x3449, 0x344A],
];
/// Most devices the software device is looked for among.
const DEVICES_MAX: usize = 32;

/// `EGL_DEVICE_EXT`, the display's device in `eglQueryDisplayAttribEXT`.
const DEVICE_EXT: egl::Int = 0x322C;
/// The extension a device of Mesa's software renderer names.
const SOFTWARE: &str = "EGL_MESA_device_software";

type QueryDevices = unsafe extern "system" fn(egl::Int, *mut *mut c_void, *mut egl::Int) -> u32;
type QueryDeviceString = unsafe extern "system" fn(*mut c_void, egl::Int) -> *const c_char;
type QueryDisplayAttrib = unsafe extern "system" fn(*mut c_void, egl::Int, *mut egl::Attrib) -> u32;

/// An EGL display with one context, current on the thread that made it.
pub(super) struct Egl {
    api: Instance,
    display: egl::Display,
    context: egl::Context,
    /// The display's extensions.
    extensions: String,
    /// Whether dropping it terminates the display. The software device's
    /// display is one for the whole process, which every software context
    /// shares, so it is never terminated.
    terminate: bool,
    /// The GBM device the display was made over, kept until the display is
    /// terminated.
    _gbm: Option<Gbm>,
}

fn setup(step: Step, detail: impl Into<String>) -> SetupError {
    SetupError {
        step,
        detail: detail.into(),
    }
}

/// Opens libEGL.
fn load() -> Result<Instance, SetupError> {
    // SAFETY: libEGL.so.1 is the Khronos ABI that khronos-egl declares, and
    // loading it runs only its own constructors.
    #[expect(unsafe_code, reason = "opening libEGL")]
    unsafe { Instance::load_required() }.map_err(|e| {
        setup(
            Step::EglDisplay,
            format!("libEGL.so.1 cannot be loaded or lacks EGL 1.5: {e}"),
        )
    })
}

/// Whether `list`, EGL's space-separated extensions, names `name`.
fn names(list: &str, name: &str) -> bool {
    list.split_ascii_whitespace().any(|e| e == name)
}

/// The client extensions, which EGL reports with no display.
fn client_extensions(api: &Instance) -> String {
    api.query_string(None, egl::EXTENSIONS)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Egl {
    /// A display over `gbm`, a GBM device on a render node, and its context.
    pub(super) fn on_gbm(gbm: Gbm) -> Result<Egl, SetupError> {
        let api = load()?;
        let client = client_extensions(&api);
        if !names(&client, "EGL_KHR_platform_gbm") && !names(&client, "EGL_MESA_platform_gbm") {
            return Err(setup(
                Step::EglDisplay,
                "libEGL has no GBM platform (EGL_KHR_platform_gbm)",
            ));
        }
        // SAFETY: the GBM platform takes a `struct gbm_device *`, which
        // `gbm` holds live until the display is terminated.
        #[expect(unsafe_code, reason = "eglGetPlatformDisplay over a live GBM device")]
        let display = unsafe {
            api.get_platform_display(PLATFORM_GBM_KHR, gbm.as_ptr(), &[egl::ATTRIB_NONE])
        }
        .map_err(|e| setup(Step::EglDisplay, format!("eglGetPlatformDisplay: {e}")))?;
        let egl = Egl::finish(api, display, true, Some(gbm))?;
        // Mesa's GBM falls back to its software renderer on a device whose
        // driver it lacks; drawing a head through it is no GPU path
        if egl.device_extensions().is_some_and(|e| names(&e, SOFTWARE)) {
            return Err(setup(
                Step::EglDisplay,
                "the render node gives Mesa's software renderer, not a GPU",
            ));
        }
        Ok(egl)
    }

    /// A display on Mesa's software device, llvmpipe, and its context.
    pub(super) fn software() -> Result<Egl, SetupError> {
        let api = load()?;
        let client = client_extensions(&api);
        for needed in ["EGL_EXT_device_enumeration", "EGL_EXT_platform_device"] {
            if !names(&client, needed) {
                return Err(setup(
                    Step::EglDisplay,
                    format!("libEGL lacks {needed}, which finds Mesa's software device"),
                ));
            }
        }
        let device = software_device(&api)?;
        // SAFETY: the device platform takes an `EGLDeviceEXT`, which
        // eglQueryDevicesEXT gave and which lives as long as libEGL.
        #[expect(unsafe_code, reason = "eglGetPlatformDisplay over an EGL device")]
        let display =
            unsafe { api.get_platform_display(PLATFORM_DEVICE_EXT, device, &[egl::ATTRIB_NONE]) }
                .map_err(|e| setup(Step::EglDisplay, format!("eglGetPlatformDisplay: {e}")))?;
        Egl::finish(api, display, false, None)
    }

    /// Initializes `display` and makes its context.
    fn finish(
        api: Instance,
        display: egl::Display,
        terminate: bool,
        gbm: Option<Gbm>,
    ) -> Result<Egl, SetupError> {
        api.initialize(display)
            .map_err(|e| setup(Step::EglDisplay, format!("eglInitialize: {e}")))?;
        match Egl::context(&api, display) {
            Ok((context, extensions)) => Ok(Egl {
                api,
                display,
                context,
                extensions,
                terminate,
                _gbm: gbm,
            }),
            Err(e) => {
                if terminate {
                    let _ = api.terminate(display);
                }
                Err(e)
            }
        }
    }

    /// Checks the display's extensions and makes its context, current on
    /// this thread.
    fn context(
        api: &Instance,
        display: egl::Display,
    ) -> Result<(egl::Context, String), SetupError> {
        let extensions = api
            .query_string(Some(display), egl::EXTENSIONS)
            .map(|s| s.to_string_lossy().into_owned())
            .map_err(|e| setup(Step::EglExtensions, format!("eglQueryString: {e}")))?;
        if !names(&extensions, "EGL_KHR_surfaceless_context") {
            return Err(setup(
                Step::EglExtensions,
                "the EGL display lacks EGL_KHR_surfaceless_context",
            ));
        }
        api.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| setup(Step::Context, format!("eglBindAPI(OpenGL ES): {e}")))?;
        let config = if names(&extensions, "EGL_KHR_no_config_context") {
            // SAFETY: EGL_NO_CONFIG_KHR, the null config, is what
            // EGL_KHR_no_config_context asks for.
            #[expect(unsafe_code, reason = "EGL_NO_CONFIG_KHR")]
            unsafe {
                egl::Config::from_ptr(std::ptr::null_mut())
            }
        } else {
            api.choose_first_config(display, &[egl::RENDERABLE_TYPE, OPENGL_ES3_BIT, egl::NONE])
                .ok()
                .flatten()
                .ok_or_else(|| setup(Step::Context, "no EGL config renders OpenGL ES 3"))?
        };
        let version = [CONTEXT_MAJOR_VERSION, 3, CONTEXT_MINOR_VERSION, 0];
        let robust = [
            CONTEXT_OPENGL_ROBUST_ACCESS_EXT,
            ATTRIB_TRUE,
            CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT,
            LOSE_CONTEXT_ON_RESET_EXT,
        ];
        let plain: Vec<egl::Int> = version.iter().copied().chain([egl::NONE]).collect();
        let context = if names(&extensions, "EGL_EXT_create_context_robustness") {
            let with: Vec<egl::Int> = version
                .iter()
                .chain(&robust)
                .copied()
                .chain([egl::NONE])
                .collect();
            api.create_context(display, config, None, &with)
                .or_else(|_| api.create_context(display, config, None, &plain))
        } else {
            api.create_context(display, config, None, &plain)
        }
        .map_err(|e| {
            setup(
                Step::Context,
                format!("eglCreateContext(OpenGL ES 3.0): {e}"),
            )
        })?;
        if let Err(e) = api.make_current(display, None, None, Some(context)) {
            let _ = api.destroy_context(display, context);
            return Err(setup(Step::Context, format!("eglMakeCurrent: {e}")));
        }
        Ok((context, extensions))
    }

    /// The extensions of the display's EGL device, where EGL reports its
    /// device (`EGL_EXT_device_query`).
    fn device_extensions(&self) -> Option<String> {
        let attrib = self.api.get_proc_address("eglQueryDisplayAttribEXT")?;
        let string = self.api.get_proc_address("eglQueryDeviceStringEXT")?;
        // SAFETY: both pointers are the functions of their names, which
        // EGL_EXT_device_query declares with these signatures; the first
        // writes one attribute into `device`, and the second returns a
        // string EGL owns, or null.
        #[expect(unsafe_code, reason = "EGL's device query, loaded by name")]
        unsafe {
            let attrib = std::mem::transmute::<extern "system" fn(), QueryDisplayAttrib>(attrib);
            let string = std::mem::transmute::<extern "system" fn(), QueryDeviceString>(string);
            let mut device: egl::Attrib = 0;
            if attrib(self.display.as_ptr(), DEVICE_EXT, &mut device) != egl::TRUE {
                return None;
            }
            let list = string(
                std::ptr::with_exposed_provenance_mut(device),
                egl::EXTENSIONS,
            );
            (!list.is_null()).then(|| CStr::from_ptr(list).to_string_lossy().into_owned())
        }
    }

    /// Whether the display has the extension `name`.
    pub(super) fn has(&self, name: &str) -> bool {
        names(&self.extensions, name)
    }

    /// Makes the context current on this thread where it is not, so a call
    /// never reaches another context the thread made current since.
    pub(super) fn current(&self) -> Result<(), String> {
        if self.api.get_current_context() == Some(self.context) {
            return Ok(());
        }
        self.api
            .make_current(self.display, None, None, Some(self.context))
            .map_err(|e| format!("eglMakeCurrent: {e}"))
    }

    /// A GL or EGL function by name.
    pub(super) fn proc_address(&self, name: &str) -> Option<extern "system" fn()> {
        self.api.get_proc_address(name)
    }

    /// Imports `buffer` as an EGL image.
    pub(super) fn import(&self, buffer: &DmaBuf) -> Result<egl::Image, String> {
        if !self.has("EGL_EXT_image_dma_buf_import") {
            return Err("the EGL display lacks EGL_EXT_image_dma_buf_import".to_owned());
        }
        let modifier = buffer.modifier();
        let explicit = modifier != MODIFIER_INVALID;
        if explicit && !self.has("EGL_EXT_image_dma_buf_import_modifiers") {
            return Err(format!(
                "the EGL display cannot import a buffer of modifier {modifier:#x}"
            ));
        }
        let attrib = |v: u32| egl::Attrib::try_from(v).map_err(|e| e.to_string());
        let (width, height) = buffer.size();
        let mut attribs = vec![
            egl::Attrib::try_from(egl::WIDTH).map_err(|e| e.to_string())?,
            attrib(width)?,
            egl::Attrib::try_from(egl::HEIGHT).map_err(|e| e.to_string())?,
            attrib(height)?,
            LINUX_DRM_FOURCC_EXT,
            attrib(buffer.format())?,
        ];
        let lo = attrib(u32::try_from(modifier & 0xffff_ffff).unwrap_or(0))?;
        let hi = attrib(u32::try_from(modifier >> 32).unwrap_or(0))?;
        for ((fd, offset, stride), names) in buffer.planes().zip(PLANE_ATTRIBS) {
            let [fd_name, offset_name, pitch_name, lo_name, hi_name] = names;
            let fd = egl::Attrib::try_from(fd.as_raw_fd()).map_err(|e| e.to_string())?;
            attribs.extend([
                fd_name,
                fd,
                offset_name,
                attrib(offset)?,
                pitch_name,
                attrib(stride)?,
            ]);
            if explicit {
                attribs.extend([lo_name, lo, hi_name, hi]);
            }
        }
        attribs.push(egl::ATTRIB_NONE);
        // SAFETY: EGL_LINUX_DMA_BUF_EXT takes no context and no client
        // buffer, and the descriptors the list names stay open through the
        // call; EGL takes no ownership of them.
        #[expect(unsafe_code, reason = "EGL_NO_CONTEXT and a null client buffer")]
        let (no_context, no_buffer) = unsafe {
            (
                egl::Context::from_ptr(egl::NO_CONTEXT),
                egl::ClientBuffer::from_ptr(std::ptr::null_mut()),
            )
        };
        self.api
            .create_image(
                self.display,
                no_context,
                LINUX_DMA_BUF_EXT,
                no_buffer,
                &attribs,
            )
            .map_err(|e| format!("eglCreateImage(EGL_LINUX_DMA_BUF_EXT): {e}"))
    }

    /// Destroys an image [`Egl::import`] made.
    pub(super) fn destroy_image(&self, image: egl::Image) {
        if let Err(e) = self.api.destroy_image(self.display, image) {
            log::warn!("eglDestroyImage: {e}");
        }
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        let _ = self.api.make_current(self.display, None, None, None);
        let _ = self.api.destroy_context(self.display, self.context);
        if self.terminate {
            let _ = self.api.terminate(self.display);
        }
    }
}

/// Mesa's software device: the EGL device whose extensions name
/// `EGL_MESA_device_software`.
fn software_device(api: &Instance) -> Result<*mut c_void, SetupError> {
    let missing = || {
        setup(
            Step::EglDisplay,
            "EGL has no software device: Mesa's llvmpipe is not installed \
             (Debian and Ubuntu: libgl1-mesa-dri and libegl-mesa0; Arch: mesa)",
        )
    };
    let (Some(query), Some(string)) = (
        api.get_proc_address("eglQueryDevicesEXT"),
        api.get_proc_address("eglQueryDeviceStringEXT"),
    ) else {
        return Err(missing());
    };
    // SAFETY: both pointers are the functions of their names, which
    // EGL_EXT_device_enumeration and EGL_EXT_device_query declare with
    // these signatures. eglQueryDevicesEXT writes at most `DEVICES_MAX`
    // devices into a list that long, and eglQueryDeviceStringEXT returns a
    // string EGL owns, or null.
    #[expect(unsafe_code, reason = "EGL's device enumeration, loaded by name")]
    unsafe {
        let query: QueryDevices = std::mem::transmute::<extern "system" fn(), QueryDevices>(query);
        let string: QueryDeviceString =
            std::mem::transmute::<extern "system" fn(), QueryDeviceString>(string);
        let mut devices = [std::ptr::null_mut(); DEVICES_MAX];
        let mut count: egl::Int = 0;
        let max = egl::Int::try_from(DEVICES_MAX).unwrap_or(0);
        if query(max, devices.as_mut_ptr(), &mut count) != egl::TRUE {
            return Err(missing());
        }
        let count = usize::try_from(count).unwrap_or(0).min(DEVICES_MAX);
        for &device in devices.iter().take(count) {
            let extensions = string(device, egl::EXTENSIONS);
            if !extensions.is_null()
                && names(&CStr::from_ptr(extensions).to_string_lossy(), SOFTWARE)
            {
                return Ok(device);
            }
        }
    }
    Err(missing())
}
