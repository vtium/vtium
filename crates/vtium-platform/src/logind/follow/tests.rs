//! Following the system's keyboard layout against a stand-in system bus:
//! localed on it or gone, its signals routed as the bus routes them, its
//! properties read as sd-bus reads them, and keyboards that stand in for
//! evdev's.

#![expect(clippy::panic, reason = "a test fails by panicking")]
#![expect(
    clippy::disallowed_methods,
    reason = "the input reads its keys at a time, here the test's"
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use xkbcommon::xkb::keysyms;

use super::*;
use crate::input::fake::{self, Nodes};
use crate::input::{InputConfig, InputEvent, Mods, RawEvent};
use crate::ioctl::{EV_KEY, LED_CAPSL};
use crate::logind::classify;
use crate::logind::localed::{Bus, layout_from};

/// A signal sent on the stand-in bus: the names its sender owned when it
/// sent it, the object, the interface and the member, and its arguments.
struct Sent {
    sender: Vec<String>,
    /// The unique name a unicast signal is addressed to.
    destination: Option<String>,
    path: String,
    interface: String,
    member: String,
    changed: Changed,
}

/// The system bus, with localed on it or not.
#[derive(Default)]
struct World {
    /// localed's unique name while it owns `org.freedesktop.locale1`.
    localed: Option<String>,
    /// The unique names given so far.
    connections: u32,
    /// localed's properties, which it keeps while it is gone, as it keeps
    /// them in the system's files.
    properties: BTreeMap<&'static str, String>,
    sent: Vec<Sent>,
    /// The properties vtium asked localed for, in order.
    asked: Vec<String>,
}

type Shared = Rc<RefCell<World>>;

const LOCALED: &str = "org.freedesktop.locale1";

impl World {
    /// localed starts, on a unique name of its own, as the bus starts it
    /// for a call.
    fn start_localed(&mut self) {
        self.connections = self.connections.saturating_add(1);
        self.localed = Some(format!(":1.{}", self.connections));
    }

    /// localed exits, as it does when it has been idle.
    fn stop_localed(&mut self) {
        self.localed = None;
    }

    /// Sends `PropertiesChanged` from `sender` on `path`, about the
    /// properties `properties` of `interface`.
    fn send(&mut self, sender: &str, path: &str, interface: &str, properties: &[&str]) {
        self.send_to(sender, None, path, interface, properties);
    }

    /// [`World::send`], addressed to `destination` when given.
    fn send_to(
        &mut self,
        sender: &str,
        destination: Option<&str>,
        path: &str,
        interface: &str,
        properties: &[&str],
    ) {
        let mut names = vec![sender.to_owned()];
        if self.localed.as_deref() == Some(sender) {
            names.push(LOCALED.to_owned());
        }
        self.sent.push(Sent {
            sender: names,
            destination: destination.map(str::to_owned),
            path: path.to_owned(),
            interface: "org.freedesktop.DBus.Properties".to_owned(),
            member: "PropertiesChanged".to_owned(),
            changed: Changed {
                destination: destination.map(str::to_owned),
                interface: interface.to_owned(),
                properties: properties.iter().map(|p| (*p).to_owned()).collect(),
            },
        });
    }

