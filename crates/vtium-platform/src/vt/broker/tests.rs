//! The broker's device holder against the stand-in console and nodes, and
//! the drawer's VT backend against the stand-in broker.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]

use std::cell::RefCell;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium_broker_protocol::{DevicePath, Errno, Order, Request};

use super::stand_in::{CARD, FB, KEYBOARD, MOUSE, NodeCall, StandIn, seat_node};
use super::{BrokerVt, RELEASE_BOUND};
use crate::input::fake::{self, FakeDevice, Nodes as FakeInputs};
use crate::input::{Input, InputConfig, InputEvent, Keymap, Open, Opening, RawEvent};
use crate::udev::{DRM_MAJOR, FB_MAJOR, INPUT_MAJOR, NodeFacts, NodeRefusal, admit};
use crate::vt::fake::{COOKED, Call};
use crate::vt::{DisplayMode, KbMode, Reldisp, Switching, Vt, VtEvent};

const VT: u16 = 2;

fn path(p: &str) -> DevicePath {
    DevicePath::new(p).unwrap_or_else(|| panic!("{p} is a device path"))
}

fn open(device: u32, p: &str) -> Request {
    Request::OpenDevice {
        device,
        path: path(p),
    }
}

/// The orders without their descriptors, and whether each came with one.
fn plain(orders: Vec<(Order, Option<OwnedFd>)>) -> Vec<(Order, bool)> {
    orders
        .into_iter()
        .map(|(o, fd)| (o, fd.is_some()))
        .collect()
}

/// A broker whose VT is shown, with the first turn's `Back` taken, and a
/// card and a keyboard open, master taken on the card.
fn holding() -> StandIn {
    let (mut s, _drawer) = StandIn::new(VT, VT, KbMode::Xlate);
    let now = s.now.get();
    assert_eq!(plain(s.holder.turn(now)), [(Order::Back, false)]);
    for (id, p) in [(1, CARD), (2, KEYBOARD)] {
        let answer = plain(s.holder.serve(open(id, p), now));
        assert_eq!(
            answer,
            [(
                Order::Device {
                    device: id,
                    opened: Ok(())
                },
                true
            )]
        );
    }
    let taken = plain(s.holder.serve(Request::TakeMaster { device: 1 }, now));
    assert_eq!(
        taken,
        [(
            Order::Master {
                device: 1,
                taken: true
            },
            false
        )]
    );
    s
}

/// The row's first test: a drawer that never answers a release still
/// loses its devices once the bound has passed. Before the bound the
/// broker keeps the VT and touches no device; at the bound it drops master
/// on the card, revokes the keyboard and lets the VT go, and the drawer
/// can then take neither master nor a new input device.
#[test]
fn a_release_past_the_bound_still_loses_the_devices() {
    let mut s = holding();
    let start = s.now.get();
    s.console.clear();
    s.nodes.borrow_mut().calls.clear();
    s.switch_away(5);
    assert_eq!(plain(s.holder.turn(start)), [(Order::Release, false)]);

    let before = start + RELEASE_BOUND - Duration::from_millis(1);
    assert!(s.holder.turn(before).is_empty());
    assert_eq!(s.holder.due(), Some(start + RELEASE_BOUND));
    assert!(
        s.node_calls().is_empty(),
        "nothing is let go before the bound"
    );
    assert!(!s.console.calls().contains(&Call::Reldisp(Reldisp::Allow)));

    assert!(s.holder.turn(start + RELEASE_BOUND).is_empty());
    assert_eq!(
        s.node_calls(),
        [
            NodeCall::DropMaster(PathBuf::from(CARD)),
            NodeCall::Revoke(PathBuf::from(KEYBOARD)),
        ]
    );
    assert!(s.console.calls().contains(&Call::Reldisp(Reldisp::Allow)));
    assert_eq!(s.console.0.borrow().on_screen, 5);
    assert_eq!(s.holder.due(), None);

    let now = start + RELEASE_BOUND;
    let taken = plain(s.holder.serve(Request::TakeMaster { device: 1 }, now));
    assert_eq!(
        taken,
        [(
            Order::Master {
                device: 1,
                taken: false
            },
            false
        )]
    );
    let refused = plain(s.holder.serve(open(3, MOUSE), now));
    assert_eq!(
        refused,
        [(
            Order::Device {
                device: 3,
                opened: Err(Errno(1))
            },
            false
        )]
    );
}

