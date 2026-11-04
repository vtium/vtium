//! A stand-in DRM card for the tests: connectors, CRTCs and planes in
//! memory, with every commit recorded by property name, flips completed
//! when a test says, and failures planted where a test wants them. It
//! behaves as the kernel does where drm2d depends on it. A commit or a
//! modeset without DRM master is refused with EACCES, and so is a commit
//! that puts a plane or a connector on a CRTC the card does not have
//! (`drm_atomic_plane_set_property`, `drm_atomic_connector_set_property`).
//! A commit while a flip is pending is refused with EBUSY. A commit asking
//! for a flip event that names no CRTC (`prepare_signaling`) or a CRTC that
//! is off and stays off (`drm_atomic_crtc_check`) is refused with EINVAL,
//! and so is a commit that puts a plane on a CRTC the plane cannot drive
//! (`drm_atomic_plane_check`). A commit that leaves a CRTC in its state
//! with a mode and no connector, or a connector and no mode
//! (`drm_atomic_helper_check_modeset`), active with no mode, or with a
//! plane on it and no mode (`drm_atomic_helper_check_plane_state`), is
//! refused with EINVAL; a connector the commit moves puts the CRTC it
//! leaves in the state, as `drm_atomic_set_crtc_for_connector` does. What a
//! commit sets is what the card reports after it: a connector's encoder and
//! that encoder's CRTC, a connector's `CRTC_ID`, and a CRTC's mode, as the
//! kernel's getters read them back. A wait for a CRTC's next vblank is
//! refused with EINVAL where the CRTC is not active, and with EOPNOTSUPP on
//! a card with no vblank interrupt. Its event comes when a test says the
//! vblank passed, or at once when the CRTC stops being active, as
//! `drm_crtc_vblank_off` sends the events still waiting.
//!
//! A framebuffer smaller than what it is asked to show is refused with
//! ENOSPC, as the kernel refuses it: a commit whose plane reads past its
//! framebuffer's edge (`drm_atomic_plane_check`), and a legacy CRTC set or
//! page flip whose framebuffer is smaller than the CRTC's mode
//! (`drm_crtc_check_viewport`).
//!
//! A framebuffer removed while a plane shows it takes the plane off its
//! CRTC (`atomic_remove_fb`). What happens to the CRTC depends on the
//! driver, which [`State::remove_fb`] picks. On amdgpu, the default, the
//! driver refuses a CRTC with no primary plane, so the kernel's
//! `atomic_remove_fb` tries again with the CRTC off and every connector
//! taken off it; the captures of the test seat's amdgpu cards showed
//! exactly that. Drivers that allow a CRTC with no primary plane, i915
//! among them, leave the CRTC on. A dumb buffer keeps what its mappings
//! wrote, for a test to read back. A connector's DPMS and its CRTC's
//! `ACTIVE` are one state, as on an atomic driver: setting either sets
//! the other.
//!
//! The kernel's console stands in too. At a hotplug while no process
//! holds master ([`FakeCard::hotplug`]), it lights each connected
//! connector that is on no CRTC with its own framebuffer, as the kernel's
//! fbdev emulation does (`drm_fb_helper_hotplug_event`). A CRTC whose mode
//! the console set then shows nothing until a commit sets the CRTC's mode,
//! its `ACTIVE` or its connectors anew, which is a full modeset
//! (`drm_atomic_crtc_needs_modeset`). That is not a rule the kernel
//! states. It is what the operator saw on this machine's amdgpu cards on
//! 2026-10-30: the small panel stayed dark under vtium's frames committed
//! on the console's mode, until a VT switch set its mode again.

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
use std::sync::{Arc, Mutex};

use drm_ffi::{
    DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH, DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC,
    DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, DRM_CLIENT_CAP_UNIVERSAL_PLANES,
    DRM_MODE_ATOMIC_TEST_ONLY, DRM_MODE_PAGE_FLIP_EVENT, drm_mode_modeinfo,
};
use rustix::io::Errno;

