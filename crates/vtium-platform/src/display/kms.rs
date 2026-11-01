//! What drm2d asks of a DRM card, as a trait: [`super::Card`] implements it
//! through the kernel's ioctls, and the tests' stand-in device in memory.

use std::io;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::rc::Rc;

use super::Mode;
use crate::bounds::{BLOB_MAX, ENCODERS_MAX, MODES_MAX, OBJECT_PROPERTIES_MAX, PLANES_MAX};

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
    /// The physical size in millimetres, across and down, as the kernel
    /// reads it from the EDID; 0 where it has none.
    pub(crate) mm: (u32, u32),
    /// The encoder it is bound to, 0 for none.
    pub(crate) encoder: u32,
    pub(crate) encoders: Vec<u32>,
}

/// What one read of a connector gives besides its lists: the counts of
/// its modes and encoders, which can be larger than the buffers the read
/// was given.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ConnectorRead {
    pub(crate) kind: u32,
    pub(crate) kind_id: u32,
    pub(crate) connected: bool,
    pub(crate) mm: (u32, u32),
    pub(crate) encoder: u32,
    pub(crate) modes: usize,
    pub(crate) encoders: usize,
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

/// An event the card reported, with the CRTC it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrtcEvent {
    /// A page flip completed.
    Flipped(u32),
    /// The vblank [`Kms::wait_vblank`] asked for passed.
    Vblank(u32),
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

    /// One read of connector `id`, a GETCONNECTOR: its modes into `modes`
    /// and its encoders into `encoders`, each list only where its buffer
    /// holds the whole list, and the counts the connector has either way.
    /// A read whose `modes` is empty probes the connector first while
    /// vtium holds DRM master, so one plugged in since the last probe shows
    /// its modes; without master the kernel reads what it knows.
    fn read_connector(
        &self,
        id: u32,
        modes: &mut [Mode],
        encoders: &mut [u32],
    ) -> io::Result<ConnectorRead>;

    /// Connector `id`, read in two: a probe that gives its counts, then
    /// its lists into buffers of those counts. A connector with more modes
    /// than [`MODES_MAX`] or more encoders than [`ENCODERS_MAX`] is
    /// refused unread, and so is one whose counts changed between the two
    /// reads, as when the connector was torn down and its id given to
    /// another. The caller reads it again the next time it needs it.
    fn connector(&self, id: u32) -> io::Result<Connector> {
        let first = self.read_connector(id, &mut [], &mut [])?;
        let modes_len = within(first.modes, MODES_MAX, "modes")?;
        let encoders_len = within(first.encoders, ENCODERS_MAX, "encoders")?;
        // Room for one mode where it has none, so the second read does not
        // probe again.
        let mut modes = vec![Mode::default(); modes_len.max(1)];
        let mut encoders = vec![0; encoders_len];
        let info = self.read_connector(id, &mut modes, &mut encoders)?;
        unchanged("modes", modes_len, info.modes)?;
        unchanged("encoders", encoders_len, info.encoders)?;
        modes.truncate(modes_len);
        Ok(Connector {
            id,
            kind: info.kind,
            kind_id: info.kind_id,
            connected: info.connected,
            modes,
            mm: info.mm,
            encoder: info.encoder,
            encoders,
        })
    }

    /// The encoder connector `id` is on, 0 for none, in one read with
    /// room for one mode and no encoders, so the connector is not probed,
    /// as libdrm's `drmModeGetConnectorCurrent` reads it. The read's lists
    /// are not used, so no count of them is trusted.
    fn connector_encoder(&self, id: u32) -> io::Result<u32> {
        let mut one = [Mode::default()];
        Ok(self.read_connector(id, &mut one, &mut [])?.encoder)
    }

    fn encoder(&self, id: u32) -> io::Result<Encoder>;
    /// The mode a CRTC shows now, if any.
    fn crtc_mode(&self, id: u32) -> io::Result<Option<Mode>>;

    /// One read of the card's planes, a GETPLANERESOURCES: their ids into
    /// `ids`, at most its length of them, and the count the card has,
    /// which can be larger.
    fn read_planes(&self, ids: &mut [u32]) -> io::Result<usize>;

    /// The card's planes, read in two: their count, then their ids into a
    /// buffer of that count. More planes than [`PLANES_MAX`] are refused
    /// unread, and so is a count that changed between the two reads.
    fn planes(&self) -> io::Result<Vec<u32>> {
        let len = within(self.read_planes(&mut [])?, PLANES_MAX, "planes")?;
        let mut ids = vec![0; len];
        unchanged("planes", len, self.read_planes(&mut ids)?)?;
        Ok(ids)
    }

    /// A plane's possible CRTCs, a bit per index into
    /// [`Resources::crtcs`].
    fn plane_crtcs(&self, id: u32) -> io::Result<u32>;

    /// One read of the properties of object `id`, an OBJ_GETPROPERTIES:
    /// their ids into `ids` and their values into `values`, at most the
    /// shorter buffer's length of them, and the count the object has,
    /// which can be larger.
    fn read_properties(
        &self,
        id: u32,
        kind: Object,
        ids: &mut [u32],
        values: &mut [u64],
    ) -> io::Result<usize>;

    /// The name of property `id`, in one GETPROPERTY that asks for none of
    /// the property's values or enumerators.
    fn property_name(&self, id: u32) -> io::Result<String>;

    /// The properties of object `id`, read in two: their count, then
    /// their ids and values into buffers of that count, and then each
    /// one's name. More properties than [`OBJECT_PROPERTIES_MAX`] are
    /// refused unread, and so is a count that changed between the two
    /// reads.
    fn properties(&self, id: u32, kind: Object) -> io::Result<Vec<Property>> {
        let first = self.read_properties(id, kind, &mut [], &mut [])?;
        let len = within(first, OBJECT_PROPERTIES_MAX, "properties")?;
        let (mut ids, mut values) = (vec![0; len], vec![0; len]);
        let again = self.read_properties(id, kind, &mut ids, &mut values)?;
        unchanged("properties", len, again)?;
        ids.into_iter()
            .zip(values)
            .map(|(id, value)| {
                Ok(Property {
                    id,
                    name: self.property_name(id)?,
                    value,
                })
            })
            .collect()
    }

    /// A 32-bit dumb buffer.
    fn create_dumb(&self, width: u32, height: u32) -> io::Result<Dumb>;
    fn destroy_dumb(&self, handle: u32) -> io::Result<()>;
    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Self::Map>;
    /// A framebuffer of `format`, a fourcc, over one buffer the card
    /// holds: its handle and its pitch in bytes. The buffer is a dumb one
    /// or any other the card can scan out.
    fn add_fb(
        &self,
        handle: u32,
        pitch: u32,
        width: u32,
        height: u32,
        format: u32,
    ) -> io::Result<u32>;
    fn rm_fb(&self, id: u32) -> io::Result<()>;
    fn create_blob(&self, data: &[u8]) -> io::Result<u32>;
    fn destroy_blob(&self, id: u32) -> io::Result<()>;
    /// One read of the property blob `id`, as a blob property's value
    /// names it, into `data`: the card copies the blob only when `data` is
    /// exactly the blob's length, and gives back the blob's length either
    /// way. A blob the card does not hold is `ENOENT`, 0 included.
    fn read_blob(&self, id: u32, data: &mut [u8]) -> io::Result<usize>;

    /// The bytes of the property blob `id`, read in two: its length, then
    /// its bytes into a buffer of that length. A blob longer than
    /// [`BLOB_MAX`] is refused unread, and so is one whose length changed
    /// between the two reads, as when the blob was freed and its id given
    /// to another.
    fn blob(&self, id: u32) -> io::Result<Vec<u8>> {
        let len = self.read_blob(id, &mut [])?;
        if len > BLOB_MAX {
            return Err(io::Error::other(format!(
                "the blob is {len} bytes, past the {BLOB_MAX} vtium reads"
            )));
        }
        let mut data = vec![0; len];
        let again = self.read_blob(id, &mut data)?;
        if again != len {
            return Err(io::Error::other(format!(
                "the blob changed from {len} to {again} bytes between two reads"
            )));
        }
        Ok(data)
    }

    /// An atomic commit with `DRM_MODE_*` flags.
    fn commit(&self, request: &Request, flags: u32) -> io::Result<()>;
    /// The legacy modeset of one CRTC.
    fn set_crtc(&self, crtc: u32, fb: u32, connector: u32, mode: &Mode) -> io::Result<()>;
    /// The legacy modeset that turns a CRTC off: no framebuffer, no
    /// connector and no mode. A card that cannot says so with
    /// `Unsupported`.
    fn disable_crtc(&self, _crtc: u32) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    /// The legacy page flip, with a completion event.
    fn page_flip(&self, crtc: u32, fb: u32) -> io::Result<()>;
    /// Hides the legacy cursor of a CRTC.
    fn hide_cursor(&self, crtc: u32) -> io::Result<()>;
    fn set_connector_property(&self, connector: u32, property: u32, value: u64) -> io::Result<()>;

    /// Asks for an event at the next vblank of the CRTC at `index` into
    /// [`Resources::crtcs`], which [`Kms::read_events`] reads.
    fn wait_vblank(&self, index: usize) -> io::Result<()>;

    /// Reads the pending events, adding each to `events`.
    fn read_events(&self, events: &mut Vec<CrtcEvent>) -> io::Result<()>;

    /// The levels of the 1-bit or gray panel the card drives, `None` for
    /// a color display ([`super::depth`]).
    fn levels(&self) -> Option<u8> {
        None
    }

    /// The card's directory in sysfs, which holds a directory for each of
    /// its connectors.
    fn sysfs(&self) -> Option<PathBuf> {
        None
    }
}

/// `count` of a list, refused where it passes `max`, the most vtium reads.
fn within(count: usize, max: usize, what: &str) -> io::Result<usize> {
    if count > max {
        return Err(io::Error::other(format!(
            "it has {count} {what}, past the {max} vtium reads"
        )));
    }
    Ok(count)
}

/// A list's count at the read that filled it, refused where it differs
/// from `sized`, the count at the read that sized its buffer.
fn unchanged(what: &str, sized: usize, filled: usize) -> io::Result<()> {
    if filled != sized {
        return Err(io::Error::other(format!(
            "its {what} changed from {sized} to {filled} between two reads"
        )));
    }
    Ok(())
}