/// A drawer that answers lets the VT go at once, and a `Released` with no
/// release waiting does nothing.
#[test]
fn the_drawers_answer_ends_the_release_before_the_bound() {
    let mut s = holding();
    let now = s.now.get();
    s.nodes.borrow_mut().calls.clear();
    assert!(s.holder.serve(Request::Released, now).is_empty());
    assert!(s.node_calls().is_empty());
    s.switch_away(5);
    assert_eq!(plain(s.holder.turn(now)), [(Order::Release, false)]);
    assert!(s.holder.serve(Request::Released, now).is_empty());
    assert_eq!(
        s.node_calls(),
        [
            NodeCall::DropMaster(PathBuf::from(CARD)),
            NodeCall::Revoke(PathBuf::from(KEYBOARD)),
        ]
    );
    assert_eq!(s.console.0.borrow().on_screen, 5);
    assert_eq!(s.holder.due(), None);
}

/// The row's first test, its second half: after the drawer's end the
/// broker sets text mode, `VT_AUTO` and the keyboard mode it saved at the
/// start, in that order, and only then drops master on the card and
/// revokes the keyboard.
#[test]
fn the_restore_after_the_drawers_death_makes_its_calls_in_the_records_order() {
    let mut s = holding();
    s.console.clear();
    s.nodes.borrow_mut().calls.clear();
    s.holder.give_back();
    assert_eq!(
        s.console.calls(),
        [
            Call::SetDisplay(DisplayMode::Text),
            Call::SetSwitching(Switching::Auto),
            Call::SetKeyboard(KbMode::Xlate),
            Call::SetTermios(COOKED),
        ]
    );
    let keyboard_at = 3;
    let nodes = s.nodes.borrow().calls.clone();
    assert_eq!(
        nodes,
        [
            (NodeCall::DropMaster(PathBuf::from(CARD)), 4),
            (NodeCall::Revoke(PathBuf::from(KEYBOARD)), 4),
        ]
    );
    assert!(nodes.iter().all(|(_, seen)| *seen > keyboard_at));
    assert_eq!(
        s.console.modes(),
        (DisplayMode::Text, Switching::Auto, KbMode::Xlate)
    );
    // The holder's drop that follows puts nothing back again.
    s.console.clear();
    drop(s.holder);
    assert!(
        s.console.calls().iter().all(|c| !matches!(
            c,
            Call::SetDisplay(_) | Call::SetSwitching(_) | Call::SetKeyboard(_)
        )),
        "{:?}",
        s.console.calls()
    );
}

/// An acquire takes master on each card again, opens each input device
/// anew under the drawer's id, and then sends `Back`.
#[test]
fn an_acquire_takes_master_and_opens_each_input_device_anew() {
    let mut s = holding();
    let now = s.now.get();
    let first = s.nodes.borrow().files.get(Path::new(KEYBOARD)).copied();
    s.switch_away(5);
    let _ = s.holder.turn(now);
    let _ = s.holder.serve(Request::Released, now);
    s.nodes.borrow_mut().calls.clear();
    s.switch_back();
    let orders = s.holder.turn(now);
    let again = s.nodes.borrow().files.get(Path::new(KEYBOARD)).copied();
    assert_ne!(first, again, "the keyboard is a new open file");
    let inode = |fd: &OwnedFd| rustix::fs::fstat(fd).map(|st| st.st_ino).ok();
    assert_eq!(orders.len(), 2, "{orders:?}");
    assert_eq!(
        orders[0].0,
        Order::Device {
            device: 2,
            opened: Ok(())
        }
    );
    assert_eq!(orders[0].1.as_ref().and_then(inode), again);
    assert_eq!(orders[1].0, Order::Back);
    assert_eq!(
        s.node_calls(),
        [
            NodeCall::SetMaster(PathBuf::from(CARD)),
            NodeCall::Open(PathBuf::from(KEYBOARD)),
        ]
    );
}

