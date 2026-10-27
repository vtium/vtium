//! What drm2d asks of a DRM card, as a trait: [`super::Card`] implements it
//! through the kernel's ioctls, and the tests' stand-in device in memory.

use std::io;
use std::os::fd::OwnedFd;
use std::rc::Rc;

use super::Mode;

/// The card's CRTCs and connectors, in the kernel's order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Resources {
    pub(crate) crtcs: Vec<u32>,
    pub(crate) connectors: Vec<u32>,
}

/// A connector as the kernel reports it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Connector {
    pub(crate) id: u32,
    /// `DRM_MODE_CONNECTOR_*`.
    pub(crate) kind: u32,
    /// The connector's number among those of its kind.
    pub(crate) kind_id: u32,
    pub(crate) connected: bool,
    pub(crate) modes: Vec<Mode>,
    /// The encoder it is bound to, 0 for none.
    pub(crate) encoder: u32,
    pub(crate) encoders: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Encoder {
    /// The CRTC it drives, 0 for none.
    pub(crate) crtc: u32,
    /// A bit per index into [`Resources::crtcs`].
    pub(crate) possible_crtcs: u32,
}

/// An object's property, by id and name, with its value.
#[derive(Clone, Debug, Default)]
pub(crate) struct Property {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) value: u64,
}

/// The kinds of object whose properties drm2d reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Object {
    Connector,
    Crtc,
    Plane,
}

/// A dumb buffer the card made.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Dumb {
    pub(crate) handle: u32,
    pub(crate) pitch: u32,
    pub(crate) size: u64,
}

/// An atomic request: property values for objects, in the order set.
#[derive(Clone, Debug, Default)]
pub(crate) struct Request {
    pub(crate) entries: Vec<(u32, u32, u64)>,
}

impl Request {
    pub(crate) fn set(&mut self, object: u32, property: u32, value: u64) {
        self.entries.push((object, property, value));
    }
}

/// The operations of a DRM card drm2d uses. Ids are the kernel's.
pub(crate) trait Kms {
    /// A dumb buffer mapped into memory.
    type Map: AsMut<[u8]>;

    /// The descriptor the loop watches: readable when events wait.
    fn fd(&self) -> Rc<OwnedFd>;

    fn set_client_cap(&self, cap: u32, on: bool) -> io::Result<()>;
    fn cap(&self, cap: u32) -> io::Result<u64>;
    fn set_master(&self) -> io::Result<()>;
    fn drop_master(&self) -> io::Result<()>;

    fn resources(&self) -> io::Result<Resources>;
    fn connector(&self, id: u32) -> io::Result<Connector>;
    fn encoder(&self, id: u32) -> io::Result<Encoder>;
    /// The mode a CRTC shows now, if any.
    fn crtc_mode(&self, id: u32) -> io::Result<Option<Mode>>;
    fn planes(&self) -> io::Result<Vec<u32>>;
    /// A plane's possible CRTCs, a bit per index into
    /// [`Resources::crtcs`].
    fn plane_crtcs(&self, id: u32) -> io::Result<u32>;
    fn properties(&self, id: u32, kind: Object) -> io::Result<Vec<Property>>;

    /// A 32-bit dumb buffer.
    fn create_dumb(&self, width: u32, height: u32) -> io::Result<Dumb>;
    fn destroy_dumb(&self, handle: u32) -> io::Result<()>;
    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Self::Map>;
    /// A framebuffer of `format`, a fourcc, over one dumb buffer.
    fn add_fb(&self, dumb: &Dumb, width: u32, height: u32, format: u32) -> io::Result<u32>;
    fn rm_fb(&self, id: u32) -> io::Result<()>;
    fn create_blob(&self, data: &[u8]) -> io::Result<u32>;
    fn destroy_blob(&self, id: u32) -> io::Result<()>;

    /// An atomic commit with `DRM_MODE_*` flags.
    fn commit(&self, request: &Request, flags: u32) -> io::Result<()>;
    /// The legacy modeset of one CRTC.
    fn set_crtc(&self, crtc: u32, fb: u32, connector: u32, mode: &Mode) -> io::Result<()>;
    /// The legacy page flip, with a completion event.
    fn page_flip(&self, crtc: u32, fb: u32) -> io::Result<()>;
    /// Hides the legacy cursor of a CRTC.
    fn hide_cursor(&self, crtc: u32) -> io::Result<()>;
    fn set_connector_property(&self, connector: u32, property: u32, value: u64) -> io::Result<()>;

    /// Reads the pending events, adding the CRTC of each completed flip to
    /// `flipped`.
    fn read_events(&self, flipped: &mut Vec<u32>) -> io::Result<()>;
}
