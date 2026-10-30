//! A stand-in DRM card for the tests: connectors, CRTCs and planes in
//! memory, with every commit recorded by property name, flips completed
//! when a test says, and failures planted where a test wants them. It
//! behaves as the kernel does where drm2d depends on it: a commit or a
//! modeset without DRM master is refused with EACCES, a commit while a
//! flip is pending with EBUSY, and a commit asking for a flip event that
//! names no CRTC (`prepare_signaling`) or a CRTC that is off and stays off
//! (`drm_atomic_crtc_check`) with EINVAL. A commit that leaves a CRTC in
//! its state with a mode and no connector, or a connector and no mode
//! (`drm_atomic_helper_check_modeset`), active with no mode, or with a
//! plane on it and no mode (`drm_atomic_helper_check_plane_state`), is
//! refused with EINVAL; a connector the commit moves puts the CRTC it
//! leaves in the state, as `drm_atomic_set_crtc_for_connector` does. A framebuffer removed while a
//! plane shows it takes the plane off its CRTC (`atomic_remove_fb`); on
//! a primary plane it turns the CRTC off too and takes every connector
//! off it, as on a driver that refuses a CRTC with no primary plane,
//! amdgpu's, which retries the removal with the CRTC off. A dumb buffer
//! keeps what its mappings wrote, for a test to read back.
//! A connector's DPMS and its CRTC's `ACTIVE` are one state, as on an
//! atomic driver: setting either sets the other.
//!
//! The kernel's console stands in too: at a hotplug while no process
//! holds master ([`FakeCard::hotplug`]) it lights each connected
//! connector on no CRTC with its own framebuffer, as the kernel's fbdev
//! emulation does (`drm_fb_helper_hotplug_event`). A CRTC whose mode it
//! set shows nothing until a commit sets the CRTC's mode, its `ACTIVE` or
//! its connectors anew, a full modeset (`drm_atomic_crtc_needs_modeset`),
//! as the small panel on vtium's test seat stayed dark under the frames
//! committed on the console's mode.

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
    /// Planes whose possible CRTCs cannot be read, and the reads that
    /// fail for every object: `planes`, `crtc_mode` and `cap`. Each fails
    /// with EIO.
    pub(crate) crtcs_unreadable: Vec<u32>,
    pub(crate) failing: Vec<&'static str>,
    /// The CRTC each connector is on, each CRTC's `ACTIVE`, and the CRTC
    /// each plane is on, as the commits so far left them.
    pub(crate) bound: BTreeMap<u32, u32>,
    pub(crate) active: BTreeMap<u32, bool>,
    /// Each CRTC's mode is set, as the commits so far left it; a CRTC no
    /// commit has named has one where [`State::crtc_modes`] shows one.
    pub(crate) enabled: BTreeMap<u32, bool>,
    pub(crate) plane_crtc: BTreeMap<u32, u32>,
    /// The framebuffer each plane shows.
    pub(crate) plane_fb: BTreeMap<u32, u32>,
    /// The card is this process's DRM master. The open makes it so, as the
    /// kernel makes the first opener of a card no process holds master of.
    pub(crate) master: bool,
    /// Another process holds master: taking it fails.
    pub(crate) master_elsewhere: bool,
    /// The times master was let go while held.
    pub(crate) master_drops: usize,
    pub(crate) commits: Vec<Commit>,
    /// Errors the next commits return, one each.
    pub(crate) commit_errors: VecDeque<Errno>,
    /// Errors the next dumb buffers' creations return, one each.
    pub(crate) dumb_errors: VecDeque<Errno>,
    /// The cursor plane's side the card reports; 0 reports 256.
    pub(crate) cursor_side: u64,
    /// The card's directory in sysfs, a stand-in one; none by default.
    pub(crate) sysfs: Option<std::path::PathBuf>,
    /// CRTCs no commit may drive, as past a bandwidth limit: a commit
    /// that names one is refused with EINVAL.
    pub(crate) refused: Vec<u32>,
    pub(crate) page_flips: Vec<(u32, u32)>,
    pub(crate) set_crtcs: Vec<(u32, u32, u32, Mode)>,
    /// The CRTCs the legacy calls turned off, in order.
    pub(crate) disabled_crtcs: Vec<u32>,
    /// CRTCs with a flip pending, and those whose flip completed.
    pub(crate) pending: Vec<u32>,
    /// Flips complete without their events, as a driver that loses them.
    pub(crate) lose_flips: bool,
    pub(crate) completed: Vec<u32>,
    pub(crate) blobs: BTreeMap<u32, Vec<u8>>,
    pub(crate) dumbs: BTreeMap<u32, (u32, u32)>,
    pub(crate) fbs: BTreeMap<u32, (u32, u32)>,
    /// Each CRTC's mode, as the commits and the console set it.
    pub(crate) modes: BTreeMap<u32, Vec<u8>>,
    /// The CRTCs whose mode the console set and no full modeset has set
    /// since: they show nothing.
    pub(crate) console_lit: Vec<u32>,
    /// The framebuffers the console put on a plane.
    pub(crate) console_fbs: Vec<u32>,
}