/// While the VT is not the drawer's, a card opens but takes no master,
/// and an input device is refused `EPERM`, so the drawer never reads keys
/// typed on another VT.
#[test]
fn master_and_the_input_devices_wait_for_the_vt() {
    let (mut s, _drawer) = StandIn::new(VT, 1, KbMode::Unicode);
    let now = s.now.get();
    assert!(s.holder.turn(now).is_empty());
    assert!(plain(s.holder.serve(open(1, CARD), now))[0].1);
    assert_eq!(
        plain(s.holder.serve(Request::TakeMaster { device: 1 }, now)),
        [(
            Order::Master {
                device: 1,
                taken: false
            },
            false
        )]
    );
    assert_eq!(
        plain(s.holder.serve(open(2, KEYBOARD), now)),
        [(
            Order::Device {
                device: 2,
                opened: Err(Errno(1))
            },
            false
        )]
    );
    assert!(!s.nodes.borrow().masters.contains(&PathBuf::from(CARD)));
}

/// The real broker's checks, which the stand-in shares: only a character
/// device of DRM's, fbdev's or input's major number that udev knows as a
/// card, a framebuffer or an event device, of the broker's seat, and
/// tagged where udev runs.
#[test]
fn the_broker_admits_only_its_seats_cards_framebuffers_and_event_devices() {
    let ok = |facts: &NodeFacts| admit(facts, "seat0");
    assert_eq!(
        ok(&seat_node(DRM_MAJOR, "drm", "card0")),
        Ok(crate::udev::DeviceKind::Drm)
    );
    assert_eq!(
        ok(&seat_node(FB_MAJOR, "graphics", "fb1")),
        Ok(crate::udev::DeviceKind::Fbdev)
    );
    assert_eq!(
        ok(&seat_node(INPUT_MAJOR, "input", "event12")),
        Ok(crate::udev::DeviceKind::Input)
    );
    let refused = [
        (
            NodeFacts {
                char_device: false,
                ..seat_node(DRM_MAJOR, "drm", "card0")
            },
            NodeRefusal::NotADevice,
        ),
        (seat_node(8, "block", "sda"), NodeRefusal::Major(8)),
        (seat_node(4, "tty", "tty1"), NodeRefusal::Major(4)),
        (
            seat_node(DRM_MAJOR, "drm", "renderD128"),
            NodeRefusal::Kind("renderD128".into()),
        ),
        (
            seat_node(INPUT_MAJOR, "input", "mouse0"),
            NodeRefusal::Kind("mouse0".into()),
        ),
        (
            seat_node(INPUT_MAJOR, "drm", "event1"),
            NodeRefusal::Kind("event1".into()),
        ),
        (
            NodeFacts {
                seat: Some("seat1".into()),
                ..seat_node(INPUT_MAJOR, "input", "event5")
            },
            NodeRefusal::Seat("seat1".into()),
        ),
        (
            NodeFacts {
                tagged: Some(false),
                ..seat_node(DRM_MAJOR, "drm", "card1")
            },
            NodeRefusal::Untagged,
        ),
    ];
    for (facts, why) in refused {
        assert_eq!(ok(&facts), Err(why.clone()), "{facts:?}");
    }
    // Without a udev daemon no device carries tags, so none is asked for.
    let untagged = NodeFacts {
        tagged: None,
        ..seat_node(DRM_MAJOR, "drm", "card0")
    };
    assert!(ok(&untagged).is_ok());
    // A device of another seat is the broker's when the broker is that
    // seat's.
    let other = NodeFacts {
        seat: Some("seat1".into()),
        ..seat_node(DRM_MAJOR, "drm", "card2")
    };
    assert!(admit(&other, "seat1").is_ok());
}

