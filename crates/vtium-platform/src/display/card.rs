//! A DRM card, through the kernel's ioctls as `drm-ffi` wraps them.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::Path;
use std::rc::Rc;

use drm_ffi::mode::dumbbuffer;
use drm_ffi::{
    DRM_EVENT_FLIP_COMPLETE, DRM_MODE_OBJECT_CONNECTOR, DRM_MODE_OBJECT_CRTC,
    DRM_MODE_OBJECT_PLANE, DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::fs::{Mode as FileMode, OFlags, open};
use rustix::io::Errno;

use super::Mode;
use super::kms::{Connector, Dumb, Encoder, Kms, Object, Property, Request, Resources};
use crate::mmap::Mapping;

/// `DRM_MODE_CONNECTED`, `drm/drm_mode.h`.
const CONNECTED: u32 = 1;

/// A DRM card's primary node, opened read-write and non-blocking.
pub struct Card {
    fd: Rc<OwnedFd>,
}

impl Card {
    /// Takes a card the seat opened. Reads of its events must not block,
    /// so the descriptor is made non-blocking.
    pub fn new(fd: OwnedFd) -> io::Result<Card> {
        let flags = rustix::fs::fcntl_getfl(&fd)?;
        rustix::fs::fcntl_setfl(&fd, flags | OFlags::NONBLOCK)?;
        Ok(Card { fd: Rc::new(fd) })
    }

    /// Opens a card by its path.
    pub fn open(path: &Path) -> io::Result<Card> {
        let fd = open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOCTTY,
            FileMode::empty(),
        )?;
        Ok(Card { fd: Rc::new(fd) })
    }

    fn raw(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// A NUL-terminated name from a kernel structure.
fn name(raw: &[core::ffi::c_char]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .map(|&c| c.to_ne_bytes()[0])
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn len(n: u64) -> io::Result<usize> {
    usize::try_from(n).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
}

impl Kms for Card {
    type Map = Mapping;

    fn fd(&self) -> Rc<OwnedFd> {
        Rc::clone(&self.fd)
    }

    fn set_client_cap(&self, cap: u32, on: bool) -> io::Result<()> {
        drm_ffi::set_capability(self.raw(), u64::from(cap), on)?;
        Ok(())
    }

    fn cap(&self, cap: u32) -> io::Result<u64> {
        Ok(drm_ffi::get_capability(self.raw(), u64::from(cap))?.value)
    }

    fn set_master(&self) -> io::Result<()> {
        drm_ffi::auth::acquire_master(self.raw())
    }

    fn drop_master(&self) -> io::Result<()> {
        drm_ffi::auth::release_master(self.raw())
    }

    fn resources(&self) -> io::Result<Resources> {
        let mut res = Resources::default();
        drm_ffi::mode::get_resources(
            self.raw(),
            None,
            Some(&mut res.crtcs),
            Some(&mut res.connectors),
            None,
        )?;
        Ok(res)
    }

    fn connector(&self, id: u32) -> io::Result<Connector> {
        let mut modes = Vec::new();
        let mut encoders = Vec::new();
        // A probe, so a connector plugged in since the last one shows its
        // modes.
        let info = drm_ffi::mode::get_connector(
            self.raw(),
            id,
            None,
            None,
            Some(&mut modes),
            Some(&mut encoders),
            true,
        )?;
        Ok(Connector {
            id,
            kind: info.connector_type,
            kind_id: info.connector_type_id,
            connected: info.connection == CONNECTED,
            modes: modes.into_iter().map(Mode).collect(),
            encoder: info.encoder_id,
            encoders,
        })
    }

    fn encoder(&self, id: u32) -> io::Result<Encoder> {
        let info = drm_ffi::mode::get_encoder(self.raw(), id)?;
        Ok(Encoder {
            crtc: info.crtc_id,
            possible_crtcs: info.possible_crtcs,
        })
    }

    fn crtc_mode(&self, id: u32) -> io::Result<Option<Mode>> {
        let info = drm_ffi::mode::get_crtc(self.raw(), id)?;
        Ok((info.mode_valid != 0).then_some(Mode(info.mode)))
    }

    fn planes(&self) -> io::Result<Vec<u32>> {
        let mut planes = Vec::new();
        drm_ffi::mode::get_plane_resources(self.raw(), Some(&mut planes))?;
        Ok(planes)
    }

    fn plane_crtcs(&self, id: u32) -> io::Result<u32> {
        Ok(drm_ffi::mode::get_plane(self.raw(), id, None)?.possible_crtcs)
    }

    fn properties(&self, id: u32, kind: Object) -> io::Result<Vec<Property>> {
        let kind = match kind {
            Object::Connector => DRM_MODE_OBJECT_CONNECTOR,
            Object::Crtc => DRM_MODE_OBJECT_CRTC,
            Object::Plane => DRM_MODE_OBJECT_PLANE,
        };
        let mut ids = Vec::new();
        let mut values = Vec::new();
        drm_ffi::mode::get_properties(self.raw(), id, kind, Some(&mut ids), Some(&mut values))?;
        ids.into_iter()
            .zip(values)
            .map(|(id, value)| {
                let info = drm_ffi::mode::get_property(self.raw(), id, None, None)?;
                Ok(Property {
                    id,
                    name: name(&info.name),
                    value,
                })
            })
            .collect()
    }

    fn create_dumb(&self, width: u32, height: u32) -> io::Result<Dumb> {
        let info = dumbbuffer::create(self.raw(), width, height, 32, 0)?;
        Ok(Dumb {
            handle: info.handle,
            pitch: info.pitch,
            size: info.size,
        })
    }

    fn destroy_dumb(&self, handle: u32) -> io::Result<()> {
        dumbbuffer::destroy(self.raw(), handle)?;
        Ok(())
    }

    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Mapping> {
        let offset = dumbbuffer::map(self.raw(), dumb.handle, 0, 0)?.offset;
        Mapping::new(self.raw(), len(dumb.size)?, offset)
    }

    fn add_fb(&self, dumb: &Dumb, width: u32, height: u32, format: u32) -> io::Result<u32> {
        let info = drm_ffi::mode::add_fb2(
            self.raw(),
            width,
            height,
            format,
            &[dumb.handle, 0, 0, 0],
            &[dumb.pitch, 0, 0, 0],
            &[0; 4],
            &[0; 4],
            0,
        )?;
        Ok(info.fb_id)
    }

    fn rm_fb(&self, id: u32) -> io::Result<()> {
        drm_ffi::mode::rm_fb(self.raw(), id)
    }

    fn create_blob(&self, data: &[u8]) -> io::Result<u32> {
        let mut data = data.to_vec();
        Ok(drm_ffi::mode::create_property_blob(self.raw(), &mut data)?.blob_id)
    }

    fn destroy_blob(&self, id: u32) -> io::Result<()> {
        drm_ffi::mode::destroy_property_blob(self.raw(), id)?;
        Ok(())
    }

    fn commit(&self, request: &Request, flags: u32) -> io::Result<()> {
        // The kernel takes the properties grouped by object.
        let mut objects: Vec<u32> = Vec::new();
        for &(object, _, _) in &request.entries {
            if !objects.contains(&object) {
                objects.push(object);
            }
        }
        let mut counts = Vec::with_capacity(objects.len());
        let mut props = Vec::with_capacity(request.entries.len());
        let mut values = Vec::with_capacity(request.entries.len());
        for &object in &objects {
            let before = props.len();
            for &(_, prop, value) in request.entries.iter().filter(|e| e.0 == object) {
                props.push(prop);
                values.push(value);
            }
            counts.push(u32::try_from(props.len().saturating_sub(before)).unwrap_or(u32::MAX));
        }
        drm_ffi::mode::atomic_commit(
            self.raw(),
            flags,
            &mut objects,
            &mut counts,
            &mut props,
            &mut values,
        )
    }

    fn set_crtc(&self, crtc: u32, fb: u32, connector: u32, mode: &Mode) -> io::Result<()> {
        drm_ffi::mode::set_crtc(self.raw(), crtc, fb, 0, 0, &[connector], Some(mode.0))?;
        Ok(())
    }

    fn page_flip(&self, crtc: u32, fb: u32) -> io::Result<()> {
        drm_ffi::mode::page_flip(self.raw(), crtc, fb, DRM_MODE_PAGE_FLIP_EVENT, 0)
    }

    fn hide_cursor(&self, crtc: u32) -> io::Result<()> {
        #[expect(
            deprecated,
            reason = "only the legacy path, with no cursor plane, hides a cursor another program left"
        )]
        drm_ffi::mode::set_cursor(self.raw(), crtc, 0, 0, 0)?;
        Ok(())
    }

    fn set_connector_property(&self, connector: u32, property: u32, value: u64) -> io::Result<()> {
        drm_ffi::mode::set_connector_property(self.raw(), connector, property, value)?;
        Ok(())
    }

    fn read_events(&self, flipped: &mut Vec<u32>) -> io::Result<()> {
        let mut buf = [0u8; 1024];
        let n = match rustix::io::read(&*self.fd, &mut buf) {
            Ok(n) => n,
            Err(Errno::AGAIN | Errno::INTR) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        flip_events(buf.get(..n).unwrap_or_default(), flipped);
        Ok(())
    }
}

/// Adds the CRTC of each flip completed in `events`, the bytes a read of
/// the card returned: `struct drm_event` headers, each followed by its
/// body, the CRTC at byte 28 of a `struct drm_event_vblank`.
fn flip_events(mut events: &[u8], flipped: &mut Vec<u32>) {
    let word = |bytes: &[u8], at: usize| -> Option<u32> {
        let end = at.checked_add(4)?;
        Some(u32::from_ne_bytes(bytes.get(at..end)?.try_into().ok()?))
    };
    while let (Some(kind), Some(length)) = (word(events, 0), word(events, 4)) {
        let Ok(length) = usize::try_from(length) else {
            return;
        };
        let Some(event) = events.get(..length).filter(|_| length >= 8) else {
            return;
        };
        if kind == DRM_EVENT_FLIP_COMPLETE {
            flipped.extend(word(event, 28));
        }
        events = events.get(length..).unwrap_or_default();
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn event(kind: u32, crtc: u32) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend(kind.to_ne_bytes());
        e.extend(32u32.to_ne_bytes());
        e.extend([0u8; 20]);
        e.extend(crtc.to_ne_bytes());
        e
    }

    #[test]
    fn flip_events_name_their_crtcs() {
        let mut bytes = event(DRM_EVENT_FLIP_COMPLETE, 41);
        bytes.extend(event(1, 99));
        bytes.extend(event(DRM_EVENT_FLIP_COMPLETE, 42));
        let mut flipped = Vec::new();
        flip_events(&bytes, &mut flipped);
        assert_eq!(flipped, [41, 42]);
    }

    #[test]
    fn a_truncated_event_ends_the_read() {
        let mut bytes = event(DRM_EVENT_FLIP_COMPLETE, 41);
        bytes.extend(&event(DRM_EVENT_FLIP_COMPLETE, 42)[..20]);
        let mut flipped = Vec::new();
        flip_events(&bytes, &mut flipped);
        assert_eq!(flipped, [41]);
        let mut zero = event(DRM_EVENT_FLIP_COMPLETE, 41);
        zero[4..8].copy_from_slice(&0u32.to_ne_bytes());
        flip_events(&zero, &mut flipped);
        assert_eq!(flipped, [41]);
    }

    /// The real ioctls on a render node, which needs no DRM master and
    /// changes nothing on the screen: a capability read, an empty event
    /// read, and a dumb buffer, which the kernel keeps to primary nodes
    /// and refuses here with EACCES. The buffer's path runs on a primary
    /// node in tier 3. A host with no render node says so and proves
    /// nothing here.
    #[test]
    fn a_render_node_through_the_real_ioctls() {
        let Some(path) = (128..136)
            .map(|n| std::path::PathBuf::from(format!("/dev/dri/renderD{n}")))
            .find(|p| p.exists())
        else {
            eprintln!("no render node: skipped");
            return;
        };
        let card = Card::open(&path).expect("open the render node");
        card.cap(drm_ffi::DRM_CAP_DUMB_BUFFER)
            .expect("read a capability");
        let mut flipped = Vec::new();
        card.read_events(&mut flipped).expect("read no events");
        assert!(flipped.is_empty());
        match card.create_dumb(64, 32) {
            Ok(dumb) => {
                assert!(dumb.pitch >= 64 * 4);
                let mut map = card.map_dumb(&dumb).expect("map the dumb buffer");
                map.as_mut().fill(0x5a);
                assert!(map.as_mut().iter().all(|&b| b == 0x5a));
                drop(map);
                card.destroy_dumb(dumb.handle)
                    .expect("destroy the dumb buffer");
            }
            Err(e) => assert_eq!(e.raw_os_error(), Some(Errno::ACCESS.raw_os_error())),
        }
    }
}
