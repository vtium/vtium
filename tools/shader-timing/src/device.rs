//! The GL context the harness draws on: OpenGL ES 3.0 or later, with no
//! surface, on a device the caller chooses.
//!
//! The harness opens libEGL at run time and asks for one of three devices.
//! Mesa's surfaceless platform gives the GPU Mesa picks by itself. EGL's
//! device platform gives the GPU behind a named render node, or Mesa's
//! software renderer, llvmpipe. Every context is surfaceless: it draws only
//! into textures it owns, so it needs no display, no DRM master and no mode.

use std::ffi::{CStr, c_char, c_void};

use khronos_egl as egl;

type Instance = egl::DynamicInstance<egl::EGL1_5>;

/// The runtime's soname, never the unversioned name a development package
/// installs.
const LIBRARY: &str = "libEGL.so.1";
const PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const CONTEXT_MAJOR_VERSION: egl::Int = 0x3098;
const CONTEXT_MINOR_VERSION: egl::Int = 0x30FB;
const OPENGL_ES3_BIT: egl::Int = 0x40;
/// `EGL_DEVICE_EXT`, the display's device in `eglQueryDisplayAttribEXT`.
const DEVICE_EXT: egl::Int = 0x322C;
/// `EGL_DRM_RENDER_NODE_FILE_EXT`, a device's render node path.
const DRM_RENDER_NODE_FILE_EXT: egl::Int = 0x3377;
/// The extension a device of Mesa's software renderer names.
const SOFTWARE: &str = "EGL_MESA_device_software";
/// Most devices the harness looks through.
const DEVICES_MAX: usize = 32;

type QueryDevices = unsafe extern "system" fn(egl::Int, *mut *mut c_void, *mut egl::Int) -> u32;
type QueryDeviceString = unsafe extern "system" fn(*mut c_void, egl::Int) -> *const c_char;
type QueryDisplayAttrib = unsafe extern "system" fn(*mut c_void, egl::Int, *mut egl::Attrib) -> u32;

/// Which device the harness draws on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Mesa's surfaceless platform, which picks a GPU by itself.
    Surfaceless,
    /// Mesa's software renderer, llvmpipe, through the device platform.
    Software,
    /// The GPU behind this render node, such as `/dev/dri/renderD128`.
    Node(String),
}

impl Choice {
    /// Reads `surfaceless`, `software` or a render node's path.
    pub fn parse(text: &str) -> Choice {
        match text {
            "surfaceless" => Choice::Surfaceless,
            "software" => Choice::Software,
            path => Choice::Node(path.to_owned()),
        }
    }
}

/// A surfaceless GL context, current on the thread that made it.
pub struct Device {
    pub gl: glow::Context,
    /// `GL_RENDERER`.
    pub renderer: String,
    /// `GL_VERSION`.
    pub version: String,
    /// The render node EGL reports for the display's device, or
    /// "software" for llvmpipe, or "unknown".
    pub node: String,
    api: Instance,
    display: egl::Display,
    context: egl::Context,
}

impl Drop for Device {
    /// Releases the context, and leaves the display initialized: EGL gives
    /// every caller of the same device one display, so terminating it
    /// would end the contexts other threads still draw with.
    fn drop(&mut self) {
        let _ = self.api.make_current(self.display, None, None, None);
        let _ = self.api.destroy_context(self.display, self.context);
    }
}

fn names(list: &str, name: &str) -> bool {
    list.split_ascii_whitespace().any(|e| e == name)
}

/// EGL's device query functions, loaded by name.
struct DeviceQuery {
    devices: QueryDevices,
    string: QueryDeviceString,
    attrib: QueryDisplayAttrib,
}

impl DeviceQuery {
    fn load(api: &Instance) -> Option<DeviceQuery> {
        let devices = api.get_proc_address("eglQueryDevicesEXT")?;
        let string = api.get_proc_address("eglQueryDeviceStringEXT")?;
        let attrib = api.get_proc_address("eglQueryDisplayAttribEXT")?;
        // SAFETY: each pointer is the EGL function of its name, which
        // EGL_EXT_device_enumeration and EGL_EXT_device_query declare with
        // these signatures.
        unsafe {
            Some(DeviceQuery {
                devices: std::mem::transmute::<extern "system" fn(), QueryDevices>(devices),
                string: std::mem::transmute::<extern "system" fn(), QueryDeviceString>(string),
                attrib: std::mem::transmute::<extern "system" fn(), QueryDisplayAttrib>(attrib),
            })
        }
    }

    fn all(&self) -> Vec<*mut c_void> {
        let mut list = [std::ptr::null_mut(); DEVICES_MAX];
        let mut count: egl::Int = 0;
        // SAFETY: the list holds DEVICES_MAX entries, the count EGL may
        // write at most.
        let ok = unsafe { (self.devices)(DEVICES_MAX as egl::Int, list.as_mut_ptr(), &mut count) };
        if ok != egl::TRUE {
            return Vec::new();
        }
        list[..usize::try_from(count).unwrap_or(0).min(DEVICES_MAX)].to_vec()
    }

    fn text(&self, device: *mut c_void, name: egl::Int) -> Option<String> {
        // SAFETY: `device` is one eglQueryDevicesEXT gave, or one the
        // display reports; the string EGL returns is its own, or null.
        unsafe {
            let raw = (self.string)(device, name);
            (!raw.is_null()).then(|| CStr::from_ptr(raw).to_string_lossy().into_owned())
        }
    }