/// The stand-in broker refuses what the real broker refuses, with the real
/// broker's answer, and never opens the node: `EACCES` for a node the
/// checks refuse, and `EINVAL` for a path outside `/dev/`, which the
/// protocol's reader refuses.
#[test]
fn the_stand_in_broker_refuses_what_the_real_broker_refuses() {
    let (mut s, drawer) = StandIn::new(VT, VT, KbMode::Unicode);
    let now = s.now.get();
    let _ = s.holder.turn(now);
    for (p, facts) in [
        (
            "/dev/dri/renderD128",
            seat_node(DRM_MAJOR, "drm", "renderD128"),
        ),
        ("/dev/sda", seat_node(8, "block", "sda")),
        (
            "/dev/input/event9",
            NodeFacts {
                seat: Some("seat1".into()),
                ..seat_node(INPUT_MAJOR, "input", "event9")
            },
        ),
        (
            "/dev/input/event8",
            NodeFacts {
                tagged: Some(false),
                ..seat_node(INPUT_MAJOR, "input", "event8")
            },
        ),
    ] {
        s.nodes.borrow_mut().facts.insert(PathBuf::from(p), facts);
    }
    for (id, p) in [
        (1, "/dev/dri/renderD128"),
        (2, "/dev/sda"),
        (3, "/dev/input/event9"),
        (4, "/dev/input/event8"),
    ] {
        assert_eq!(
            plain(s.holder.serve(open(id, p), now)),
            [(
                Order::Device {
                    device: id,
                    opened: Err(Errno(13))
                },
                false
            )],
            "{p}"
        );
    }
    // A path outside /dev/ never passes the reader.
    let mut bytes = open(5, CARD).encode();
    let at = bytes
        .windows(4)
        .position(|w| w == b"/dev")
        .unwrap_or_else(|| panic!("the path is in the message"));
    bytes[at + 1..at + 4].copy_from_slice(b"tmp");
    rustix::net::send(&drawer, &bytes, rustix::net::SendFlags::empty())
        .unwrap_or_else(|e| panic!("send: {e}"));
    s.pump();
    let answer = drawer.next_order().unwrap_or_else(|e| panic!("read: {e}"));
    assert!(
        matches!(
            answer,
            vtium_broker_protocol::Got::Message(Ok((
                Order::Device {
                    device: 5,
                    opened: Err(Errno(22))
                },
                None
            )))
        ),
        "{answer:?}"
    );
    assert!(
        s.node_calls().is_empty(),
        "no refused node was opened: {:?}",
        s.node_calls()
    );
}

/// The switch requests: the drawer's own VT whenever it asks, another VT
/// only while the VT is the drawer's and shown, and the VT's modes put
/// back only while it is the drawer's.
#[test]
fn the_broker_switches_only_from_the_drawers_vt() {
    let (mut s, _drawer) = StandIn::new(VT, 1, KbMode::Unicode);
    let now = s.now.get();
    let _ = s.holder.turn(now);
    s.console.clear();
    let _ = s.holder.serve(Request::SwitchVt { vt: 3 }, now);
    let _ = s.holder.serve(Request::RestoreVt, now);
    assert!(
        !s.console
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Activate(_) | Call::SetDisplay(_))),
        "{:?}",
        s.console.calls()
    );
    let _ = s.holder.serve(Request::SwitchVt { vt: 2 }, now);
    assert!(s.console.calls().contains(&Call::Activate(2)));

    let (mut s, _drawer) = StandIn::new(VT, VT, KbMode::Unicode);
    let _ = s.holder.turn(now);
    s.console.clear();
    let _ = s.holder.serve(Request::SwitchVt { vt: 3 }, now);
    assert!(s.console.calls().contains(&Call::Activate(3)));
    s.console.0.borrow_mut().display = DisplayMode::Text;
    let _ = s.holder.serve(Request::RestoreVt, now);
    assert_eq!(s.console.modes().0, DisplayMode::Graphics);
}

/// A device the drawer closes is let go first: master dropped on a card
/// and an input device revoked, so the drawer's copy keeps nothing the
/// broker no longer tracks.
#[test]
fn a_closed_device_is_let_go_first() {
    let mut s = holding();
    let now = s.now.get();
    s.nodes.borrow_mut().calls.clear();
    let _ = s.holder.serve(Request::CloseDevice { device: 1 }, now);
    let _ = s.holder.serve(Request::CloseDevice { device: 2 }, now);
    assert_eq!(
        s.node_calls(),
        [
            NodeCall::DropMaster(PathBuf::from(CARD)),
            NodeCall::Revoke(PathBuf::from(KEYBOARD)),
        ]
    );
    // The ids are free again.
    assert!(plain(s.holder.serve(open(1, FB), now))[0].1);
}