impl State {
    /// Whether `crtc` passes the kernel's checks as `commit` leaves it:
    /// a mode exactly where a connector is on it, and active or showing a
    /// plane only with a mode.
    fn consistent(&self, commit: &Commit, crtc: u32) -> bool {
        let after = |object: u32, name: &str, before: Option<u32>| {
            commit
                .get(object, name)
                .map_or(before, |v| u32::try_from(v).ok())
                .unwrap_or(0)
        };
        let enabled = commit.get(crtc, "MODE_ID").map_or_else(
            || {
                self.enabled
                    .get(&crtc)
                    .copied()
                    .unwrap_or_else(|| self.crtc_modes.contains_key(&crtc))
            },
            |v| v != 0,
        );
        let active = commit.get(crtc, "ACTIVE").map_or_else(
            || self.active.get(&crtc).copied().unwrap_or(false),
            |v| v != 0,
        );
        let connected = self
            .connectors
            .keys()
            .any(|&c| after(c, "CRTC_ID", self.bound.get(&c).copied()) == crtc);
        let planes = self
            .planes
            .keys()
            .any(|&p| after(p, "CRTC_ID", self.plane_crtc.get(&p).copied()) == crtc);
        enabled == connected && (enabled || !active && !planes)
    }
}

/// The bytes of each dumb buffer, by handle: what its mappings wrote.
type Memory = Rc<RefCell<BTreeMap<u32, Vec<u8>>>>;

/// A dumb buffer's mapping: its bytes, written back to the card's memory
/// when the mapping goes, as device memory keeps what was written to it.
#[derive(Debug)]
pub(crate) struct Mapping {
    handle: u32,
    bytes: Vec<u8>,
    memory: Memory,
}

impl std::ops::Deref for Mapping {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsMut<[u8]> for Mapping {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.bytes);
        if let Some(m) = self.memory.borrow_mut().get_mut(&self.handle) {
            *m = bytes;
        }
    }
}

/// The stand-in card.
#[derive(Debug)]
pub(crate) struct FakeCard {
    state: RefCell<State>,
    memory: Memory,
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
            state: RefCell::new(State {
                master: true,
                ..State::default()
            }),
            memory: Memory::default(),
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
                        mm: (0, 0),
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