use super::kms::{
    Connector, ConnectorRead, CrtcEvent, Dumb, Encoder, Kms, Object, Property, Request, Resources,
};
use super::{Mode, lock};

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
    /// The `PATH` property of each connector that has one, a blob's id,
    /// as a driver gives a DisplayPort multi-stream connector its own; one
    /// absent here has no `PATH` property, as every other connector.
    pub(crate) paths: BTreeMap<u32, u32>,
    /// Blobs replaced once read: the next read of the id finds these
    /// bytes, as when the blob is freed between two reads and its id given
    /// to another.
    pub(crate) replaced: BTreeMap<u32, Vec<u8>>,
    /// Objects whose properties cannot be read: the read fails with
    /// EFAULT.
    pub(crate) unreadable: Vec<u32>,
    /// Objects given this many more properties than their own, named
    /// `EXTRA0` on.
    pub(crate) extra_props: BTreeMap<u32, usize>,
    /// The connectors probed, one entry a probe, as a read of a connector
    /// with no room for modes probes it while the card is master.
    pub(crate) probes: Vec<u32>,
    /// Planes whose possible CRTCs cannot be read, and the reads that
    /// fail for every object: `planes`, `crtc_mode`, `cap` and `encoder`.
    /// Each fails with EIO.
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
    /// The part of its framebuffer each plane shows, as the commits so far
    /// left it: each plane's `SRC_X`, `SRC_Y`, `SRC_W` and `SRC_H`, in 16.16
    /// fixed point.
    pub(crate) plane_src: BTreeMap<(u32, &'static str), u64>,
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
    /// Errors the next legacy CRTC sets return, one each.
    pub(crate) set_crtc_errors: VecDeque<Errno>,
    /// A mode's size at which every legacy CRTC set fails with EBUSY, as a
    /// CRTC that stays busy for that mode.
    pub(crate) busy_size: Option<(u32, u32)>,
    /// Errors the next framebuffers' creations return, one each.
    pub(crate) fb_errors: VecDeque<Errno>,
    /// Errors the next dumb buffers' mappings return, one each.
    pub(crate) map_errors: VecDeque<Errno>,
    /// The calls on buffers and framebuffers, in order, failed ones
    /// included.
    pub(crate) buffer_calls: Vec<BufferCall>,
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
    /// The CRTCs whose next vblank was asked for, and the events read
    /// next: the flips completed and the vblanks passed.
    pub(crate) vblanks: Vec<u32>,
    /// The card has no vblank interrupt and no software vblank timer, as
    /// simpledrm, udl and gud have none: a wait for a vblank is refused
    /// with EOPNOTSUPP.
    pub(crate) no_vblank: bool,
    pub(crate) completed: Vec<CrtcEvent>,
    pub(crate) blobs: BTreeMap<u32, Vec<u8>>,
    pub(crate) dumbs: BTreeMap<u32, (u32, u32)>,
    pub(crate) fbs: BTreeMap<u32, (u32, u32)>,
    /// Each CRTC's mode, as the commits and the console set it.
    pub(crate) modes: BTreeMap<u32, Vec<u8>>,
    /// The CRTCs whose mode the console set and no full modeset has set
    /// since. They show nothing, as the operator saw on this machine's
    /// amdgpu cards; the kernel states no such rule.
    pub(crate) console_lit: Vec<u32>,
    /// The framebuffers the console put on a plane.
    pub(crate) console_fbs: Vec<u32>,
    /// What removing a framebuffer a primary plane shows does to its CRTC.
    pub(crate) remove_fb: RemoveFb,
}

/// What removing the framebuffer a primary plane shows does to the plane's
/// CRTC, which depends on the driver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RemoveFb {
    /// The CRTC goes off and its connectors leave it, as on amdgpu.
    #[default]
    CrtcOff,
    /// Only the plane goes off, and the CRTC stays on with its mode and
    /// connectors, as on i915.
    PlaneOff,
}

impl State {
    /// Ends each wait for the vblank of a CRTC that is no longer active,
    /// as `drm_crtc_vblank_off` does: its event is read next. Says whether
    /// any ended.
    fn vblank_off(&mut self) -> bool {
        let (off, on): (Vec<u32>, Vec<u32>) = std::mem::take(&mut self.vblanks)
            .into_iter()
            .partition(|c| !self.active.get(c).copied().unwrap_or(false));
        self.vblanks = on;
        let ended = !off.is_empty();
        self.completed
            .extend(off.into_iter().map(CrtcEvent::Vblank));
        ended
    }