/// Input on the tty, as after another program changed the VT's modes,
/// takes the VT again, and the drawer is told with `Back` alone. A hangup
/// is reported for the broker to stop the drawer.
#[test]
fn a_retake_sends_back_and_a_hangup_is_reported() {
    let mut s = holding();
    let now = s.now.get();
    s.nodes.borrow_mut().calls.clear();
    assert!(s.holder.vt().tty_ready(true, false, now));
    assert_eq!(plain(s.holder.turn(now)), [(Order::Back, false)]);
    assert!(s.node_calls().is_empty());
    assert!(!s.holder.hung_up());
    assert!(!s.holder.vt().tty_ready(false, true, now));
    let _ = s.holder.turn(now);
    assert!(s.holder.hung_up());
}

/// The drawer and its stand-in broker, which answers each time the drawer
/// waits for an answer.
fn drawer(on_screen: u16) -> (BrokerVt, Rc<RefCell<StandIn>>) {
    let (s, link) = StandIn::new(VT, on_screen, KbMode::Unicode);
    let s = Rc::new(RefCell::new(s));
    let console = s.borrow().console.clone();
    let mut vt = BrokerVt::with(link, VT, Box::new(move || Ok(console.0.borrow().on_screen)));
    let pumped = Rc::downgrade(&s);
    vt.pumped_by(Rc::new(move || {
        if let Some(s) = pumped.upgrade() {
            s.borrow_mut().pump();
        }
    }));
    s.borrow_mut().pump();
    vt.readable();
    (vt, s)
}

fn events(vt: &mut BrokerVt) -> Vec<VtEvent> {
    vt.events().collect()
}

/// The drawer's backend follows the broker: `Back` is the acquire, a
/// release is acknowledged with `Released` once the owner says so, and the
/// broker then lets the devices go; `Back` after it is the next acquire.
#[test]
fn the_drawer_follows_the_brokers_release_and_acquire() {
    let (mut vt, s) = drawer(VT);
    assert_eq!(events(&mut vt), [VtEvent::Acquired]);
    assert!(vt.is_active());
    let card = vt
        .open_device(Path::new(CARD))
        .unwrap_or_else(|e| panic!("the card opens: {e}"));
    let inode = |fd: &OwnedFd| rustix::fs::fstat(fd).map(|st| st.st_ino).ok();
    assert_eq!(
        inode(&card),
        s.borrow()
            .nodes
            .borrow()
            .files
            .get(Path::new(CARD))
            .copied()
    );
    let keeper = vt
        .master_keeper(Path::new(CARD))
        .unwrap_or_else(|| panic!("the card has a master keeper"));
    keeper
        .take()
        .unwrap_or_else(|e| panic!("master is taken: {e}"));
    assert!(
        s.borrow()
            .nodes
            .borrow()
            .masters
            .contains(&PathBuf::from(CARD))
    );

    s.borrow_mut().switch_away(5);
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Release]);
    assert!(!vt.is_active());
    vt.acknowledge_release()
        .unwrap_or_else(|e| panic!("the release is acknowledged: {e}"));
    s.borrow_mut().pump();
    assert!(s.borrow().requests.contains(&Request::Released));
    assert!(s.borrow().nodes.borrow().masters.is_empty());
    assert_eq!(s.borrow().console.0.borrow().on_screen, 5);
    // Master is refused while the VT is another's.
    assert_eq!(
        keeper.take().map_err(|e| e.raw_os_error()),
        Err(Some(rustix::io::Errno::BUSY.raw_os_error()))
    );

    s.borrow_mut().switch_back();
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Acquired]);
    keeper
        .take()
        .unwrap_or_else(|e| panic!("master is taken again: {e}"));
}