    /// The kernel's hotplug, before the process sees it: while no process
    /// holds master, the console lights each connected connector on no
    /// CRTC with its own framebuffer on the first free CRTC's primary
    /// plane, at `mode`, the bytes of a mode as drm2d sends them.
    pub(crate) fn hotplug(&self, mode: &[u8]) {
        let mut s = self.state.borrow_mut();
        if s.master || s.master_elsewhere {
            return;
        }
        let connected: Vec<(u32, Vec<u32>)> = s
            .connectors
            .values()
            .filter(|c| c.connected && !c.modes.is_empty())
            .filter(|c| s.bound.get(&c.id).is_none_or(|&crtc| crtc == 0))
            .map(|c| (c.id, c.encoders.clone()))
            .collect();
        for (conn, encoders) in connected {
            let Some((encoder, crtc, bit)) = encoders.iter().find_map(|&e| {
                let possible = s.encoders.get(&e)?.possible_crtcs;
                s.crtcs.iter().enumerate().find_map(|(i, &crtc)| {
                    let bit = 1u32 << i;
                    let free = !s.enabled.get(&crtc).copied().unwrap_or(false);
                    (possible & bit != 0 && free).then_some((e, crtc, bit))
                })
            }) else {
                continue;
            };
            let Some(primary) = s
                .planes
                .iter()
                .find(|(_, p)| p.kind == PRIMARY && p.crtcs & bit != 0)
                .map(|(&id, _)| id)
            else {
                continue;
            };
            let fb = self.id();
            s.fbs.insert(fb, (0, 0));
            s.console_fbs.push(fb);
            if let Some(c) = s.connectors.get_mut(&conn) {
                c.encoder = encoder;
            }
            if let Some(e) = s.encoders.get_mut(&encoder) {
                e.crtc = crtc;
            }
            s.bound.insert(conn, crtc);
            s.active.insert(crtc, true);
            s.enabled.insert(crtc, true);
            s.modes.insert(crtc, mode.to_vec());
            if let Some(&m) = s.connectors.get(&conn).and_then(|c| c.modes.first()) {
                s.crtc_modes.insert(crtc, m);
            }
            s.plane_crtc.insert(primary, crtc);
            s.plane_fb.insert(primary, fb);
            s.dpms.insert(conn, 0);
            s.console_lit.push(crtc);
        }
    }

    /// Whether `crtc` shows the framebuffer `fb` on its primary plane:
    /// active, on a mode no console left, with the plane on it.
    pub(crate) fn shows(&self, crtc: u32, fb: u32) -> bool {
        let s = self.state.borrow();
        let on_it = s.plane_fb.iter().any(|(plane, &shown)| {
            shown == fb
                && s.plane_crtc.get(plane) == Some(&crtc)
                && s.planes.get(plane).is_some_and(|p| p.kind == PRIMARY)
        });
        on_it && s.active.get(&crtc).copied().unwrap_or(false) && !s.console_lit.contains(&crtc)
    }

