//! A stand-in DRM card for the tests: connectors, CRTCs and planes in
//! memory, with every commit recorded by property name, flips completed
//! when a test says, and failures planted where a test wants them. It
//! behaves as the kernel does where drm2d depends on it: a commit or a
//! modeset without DRM master is refused with EACCES, a commit while a
//! flip is pending with EBUSY, and a commit asking for a flip event that
//! names no CRTC (`prepare_signaling`) or a CRTC that is off and stays off
//! (`drm_atomic_crtc_check`) with EINVAL. A framebuffer removed while a
//! plane shows it takes the plane off its CRTC (`atomic_remove_fb`).
//! A connector's DPMS and its CRTC's `ACTIVE` are one state, as on an
//! atomic driver: setting either sets the other.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::panic,
    reason = "the stand-in's ids and sizes are small, tests build with overflow checks, and a test fails by panicking"
)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::rc::Rc;

use drm_ffi::{
    DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH, DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC,
    DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, DRM_CLIENT_CAP_UNIVERSAL_PLANES,
    DRM_MODE_ATOMIC_TEST_ONLY, DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::io::Errno;

use super::Mode;
use super::kms::{Connector, Dumb, Encoder, Kms, Object, Property, Request, Resources};

pub(crate) const PRIMARY: u64 = 1;
pub(crate) const CURSOR: u64 = 2;
pub(crate) const OVERLAY: u64 = 0;

/// A commit as the card saw it.
#[derive(Clone, Debug)]
pub(crate) struct Commit {
    pub(crate) flags: u32,
    /// (object, property name, value), in the order set.
    pub(crate) props: Vec<(u32, String, u64)>,
    /// The blobs the commit named, as they were when it was made.
    pub(crate) blobs: BTreeMap<u64, Vec<u8>>,
}

impl Commit {
    pub(crate) fn get(&self, object: u32, name: &str) -> Option<u64> {
        self.props
            .iter()
            .rev()
            .find(|(o, n, _)| *o == object && n == name)
            .map(|p| p.2)
    }

    /// The blob a property named, as it was when the commit was made.
    pub(crate) fn blob(&self, object: u32, name: &str) -> Option<&Vec<u8>> {
        self.blobs.get(&self.get(object, name)?)
    }

    pub(crate) fn test_only(&self) -> bool {
        self.flags & DRM_MODE_ATOMIC_TEST_ONLY != 0
    }

    /// Whether any property of the commit names `object` or holds it.
    pub(crate) fn mentions(&self, object: u32) -> bool {
        self.props
            .iter()
            .any(|(o, _, v)| *o == object || *v == u64::from(object))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Plane {
    pub(crate) crtcs: u32,
    pub(crate) kind: u64,
    pub(crate) damage: bool,
}

/// What the card holds. Tests reach it through [`FakeCard::state`].
#[derive(Debug, Default)]
pub(crate) struct State {
    pub(crate) atomic: bool,
    pub(crate) universal: bool,
    pub(crate) hotspot: bool,
    pub(crate) crtcs: Vec<u32>,
    /// Connectors listed by the card, in order.
    pub(crate) listed: Vec<u32>,
    /// Connectors that answer; one listed and absent here is gone.
    pub(crate) connectors: BTreeMap<u32, Connector>,
    pub(crate) encoders: BTreeMap<u32, Encoder>,
    pub(crate) crtc_modes: BTreeMap<u32, Mode>,
    pub(crate) planes: BTreeMap<u32, Plane>,
    /// The DPMS value of each connector.
    pub(crate) dpms: BTreeMap<u32, u64>,
    /// Connectors that have no DPMS property.
    pub(crate) no_dpms: Vec<u32>,
    /// Objects whose properties cannot be read: the read fails with
    /// EFAULT.
    pub(crate) unreadable: Vec<u32>,
    /// Planes whose possible CRTCs cannot be read, and whether the plane
    /// list cannot: each read fails with EIO.
    pub(crate) crtcs_unreadable: Vec<u32>,
    pub(crate) planes_unreadable: bool,
    /// The CRTC each connector is on, each CRTC's `ACTIVE`, and the CRTC
    /// each plane is on, as the commits so far left them.
    pub(crate) bound: BTreeMap<u32, u32>,
    pub(crate) active: BTreeMap<u32, bool>,
    pub(crate) plane_crtc: BTreeMap<u32, u32>,
    /// The framebuffer each plane shows.
    pub(crate) plane_fb: BTreeMap<u32, u32>,
    pub(crate) master: bool,
    /// Another process holds master: taking it fails.
    pub(crate) master_elsewhere: bool,
    pub(crate) commits: Vec<Commit>,
    /// Errors the next commits return, one each.
    pub(crate) commit_errors: VecDeque<Errno>,
    /// CRTCs no commit may drive, as past a bandwidth limit: a commit
    /// that names one is refused with EINVAL.
    pub(crate) refused: Vec<u32>,
    pub(crate) page_flips: Vec<(u32, u32)>,
    pub(crate) set_crtcs: Vec<(u32, u32, u32, Mode)>,
    /// CRTCs with a flip pending, and those whose flip completed.
    pub(crate) pending: Vec<u32>,
    /// Flips complete without their events, as a driver that loses them.
    pub(crate) lose_flips: bool,
    pub(crate) completed: Vec<u32>,
    pub(crate) blobs: BTreeMap<u32, Vec<u8>>,
    pub(crate) dumbs: BTreeMap<u32, (u32, u32)>,
    pub(crate) fbs: BTreeMap<u32, (u32, u32)>,
}

/// The stand-in card.
#[derive(Debug)]
pub(crate) struct FakeCard {
    state: RefCell<State>,
    next: Cell<u32>,
    /// Property names by id; ids are given per name.
    names: RefCell<Vec<String>>,
    /// A socket pair: a byte written to the one makes the other, which
    /// the loop watches, readable while flip events wait.
    events: (Rc<OwnedFd>, UnixStream),
}

impl Default for FakeCard {
    fn default() -> FakeCard {
        let (watched, writer) = UnixStream::pair().unwrap_or_else(|e| panic!("socketpair: {e}"));
        watched
            .set_nonblocking(true)
            .unwrap_or_else(|e| panic!("non-blocking: {e}"));
        FakeCard {
            state: RefCell::default(),
            next: Cell::new(1000),
            names: RefCell::default(),
            events: (Rc::new(OwnedFd::from(watched)), writer),
        }
    }
}

impl FakeCard {
    /// An atomic card with universal planes, one CRTC per connector, each
    /// with a primary plane taking damage clips and a cursor plane, and
    /// one overlay plane on every CRTC.
    pub(crate) fn new(connectors: &[(u32, Vec<Mode>)]) -> FakeCard {
        let card = FakeCard::default();
        {
            let mut s = card.state.borrow_mut();
            s.atomic = true;
            s.universal = true;
            s.hotspot = true;
            let all = (1u32 << connectors.len()) - 1;
            for (i, (id, modes)) in connectors.iter().enumerate() {
                let crtc = 100 + u32::try_from(i).unwrap_or(0);
                let encoder = 200 + u32::try_from(i).unwrap_or(0);
                s.crtcs.push(crtc);
                s.encoders.insert(
                    encoder,
                    Encoder {
                        crtc: 0,
                        possible_crtcs: 1 << i,
                    },
                );
                s.planes.insert(
                    300 + 2 * u32::try_from(i).unwrap_or(0),
                    Plane {
                        crtcs: 1 << i,
                        kind: PRIMARY,
                        damage: true,
                    },
                );
                s.planes.insert(
                    301 + 2 * u32::try_from(i).unwrap_or(0),
                    Plane {
                        crtcs: 1 << i,
                        kind: CURSOR,
                        damage: false,
                    },
                );
                s.listed.push(*id);
                s.dpms.insert(*id, 0);
                s.connectors.insert(
                    *id,
                    Connector {
                        id: *id,
                        kind: 11,
                        kind_id: u32::try_from(i).unwrap_or(0) + 1,
                        connected: true,
                        modes: modes.clone(),
                        encoder: 0,
                        encoders: vec![encoder],
                    },
                );
            }
            s.planes.insert(
                399,
                Plane {
                    crtcs: all,
                    kind: OVERLAY,
                    damage: false,
                },
            );
        }
        card
    }

    pub(crate) fn state(&self) -> std::cell::RefMut<'_, State> {
        self.state.borrow_mut()
    }

    /// Completes every pending flip: their events are ready to read, or
    /// lost while [`State::lose_flips`] is set.
    pub(crate) fn complete_flips(&self) {
        let mut s = self.state.borrow_mut();
        let pending = std::mem::take(&mut s.pending);
        if s.lose_flips {
            return;
        }
        if !pending.is_empty() {
            let _ = (&self.events.1).write(b"e");
        }
        s.completed.extend(pending);
    }

    /// The commits that were not test-only.
    pub(crate) fn real_commits(&self) -> Vec<Commit> {
        let s = self.state.borrow();
        s.commits
            .iter()
            .filter(|c| !c.test_only())
            .cloned()
            .collect()
    }

    pub(crate) fn last_commit(&self) -> Option<Commit> {
        self.real_commits().pop()
    }

    fn id(&self) -> u32 {
        let id = self.next.get();
        self.next.set(id + 1);
        id
    }

    fn prop_id(&self, name: &str) -> u32 {
        let mut names = self.names.borrow_mut();
        let at = names.iter().position(|n| n == name).unwrap_or_else(|| {
            names.push(name.to_owned());
            names.len() - 1
        });
        10_000 + u32::try_from(at).unwrap_or(0)
    }

    fn prop_name(&self, id: u32) -> String {
        let at = usize::try_from(id - 10_000).unwrap_or(usize::MAX);
        self.names.borrow().get(at).cloned().unwrap_or_default()
    }

    fn props(&self, names: &[(&str, u64)]) -> Vec<Property> {
        names
            .iter()
            .map(|&(name, value)| Property {
                id: self.prop_id(name),
                name: name.to_owned(),
                value,
            })
            .collect()
    }
}

fn err(e: Errno) -> io::Error {
    io::Error::from_raw_os_error(e.raw_os_error())
}

const PLANE_PROPS: [&str; 10] = [
    "FB_ID", "CRTC_ID", "SRC_X", "SRC_Y", "SRC_W", "SRC_H", "CRTC_X", "CRTC_Y", "CRTC_W", "CRTC_H",
];

impl Kms for FakeCard {
    type Map = Vec<u8>;

    fn fd(&self) -> Rc<OwnedFd> {
        Rc::clone(&self.events.0)
    }

    fn set_client_cap(&self, cap: u32, _on: bool) -> io::Result<()> {
        let s = self.state.borrow();
        let ok = match cap {
            DRM_CLIENT_CAP_UNIVERSAL_PLANES => s.universal,
            DRM_CLIENT_CAP_ATOMIC => s.atomic,
            DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT => s.hotspot,
            _ => false,
        };
        if ok { Ok(()) } else { Err(err(Errno::INVAL)) }
    }

    fn cap(&self, cap: u32) -> io::Result<u64> {
        match cap {
            DRM_CAP_DUMB_BUFFER => Ok(1),
            DRM_CAP_CURSOR_WIDTH | DRM_CAP_CURSOR_HEIGHT => Ok(256),
            _ => Err(err(Errno::INVAL)),
        }
    }

    fn set_master(&self) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if s.master_elsewhere {
            return Err(err(Errno::BUSY));
        }
        s.master = true;
        Ok(())
    }

    fn drop_master(&self) -> io::Result<()> {
        self.state.borrow_mut().master = false;
        Ok(())
    }

    fn resources(&self) -> io::Result<Resources> {
        let s = self.state.borrow();
        Ok(Resources {
            crtcs: s.crtcs.clone(),
            connectors: s.listed.clone(),
        })
    }

    fn connector(&self, id: u32) -> io::Result<Connector> {
        let s = self.state.borrow();
        s.connectors
            .get(&id)
            .cloned()
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn encoder(&self, id: u32) -> io::Result<Encoder> {
        let s = self.state.borrow();
        s.encoders
            .get(&id)
            .copied()
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn crtc_mode(&self, id: u32) -> io::Result<Option<Mode>> {
        Ok(self.state.borrow().crtc_modes.get(&id).copied())
    }

    fn planes(&self) -> io::Result<Vec<u32>> {
        let s = self.state.borrow();
        if s.planes_unreadable {
            return Err(err(Errno::IO));
        }
        Ok(s.planes.keys().copied().collect())
    }

    fn plane_crtcs(&self, id: u32) -> io::Result<u32> {
        let s = self.state.borrow();
        if s.crtcs_unreadable.contains(&id) {
            return Err(err(Errno::IO));
        }
        s.planes
            .get(&id)
            .map(|p| p.crtcs)
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn properties(&self, id: u32, kind: Object) -> io::Result<Vec<Property>> {
        let s = self.state.borrow();
        if s.unreadable.contains(&id) {
            return Err(err(Errno::FAULT));
        }
        match kind {
            Object::Connector => {
                let dpms = *s.dpms.get(&id).ok_or_else(|| err(Errno::NOENT))?;
                let with = !s.no_dpms.contains(&id);
                drop(s);
                let names: Vec<(&str, u64)> = std::iter::once(("CRTC_ID", 0))
                    .chain(with.then_some(("DPMS", dpms)))
                    .collect();
                Ok(self.props(&names))
            }
            Object::Crtc => {
                let active = u64::from(s.active.get(&id).copied().unwrap_or(false));
                drop(s);
                Ok(self.props(&[("MODE_ID", 0), ("ACTIVE", active)]))
            }
            Object::Plane => {
                let plane = s
                    .planes
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| err(Errno::NOENT))?;
                let crtc = u64::from(s.plane_crtc.get(&id).copied().unwrap_or(0));
                drop(s);
                let mut names: Vec<(&str, u64)> = PLANE_PROPS
                    .iter()
                    .map(|&n| (n, if n == "CRTC_ID" { crtc } else { 0 }))
                    .collect();
                names.push(("type", plane.kind));
                if plane.damage {
                    names.push(("FB_DAMAGE_CLIPS", 0));
                }
                if plane.kind == CURSOR {
                    names.extend([("HOTSPOT_X", 0), ("HOTSPOT_Y", 0)]);
                }
                Ok(self.props(&names))
            }
        }
    }

    fn create_dumb(&self, width: u32, height: u32) -> io::Result<Dumb> {
        let handle = self.id();
        self.state
            .borrow_mut()
            .dumbs
            .insert(handle, (width, height));
        Ok(Dumb {
            handle,
            pitch: width * 4,
            size: u64::from(width * 4 * height),
        })
    }

    fn destroy_dumb(&self, handle: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        s.dumbs
            .remove(&handle)
            .map(|_| ())
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Vec<u8>> {
        // A mapping holds whatever the device memory held.
        Ok(vec![0xa5; usize::try_from(dumb.size).unwrap_or(0)])
    }

    fn add_fb(&self, _: &Dumb, width: u32, height: u32, _: u32) -> io::Result<u32> {
        let fb = self.id();
        self.state.borrow_mut().fbs.insert(fb, (width, height));
        Ok(fb)
    }

    /// Removing a framebuffer a plane shows takes the plane off its CRTC,
    /// as the kernel's `drm_mode_rmfb` does (`atomic_remove_fb`).
    fn rm_fb(&self, id: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        s.fbs.remove(&id).ok_or_else(|| err(Errno::NOENT))?;
        let shown: Vec<u32> = s
            .plane_fb
            .iter()
            .filter(|&(_, &fb)| fb == id)
            .map(|(&plane, _)| plane)
            .collect();
        for plane in shown {
            s.plane_fb.remove(&plane);
            s.plane_crtc.remove(&plane);
        }
        Ok(())
    }

    fn create_blob(&self, data: &[u8]) -> io::Result<u32> {
        let id = self.id();
        self.state.borrow_mut().blobs.insert(id, data.to_vec());
        Ok(id)
    }

    fn destroy_blob(&self, id: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        s.blobs
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn commit(&self, request: &Request, flags: u32) -> io::Result<()> {
        let props: Vec<(u32, String, u64)> = request
            .entries
            .iter()
            .map(|&(o, p, v)| (o, self.prop_name(p), v))
            .collect();
        let mut s = self.state.borrow_mut();
        let blobs = props
            .iter()
            .filter(|(_, n, _)| n == "MODE_ID" || n == "FB_DAMAGE_CLIPS")
            .filter_map(|&(_, _, v)| {
                let data = s.blobs.get(&u32::try_from(v).ok()?)?;
                Some((v, data.clone()))
            })
            .collect();
        let commit = Commit {
            flags,
            props,
            blobs,
        };
        s.commits.push(commit.clone());
        if let Some(e) = s.commit_errors.pop_front() {
            return Err(err(e));
        }
        if s.refused.iter().any(|&c| commit.mentions(c)) {
            return Err(err(Errno::INVAL));
        }
        if !s.master {
            return Err(err(Errno::ACCESS));
        }
        // The CRTCs in the commit's state: each set directly, each a plane
        // is put on, and each a plane in the commit was on. The kernel
        // sends a flip event for every one.
        let mut crtcs: Vec<u32> = commit
            .props
            .iter()
            .flat_map(|(o, n, v)| {
                let set = if s.crtcs.contains(o) {
                    Some(*o)
                } else if n == "CRTC_ID" && *v != 0 && s.planes.contains_key(o) {
                    u32::try_from(*v).ok()
                } else {
                    None
                };
                set.into_iter().chain(s.plane_crtc.get(o).copied())
            })
            .collect();
        crtcs.sort_unstable();
        crtcs.dedup();
        let event = flags & DRM_MODE_PAGE_FLIP_EVENT != 0;
        let active = |c: u32| s.active.get(&c).copied().unwrap_or(false);
        let stays_off = crtcs.iter().any(|&c| {
            let now = commit.get(c, "ACTIVE").map_or(active(c), |v| v != 0);
            !active(c) && !now
        });
        if event && (crtcs.is_empty() || stays_off) {
            return Err(err(Errno::INVAL));
        }
        if crtcs.iter().any(|c| s.pending.contains(c)) {
            return Err(err(Errno::BUSY));
        }
        if commit.test_only() {
            return Ok(());
        }
        for (o, n, v) in &commit.props {
            let v32 = u32::try_from(*v).unwrap_or(0);
            match n.as_str() {
                "CRTC_ID" if s.connectors.contains_key(o) => {
                    s.bound.insert(*o, v32);
                }
                "CRTC_ID" if s.planes.contains_key(o) => {
                    s.plane_crtc.insert(*o, v32);
                }
                "FB_ID" if s.planes.contains_key(o) => {
                    s.plane_fb.insert(*o, v32);
                }
                "ACTIVE" => {
                    s.active.insert(*o, *v != 0);
                }
                _ => {}
            }
        }
        s.plane_crtc.retain(|_, c| *c != 0);
        s.plane_fb.retain(|_, fb| *fb != 0);
        let tied: Vec<(u32, u64)> = s
            .bound
            .iter()
            .filter_map(|(&conn, &crtc)| {
                let on = commit.get(crtc, "ACTIVE")? != 0;
                Some((conn, if on { 0 } else { 3 }))
            })
            .collect();
        s.dpms.extend(tied);
        if event {
            s.pending.extend(crtcs);
        }
        Ok(())
    }

    fn set_crtc(&self, crtc: u32, fb: u32, connector: u32, mode: &Mode) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if !s.master {
            return Err(err(Errno::ACCESS));
        }
        s.set_crtcs.push((crtc, fb, connector, *mode));
        s.bound.insert(connector, crtc);
        s.active.insert(crtc, true);
        s.dpms.insert(connector, 0);
        Ok(())
    }

    fn page_flip(&self, crtc: u32, fb: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if let Some(e) = s.commit_errors.pop_front() {
            return Err(err(e));
        }
        if s.pending.contains(&crtc) {
            return Err(err(Errno::BUSY));
        }
        s.page_flips.push((crtc, fb));
        s.pending.push(crtc);
        Ok(())
    }

    fn hide_cursor(&self, _: u32) -> io::Result<()> {
        Ok(())
    }

    fn set_connector_property(&self, connector: u32, property: u32, value: u64) -> io::Result<()> {
        if self.prop_name(property) != "DPMS" {
            return Err(err(Errno::INVAL));
        }
        let mut s = self.state.borrow_mut();
        s.dpms.insert(connector, value);
        if let Some(&crtc) = s.bound.get(&connector) {
            s.active.insert(crtc, value == 0);
        }
        Ok(())
    }

    fn read_events(&self, flipped: &mut Vec<u32>) -> io::Result<()> {
        let mut drained = [0u8; 64];
        let mut watched = UnixStream::from(self.events.0.try_clone()?);
        while watched.read(&mut drained).is_ok_and(|n| n > 0) {}
        flipped.append(&mut self.state.borrow_mut().completed);
        Ok(())
    }
}

/// Log records, captured per thread, so a test reads what it logged.
pub(crate) mod logs {
    use std::cell::RefCell;
    use std::sync::Once;

    use log::{Level, Log, Metadata, Record};

    thread_local! {
        static RECORDS: RefCell<Vec<(Level, String)>> = const { RefCell::new(Vec::new()) };
    }

    struct Capture;

    impl Log for Capture {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &Record<'_>) {
            let line = record.args().to_string();
            RECORDS.with(|r| r.borrow_mut().push((record.level(), line)));
        }

        fn flush(&self) {}
    }

    static CAPTURE: Capture = Capture;
    static INSTALL: Once = Once::new();

    /// Starts capturing this thread's records, dropping earlier ones.
    pub(crate) fn capture() {
        INSTALL.call_once(|| {
            let _ = log::set_logger(&CAPTURE);
            log::set_max_level(log::LevelFilter::Trace);
        });
        RECORDS.with(|r| r.borrow_mut().clear());
    }

    /// The records at `level` or above since [`capture`].
    pub(crate) fn at_least(level: Level) -> Vec<String> {
        RECORDS.with(|r| {
            r.borrow()
                .iter()
                .filter(|(l, _)| *l <= level)
                .map(|(_, line)| line.clone())
                .collect()
        })
    }
}