/// A `Back` that comes while the drawer has not yet acknowledged a release,
/// as when the broker's bound passed and the VT came back, is reported
/// once the owner acknowledges, so the owner always sees the release
/// finish before the next acquire.
#[test]
fn a_back_during_a_release_waits_for_the_acknowledgement() {
    let (mut vt, s) = drawer(VT);
    let _ = events(&mut vt);
    let start = s.borrow().now.get();
    s.borrow_mut().switch_away(5);
    s.borrow_mut().pump();
    s.borrow().now.set(start + RELEASE_BOUND);
    s.borrow_mut().pump();
    s.borrow_mut().switch_back();
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Release]);
    vt.acknowledge_release()
        .unwrap_or_else(|e| panic!("the release is acknowledged: {e}"));
    assert_eq!(events(&mut vt), [VtEvent::Acquired]);
    assert!(vt.is_active());
}

/// The switch keys and the switches the owner asks for go to the broker as
/// `SwitchVt`; this VT's own key does nothing. `leave` goes back to the VT
/// shown when `activate` asked, and the broker's closed socket is a
/// hangup.
#[test]
fn the_drawers_switches_go_to_the_broker() {
    let (mut vt, s) = drawer(1);
    assert!(events(&mut vt).is_empty());
    assert_eq!(vt.wait().ok(), Some(crate::vt::Switch::Pending));
    assert_eq!(vt.activate().ok(), Some(crate::vt::Switch::Pending));
    s.borrow_mut().pump();
    s.borrow().console.run_worker();
    // The kernel's worker switches from VT 1, in VT_AUTO and text mode.
    s.borrow_mut().holder.vt().acquire_signal();
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Acquired]);

    let key = |n: u32| crate::input::KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods: crate::input::Mods::CTRL.union(crate::input::Mods::ALT),
        keysyms: vec![xkbcommon::xkb::keysyms::KEY_F1 + n - 1],
        codepoints: vec![None],
        repeat: false,
    };
    assert!(vt.key(&key(2)));
    assert!(vt.is_own_key(&key(2)));
    assert!(vt.key(&key(3)));
    assert_eq!(vt.leave().ok(), Some(crate::vt::Switch::Pending));
    s.borrow_mut().pump();
    let asked: Vec<Request> = s.borrow().requests.clone();
    assert_eq!(
        asked,
        [
            Request::SwitchVt { vt: 2 },
            Request::SwitchVt { vt: 3 },
            Request::SwitchVt { vt: 1 },
        ]
    );
    vt.restore().unwrap_or_else(|e| panic!("restore: {e}"));
    s.borrow_mut().pump();
    assert_eq!(s.borrow().requests.last(), Some(&Request::RestoreVt));

    drop(s);
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Hangup]);
    assert!(vt.open_device(Path::new(FB)).is_err());
}

/// The broker's other orders wait for the owner, and the owner's own
/// requests reach the broker.
#[test]
fn the_brokers_other_orders_wait_for_the_owner() {
    let (mut vt, s) = drawer(VT);
    let _ = events(&mut vt);
    s.borrow()
        .link
        .order(&Order::Blank, None)
        .unwrap_or_else(|e| panic!("blank: {e}"));
    s.borrow()
        .link
        .order(&Order::Wake, None)
        .unwrap_or_else(|e| panic!("wake: {e}"));
    // An order read while the drawer waits for an answer waits too.
    let _ = vt.open_device(Path::new(FB));
    vt.readable();
    let orders: Vec<Order> = vt.orders().into_iter().map(|(o, _)| o).collect();
    assert_eq!(orders, [Order::Blank, Order::Wake]);
    vt.request(&Request::Ready)
        .unwrap_or_else(|e| panic!("ready: {e}"));
    s.borrow_mut().pump();
    assert_eq!(s.borrow().requests.last(), Some(&Request::Ready));
}

fn keymap() -> Keymap {
    Keymap::fallback().unwrap_or_else(|| panic!("the built-in keymap compiles"))
}

const KEY_A: u16 = 30;