    fn of_display(&self, display: egl::Display) -> Option<*mut c_void> {
        let mut device: egl::Attrib = 0;
        // SAFETY: the display is initialized, and EGL writes one attribute.
        let ok = unsafe { (self.attrib)(display.as_ptr(), DEVICE_EXT, &mut device) };
        (ok == egl::TRUE).then(|| std::ptr::with_exposed_provenance_mut(device))
    }

    /// The render node, or "software", that names a device.
    fn describe(&self, device: *mut c_void) -> String {
        if self
            .text(device, egl::EXTENSIONS)
            .is_some_and(|e| names(&e, SOFTWARE))
        {
            return "software".to_owned();
        }
        self.text(device, DRM_RENDER_NODE_FILE_EXT)
            .unwrap_or_else(|| "unknown".to_owned())
    }
}

impl Device {
    /// Opens the chosen device and makes an OpenGL ES 3.0 context on it.
    pub fn open(choice: &Choice) -> Result<Device, String> {
        // SAFETY: libEGL.so.1 is the Khronos ABI khronos-egl declares, and
        // loading it runs only its own constructors.
        let api = unsafe { Instance::load_required_from_filename(LIBRARY) }
            .map_err(|e| format!("{LIBRARY} cannot be loaded or lacks EGL 1.5: {e}"))?;
        let client = api
            .query_string(None, egl::EXTENSIONS)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let query = DeviceQuery::load(&api);
        let display = match choice {
            Choice::Surfaceless => {
                if !names(&client, "EGL_MESA_platform_surfaceless") {
                    return Err("libEGL has no surfaceless platform".to_owned());
                }
                // SAFETY: the surfaceless platform takes EGL_DEFAULT_DISPLAY,
                // a null pointer.
                unsafe {
                    api.get_platform_display(
                        PLATFORM_SURFACELESS_MESA,
                        std::ptr::null_mut(),
                        &[egl::ATTRIB_NONE],
                    )
                }
            }
            Choice::Software | Choice::Node(_) => {
                let query = query
                    .as_ref()
                    .ok_or("libEGL lacks EGL_EXT_device_enumeration or EGL_EXT_device_query")?;
                let wanted = match choice {
                    Choice::Node(path) => path.as_str(),
                    _ => "software",
                };
                let device = query
                    .all()
                    .into_iter()
                    .find(|&d| query.describe(d) == wanted)
                    .ok_or_else(|| format!("EGL lists no device for {wanted}"))?;
                // SAFETY: the device platform takes an EGLDeviceEXT, which
                // eglQueryDevicesEXT gave and which lives as long as libEGL.
                unsafe {
                    api.get_platform_display(PLATFORM_DEVICE_EXT, device, &[egl::ATTRIB_NONE])
                }
            }
        }
        .map_err(|e| format!("eglGetPlatformDisplay: {e}"))?;
        api.initialize(display)
            .map_err(|e| format!("eglInitialize: {e}"))?;
        let node = query
            .as_ref()
            .and_then(|q| q.of_display(display).map(|d| q.describe(d)))
            .unwrap_or_else(|| "unknown".to_owned());
        let context = Self::context(&api, display)?;
        // SAFETY: the context is current on this thread, and each name
        // eglGetProcAddress resolves is the GL function of that name.
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                api.get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        };
        use glow::HasContext;
        // SAFETY: plain queries on the current context.
        let (renderer, version) = unsafe {
            (
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
            )
        };
        Ok(Device {
            gl,
            renderer,
            version,
            node,
            api,
            display,
            context,
        })
    }

    fn context(api: &Instance, display: egl::Display) -> Result<egl::Context, String> {
        let extensions = api
            .query_string(Some(display), egl::EXTENSIONS)
            .map(|s| s.to_string_lossy().into_owned())
            .map_err(|e| format!("eglQueryString: {e}"))?;
        if !names(&extensions, "EGL_KHR_surfaceless_context") {
            return Err("the EGL display lacks EGL_KHR_surfaceless_context".to_owned());
        }
        api.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| format!("eglBindAPI(OpenGL ES): {e}"))?;
        let config = if names(&extensions, "EGL_KHR_no_config_context") {
            // SAFETY: EGL_NO_CONFIG_KHR, the null config, is what
            // EGL_KHR_no_config_context asks for.
            unsafe { egl::Config::from_ptr(std::ptr::null_mut()) }
        } else {
            api.choose_first_config(display, &[egl::RENDERABLE_TYPE, OPENGL_ES3_BIT, egl::NONE])
                .ok()
                .flatten()
                .ok_or("no EGL config renders OpenGL ES 3")?
        };
        let attributes = [
            CONTEXT_MAJOR_VERSION,
            3,
            CONTEXT_MINOR_VERSION,
            0,
            egl::NONE,
        ];
        let context = api
            .create_context(display, config, None, &attributes)
            .map_err(|e| format!("eglCreateContext(OpenGL ES 3.0): {e}"))?;
        if let Err(e) = api.make_current(display, None, None, Some(context)) {
            let _ = api.destroy_context(display, context);
            return Err(format!("eglMakeCurrent: {e}"));
        }
        Ok(context)
    }
}