    /// Puts connector `conn` on `crtc`, or on none when 0, as the card
    /// reports it afterwards: the connector's encoder, and that encoder's
    /// CRTC.
    pub(crate) fn bind(&mut self, conn: u32, crtc: u32) {
        self.bound.insert(conn, crtc);
        let Some(c) = self.connectors.get(&conn) else {
            return;
        };
        let (old, encoders) = (c.encoder, c.encoders.clone());
        let bit = self
            .crtcs
            .iter()
            .position(|&x| x == crtc)
            .map_or(0, |i| 1u32 << i);
        let can = |s: &State, e: u32| {
            s.encoders
                .get(&e)
                .is_some_and(|e| e.possible_crtcs & bit != 0)
        };
        let new = if crtc == 0 {
            0
        } else if old != 0 && can(self, old) {
            old
        } else {
            encoders.into_iter().find(|&e| can(self, e)).unwrap_or(0)
        };
        if let Some(c) = self.connectors.get_mut(&conn) {
            c.encoder = new;
        }
        let shared = |s: &State, e: u32| {
            s.connectors
                .values()
                .any(|c| c.id != conn && c.encoder == e)
        };
        if old != 0
            && old != new
            && !shared(self, old)
            && let Some(e) = self.encoders.get_mut(&old)
        {
            e.crtc = 0;
        }
        if let Some(e) = self.encoders.get_mut(&new) {
            e.crtc = crtc;
        }
    }

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

/// A call on a buffer or a framebuffer, with the id it names: the dumb
/// buffer's handle, or for `RmFb` the framebuffer's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferCall {
    CreateDumb(u32),
    AddFb(u32),
    MapDumb(u32),
    RmFb(u32),
    DestroyDumb(u32),
}

/// A read of a list, after which a test can change the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListRead {
    /// A read of a connector's modes and encoders.
    Connector(u32),
    /// A read of an object's properties.
    Properties(u32),
    /// A read of the card's planes.
    Planes,
}

/// A change a test makes to the card.
type Change = Box<dyn FnOnce(&mut State)>;

/// Changes to make to the card, each once, right after a read.
#[derive(Default)]
struct Between(RefCell<Vec<(ListRead, Change)>>);

impl std::fmt::Debug for Between {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reads: Vec<ListRead> = self.0.borrow().iter().map(|(r, _)| *r).collect();
        f.debug_tuple("Between").field(&reads).finish()
    }
}