    /// The bytes of the dumb buffer `handle` as its last mapping left
    /// them, if it is still there.
    pub(crate) fn memory(&self, handle: u32) -> Option<Vec<u8>> {
        self.memory.borrow().get(&handle).cloned()
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
    type Map = Mapping;

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
        if self.state.borrow().failing.contains(&"cap") {
            return Err(err(Errno::IO));
        }
        match cap {
            DRM_CAP_DUMB_BUFFER => Ok(1),
            DRM_CAP_CURSOR_WIDTH | DRM_CAP_CURSOR_HEIGHT => {
                Ok(match self.state.borrow().cursor_side {
                    0 => 256,
                    side => side,
                })
            }
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
        let mut s = self.state.borrow_mut();
        if !s.master {
            return Err(err(Errno::INVAL));
        }
        s.master = false;
        s.master_drops += 1;
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
        if self.state.borrow().failing.contains(&"crtc_mode") {
            return Err(err(Errno::IO));
        }
        Ok(self.state.borrow().crtc_modes.get(&id).copied())
    }

    fn planes(&self) -> io::Result<Vec<u32>> {
        let s = self.state.borrow();
        if s.failing.contains(&"planes") {
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
        if let Some(e) = self.state.borrow_mut().dumb_errors.pop_front() {
            return Err(err(e));
        }
        let handle = self.id();
        self.state
            .borrow_mut()
            .dumbs
            .insert(handle, (width, height));
        let size = width * 4 * height;
        // A new buffer holds whatever the device memory held.
        self.memory
            .borrow_mut()
            .insert(handle, vec![0xa5; usize::try_from(size).unwrap_or(0)]);
        Ok(Dumb {
            handle,
            pitch: width * 4,
            size: u64::from(size),
        })
    }

    fn destroy_dumb(&self, handle: u32) -> io::Result<()> {
        self.memory.borrow_mut().remove(&handle);
        let mut s = self.state.borrow_mut();
        s.dumbs
            .remove(&handle)
            .map(|_| ())
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Mapping> {
        let bytes = self.memory(dumb.handle).ok_or_else(|| err(Errno::NOENT))?;
        Ok(Mapping {
            handle: dumb.handle,
            bytes,
            memory: Rc::clone(&self.memory),
        })
    }

    fn add_fb(&self, _: &Dumb, width: u32, height: u32, _: u32) -> io::Result<u32> {
        let fb = self.id();
        self.state.borrow_mut().fbs.insert(fb, (width, height));
        Ok(fb)
    }

    /// Removing a framebuffer a plane shows takes the plane off its CRTC,
    /// as the kernel's `drm_mode_rmfb` does (`atomic_remove_fb`). On a
    /// primary plane the CRTC goes off with it, every connector on it
    /// taken off, as amdgpu's refusal of a CRTC without its primary plane
    /// makes the kernel retry (`disable_crtcs`).
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
            let crtc = s.plane_crtc.remove(&plane);
            let primary = s.planes.get(&plane).is_some_and(|p| p.kind == PRIMARY);
            if let Some(crtc) = crtc.filter(|_| primary) {
                s.active.insert(crtc, false);
                s.enabled.insert(crtc, false);
                s.modes.remove(&crtc);
                s.crtc_modes.remove(&crtc);
                s.console_lit.retain(|&c| c != crtc);
                let on: Vec<u32> = s
                    .bound
                    .iter()
                    .filter(|&(_, &c)| c == crtc)
                    .map(|(&conn, _)| conn)
                    .collect();
                for conn in on {
                    s.bound.insert(conn, 0);
                    if let Some(c) = s.connectors.get_mut(&conn) {
                        c.encoder = 0;
                    }
                }
                for e in s.encoders.values_mut().filter(|e| e.crtc == crtc) {
                    e.crtc = 0;
                }
            }
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
        // A connector's CRTCs count the same way.
        let mut crtcs: Vec<u32> = commit
            .props
            .iter()
            .flat_map(|(o, n, v)| {
                let on = s.planes.contains_key(o) || s.connectors.contains_key(o);
                let set = if s.crtcs.contains(o) {
                    Some(*o)
                } else if n == "CRTC_ID" && *v != 0 && on {
                    u32::try_from(*v).ok()
                } else {
                    None
                };
                let was = s.plane_crtc.get(o).or_else(|| s.bound.get(o));
                set.into_iter().chain(was.copied().filter(|&c| c != 0))
            })
            .collect();
        crtcs.sort_unstable();
        crtcs.dedup();
        if crtcs.iter().any(|&c| !s.consistent(&commit, c)) {
            return Err(err(Errno::INVAL));
        }
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
        // What each CRTC is before the commit: a full modeset changes it.
        let before = |s: &State, crtc: u32| {
            let mut on: Vec<u32> = s
                .bound
                .iter()
                .filter(|&(_, &c)| c == crtc)
                .map(|(&conn, _)| conn)
                .collect();
            on.sort_unstable();
            (
                s.enabled.get(&crtc).copied().unwrap_or(false),
                s.active.get(&crtc).copied().unwrap_or(false),
                s.modes.get(&crtc).cloned(),
                on,
            )
        };
        let was: Vec<_> = crtcs.iter().map(|&c| (c, before(&s, c))).collect();
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
                "MODE_ID" => {
                    s.enabled.insert(*o, *v != 0);
                    match commit.blobs.get(v) {
                        Some(bytes) if *v != 0 => s.modes.insert(*o, bytes.clone()),
                        _ => s.modes.remove(o),
                    };
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
        for (crtc, then) in was {
            if before(&s, crtc) != then {
                s.console_lit.retain(|&c| c != crtc);
            }
        }
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

    fn disable_crtc(&self, crtc: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if !s.master {
            return Err(err(Errno::ACCESS));
        }
        s.disabled_crtcs.push(crtc);
        s.bound.retain(|_, c| *c != crtc);
        s.active.insert(crtc, false);
        s.enabled.insert(crtc, false);
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

    fn sysfs(&self) -> Option<std::path::PathBuf> {
        self.state.borrow().sysfs.clone()
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