/// The stand-in keyboards and pointers, opened through the broker. Each
/// device is made only from a descriptor of the open file the stand-in
/// broker opened for its node, and only once vtium has made the
/// descriptor non-blocking.
fn through_broker(
    vt: &BrokerVt,
    s: &Rc<RefCell<StandIn>>,
    nodes: &FakeInputs,
) -> super::BrokerInput<FakeDevice> {
    let s = Rc::clone(s);
    let nodes = nodes.clone();
    vt.input_with(Box::new(move |path: &Path, fd: OwnedFd| {
        let inode = rustix::fs::fstat(fd.as_fd())?.st_ino;
        if s.borrow().nodes.borrow().files.get(path) != Some(&inode) {
            return Err(std::io::Error::other(format!(
                "{} is not the open file the broker gave",
                path.display()
            )));
        }
        let flags = rustix::fs::fcntl_getfl(&fd)?;
        if !flags.contains(rustix::fs::OFlags::NONBLOCK) {
            return Err(std::io::Error::other("the broker's descriptor blocks"));
        }
        nodes.device(path)
    }))
}

/// A keyboard opened through the broker types while the VT is the
/// drawer's. A release closes it and keeps its id at the broker, which
/// opens it anew at the acquire, and the input's wake-up opens it from the
/// new descriptor without asking.
#[test]
fn a_keyboard_opened_through_the_broker_is_opened_anew_at_each_acquire() {
    let (mut vt, s) = drawer(VT);
    let _ = events(&mut vt);
    let nodes = FakeInputs::default();
    let node = nodes.add(KEYBOARD, fake::keyboard());
    let mut input = Input::new(nodes.clone(), keymap(), InputConfig::default());
    input.open_with(through_broker(&vt, &s, &nodes));
    input.wake_up();
    let id = input
        .add(Path::new(KEYBOARD))
        .unwrap_or_else(|| panic!("the keyboard opens through the broker"));
    let first = s
        .borrow()
        .nodes
        .borrow()
        .files
        .get(Path::new(KEYBOARD))
        .copied();
    for value in [1, 0] {
        node.borrow_mut().queue.push_back(RawEvent {
            kind: crate::ioctl::EV_KEY,
            code: KEY_A,
            value,
        });
    }
    #[expect(clippy::disallowed_methods, reason = "the input reads keys at a time")]
    let now = Instant::now();
    input.dispatch(id, now);
    assert!(
        input.events().any(|e| matches!(e, InputEvent::Key(_))),
        "the keyboard types"
    );

    s.borrow_mut().switch_away(5);
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Release]);
    input.sleep();
    assert!(!node.borrow().open);
    vt.acknowledge_release()
        .unwrap_or_else(|e| panic!("ack: {e}"));
    s.borrow_mut().pump();
    assert!(
        !s.borrow()
            .requests
            .iter()
            .any(|r| matches!(r, Request::CloseDevice { .. })),
        "a release keeps the keyboard's id at the broker"
    );

    s.borrow_mut().switch_back();
    s.borrow_mut().pump();
    vt.readable();
    assert_eq!(events(&mut vt), [VtEvent::Acquired]);
    let asked = s.borrow().requests.len();
    input.wake_up();
    assert!(node.borrow().open, "the keyboard opens again");
    let again = s
        .borrow()
        .nodes
        .borrow()
        .files
        .get(Path::new(KEYBOARD))
        .copied();
    assert_ne!(first, again);
    s.borrow_mut().pump();
    assert_eq!(
        s.borrow().requests.len(),
        asked,
        "the wake-up asked the broker nothing"
    );
}

/// A device the input asks for while the VT is not the drawer's waits for
/// the next wake-up, and so does one the broker refuses with `EPERM`.
#[test]
fn an_input_device_waits_for_the_vt() {
    let (mut vt, s) = drawer(1);
    let _ = events(&mut vt);
    let nodes = FakeInputs::default();
    nodes.add(KEYBOARD, fake::keyboard());
    let mut open = through_broker(&vt, &s, &nodes);
    assert!(matches!(open.open(Path::new(KEYBOARD)), Ok(Opening::Wake)));
    assert!(s.borrow().requests.is_empty(), "nothing was asked");
}