    /// localed takes `values`, as `SetX11Keyboard` or `SetVConsoleKeyboard`
    /// does, and announces each changed property on its object.
    fn localed_sets(&mut self, values: &[(&'static str, &str)]) {
        for (name, value) in values {
            self.properties.insert(name, (*value).to_owned());
        }
        let sender = self
            .localed
            .clone()
            .unwrap_or_else(|| panic!("localed runs to take a new layout"));
        let names: Vec<&str> = values.iter().map(|(n, _)| *n).collect();
        self.send(&sender, "/org/freedesktop/locale1", LOCALED, &names);
    }
}

/// One part of a match: `None` matches anything.
fn part_matches(part: Option<&CString>, value: &str) -> bool {
    part.is_none_or(|p| p.to_bytes() == value.as_bytes())
}

/// vtium's unique name on the stand-in bus.
const VTIUM: &str = ":1.100";

/// vtium's connection to the stand-in bus. Its match works as the bus's
/// does: a sender part that is a well-known name matches the signals that
/// the name's owner sent while it owned the name. A signal addressed to
/// vtium reaches it whatever the match says, as dbus-daemon delivers a
/// unicast message to its destination, and one addressed to another
/// connection never does.
struct StandInSignals {
    world: Shared,
    /// The match vtium asked for: sender, path, interface and member.
    rule: Option<[Option<CString>; 4]>,
    /// How many signals sent on the bus this connection has seen.
    seen: usize,
}

impl StandInSignals {
    fn new(world: &Shared) -> StandInSignals {
        StandInSignals {
            world: Rc::clone(world),
            rule: None,
            seen: 0,
        }
    }
}

impl Signals for StandInSignals {
    fn watch(
        &mut self,
        sender: Option<&CStr>,
        path: Option<&CStr>,
        interface: Option<&CStr>,
        member: Option<&CStr>,
    ) -> Result<(), Error> {
        // sd-bus refuses an object path that does not start with a slash,
        // and an interface with no dot (sd_bus_match_signal(3))
        let path_ok = path.is_none_or(|p| p.to_bytes().starts_with(b"/"));
        let interface_ok = interface.is_none_or(|i| i.to_bytes().contains(&b'.'));
        if !path_ok || !interface_ok {
            return Err(Error::NoBus(io::Error::from_raw_os_error(libc::EINVAL)));
        }
        // vtium's connection asks for one match
        assert!(self.rule.is_none(), "one match");
        let own = |s: Option<&CStr>| s.map(CStr::to_owned);
        self.rule = Some([own(sender), own(path), own(interface), own(member)]);
        self.seen = self.world.borrow().sent.len();
        Ok(())
    }

    fn take(&mut self) -> Result<Vec<Changed>, Error> {
        let world = self.world.borrow();
        let new = world.sent.get(self.seen..).unwrap_or_default();
        self.seen = world.sent.len();
        let Some([sender, path, interface, member]) = &self.rule else {
            return Ok(Vec::new());
        };
        Ok(new
            .iter()
            .filter(|s| match s.destination.as_deref() {
                Some(destination) => destination == VTIUM,
                None => {
                    sender
                        .as_ref()
                        .is_none_or(|n| s.sender.iter().any(|o| o.as_bytes() == n.to_bytes()))
                        && part_matches(path.as_ref(), &s.path)
                        && part_matches(interface.as_ref(), &s.interface)
                        && part_matches(member.as_ref(), &s.member)
                }
            })
            .map(|s| s.changed.clone())
            .collect())
    }
}

/// localed's properties on the stand-in bus, read as sd-bus reads them:
/// a service nobody owns is `ServiceUnknown`; on localed, another object
/// is `UnknownObject`, another interface `UnknownInterface` and another
/// property `UnknownProperty`.
struct StandInProperties(Shared);

fn refused(name: &str) -> Error {
    Error::Refused {
        name: name.to_owned(),
        message: String::new(),
    }
}

impl Bus for StandInProperties {
    fn string_property(
        &mut self,
        destination: &CStr,
        path: &CStr,
        interface: &CStr,
        member: &CStr,
    ) -> Result<String, Error> {
        let mut world = self.0.borrow_mut();
        let member = member.to_string_lossy().into_owned();
        world.asked.push(member.clone());
        if world.localed.is_none() || destination != LOCALE1 {
            return Err(classify(
                Some("org.freedesktop.DBus.Error.ServiceUnknown".to_owned()),
                None,
                -2,
            ));
        }
        if path != LOCALE1_PATH {
            return Err(refused("org.freedesktop.DBus.Error.UnknownObject"));
        }
        if interface != LOCALE1 {
            return Err(refused("org.freedesktop.DBus.Error.UnknownInterface"));
        }
        world
            .properties
            .get(member.as_str())
            .cloned()
            .ok_or_else(|| refused("org.freedesktop.DBus.Error.UnknownProperty"))
    }
}

/// A few rows of systemd 262's kbd-model-map.
const TABLE: &str = "\
# consolelayout\t\txlayout\txmodel\t\txvariant\txoptions\t\t\t\t\tbcp47
de-latin1\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t\t\t-
us\t\t\tus\tpc105+inet\t-\t\tterminate:ctrl_alt_bksp\t\t\t\ten-US,en
";

/// A root of its own for one test, with the table and, when given,
/// `vconsole` as etc/vconsole.conf.
fn root(name: &str, vconsole: Option<&str>) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("vtium-layout-follow-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "usr/share/systemd/kbd-model-map", TABLE);
    if let Some(text) = vconsole {
        write(&root, "etc/vconsole.conf", text);
    }
    root
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{e}"));
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
}

/// The US layout, as localed holds it at the start of each test.
fn us() -> [(&'static str, &'static str); 4] {
    [
        ("X11Layout", "us"),
        ("X11Model", "pc105"),
        ("X11Variant", ""),
        ("X11Options", ""),
    ]
}

/// The bus with localed on it holding the US layout.
fn world() -> Shared {
    let mut world = World::default();
    world.start_localed();
    for (name, value) in us() {
        world.properties.insert(name, value.to_owned());
    }
    Rc::new(RefCell::new(world))
}

const KEYBOARDS: [&str; 2] = ["/dev/input/event0", "/dev/input/event1"];

/// The seat's input, awake, with two keyboards, on the keymap that `own`
/// and the system's names `system` give, as at vtium's start.
fn input(own: &KeymapConfig, system: &RuleNames) -> (Input<Nodes>, Nodes) {
    let keymap =
        Keymap::new(&own.clone().follow(system)).unwrap_or_else(|| panic!("the keymap compiles"));
    let nodes = Nodes::default();
    for node in KEYBOARDS {
        nodes.add(node, fake::keyboard());
    }
    let mut input = Input::new(nodes.clone(), keymap, InputConfig::default());
    input.wake_up();
    for node in KEYBOARDS {
        input
            .add(Path::new(node))
            .unwrap_or_else(|| panic!("{node} is taken"));
    }
    (input, nodes)
}

/// The keysyms a press and release of the evdev key `code` on the keyboard
/// at `node` give.
fn press(input: &mut Input<Nodes>, nodes: &Nodes, node: &str, code: u16) -> Vec<u32> {
    let id = input
        .find(Path::new(node))
        .unwrap_or_else(|| panic!("{node} is in the input"));
    let device = nodes
        .0
        .borrow()
        .get(Path::new(node))
        .cloned()
        .unwrap_or_else(|| panic!("{node} stands in"));
    for value in [1, 0] {
        device.borrow_mut().queue.push_back(RawEvent {
            kind: EV_KEY,
            code,
            value,
        });
    }
    input.dispatch(id, Instant::now());
    input
        .events()
        .filter_map(|e| match e {
            InputEvent::Key(k) => Some(k.keysyms),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The key labelled Y on a US keyboard, which types z on a German layout.
const KEY_Y: u16 = 21;
/// Caps Lock, in evdev's codes.
const KEY_CAPSLOCK: u16 = 58;

/// What the key labelled Y types on each keyboard.
fn key_y(input: &mut Input<Nodes>, nodes: &Nodes) -> Vec<Vec<u32>> {
    KEYBOARDS
        .iter()
        .map(|node| press(input, nodes, node, KEY_Y))
        .collect()
}

/// Everything one test drives: the bus, vtium's watch, its input, and
/// the root its files are under.
struct Rig {
    world: Shared,
    follow: LayoutFollow<StandInSignals>,
    input: Input<Nodes>,
    nodes: Nodes,
    root: PathBuf,
}

impl Rig {
    /// vtium started with `own` from vtium.conf on the system under `root`,
    /// with localed holding the US layout.
    fn new(own: KeymapConfig, root: PathBuf) -> Rig {
        let world = world();
        let system = system::find(
            &root,
            Some(|| layout_from(&mut StandInProperties(Rc::clone(&world)))),
        )
        .unwrap_or_else(|e| panic!("the layout at start: {e}"))
        .names;
        let (input, nodes) = input(&own, &system);
        let follow = LayoutFollow::with(StandInSignals::new(&world), own, &system)
            .unwrap_or_else(|e| panic!("the watch starts: {e}"));
        world.borrow_mut().asked.clear();
        Rig {
            world,
            follow,
            input,
            nodes,
            root,
        }
    }

    /// What vtium does when the bus's descriptor turns readable: whether a
    /// new keymap reached the input.
    fn dispatch(&mut self) -> bool {
        let world = Rc::clone(&self.world);
        self.follow
            .dispatch_in(
                &self.root,
                move || layout_from(&mut StandInProperties(world)),
                Some(&mut self.input),
            )
            .unwrap_or_else(|e| panic!("the bus answers: {e}"))
    }

    fn key_y(&mut self) -> Vec<Vec<u32>> {
        key_y(&mut self.input, &self.nodes)
    }

    fn asked(&self) -> Vec<String> {
        self.world.borrow().asked.clone()
    }
}

const Y: u32 = keysyms::KEY_y;
const Z: u32 = keysyms::KEY_z;

/// vtium asks the bus for localed's `PropertiesChanged` on localed's
/// object, and nothing wider.
#[test]
fn the_watch_asks_for_localeds_signal_on_localeds_object() {
    let rig = Rig::new(KeymapConfig::default(), root("rule", None));
    let rule = rig.follow.signals.rule.clone();
    let text = |p: &Option<CString>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
    let rule = rule.unwrap_or_else(|| panic!("a match"));
    assert_eq!(
        rule.iter().map(text).collect::<Vec<_>>(),
        [
            Some("org.freedesktop.locale1".to_owned()),
            Some("/org/freedesktop/locale1".to_owned()),
            Some("org.freedesktop.DBus.Properties".to_owned()),
            Some("PropertiesChanged".to_owned()),
        ]
    );
}

/// `localectl set-x11-keymap de` while vtium runs: every keyboard types
/// with the German layout, and so does a keyboard plugged in afterwards.
// covers: platform.system-layout-follow
#[test]
fn a_change_moves_the_layout_on_every_keyboard() {
    let mut rig = Rig::new(KeymapConfig::default(), root("moves", None));
    assert_eq!(rig.key_y(), [[Y], [Y]]);
    rig.world.borrow_mut().localed_sets(&[
        ("X11Layout", "de"),
        ("X11Model", "pc105"),
        ("X11Variant", "nodeadkeys"),
    ]);
    assert!(rig.dispatch(), "a new keymap");
    assert_eq!(
        rig.asked(),
        ["X11Layout", "X11Model", "X11Variant", "X11Options"]
    );
    assert_eq!(rig.key_y(), [[Z], [Z]]);
    let node = "/dev/input/event2";
    rig.nodes.add(node, fake::keyboard());
    rig.input
        .add(Path::new(node))
        .unwrap_or_else(|| panic!("the new keyboard is taken"));
    assert_eq!(press(&mut rig.input, &rig.nodes, node, KEY_Y), [Z]);
    // the same layout announced again builds nothing
    rig.world.borrow_mut().localed_sets(&[("X11Layout", "de")]);
    assert!(!rig.dispatch(), "the keymap in use already");
}

/// vtium.conf sets `xkb-layout = us`, so a new layout in localed leaves
/// the keymap as it is, while a name vtium.conf leaves unset still
/// follows the system.
// covers: platform.system-layout-follow
#[test]
fn vtium_confs_keys_still_win_after_a_change() {
    let own = KeymapConfig {
        layout: "us".to_owned(),
        ..KeymapConfig::default()
    };
    let mut rig = Rig::new(own, root("conf", None));
    rig.world
        .borrow_mut()
        .localed_sets(&[("X11Layout", "de"), ("X11Variant", "nodeadkeys")]);
    assert!(!rig.dispatch(), "vtium.conf's layout stays");
    assert_eq!(rig.key_y(), [[Y], [Y]]);
    rig.world
        .borrow_mut()
        .localed_sets(&[("X11Options", "ctrl:nocaps")]);
    assert!(
        rig.dispatch(),
        "the system's options fill the key vtium.conf leaves unset"
    );
    assert_eq!(rig.follow.applied.layout, "us");
    assert_eq!(rig.follow.applied.variant, "");
    assert_eq!(rig.follow.applied.options, "ctrl:nocaps");
    assert_eq!(rig.key_y(), [[Y], [Y]]);
}

/// `localectl set-keymap de-latin1` rewrites vconsole.conf and localed's
/// X11 names together. The console keymap wins over localed's names, as at
/// vtium's start, so localed is not asked.
// covers: platform.system-layout-follow
#[test]
fn a_change_in_vconsole_conf_wins_over_localed() {
    let root = root("vconsole", Some("KEYMAP=us\n"));
    let mut rig = Rig::new(KeymapConfig::default(), root.clone());
    write(&root, "etc/vconsole.conf", "KEYMAP=de-latin1\n");
    rig.world
        .borrow_mut()
        .localed_sets(&[("VConsoleKeymap", "de-latin1"), ("X11Layout", "fr")]);
    assert!(rig.dispatch(), "a new keymap");
    assert_eq!(rig.asked(), Vec::<String>::new(), "localed is not asked");
    assert_eq!(rig.follow.applied.layout, "de");
    assert_eq!(rig.follow.applied.model, "pc105");
    assert_eq!(rig.key_y(), [[Z], [Z]]);
}

/// localed exits when it has been idle, and the bus starts it again, on a
/// new unique name, for the next `localectl`. vtium hears the new localed's
/// announcement. When localed announces a layout and exits before vtium
/// can read it, the keymap stays as it is.
// covers: platform.system-layout-follow
#[test]
fn localed_vanishing_and_coming_back() {
    let mut rig = Rig::new(KeymapConfig::default(), root("vanish", None));
    rig.world.borrow_mut().stop_localed();
    assert!(!rig.dispatch(), "nothing announced");
    rig.world.borrow_mut().start_localed();
    rig.world.borrow_mut().localed_sets(&[("X11Layout", "de")]);
    assert!(rig.dispatch(), "the new localed's layout");
    assert_eq!(rig.key_y(), [[Z], [Z]]);
    rig.world.borrow_mut().asked.clear();
    {
        let mut world = rig.world.borrow_mut();
        world.localed_sets(&[("X11Layout", "us")]);
        world.stop_localed();
    }
    assert!(!rig.dispatch(), "localed cannot be read");
    assert_eq!(rig.asked(), ["X11Layout"]);
    assert_eq!(rig.follow.applied.layout, "de");
    assert_eq!(rig.key_y(), [[Z], [Z]]);
}

/// A signal about localed's other properties, or about another interface,
/// leaves the keymap alone and asks localed nothing.
// covers: platform.system-layout-follow
#[test]
fn a_signal_with_no_layout_properties_changes_nothing() {
    let mut rig = Rig::new(KeymapConfig::default(), root("other-properties", None));
    rig.world
        .borrow_mut()
        .properties
        .insert("X11Layout", "de".to_owned());
    let localed = rig.world.borrow().localed.clone().unwrap_or_default();
    rig.world.borrow_mut().send(
        &localed,
        "/org/freedesktop/locale1",
        LOCALED,
        &["Locale", "VConsoleKeymapToggle"],
    );
    rig.world.borrow_mut().send(
        &localed,
        "/org/freedesktop/locale1",
        "org.freedesktop.DBus.Peer",
        &["X11Layout"],
    );
    rig.world
        .borrow_mut()
        .send(&localed, "/org/freedesktop/locale1", LOCALED, &[]);
    assert!(!rig.dispatch());
    assert_eq!(rig.asked(), Vec::<String>::new());
    assert_eq!(rig.key_y(), [[Y], [Y]]);
}

/// A `PropertiesChanged` about localed's interface on another object, from
/// a sender that is not localed, or addressed to vtium alone, changes
/// nothing.
// covers: platform.system-layout-follow
#[test]
fn a_signal_on_another_object_or_from_another_sender_changes_nothing() {
    let mut rig = Rig::new(KeymapConfig::default(), root("other-object", None));
    rig.world
        .borrow_mut()
        .properties
        .insert("X11Layout", "de".to_owned());
    let localed = rig.world.borrow().localed.clone().unwrap_or_default();
    rig.world.borrow_mut().send(
        &localed,
        "/org/freedesktop/locale1/other",
        LOCALED,
        &["X11Layout"],
    );
    rig.world
        .borrow_mut()
        .send(":1.99", "/org/freedesktop/locale1", LOCALED, &["X11Layout"]);
    // A signal addressed to vtium reaches it past the match, from another
    // program or from localed itself, which only ever broadcasts.
    for sender in [":1.99", localed.as_str()] {
        rig.world.borrow_mut().send_to(
            sender,
            Some(VTIUM),
            "/org/freedesktop/locale1",
            LOCALED,
            &["X11Layout"],
        );
    }
    assert!(!rig.dispatch());
    assert_eq!(rig.asked(), Vec::<String>::new());
    assert_eq!(rig.key_y(), [[Y], [Y]]);
}

/// Caps Lock stays on across a change of layout: the keyboard still holds
/// Lock, the key labelled Y types a capital Z on the new German layout, and
/// vtium writes the keyboard's LEDs again with Caps Lock lit.
// covers: platform.system-layout-follow
#[test]
fn caps_lock_stays_on_across_a_change() {
    let mut rig = Rig::new(KeymapConfig::default(), root("caps", None));
    let first = KEYBOARDS[0];
    press(&mut rig.input, &rig.nodes, first, KEY_CAPSLOCK);
    let node = rig
        .nodes
        .0
        .borrow()
        .get(Path::new(first))
        .cloned()
        .unwrap_or_else(|| panic!("{first} stands in"));
    let writes = node.borrow().leds.len();
    assert!(rig.input.mods().contains(Mods::LOCK), "Caps Lock is on");
    rig.world.borrow_mut().localed_sets(&[("X11Layout", "de")]);
    assert!(rig.dispatch(), "a new keymap");
    assert!(rig.input.mods().contains(Mods::LOCK), "Caps Lock stays on");
    let leds = node.borrow().leds.clone();
    assert_eq!(leds.len(), writes + 1, "the LEDs are written again");
    assert!(
        leds.last().is_some_and(|w| w.contains(&(LED_CAPSL, true))),
        "Caps Lock's LED is lit: {leds:?}"
    );
    assert_eq!(
        press(&mut rig.input, &rig.nodes, first, KEY_Y),
        [keysyms::KEY_Z]
    );
}