/// The bytes of each dumb buffer, by handle: what its mappings wrote.
/// The card's memory, by dumb buffer. A mapping can go to a render thread
/// inside a token and be dropped there, so it is shared across threads.
type Memory = Arc<Mutex<BTreeMap<u32, Vec<u8>>>>;

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
        if let Some(m) = lock(&self.memory).get_mut(&self.handle) {
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
    between: Between,
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
            between: Between::default(),
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

    /// Gives connector `conn` a `PATH` property naming a blob of `bytes`,
    /// as a driver gives a DisplayPort multi-stream connector the path
    /// `mst:<parent connector>-<port>...`, NUL-terminated.
    pub(crate) fn set_path(&self, conn: u32, bytes: &[u8]) {
        let blob = self.id();
        let mut s = self.state.borrow_mut();
        s.blobs.insert(blob, bytes.to_vec());
        s.paths.insert(conn, blob);
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
            let size = mode_of(mode).map_or((0, 0), |m| m.size());
            s.fbs.insert(fb, size);
            s.console_fbs.push(fb);
            if let Some(c) = s.connectors.get_mut(&conn) {
                c.encoder = encoder;
            }
            s.bind(conn, crtc);
            s.active.insert(crtc, true);
            s.enabled.insert(crtc, true);
            s.modes.insert(crtc, mode.to_vec());
            if let Some(m) = mode_of(mode) {
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
        lock(&self.memory).get(&handle).cloned()
    }

    /// Makes `change` to the card right after the next `read`, as an
    /// object can change between the read that sizes a list and the read
    /// that fills it: a connector torn down and its id given to another,
    /// or a plane added.
    pub(crate) fn after_read(&self, read: ListRead, change: impl FnOnce(&mut State) + 'static) {
        self.between.0.borrow_mut().push((read, Box::new(change)));
    }

    /// Makes the first change waiting for `read`.
    fn changes_after(&self, read: ListRead) {
        let mut between = self.between.0.borrow_mut();
        let Some(at) = between.iter().position(|(r, _)| *r == read) else {
            return;
        };
        let (_, change) = between.remove(at);
        drop(between);
        change(&mut self.state.borrow_mut());
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
        s.completed
            .extend(pending.into_iter().map(CrtcEvent::Flipped));
    }

    /// The next vblank passes: each wait for it is reported, and nothing
    /// else, as each flip pending stays pending.
    pub(crate) fn vblank(&self) {
        let mut s = self.state.borrow_mut();
        let waits = std::mem::take(&mut s.vblanks);
        if !waits.is_empty() {
            self.wake();
        }
        s.completed.extend(waits.into_iter().map(CrtcEvent::Vblank));
    }

    /// Makes the descriptor the loop watches readable.
    fn wake(&self) {
        let _ = (&self.events.1).write(b"e");
    }

    /// Ends the waits of each CRTC that is no longer active, and wakes the
    /// loop for their events.
    fn vblank_off(&self, s: &mut State) {
        if s.vblank_off() {
            self.wake();
        }
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

    /// Every property of object `id`, its own and those
    /// [`State::extra_props`] gives it.
    fn object_props(&self, id: u32, kind: Object) -> io::Result<Vec<Property>> {
        let mut props = self.own_props(id, kind)?;
        let extra = self.state.borrow().extra_props.get(&id).copied();
        for n in 0..extra.unwrap_or(0) {
            props.extend(self.props(&[(format!("EXTRA{n}").as_str(), 0)]));
        }
        Ok(props)
    }

    /// The properties object `id` has of its own.
    fn own_props(&self, id: u32, kind: Object) -> io::Result<Vec<Property>> {
        let s = self.state.borrow();
        if s.unreadable.contains(&id) {
            return Err(err(Errno::FAULT));
        }
        match kind {
            Object::Connector => {
                let dpms = *s.dpms.get(&id).ok_or_else(|| err(Errno::NOENT))?;
                let with = !s.no_dpms.contains(&id);
                let path = s.paths.get(&id).copied();
                let crtc = u64::from(s.bound.get(&id).copied().unwrap_or(0));
                drop(s);
                let names: Vec<(&str, u64)> = std::iter::once(("CRTC_ID", crtc))
                    .chain(with.then_some(("DPMS", dpms)))
                    .chain(path.map(|b| ("PATH", u64::from(b))))
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

/// The mode in `bytes`, a mode blob as drm2d sends it: the kernel's
/// `drm_mode_modeinfo` in memory order.
fn mode_of(bytes: &[u8]) -> Option<Mode> {
    let word = |at: usize| Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at: usize| Some(u16::from_ne_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let mut name = [0; 32];
    for (to, from) in name.iter_mut().zip(bytes.get(36..68)?) {
        *to = std::ffi::c_char::from_ne_bytes([*from]);
    }
    Some(Mode(drm_mode_modeinfo {
        clock: word(0)?,
        hdisplay: half(4)?,
        hsync_start: half(6)?,
        hsync_end: half(8)?,
        htotal: half(10)?,
        hskew: half(12)?,
        vdisplay: half(14)?,
        vsync_start: half(16)?,
        vsync_end: half(18)?,
        vtotal: half(20)?,
        vscan: half(22)?,
        vrefresh: word(24)?,
        flags: word(28)?,
        type_: word(32)?,
        name,
    }))
}

fn err(e: Errno) -> io::Error {
    io::Error::from_raw_os_error(e.raw_os_error())
}

/// Whether the framebuffer `fb`, of a size the card knows, is narrower or
/// shorter than `mode`, as `drm_crtc_check_viewport` refuses for a legacy
/// CRTC set or page flip.
fn smaller_than(s: &State, fb: u32, mode: &Mode) -> bool {
    let (w, h) = mode.size();
    s.fbs.get(&fb).is_some_and(|&(fw, fh)| w > fw || h > fh)
}

/// Whether a plane `commit` leaves on a CRTC with a framebuffer reads past
/// that framebuffer's edge, as `drm_atomic_plane_check` refuses: its source
/// rectangle, the commit's values over those the plane holds, does not lie
/// inside the framebuffer.
fn commit_reads_past_a_framebuffer(s: &State, commit: &Commit) -> bool {
    let set = |p: u32| commit.props.iter().any(|(o, _, _)| *o == p);
    s.planes.keys().filter(|&&p| set(p)).any(|&p| {
        let fb = commit
            .get(p, "FB_ID")
            .map_or(s.plane_fb.get(&p).copied(), |v| u32::try_from(v).ok())
            .unwrap_or(0);
        let crtc = commit
            .get(p, "CRTC_ID")
            .map_or(s.plane_crtc.get(&p).copied(), |v| u32::try_from(v).ok())
            .unwrap_or(0);
        let Some(&(fw, fh)) = s.fbs.get(&fb).filter(|_| crtc != 0) else {
            return false;
        };
        let [x, y, w, h] = SRC.map(|name| {
            commit
                .get(p, name)
                .or_else(|| s.plane_src.get(&(p, name)).copied())
                .unwrap_or(0)
        });
        let (fw, fh) = (u64::from(fw) << 16, u64::from(fh) << 16);
        w > fw || x > fw - w || h > fh || y > fh - h
    })
}

/// Whether `commit` puts a plane or a connector on a CRTC the card does
/// not have, which the kernel refuses with EACCES as it sets the property
/// (`drm_atomic_plane_set_property`, `drm_atomic_connector_set_property`).
fn commit_names_a_crtc_the_card_lacks(s: &State, commit: &Commit) -> bool {
    commit.props.iter().any(|(o, n, v)| {
        let on = s.planes.contains_key(o) || s.connectors.contains_key(o);
        let crtc = u32::try_from(*v).unwrap_or(u32::MAX);
        on && n == "CRTC_ID" && *v != 0 && !s.crtcs.contains(&crtc)
    })
}

/// Whether `commit` puts a plane on a CRTC outside the plane's possible
/// CRTCs, as `drm_atomic_plane_check` refuses.
fn commit_puts_a_plane_on_a_crtc_it_cannot_drive(s: &State, commit: &Commit) -> bool {
    s.planes.iter().any(|(&p, plane)| {
        let crtc = commit.get(p, "CRTC_ID").unwrap_or(0);
        let bit = s
            .crtcs
            .iter()
            .position(|&c| u64::from(c) == crtc)
            .map(|i| 1u32 << i);
        bit.is_some_and(|bit| plane.crtcs & bit == 0)
    })
}

/// A plane's source rectangle: its corner, then its width and height.
const SRC: [&str; 4] = ["SRC_X", "SRC_Y", "SRC_W", "SRC_H"];

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

    fn read_connector(
        &self,
        id: u32,
        modes: &mut [Mode],
        encoders: &mut [u32],
    ) -> io::Result<ConnectorRead> {
        let mut s = self.state.borrow_mut();
        let c = s
            .connectors
            .get(&id)
            .cloned()
            .ok_or_else(|| err(Errno::NOENT))?;
        // The kernel probes only for DRM master, and demotes anyone
        // else's probe to a read (`drm_mode_getconnector`).
        if modes.is_empty() && s.master {
            s.probes.push(id);
        }
        drop(s);
        // The kernel copies each list only into a buffer that holds all
        // of it (`drm_mode_getconnector`).
        if let Some(to) = modes.get_mut(..c.modes.len()) {
            to.copy_from_slice(&c.modes);
        }
        if let Some(to) = encoders.get_mut(..c.encoders.len()) {
            to.copy_from_slice(&c.encoders);
        }
        self.changes_after(ListRead::Connector(id));
        Ok(ConnectorRead {
            kind: c.kind,
            kind_id: c.kind_id,
            connected: c.connected,
            mm: c.mm,
            encoder: c.encoder,
            modes: c.modes.len(),
            encoders: c.encoders.len(),
        })
    }

    fn encoder(&self, id: u32) -> io::Result<Encoder> {
        let s = self.state.borrow();
        if s.failing.contains(&"encoder") {
            return Err(err(Errno::IO));
        }
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

    fn read_planes(&self, ids: &mut [u32]) -> io::Result<usize> {
        let s = self.state.borrow();
        if s.failing.contains(&"planes") {
            return Err(err(Errno::IO));
        }
        let planes: Vec<u32> = s.planes.keys().copied().collect();
        drop(s);
        for (to, &plane) in ids.iter_mut().zip(&planes) {
            *to = plane;
        }
        self.changes_after(ListRead::Planes);
        Ok(planes.len())
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

    fn read_properties(
        &self,
        id: u32,
        kind: Object,
        ids: &mut [u32],
        values: &mut [u64],
    ) -> io::Result<usize> {
        let props = self.object_props(id, kind)?;
        for ((to, value), p) in ids.iter_mut().zip(values.iter_mut()).zip(&props) {
            *to = p.id;
            *value = p.value;
        }
        self.changes_after(ListRead::Properties(id));
        Ok(props.len())
    }

    fn property_name(&self, id: u32) -> io::Result<String> {
        let at = id.checked_sub(10_000).ok_or_else(|| err(Errno::NOENT))?;
        let at = usize::try_from(at).map_err(|_| err(Errno::NOENT))?;
        self.names
            .borrow()
            .get(at)
            .cloned()
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn create_dumb(&self, width: u32, height: u32) -> io::Result<Dumb> {
        if let Some(e) = self.state.borrow_mut().dumb_errors.pop_front() {
            return Err(err(e));
        }
        let handle = self.id();
        let mut s = self.state.borrow_mut();
        s.buffer_calls.push(BufferCall::CreateDumb(handle));
        s.dumbs.insert(handle, (width, height));
        drop(s);
        let size = width * 4 * height;
        // A new buffer holds whatever the device memory held.
        lock(&self.memory).insert(handle, vec![0xa5; usize::try_from(size).unwrap_or(0)]);
        Ok(Dumb {
            handle,
            pitch: width * 4,
            size: u64::from(size),
        })
    }

    fn destroy_dumb(&self, handle: u32) -> io::Result<()> {
        lock(&self.memory).remove(&handle);
        let mut s = self.state.borrow_mut();
        s.buffer_calls.push(BufferCall::DestroyDumb(handle));
        s.dumbs
            .remove(&handle)
            .map(|_| ())
            .ok_or_else(|| err(Errno::NOENT))
    }

    fn map_dumb(&self, dumb: &Dumb) -> io::Result<Mapping> {
        let mut s = self.state.borrow_mut();
        s.buffer_calls.push(BufferCall::MapDumb(dumb.handle));
        if let Some(e) = s.map_errors.pop_front() {
            return Err(err(e));
        }
        drop(s);
        let bytes = self.memory(dumb.handle).ok_or_else(|| err(Errno::NOENT))?;
        Ok(Mapping {
            handle: dumb.handle,
            bytes,
            memory: Arc::clone(&self.memory),
        })
    }

    /// Refuses, in the kernel's order, a pitch shorter than a line of
    /// `width` pixels with `EINVAL` (`framebuffer_check`), a handle that
    /// names no buffer with `ENOENT`, and a buffer too small for `height`
    /// lines of `pitch` bytes with `EINVAL` (`drm_gem_fb_init_with_funcs`).
    fn add_fb(&self, handle: u32, pitch: u32, width: u32, height: u32, _: u32) -> io::Result<u32> {
        let mut s = self.state.borrow_mut();
        s.buffer_calls.push(BufferCall::AddFb(handle));
        if let Some(e) = s.fb_errors.pop_front() {
            return Err(err(e));
        }
        let line = u64::from(width) * 4;
        if u64::from(pitch) < line {
            return Err(err(Errno::INVAL));
        }
        let (w, h) = *s.dumbs.get(&handle).ok_or_else(|| err(Errno::NOENT))?;
        let needed = u64::from(height.saturating_sub(1)) * u64::from(pitch) + line;
        if needed > u64::from(w) * 4 * u64::from(h) {
            return Err(err(Errno::INVAL));
        }
        drop(s);
        let fb = self.id();
        self.state.borrow_mut().fbs.insert(fb, (width, height));
        Ok(fb)
    }

    /// Removing a framebuffer a plane shows takes the plane off its CRTC,
    /// as the kernel's `drm_mode_rmfb` does (`atomic_remove_fb`). On a
    /// primary plane under [`RemoveFb::CrtcOff`] the CRTC goes off too,
    /// with every connector taken off it, as the kernel's second try does
    /// once amdgpu refuses a CRTC without its primary plane
    /// (`disable_crtcs`).
    fn rm_fb(&self, id: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        s.buffer_calls.push(BufferCall::RmFb(id));
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
            let off = primary && s.remove_fb == RemoveFb::CrtcOff;
            if let Some(crtc) = crtc.filter(|_| off) {
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
                    s.bind(conn, 0);
                }
            }
        }
        self.vblank_off(&mut s);
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

    fn read_blob(&self, id: u32, data: &mut [u8]) -> io::Result<usize> {
        let mut s = self.state.borrow_mut();
        let blob = s.blobs.get(&id).ok_or_else(|| err(Errno::NOENT))?;
        let len = blob.len();
        if let Some(to) = data.get_mut(..).filter(|d| d.len() == len) {
            to.copy_from_slice(blob);
        }
        if let Some(next) = s.replaced.remove(&id) {
            s.blobs.insert(id, next);
        }
        Ok(len)
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
        if !s.master || commit_names_a_crtc_the_card_lacks(&s, &commit) {
            return Err(err(Errno::ACCESS));
        }
        if commit_reads_past_a_framebuffer(&s, &commit) {
            return Err(err(Errno::NOSPC));
        }
        if commit_puts_a_plane_on_a_crtc_it_cannot_drive(&s, &commit) {
            return Err(err(Errno::INVAL));
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
                    s.bind(*o, v32);
                }
                "CRTC_ID" if s.planes.contains_key(o) => {
                    s.plane_crtc.insert(*o, v32);
                }
                "FB_ID" if s.planes.contains_key(o) => {
                    s.plane_fb.insert(*o, v32);
                }
                _ if s.planes.contains_key(o) && SRC.contains(&n.as_str()) => {
                    if let Some(&name) = SRC.iter().find(|&&name| name == n) {
                        s.plane_src.insert((*o, name), *v);
                    }
                }
                "ACTIVE" => {
                    s.active.insert(*o, *v != 0);
                }
                "MODE_ID" => {
                    s.enabled.insert(*o, *v != 0);
                    match commit.blobs.get(v).filter(|_| *v != 0) {
                        Some(bytes) => {
                            s.modes.insert(*o, bytes.clone());
                            if let Some(m) = mode_of(bytes) {
                                s.crtc_modes.insert(*o, m);
                            }
                        }
                        None => {
                            s.modes.remove(o);
                            s.crtc_modes.remove(o);
                        }
                    }
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
        self.vblank_off(&mut s);
        Ok(())
    }

    fn set_crtc(&self, crtc: u32, fb: u32, connector: u32, mode: &Mode) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if !s.master {
            return Err(err(Errno::ACCESS));
        }
        if let Some(e) = s.set_crtc_errors.pop_front() {
            return Err(err(e));
        }
        if s.busy_size == Some(mode.size()) {
            return Err(err(Errno::BUSY));
        }
        if smaller_than(&s, fb, mode) {
            return Err(err(Errno::NOSPC));
        }
        s.set_crtcs.push((crtc, fb, connector, *mode));
        s.bind(connector, crtc);
        s.crtc_modes.insert(crtc, *mode);
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
        let on: Vec<u32> = s
            .bound
            .iter()
            .filter(|&(_, &c)| c == crtc)
            .map(|(&conn, _)| conn)
            .collect();
        for conn in on {
            s.bind(conn, 0);
        }
        s.bound.retain(|_, c| *c != 0);
        s.crtc_modes.remove(&crtc);
        s.active.insert(crtc, false);
        s.enabled.insert(crtc, false);
        self.vblank_off(&mut s);
        Ok(())
    }

    fn page_flip(&self, crtc: u32, fb: u32) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if let Some(e) = s.commit_errors.pop_front() {
            return Err(err(e));
        }
        if s.crtc_modes
            .get(&crtc)
            .is_some_and(|m| smaller_than(&s, fb, m))
        {
            return Err(err(Errno::NOSPC));
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
        self.vblank_off(&mut s);
        Ok(())
    }

    /// Refused with EOPNOTSUPP on a card with no vblank interrupt
    /// (`drm_wait_vblank_ioctl`), and with EINVAL for a CRTC that is not
    /// active, whose vblank interrupt is off (`drm_queue_vblank_event`).
    fn wait_vblank(&self, index: usize) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        if s.no_vblank {
            return Err(err(Errno::OPNOTSUPP));
        }
        let crtc = s
            .crtcs
            .get(index)
            .copied()
            .ok_or_else(|| err(Errno::INVAL))?;
        if !s.active.get(&crtc).copied().unwrap_or(false) {
            return Err(err(Errno::INVAL));
        }
        s.vblanks.push(crtc);
        Ok(())
    }

    fn read_events(&self, events: &mut Vec<CrtcEvent>) -> io::Result<()> {
        let mut drained = [0u8; 64];
        let mut watched = UnixStream::from(self.events.0.try_clone()?);
        while watched.read(&mut drained).is_ok_and(|n| n > 0) {}
        events.append(&mut self.state.borrow_mut().completed);
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

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "a test fails by panicking")]

    use rustix::io::Errno;

    use super::super::kms::{Kms, Request};
    use super::super::mode::mode;
    use super::FakeCard;

    const CONN: u32 = 41;
    const CRTC: u32 = 100;
    const PRIMARY_PLANE: u32 = 300;

    /// A framebuffer of `width` by `height` over a dumb buffer its size.
    fn fb(card: &FakeCard, width: u32, height: u32) -> u32 {
        let dumb = card.create_dumb(width, height).expect("a dumb buffer");
        card.add_fb(dumb.handle, dumb.pitch, width, height, 0)
            .expect("a framebuffer")
    }

    fn errno(r: std::io::Result<()>) -> Option<Errno> {
        r.err()
            .and_then(|e| e.raw_os_error())
            .map(Errno::from_raw_os_error)
    }

    /// A commit that puts `fb` on the primary plane, with the source
    /// rectangle `src` where given, in whole pixels.
    fn show(card: &FakeCard, fb: u32, src: Option<(u64, u64)>) -> std::io::Result<()> {
        let mut req = Request::default();
        req.set(PRIMARY_PLANE, card.prop_id("FB_ID"), u64::from(fb));
        req.set(PRIMARY_PLANE, card.prop_id("CRTC_ID"), u64::from(CRTC));
        if let Some((w, h)) = src {
            req.set(PRIMARY_PLANE, card.prop_id("SRC_X"), 0);
            req.set(PRIMARY_PLANE, card.prop_id("SRC_Y"), 0);
            req.set(PRIMARY_PLANE, card.prop_id("SRC_W"), w << 16);
            req.set(PRIMARY_PLANE, card.prop_id("SRC_H"), h << 16);
        }
        card.commit(&req, 0)
    }

    // vtium/vtium#224: a legacy CRTC set or page flip whose framebuffer is
    // smaller than the mode is refused with ENOSPC, as the kernel's
    // `drm_crtc_check_viewport` refuses it, and one the mode fits is taken.
    #[test]
    fn a_legacy_set_or_flip_over_a_framebuffer_smaller_than_the_mode_is_refused() {
        let card = FakeCard::new(&[(CONN, vec![mode(640, 480, 60, true)])]);
        let (small, large) = (fb(&card, 320, 240), fb(&card, 640, 480));
        let at_640 = mode(640, 480, 60, true);
        assert_eq!(
            errno(card.set_crtc(CRTC, small, CONN, &at_640)),
            Some(Errno::NOSPC)
        );
        assert!(card.state().set_crtcs.is_empty(), "nothing set");
        assert_eq!(errno(card.set_crtc(CRTC, large, CONN, &at_640)), None);
        assert_eq!(errno(card.page_flip(CRTC, small)), Some(Errno::NOSPC));
        assert!(card.state().page_flips.is_empty(), "nothing flipped");
        assert_eq!(errno(card.page_flip(CRTC, large)), None);
        let narrow = fb(&card, 320, 480);
        card.complete_flips();
        assert_eq!(errno(card.page_flip(CRTC, narrow)), Some(Errno::NOSPC));
    }

    // vtium/vtium#224: an atomic commit whose plane reads past its
    // framebuffer's edge is refused with ENOSPC, as the kernel's
    // `drm_atomic_plane_check` refuses it, and one that reads inside it is
    // taken. A commit that sets only the framebuffer reads the source
    // rectangle the plane holds.
    #[test]
    fn a_commit_whose_plane_reads_past_its_framebuffer_is_refused() {
        let card = FakeCard::new(&[(CONN, vec![mode(640, 480, 60, true)])]);
        let (small, large) = (fb(&card, 320, 240), fb(&card, 640, 480));
        card.set_crtc(CRTC, large, CONN, &mode(640, 480, 60, true))
            .expect("the CRTC set");
        assert_eq!(
            errno(show(&card, small, Some((640, 480)))),
            Some(Errno::NOSPC)
        );
        assert_eq!(
            card.state().plane_fb.get(&PRIMARY_PLANE),
            None,
            "nothing shown"
        );
        assert_eq!(errno(show(&card, large, Some((640, 480)))), None);
        assert_eq!(card.state().plane_fb.get(&PRIMARY_PLANE), Some(&large));
        assert_eq!(errno(show(&card, small, None)), Some(Errno::NOSPC));
        assert_eq!(errno(show(&card, small, Some((320, 240)))), None);
    }

    // vtium/vtium#249: a commit that puts a plane on a CRTC outside the
    // plane's possible CRTCs is refused with EINVAL, as the kernel's
    // `drm_atomic_plane_check` refuses it, and one that puts the CRTC's own
    // plane there is taken. A CRTC the card does not have is refused with
    // EACCES as the property is set, before any check.
    #[test]
    fn a_commit_that_puts_a_plane_on_another_crtc_is_refused() {
        let card = FakeCard::new(&[
            (CONN, vec![mode(640, 480, 60, true)]),
            (CONN + 1, vec![mode(640, 480, 60, true)]),
        ]);
        let shown = fb(&card, 640, 480);
        card.set_crtc(CRTC + 1, shown, CONN + 1, &mode(640, 480, 60, true))
            .expect("the CRTC set");
        let put = |plane: u32| {
            let mut req = Request::default();
            req.set(plane, card.prop_id("FB_ID"), u64::from(shown));
            req.set(plane, card.prop_id("CRTC_ID"), u64::from(CRTC + 1));
            card.commit(&req, 0)
        };
        assert_eq!(errno(put(PRIMARY_PLANE)), Some(Errno::INVAL));
        assert_eq!(card.state().plane_fb.get(&PRIMARY_PLANE), None);
        assert_eq!(errno(put(PRIMARY_PLANE + 2)), None);
        let mut req = Request::default();
        req.set(PRIMARY_PLANE, card.prop_id("CRTC_ID"), 999);
        assert_eq!(
            errno(card.commit(&req, 0)),
            Some(Errno::ACCESS),
            "a CRTC the card does not have"
        );
        assert_eq!(
            card.state().plane_crtc.get(&(PRIMARY_PLANE + 2)),
            Some(&(CRTC + 1))
        );
    }
}
