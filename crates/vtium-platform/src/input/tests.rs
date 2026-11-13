//! The input against stand-in devices: key events fed in through the
//! built-in keymap, compose, the repeat, the LEDs, the pointer, and the
//! devices' sleep and wake-up.

#![expect(clippy::panic, reason = "a test fails by panicking")]
#![expect(
    clippy::disallowed_methods,
    reason = "a test's instants start from the clock once"
)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use xkbcommon::xkb::keysyms;

use super::evdev::RawEvent;
use super::fake::{self, Node, Nodes};
use super::pointer::Range;
use super::{
    Button, DeviceInput, Disposition, Input, InputConfig, InputEvent, KeyEvent, Keymap,
    KeymapConfig, Leds, Mods, PointerEvent, RuleNames,
};
use crate::ioctl::{
    ABS_X, ABS_Y, BTN_LEFT, BTN_RIGHT, BTN_TOUCH, EV_ABS, EV_KEY, EV_REL, EV_SYN, LED_CAPSL,
    LED_NUML, LED_SCROLLL, REL_WHEEL, REL_X, REL_Y,
};
use crate::vt::{FakeVt, Vt};

// evdev's key codes, `linux/input-event-codes.h`.
const KEY_E: u16 = 18;
const KEY_A: u16 = 30;
const KEY_S: u16 = 31;
const KEY_X: u16 = 45;
const KEY_Z: u16 = 44;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTALT: u16 = 56;
const KEY_CAPSLOCK: u16 = 58;
const KEY_F2: u16 = 60;
const KEY_F3: u16 = 61;
const KEY_NUMLOCK: u16 = 69;
const KEY_KP1: u16 = 79;
const KEY_LEFTMETA: u16 = 125;

fn keymap() -> Keymap {
    Keymap::fallback().unwrap_or_else(|| panic!("the built-in keymap compiles"))
}

/// The input awake, with one keyboard at /dev/input/event0.
fn with_keyboard() -> (Input<Nodes>, Rc<RefCell<Node>>, super::DeviceId) {
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event0", fake::keyboard());
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("the keyboard is taken"));
    (input, node, id)
}

fn ev(kind: u16, code: u16, value: i32) -> RawEvent {
    RawEvent { kind, code, value }
}

fn press(code: u16) -> RawEvent {
    ev(EV_KEY, code, 1)
}

fn release(code: u16) -> RawEvent {
    ev(EV_KEY, code, 0)
}

/// Queues `events` on the device and reads them.
fn feed(
    input: &mut Input<Nodes>,
    node: &Rc<RefCell<Node>>,
    id: super::DeviceId,
    events: &[RawEvent],
) {
    node.borrow_mut().queue.extend(events.iter().copied());
    input.dispatch(id, Instant::now());
}

fn keys(input: &mut Input<Nodes>) -> Vec<KeyEvent> {
    input
        .events()
        .filter_map(|e| match e {
            InputEvent::Key(k) => Some(k),
            InputEvent::Release(_) | InputEvent::Pointer(_) | InputEvent::Mods(_) => None,
        })
        .collect()
}

fn syms(keys: &[KeyEvent]) -> Vec<u32> {
    keys.iter()
        .flat_map(|k| k.keysyms.iter().copied())
        .collect()
}

// covers: platform.vt-switch
#[test]
fn a_key_press_becomes_its_keysym_with_its_modifiers() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_A), release(KEY_A)]);
    let k = keys(&mut input);
    assert_eq!(syms(&k), [keysyms::KEY_a], "one event, for the press");
    let a = k.first().unwrap_or_else(|| panic!("a key"));
    assert_eq!(a.keycode, u32::from(KEY_A) + 8);
    assert_eq!(a.ascii, u32::from(b'a'));
    assert_eq!(a.codepoints, [Some('a')]);
    assert_eq!(a.mods, Mods::empty());
    assert!(!a.repeat);

    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEFTSHIFT),
            press(KEY_A),
            release(KEY_A),
            release(KEY_LEFTSHIFT),
        ],
    );
    let k = keys(&mut input);
    assert_eq!(syms(&k), [keysyms::KEY_Shift_L, keysyms::KEY_A]);
    assert_eq!(k.get(1).map(|k| k.mods), Some(Mods::SHIFT));
    assert_eq!(
        k.get(1).map(|k| k.codepoints.clone()),
        Some(vec![Some('A')])
    );

    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_LEFTCTRL), press(KEY_LEFTALT), press(KEY_F2)],
    );
    let k = keys(&mut input);
    let f2 = k.last().unwrap_or_else(|| panic!("a key"));
    assert_eq!(f2.mods, Mods::CTRL.union(Mods::ALT));
    // The keymap gives Ctrl+Alt+F2 the switch keysym the VT takes.
    assert_eq!(f2.keysyms, [keysyms::KEY_XF86Switch_VT_2]);
    assert_eq!(input.mods(), Mods::CTRL.union(Mods::ALT));
}

#[test]
fn the_kernels_own_repeat_is_ignored() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_A), ev(EV_KEY, KEY_A, 2), ev(EV_KEY, KEY_A, 2)],
    );
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_a]);
}

#[test]
fn caps_lock_sets_the_lock_modifier_and_its_led() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_CAPSLOCK), release(KEY_CAPSLOCK), press(KEY_A)],
    );
    let k = keys(&mut input);
    let a = k.last().unwrap_or_else(|| panic!("a key"));
    assert_eq!(a.keysyms, [keysyms::KEY_A], "Lock's case change");
    assert_eq!(a.mods, Mods::LOCK);
    assert_eq!(
        node.borrow().leds.last().cloned(),
        Some(vec![
            (LED_NUML, false),
            (LED_CAPSL, true),
            (LED_SCROLLL, false)
        ])
    );
}

/// The keyboard of [`with_keyboard`] with a compose table: a then b makes
/// c, a then s makes a string and no keysym.
fn with_compose() -> (Input<Nodes>, Rc<RefCell<Node>>, super::DeviceId) {
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event0", fake::keyboard());
    let mut keymap = keymap();
    keymap.set_compose_table(b"<a> <e> : \"x\" x\n<a> <s> : \"ss\"\n", "C");
    let mut input = Input::new(nodes, keymap, InputConfig::default());
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("the keyboard is taken"));
    (input, node, id)
}

#[test]
fn a_compose_sequence_makes_its_keysym_alone() {
    let (mut input, node, id) = with_compose();
    feed(&mut input, &node, id, &[press(KEY_A), release(KEY_A)]);
    assert!(keys(&mut input).is_empty(), "composing");
    feed(&mut input, &node, id, &[press(KEY_E), release(KEY_E)]);
    let k = keys(&mut input);
    assert_eq!(syms(&k), [keysyms::KEY_x]);
    assert_eq!(
        k.first().map(|k| k.codepoints.clone()),
        Some(vec![Some('x')])
    );
    // The sequence is over: a modifier, which compose ignores, is itself
    // and not the sequence again, and so is the next key.
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_LEFTSHIFT), release(KEY_LEFTSHIFT)],
    );
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_Shift_L]);
    feed(&mut input, &node, id, &[press(KEY_E)]);
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_e]);
}

#[test]
fn a_cancelled_or_string_only_sequence_sends_nothing() {
    let (mut input, node, id) = with_compose();
    feed(&mut input, &node, id, &[press(KEY_A), press(KEY_Z)]);
    assert!(keys(&mut input).is_empty(), "cancelled");
    feed(&mut input, &node, id, &[press(KEY_A), press(KEY_S)]);
    assert!(keys(&mut input).is_empty(), "a string with no keysym");
    feed(&mut input, &node, id, &[press(KEY_X)]);
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_x]);
}

#[test]
fn a_held_key_repeats_until_it_is_released() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_A)]);
    keys(&mut input);
    assert!(input.repeating(id));
    let serial = input.repeat_serial(id).unwrap_or_default();
    assert!(input.repeat(id, serial));
    assert!(input.repeat(id, serial));
    let k = keys(&mut input);
    assert_eq!(syms(&k), [keysyms::KEY_a, keysyms::KEY_a]);
    assert!(k.iter().all(|k| k.repeat));
    // Shift pressed while it repeats changes what repeats.
    feed(&mut input, &node, id, &[press(KEY_LEFTSHIFT)]);
    keys(&mut input);
    assert!(input.repeat(id, serial));
    let k = keys(&mut input);
    assert_eq!(syms(&k), [keysyms::KEY_A]);
    assert_eq!(k.first().map(|k| k.mods), Some(Mods::SHIFT));
    feed(&mut input, &node, id, &[release(KEY_A)]);
    assert!(!input.repeating(id));
    assert!(!input.repeat(id, serial), "the timer stops");
    assert!(keys(&mut input).is_empty());
}

#[test]
fn a_second_held_key_takes_the_repeat_over() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_A)]);
    let first = input.repeat_serial(id).unwrap_or_default();
    feed(&mut input, &node, id, &[press(KEY_E)]);
    let second = input.repeat_serial(id).unwrap_or_default();
    keys(&mut input);
    assert_ne!(first, second);
    assert!(!input.repeat(id, first), "the first key's timer stops");
    assert!(input.repeat(id, second));
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_e]);
    // Releasing the first key leaves the second repeating.
    feed(&mut input, &node, id, &[release(KEY_A)]);
    assert!(input.repeating(id));
}

#[test]
fn modifiers_do_not_repeat() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_LEFTSHIFT)]);
    assert!(!input.repeating(id));
}

#[test]
fn the_repeat_settings_default_to_250_and_50_ms_and_stay_below_a_second() {
    let c = InputConfig::default();
    assert_eq!(c.repeat_delay, Duration::from_millis(250));
    assert_eq!(c.repeat_rate, Duration::from_millis(50));
    assert!(c.mouse);
    let c = InputConfig::new(1000, 5000, false);
    assert_eq!(c.repeat_delay, Duration::from_millis(999));
    assert_eq!(c.repeat_rate, Duration::from_millis(999));
    let c = InputConfig::new(400, 30, true);
    assert_eq!(c.repeat_delay, Duration::from_millis(400));
    assert_eq!(c.repeat_rate, Duration::from_millis(30));
}

#[test]
fn keyboards_keep_their_own_modifiers() {
    let nodes = Nodes::default();
    let one = nodes.add("/dev/input/event0", fake::keyboard());
    let two = nodes.add("/dev/input/event1", fake::keyboard());
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.wake_up();
    let a = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("taken"));
    let b = input
        .add(Path::new("/dev/input/event1"))
        .unwrap_or_else(|| panic!("taken"));
    feed(&mut input, &one, a, &[press(KEY_LEFTSHIFT)]);
    feed(&mut input, &two, b, &[press(KEY_A)]);
    let k = keys(&mut input);
    assert_eq!(
        k.last().map(|k| k.keysyms.clone()),
        Some(vec![keysyms::KEY_a])
    );
    assert_eq!(k.last().map(|k| k.mods), Some(Mods::empty()));
}

/// Each event a turn drained, as a key's first keysym or the modifiers a
/// change reports.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Key(u32),
    Mods(Mods),
}

fn seen(input: &mut Input<Nodes>) -> Vec<Seen> {
    input
        .events()
        .filter_map(|e| match e {
            InputEvent::Key(k) => Some(Seen::Key(k.keysyms.first().copied()?)),
            InputEvent::Mods(m) => Some(Seen::Mods(m)),
            InputEvent::Release(_) | InputEvent::Pointer(_) => None,
        })
        .collect()
}

// A press or release that changes the keyboard's effective modifiers is
// reported after the press's own key, with the modifiers after it; a key
// that changes none reports nothing more, and a release no key.
#[test]
fn a_change_of_modifiers_is_reported_with_the_modifiers_after_it() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_LEFTCTRL)]);
    assert_eq!(
        seen(&mut input),
        [Seen::Key(keysyms::KEY_Control_L), Seen::Mods(Mods::CTRL)]
    );
    feed(&mut input, &node, id, &[press(KEY_A), release(KEY_A)]);
    assert_eq!(seen(&mut input), [Seen::Key(keysyms::KEY_a)]);
    feed(&mut input, &node, id, &[press(KEY_LEFTSHIFT)]);
    assert_eq!(
        seen(&mut input),
        [
            Seen::Key(keysyms::KEY_Shift_L),
            Seen::Mods(Mods::CTRL.union(Mods::SHIFT))
        ]
    );
    feed(&mut input, &node, id, &[release(KEY_LEFTCTRL)]);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::SHIFT)]);
    feed(&mut input, &node, id, &[release(KEY_LEFTSHIFT)]);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    // Caps Lock locks at its press; its release changes nothing.
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_CAPSLOCK), release(KEY_CAPSLOCK)],
    );
    assert_eq!(
        seen(&mut input),
        [Seen::Key(keysyms::KEY_Caps_Lock), Seen::Mods(Mods::LOCK)]
    );
}

// Each keyboard reports its own modifiers.
#[test]
fn a_change_of_modifiers_names_its_own_keyboard() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::keyboard(), None),
        ("/dev/input/event1", fake::keyboard(), None),
    ]);
    let [(one, a), (two, b)] = devices.as_slice() else {
        panic!("two keyboards");
    };
    feed(&mut input, one, *a, &[press(KEY_LEFTCTRL)]);
    feed(&mut input, two, *b, &[press(KEY_LEFTALT)]);
    let changes: Vec<_> = input
        .device_events(&no_vt())
        .filter_map(|d| match d.event {
            InputEvent::Mods(m) => Some((d.device, m)),
            _ => None,
        })
        .collect();
    assert_eq!(changes, [(Some(*a), Mods::CTRL), (Some(*b), Mods::ALT)]);
}

// A closed keyboard holds no modifiers: its sleep reports none, and its
// wake-up the lock it keeps, the Ctrl held through a VT switch let go.
#[test]
fn a_keyboard_reports_no_modifiers_while_closed() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_CAPSLOCK),
            release(KEY_CAPSLOCK),
            press(KEY_LEFTCTRL),
        ],
    );
    seen(&mut input);
    input.sleep();
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    assert_eq!(input.mods(), Mods::empty());
    assert_eq!(input.mods_of(&[id]), Mods::empty());
    input.wake_up();
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::LOCK)]);
    assert_eq!(input.mods(), Mods::LOCK);
}

// Neither the kernel's repeat nor vtium's own repeats a change of the
// modifiers: Ctrl held reports itself once, and A held repeats its key
// alone.
#[test]
fn a_repeat_reports_no_change_of_modifiers() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEFTCTRL),
            ev(EV_KEY, KEY_LEFTCTRL, 2),
            ev(EV_KEY, KEY_LEFTCTRL, 2),
            press(KEY_A),
            ev(EV_KEY, KEY_A, 2),
        ],
    );
    let serial = input.repeat_serial(id).unwrap_or_default();
    assert!(input.repeat(id, serial));
    assert!(input.repeat(id, serial));
    assert_eq!(
        seen(&mut input),
        [
            Seen::Key(keysyms::KEY_Control_L),
            Seen::Mods(Mods::CTRL),
            Seen::Key(keysyms::KEY_a),
            Seen::Key(keysyms::KEY_a),
            Seen::Key(keysyms::KEY_a),
        ]
    );
}

// A keyboard watched for the rescue key reports no change of its
// modifiers through either drain.
#[test]
fn a_rescue_keyboard_reports_no_change_of_modifiers() {
    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        fake::keyboard(),
        Some(Disposition::Rescue),
    )]);
    let [(node, id)] = devices.as_slice() else {
        panic!("one device");
    };
    feed(
        &mut input,
        node,
        *id,
        &[press(KEY_LEFTCTRL), release(KEY_LEFTCTRL)],
    );
    assert_eq!(seen(&mut input), []);
    feed(
        &mut input,
        node,
        *id,
        &[press(KEY_LEFTCTRL), release(KEY_LEFTCTRL)],
    );
    let mods: Vec<_> = input
        .device_events(&vt_2())
        .filter(|d| matches!(d.event, InputEvent::Mods(_)))
        .collect();
    assert_eq!(mods, []);
}

// A keyboard moved between the rescue and delivery while it stays open
// reports the modifiers its owner sees: none on the rescue, what it holds
// on delivery.
#[test]
fn a_keyboard_moved_to_or_from_the_rescue_reports_its_modifiers() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_LEFTCTRL)]);
    seen(&mut input);
    input.set_disposition(id, Disposition::Rescue);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    assert!(node.borrow().open);

    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        fake::keyboard(),
        Some(Disposition::Rescue),
    )]);
    let [(node, id)] = devices.as_slice() else {
        panic!("one device");
    };
    feed(&mut input, node, *id, &[press(KEY_LEFTSHIFT)]);
    assert_eq!(seen(&mut input), []);
    input.set_disposition(*id, Disposition::Deliver);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::SHIFT)]);
    assert_eq!(input.mods(), Mods::SHIFT);
}

// A keyboard holding Ctrl that goes away reports no modifiers, whichever
// way it goes: udev removes it, the rules close it, or it ends and a
// wake-up cannot open it again.
#[test]
fn a_keyboard_that_goes_away_reports_no_modifiers() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_LEFTCTRL)]);
    seen(&mut input);
    input.remove(id);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    assert_eq!(input.mods(), Mods::empty());

    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_LEFTCTRL)]);
    seen(&mut input);
    input.set_disposition(id, Disposition::Closed);
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    assert_eq!(input.mods(), Mods::empty());
    assert_eq!(input.mods_of(&[id]), Mods::empty());

    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event0", fake::keyboard());
    let mut input = Input::new(nodes.clone(), keymap(), InputConfig::default());
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("the keyboard is taken"));
    feed(&mut input, &node, id, &[press(KEY_LEFTCTRL)]);
    seen(&mut input);
    node.borrow_mut().end = true;
    input.dispatch(id, Instant::now());
    assert_eq!(seen(&mut input), [Seen::Mods(Mods::empty())]);
    assert_eq!(input.mods(), Mods::empty());
    assert_eq!(input.mods_of(&[id]), Mods::empty());
    nodes.0.borrow_mut().clear();
    input.sleep();
    input.wake_up();
    assert_eq!(seen(&mut input), []);
    assert_eq!(input.mods(), Mods::empty());
}

#[test]
fn sleep_closes_the_devices_and_a_wake_up_drops_held_keys_but_keeps_locks() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_CAPSLOCK),
            release(KEY_CAPSLOCK),
            press(KEY_LEFTSHIFT),
            press(KEY_A),
        ],
    );
    keys(&mut input);
    assert!(input.repeating(id));
    input.sleep();
    assert!(!node.borrow().open);
    assert!(!input.repeating(id));
    assert!(!input.is_awake());
    node.borrow_mut().leds.clear();
    input.wake_up();
    assert!(node.borrow().open);
    assert_eq!(node.borrow().opens, 2);
    // Caps Lock stays locked and lit; the Shift held across the switch is
    // gone.
    assert_eq!(
        node.borrow().leds.last().cloned(),
        Some(vec![
            (LED_NUML, false),
            (LED_CAPSL, true),
            (LED_SCROLLL, false)
        ])
    );
    assert_eq!(input.mods(), Mods::LOCK);
}

#[test]
fn wake_ups_and_sleeps_are_counted() {
    let (mut input, node, _) = with_keyboard();
    input.wake_up();
    input.sleep();
    assert!(node.borrow().open, "one wake-up still holds it");
    input.sleep();
    assert!(!node.borrow().open);
    input.sleep();
    input.wake_up();
    assert!(node.borrow().open, "a sleep too many is not counted");
}

#[test]
fn devices_added_asleep_open_at_the_wake_up() {
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event0", fake::keyboard());
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    let id = input.add(Path::new("/dev/input/event0"));
    assert!(id.is_some());
    assert!(!node.borrow().open);
    input.wake_up();
    assert!(node.borrow().open);
    assert_eq!(input.find(Path::new("/dev/input/event0")), id);
}

#[test]
fn a_device_that_is_no_keyboard_or_pointer_is_dropped() {
    let nodes = Nodes::default();
    let lid = nodes.add("/dev/input/event3", super::evdev::Bits::default());
    let pad = nodes.add("/dev/input/event4", fake::mouse());
    let broken = nodes.add("/dev/input/event5", fake::keyboard());
    broken.borrow_mut().unprobeable = true;
    let mut input = Input::new(nodes, keymap(), InputConfig::new(0, 0, false));
    input.wake_up();
    assert!(input.add(Path::new("/dev/input/event3")).is_none());
    assert!(
        input.add(Path::new("/dev/input/event4")).is_none(),
        "mouse off"
    );
    assert!(input.add(Path::new("/dev/input/event5")).is_none());
    assert!(
        input.add(Path::new("/dev/input/event9")).is_none(),
        "no such node"
    );
    for node in [lid, pad, broken] {
        assert!(!node.borrow().open);
    }
}

#[test]
fn a_device_that_ends_is_closed_and_opened_again_at_the_wake_up() {
    let (mut input, node, id) = with_keyboard();
    node.borrow_mut().end = true;
    input.dispatch(id, Instant::now());
    assert!(!node.borrow().open);
    input.sleep();
    input.wake_up();
    assert!(node.borrow().open);
    input.hangup(id);
    assert!(!node.borrow().open);
    input.remove(id);
    input.sleep();
    input.wake_up();
    assert!(!node.borrow().open, "removed");
}

#[test]
fn a_program_sets_the_leds_of_every_keyboard() {
    let (mut input, node, _) = with_keyboard();
    node.borrow_mut().leds.clear();
    input.set_leds(Leds {
        num_lock: true,
        caps_lock: false,
        scroll_lock: true,
    });
    assert_eq!(
        node.borrow().leds.last().cloned(),
        Some(vec![
            (LED_NUML, true),
            (LED_CAPSL, false),
            (LED_SCROLLL, true)
        ])
    );
}

fn pointers(input: &mut Input<Nodes>) -> Vec<PointerEvent> {
    input
        .events()
        .filter_map(|e| match e {
            InputEvent::Pointer(p) => Some(p),
            InputEvent::Key(_) | InputEvent::Release(_) | InputEvent::Mods(_) => None,
        })
        .collect()
}

fn with_device(bits: super::evdev::Bits) -> (Input<Nodes>, Rc<RefCell<Node>>, super::DeviceId) {
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event7", bits);
    node.borrow_mut().range = (Range { min: 0, max: 1000 }, Range { min: 0, max: 500 });
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.set_pointer_max(100, 50);
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event7"))
        .unwrap_or_else(|| panic!("the pointer is taken"));
    (input, node, id)
}

#[test]
fn a_mouse_moves_within_the_screen_and_reports_its_buttons_and_wheel() {
    let (mut input, node, id) = with_device(fake::mouse());
    feed(
        &mut input,
        &node,
        id,
        &[
            ev(EV_REL, REL_X, 30),
            ev(EV_REL, REL_Y, -5),
            ev(EV_SYN, 0, 0),
            ev(EV_KEY, BTN_LEFT, 1),
            ev(EV_REL, REL_X, 500),
            ev(EV_KEY, BTN_LEFT, 0),
            ev(EV_REL, REL_WHEEL, -1),
            ev(EV_KEY, BTN_RIGHT, 1),
        ],
    );
    assert_eq!(
        pointers(&mut input),
        [
            PointerEvent::Moved {
                x: 30,
                y: 0,
                held: None
            },
            PointerEvent::Moved {
                x: 30,
                y: 0,
                held: None
            },
            PointerEvent::Sync,
            PointerEvent::Button {
                button: Button::Left,
                pressed: true,
                double_click: false
            },
            PointerEvent::Moved {
                x: 100,
                y: 0,
                held: Some(Button::Left)
            },
            PointerEvent::Button {
                button: Button::Left,
                pressed: false,
                double_click: false
            },
            PointerEvent::Wheel(-1),
            PointerEvent::Button {
                button: Button::Right,
                pressed: true,
                double_click: false
            },
        ]
    );
}

#[test]
fn a_turn_reads_a_bounded_number_of_events_and_the_next_reads_on() {
    let (mut input, node, id) = with_device(fake::mouse());
    let per_turn = crate::INPUT_READS_PER_TURN * super::evdev::EVENTS_PER_READ;
    let flood = vec![ev(EV_REL, REL_X, 1); per_turn + 100];
    feed(&mut input, &node, id, &flood);
    assert_eq!(pointers(&mut input).len(), per_turn);
    assert_eq!(node.borrow().queue.len(), 100);
    input.dispatch(id, Instant::now());
    assert_eq!(pointers(&mut input).len(), 100);
}

#[test]
fn two_left_presses_within_half_a_second_are_a_double_click() {
    let (mut input, node, id) = with_device(fake::mouse());
    let start = Instant::now();
    let click = |input: &mut Input<Nodes>, at: Duration| {
        node.borrow_mut()
            .queue
            .extend([ev(EV_KEY, BTN_LEFT, 1), ev(EV_KEY, BTN_LEFT, 0)]);
        input.dispatch(id, start + at);
    };
    click(&mut input, Duration::ZERO);
    click(&mut input, Duration::from_millis(499));
    click(&mut input, Duration::from_millis(1100));
    let doubles: Vec<bool> = pointers(&mut input)
        .into_iter()
        .filter_map(|p| match p {
            PointerEvent::Button {
                pressed: true,
                double_click,
                ..
            } => Some(double_click),
            _ => None,
        })
        .collect();
    assert_eq!(doubles, [false, true, false]);
}

#[test]
fn a_touchscreen_puts_the_pointer_where_it_is_touched() {
    let (mut input, node, id) = with_device(fake::touch(true));
    feed(
        &mut input,
        &node,
        id,
        &[ev(EV_ABS, ABS_X, 500), ev(EV_ABS, ABS_Y, 500)],
    );
    assert_eq!(
        pointers(&mut input),
        [
            PointerEvent::Moved {
                x: 50,
                y: 0,
                held: None
            },
            PointerEvent::Moved {
                x: 50,
                y: 50,
                held: None
            },
        ]
    );
}

#[test]
fn an_axis_with_no_range_moves_nothing() {
    let (mut input, node, id) = with_device(fake::touch(true));
    node.borrow_mut().range = (Range { min: 5, max: 5 }, Range { min: 0, max: 500 });
    input.sleep();
    input.wake_up();
    feed(
        &mut input,
        &node,
        id,
        &[ev(EV_ABS, ABS_X, 5), ev(EV_ABS, ABS_Y, 250)],
    );
    assert_eq!(
        pointers(&mut input),
        [PointerEvent::Moved {
            x: 0,
            y: 25,
            held: None
        }]
    );
}

#[test]
fn a_touchpad_moves_the_pointer_by_the_fingers_motion() {
    let (mut input, node, id) = with_device(fake::touch(false));
    feed(
        &mut input,
        &node,
        id,
        &[
            ev(EV_KEY, BTN_TOUCH, 1),
            ev(EV_ABS, ABS_X, 400),
            ev(EV_ABS, ABS_X, 420),
            ev(EV_KEY, BTN_TOUCH, 0),
            ev(EV_ABS, ABS_X, 900),
            ev(EV_KEY, BTN_TOUCH, 1),
            ev(EV_ABS, ABS_X, 10),
            ev(EV_ABS, ABS_X, 15),
            ev(EV_ABS, ABS_X, 0),
        ],
    );
    let xs: Vec<i32> = pointers(&mut input)
        .into_iter()
        .filter_map(|p| match p {
            PointerEvent::Moved { x, .. } => Some(x),
            _ => None,
        })
        .collect();
    // The first touch starts where the pointer is, the lifted finger moves
    // nothing, and the next touch goes on from there.
    assert_eq!(xs, [0, 20, 20, 20, 25, 10]);
}

// A button held across a VT switch is released on the other VT: the
// devices' sleep lets it go, so the next move is no drag.
#[test]
fn sleep_lets_go_of_a_held_button() {
    let (mut input, node, id) = with_device(fake::mouse());
    feed(&mut input, &node, id, &[ev(EV_KEY, BTN_LEFT, 1)]);
    pointers(&mut input);
    input.sleep();
    input.wake_up();
    feed(&mut input, &node, id, &[ev(EV_REL, REL_X, 5)]);
    assert_eq!(
        pointers(&mut input),
        [PointerEvent::Moved {
            x: 5,
            y: 0,
            held: None
        }]
    );
}

// A finger on the touchpad across a VT switch: its first position after
// the wake-up is the new origin, so the pointer goes on from where it is
// rather than jumping by the finger's travel on the other VT.
#[test]
fn a_touchpads_finger_takes_its_origin_afresh_after_a_sleep() {
    let (mut input, node, id) = with_device(fake::touch(false));
    feed(
        &mut input,
        &node,
        id,
        &[
            ev(EV_KEY, BTN_TOUCH, 1),
            ev(EV_ABS, ABS_X, 10),
            ev(EV_ABS, ABS_X, 30),
        ],
    );
    pointers(&mut input);
    input.sleep();
    input.wake_up();
    feed(
        &mut input,
        &node,
        id,
        &[ev(EV_ABS, ABS_X, 80), ev(EV_ABS, ABS_X, 90)],
    );
    let xs: Vec<i32> = pointers(&mut input)
        .into_iter()
        .filter_map(|p| match p {
            PointerEvent::Moved { x, .. } => Some(x),
            _ => None,
        })
        .collect();
    assert_eq!(xs, [20, 30]);
}

#[test]
fn the_pointer_hides_after_its_timeout_without_a_frame() {
    let (mut input, node, id) = with_device(fake::mouse());
    let start = Instant::now();
    node.borrow_mut().queue.push_back(ev(EV_SYN, 0, 0));
    input.dispatch(id, start);
    pointers(&mut input);
    assert_eq!(
        input.hide(start + super::POINTER_HIDE / 2),
        Some(super::POINTER_HIDE / 2)
    );
    assert_eq!(input.hide(start + super::POINTER_HIDE), None);
    assert_eq!(pointers(&mut input), [PointerEvent::Hide]);
    assert_eq!(input.hide(start + super::POINTER_HIDE * 2), None);
    assert!(pointers(&mut input).is_empty(), "hidden once");
}

// A keyboard's frames are no pointer events: typing neither ends a
// pointer frame nor keeps the pointer from hiding.
#[test]
fn a_keyboards_frames_leave_the_pointers_hide_alone() {
    let nodes = Nodes::default();
    let mouse = nodes.add("/dev/input/event7", fake::mouse());
    let keyboard = nodes.add("/dev/input/event0", fake::keyboard());
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.wake_up();
    let m = input
        .add(Path::new("/dev/input/event7"))
        .unwrap_or_else(|| panic!("the pointer is taken"));
    let k = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("the keyboard is taken"));
    let start = Instant::now();
    mouse.borrow_mut().queue.push_back(ev(EV_SYN, 0, 0));
    input.dispatch(m, start);
    assert_eq!(pointers(&mut input), [PointerEvent::Sync]);
    let half = start + super::POINTER_HIDE / 2;
    keyboard.borrow_mut().queue.extend([
        press(KEY_A),
        ev(EV_SYN, 0, 0),
        release(KEY_A),
        ev(EV_SYN, 0, 0),
    ]);
    input.dispatch(k, half);
    assert!(pointers(&mut input).is_empty(), "no pointer frame");
    assert_eq!(input.hide(start + super::POINTER_HIDE), None);
    assert_eq!(pointers(&mut input), [PointerEvent::Hide]);
}

#[test]
fn the_pointers_hide_timeout_is_configured_and_0_never_hides() {
    let five = Duration::from_secs(5);
    for (hide, at) in [(Some(five), Some(five)), (None, None)] {
        let nodes = Nodes::default();
        let node = nodes.add("/dev/input/event7", fake::mouse());
        let config = InputConfig::default().with_pointer_hide(hide);
        let mut input = Input::new(nodes, keymap(), config);
        input.wake_up();
        let id = input
            .add(Path::new("/dev/input/event7"))
            .unwrap_or_else(|| panic!("the pointer is taken"));
        let start = Instant::now();
        node.borrow_mut().queue.push_back(ev(EV_SYN, 0, 0));
        input.dispatch(id, start);
        pointers(&mut input);
        assert_eq!(
            input.hide(start + five / 2),
            at.map(|_| five / 2),
            "{hide:?}"
        );
        assert_eq!(input.hide(start + super::POINTER_HIDE * 10), None);
        let want: &[PointerEvent] = if at.is_some() {
            &[PointerEvent::Hide]
        } else {
            &[]
        };
        assert_eq!(pointers(&mut input), want, "{hide:?}");
    }
    assert_eq!(
        InputConfig::default().pointer_hide,
        Some(super::POINTER_HIDE)
    );
}

#[test]
fn the_locale_comes_from_the_environment_as_locale_7_says() {
    // The test process's own environment: only check the fallback order's
    // result is one of the variables or C.
    let found = super::locale();
    let candidates: Vec<String> = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .chain([String::from("C")])
        .collect();
    assert_eq!(candidates.first(), Some(&found));
}

#[test]
fn the_built_in_keymap_compiles_and_a_broken_one_falls_back() {
    assert!(Keymap::fallback().is_some());
    assert!(Keymap::from_text("xkb_keymap { broken").is_none());
    let config = super::KeymapConfig {
        keymap_file: Some("/nonexistent/vtium.xkb".into()),
        layout: String::from("no\0such"),
        ..super::KeymapConfig::default()
    };
    // The file and the names fail; the system's default or the built-in
    // keymap is taken.
    assert!(Keymap::new(&config).is_some());
}

// The hide timer in a real loop: a frame arms it, and its firing reports
// the hide.
#[test]
fn the_hide_timer_fires_in_the_loop() {
    struct Data {
        input: Input<Nodes>,
    }
    let mut lp: calloop::EventLoop<'static, Data> =
        calloop::EventLoop::try_new().unwrap_or_else(|e| panic!("a loop: {e}"));
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event7", fake::mouse());
    let config = InputConfig::default().with_pointer_hide(Some(Duration::from_millis(30)));
    let mut input = Input::new(nodes, keymap(), config);
    input.insert(&lp.handle(), |d: &mut Data| Some(&mut d.input));
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event7"))
        .unwrap_or_else(|| panic!("the pointer is taken"));
    let mut data = Data { input };
    node.borrow_mut().queue.push_back(ev(EV_REL, REL_X, 3));
    node.borrow_mut().queue.push_back(ev(EV_SYN, 0, 0));
    data.input.dispatch(id, Instant::now());
    pointers(&mut data.input);
    let end = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    while seen.is_empty() && Instant::now() < end {
        lp.dispatch(Some(Duration::from_millis(10)), &mut data)
            .unwrap_or_else(|e| panic!("dispatch: {e}"));
        seen = pointers(&mut data.input);
    }
    assert_eq!(seen, [PointerEvent::Hide]);
}

/// A stand-in device and its handle in the input.
type Added = (Rc<RefCell<Node>>, super::DeviceId);

/// The input awake with the stand-in devices at `nodes`, each added with
/// its disposition.
fn with_devices(
    devices: &[(&str, super::evdev::Bits, Option<Disposition>)],
) -> (Input<Nodes>, Vec<Added>) {
    let nodes = Nodes::default();
    let made: Vec<_> = devices
        .iter()
        .map(|(path, bits, _)| nodes.add(path, *bits))
        .collect();
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.set_pointer_max(100, 50);
    input.wake_up();
    let ids = devices
        .iter()
        .zip(made)
        .map(|((path, _, disposition), node)| {
            let id = match disposition {
                Some(d) => input.add_with(Path::new(path), *d),
                None => input.add(Path::new(path)),
            };
            (node, id.unwrap_or_else(|| panic!("{path} is kept")))
        })
        .collect();
    (input, ids)
}

/// A VT with no own key, so a watched device's keys are all dropped.
fn no_vt() -> FakeVt {
    FakeVt::new()
}

/// VT 2, ours: Ctrl+Alt+F2 is its own key.
fn vt_2() -> FakeVt {
    let mut vt = FakeVt::with_number(2);
    assert!(vt.activate().is_ok());
    vt.events().for_each(drop);
    vt
}

fn tagged_keys(input: &mut Input<Nodes>, vt: &dyn Vt) -> Vec<(Option<super::DeviceId>, bool, u32)> {
    input
        .device_events(vt)
        .filter_map(
            |DeviceInput {
                 device,
                 rescue,
                 event,
             }| match event {
                InputEvent::Key(k) => Some((device, rescue, k.keysyms.first().copied()?)),
                InputEvent::Release(_) | InputEvent::Pointer(_) | InputEvent::Mods(_) => None,
            },
        )
        .collect()
}

#[test]
fn each_event_names_the_device_it_came_from() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::keyboard(), None),
        ("/dev/input/event1", fake::keyboard(), None),
        ("/dev/input/event2", fake::mouse(), None),
    ]);
    let [(one, a), (two, b), (mouse, m)] = devices.as_slice() else {
        panic!("three devices");
    };
    feed(&mut input, one, *a, &[press(KEY_A)]);
    feed(&mut input, two, *b, &[press(KEY_S)]);
    feed(&mut input, mouse, *m, &[ev(EV_REL, REL_X, 5)]);
    let tagged: Vec<_> = input
        .device_events(&no_vt())
        .map(|d| (d.device, d.rescue, matches!(d.event, InputEvent::Key(_))))
        .collect();
    assert_eq!(
        tagged,
        [
            (Some(*a), false, true),
            (Some(*b), false, true),
            (Some(*m), false, false)
        ]
    );
    // One drain empties the other.
    assert_eq!(input.events().count(), 0);
    feed(&mut input, one, *a, &[press(KEY_A)]);
    assert_eq!(input.events().count(), 1);
    assert_eq!(input.device_events(&no_vt()).count(), 0);
}

#[test]
fn a_closed_device_is_never_opened() {
    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        fake::keyboard(),
        Some(Disposition::Closed),
    )]);
    let [(node, id)] = devices.as_slice() else {
        panic!("one device");
    };
    input.sleep();
    input.wake_up();
    assert_eq!(node.borrow().opens, 0);
    assert_eq!(input.disposition(*id), Some(Disposition::Closed));
    // Delivered from now on, it opens; closed again, it closes.
    input.set_disposition(*id, Disposition::Deliver);
    assert!(node.borrow().open);
    feed(&mut input, node, *id, &[press(KEY_A)]);
    assert_eq!(syms(&keys(&mut input)), [keysyms::KEY_a]);
    input.set_disposition(*id, Disposition::Closed);
    assert!(!node.borrow().open);
    input.sleep();
    input.wake_up();
    assert!(!node.borrow().open);
}

#[test]
fn a_rescue_keyboard_delivers_nothing_but_its_keys_marked_for_the_rescue() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::keyboard(), None),
        (
            "/dev/input/event1",
            fake::keyboard(),
            Some(Disposition::Rescue),
        ),
    ]);
    let [(typing, a), (watched, w)] = devices.as_slice() else {
        panic!("two devices");
    };
    assert!(watched.borrow().open);
    feed(
        &mut input,
        watched,
        *w,
        &[press(KEY_LEFTCTRL), press(KEY_LEFTALT), press(KEY_F2)],
    );
    assert!(keys(&mut input).is_empty());
    feed(&mut input, watched, *w, &[press(KEY_F2)]);
    assert_eq!(
        tagged_keys(&mut input, &vt_2()),
        [(Some(*w), true, keysyms::KEY_XF86Switch_VT_2)]
    );
    // Its modifiers are its own and reach no one else's keys.
    assert_eq!(input.mods(), Mods::empty());
    assert_eq!(input.mods_of(&[*w]), Mods::CTRL.union(Mods::ALT));
    feed(&mut input, typing, *a, &[press(KEY_A)]);
    assert_eq!(
        tagged_keys(&mut input, &no_vt()),
        [(Some(*a), false, keysyms::KEY_a)]
    );
    // A program's LEDs reach the typing keyboard alone.
    typing.borrow_mut().leds.clear();
    watched.borrow_mut().leds.clear();
    input.set_leds(Leds {
        num_lock: true,
        caps_lock: false,
        scroll_lock: false,
    });
    assert_eq!(typing.borrow().leds.len(), 1);
    assert!(watched.borrow().leds.is_empty());
}

#[test]
fn modifiers_and_leds_follow_a_set_of_keyboards() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::keyboard(), None),
        ("/dev/input/event1", fake::keyboard(), None),
    ]);
    let [(one, a), (two, b)] = devices.as_slice() else {
        panic!("two devices");
    };
    feed(&mut input, one, *a, &[press(KEY_LEFTSHIFT)]);
    feed(&mut input, two, *b, &[press(KEY_LEFTALT)]);
    assert_eq!(input.mods_of(&[*a]), Mods::SHIFT);
    assert_eq!(input.mods_of(&[*b]), Mods::ALT);
    assert_eq!(input.mods_of(&[*a, *b]), Mods::SHIFT.union(Mods::ALT));
    assert_eq!(input.mods_of(&[]), Mods::empty());
    one.borrow_mut().leds.clear();
    two.borrow_mut().leds.clear();
    let leds = Leds {
        num_lock: false,
        caps_lock: true,
        scroll_lock: false,
    };
    input.set_leds_of(&[*b], leds);
    assert!(one.borrow().leds.is_empty());
    assert_eq!(
        two.borrow().leds.last().cloned(),
        Some(vec![
            (LED_NUML, false),
            (LED_CAPSL, true),
            (LED_SCROLLL, false)
        ])
    );
}

#[test]
fn a_pointer_moves_within_its_own_screen() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::mouse(), None),
        ("/dev/input/event1", fake::mouse(), None),
    ]);
    let [(one, a), (two, b)] = devices.as_slice() else {
        panic!("two devices");
    };
    input.set_pointer_max_of(&[*b], 400, 300);
    feed(&mut input, one, *a, &[ev(EV_REL, REL_X, 500)]);
    feed(&mut input, two, *b, &[ev(EV_REL, REL_X, 500)]);
    assert_eq!(
        pointers(&mut input),
        [
            PointerEvent::Moved {
                x: 100,
                y: 0,
                held: None
            },
            PointerEvent::Moved {
                x: 400,
                y: 0,
                held: None
            },
        ]
    );
}

#[test]
fn a_delivered_device_is_taken_where_the_default_rule_drops_it() {
    let nodes = Nodes::default();
    nodes.add("/dev/input/event0", fake::volume_keys());
    let mut input = Input::new(nodes, keymap(), InputConfig::default());
    input.wake_up();
    assert_eq!(input.add(Path::new("/dev/input/event0")), None);
    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        fake::volume_keys(),
        Some(Disposition::Deliver),
    )]);
    let [(node, id)] = devices.as_slice() else {
        panic!("one device");
    };
    feed(&mut input, node, *id, &[press(115)]);
    assert_eq!(
        keys(&mut input)
            .iter()
            .map(|k| k.keycode)
            .collect::<Vec<_>>(),
        [115 + 8]
    );
}

#[test]
fn a_repeat_comes_from_its_device_and_a_watched_ones_is_dropped() {
    let (mut input, devices) = with_devices(&[
        ("/dev/input/event0", fake::keyboard(), None),
        (
            "/dev/input/event1",
            fake::keyboard(),
            Some(Disposition::Rescue),
        ),
    ]);
    let vt = vt_2();
    let [(typing, a), (watched, w)] = devices.as_slice() else {
        panic!("two devices");
    };
    feed(&mut input, typing, *a, &[press(KEY_A)]);
    input.device_events(&vt).for_each(drop);
    let serial = input.repeat_serial(*a).unwrap_or_default();
    assert!(input.repeat(*a, serial));
    assert_eq!(
        tagged_keys(&mut input, &vt),
        [(Some(*a), false, keysyms::KEY_a)]
    );
    // The watched keyboard's own key, held: its press is the rescue, its
    // repeat nothing.
    feed(
        &mut input,
        watched,
        *w,
        &[press(KEY_LEFTCTRL), press(KEY_LEFTALT), press(KEY_F2)],
    );
    assert_eq!(
        tagged_keys(&mut input, &vt),
        [(Some(*w), true, keysyms::KEY_XF86Switch_VT_2)]
    );
    let serial = input.repeat_serial(*w).unwrap_or_default();
    assert!(input.repeat(*w, serial));
    assert!(tagged_keys(&mut input, &vt).is_empty());
}

// ADR-1401 D3, "Who switches VTs": a keyboard the rules ignore, or one tied
// to a head on another VT, switches VTs under the open policy, and the
// shown VT's own key is the rescue key (D9). A watched device delivers
// those keys alone, while the VT is ours, and vtium decides what each does.
#[test]
fn a_watched_device_delivers_the_vt_keys_alone() {
    // An OTP token the rules ignore: what it types is dropped, and so is
    // every chord but the VT keys.
    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        fake::keyboard(),
        Some(Disposition::Rescue),
    )]);
    let [(token, t)] = devices.as_slice() else {
        panic!("one device");
    };
    let vt = vt_2();
    feed(
        &mut input,
        token,
        *t,
        &[press(KEY_A), press(KEY_S), press(KEY_E), press(KEY_X)],
    );
    feed(
        &mut input,
        token,
        *t,
        &[
            press(KEY_LEFTCTRL),
            press(KEY_LEFTALT),
            press(KEY_A),
            press(KEY_F2),
            press(KEY_F3),
        ],
    );
    assert_eq!(
        tagged_keys(&mut input, &vt),
        [
            (Some(*t), true, keysyms::KEY_XF86Switch_VT_2),
            (Some(*t), true, keysyms::KEY_XF86Switch_VT_3)
        ]
    );
    feed(&mut input, token, *t, &[press(KEY_F2)]);
    assert!(tagged_keys(&mut input, &no_vt()).is_empty());
    feed(&mut input, token, *t, &[press(KEY_A)]);
    assert!(keys(&mut input).is_empty());
}

/// A keyboard with a mouse's axes and buttons on the same node, as
/// keyboard A's own node carries its wheels.
fn keyboard_with_pointer() -> super::evdev::Bits {
    let mut bits = fake::keyboard();
    let mouse = fake::mouse();
    for (word, extra) in bits.ev.iter_mut().zip(mouse.ev) {
        *word |= extra;
    }
    if let (Some(key), Some(extra)) = (bits.key.as_mut(), mouse.key) {
        for (word, extra) in key.iter_mut().zip(extra) {
            *word |= extra;
        }
    }
    bits.rel = mouse.rel;
    bits
}

#[test]
fn a_watched_device_sends_no_pointer_events_and_arms_no_hide() {
    let (mut input, devices) = with_devices(&[(
        "/dev/input/event0",
        keyboard_with_pointer(),
        Some(Disposition::Rescue),
    )]);
    let [(node, id)] = devices.as_slice() else {
        panic!("one device");
    };
    feed(
        &mut input,
        node,
        *id,
        &[
            ev(EV_REL, REL_X, 5),
            ev(EV_KEY, BTN_LEFT, 1),
            ev(EV_REL, REL_WHEEL, 1),
            ev(EV_SYN, 0, 0),
        ],
    );
    assert!(
        input
            .device_events(&no_vt())
            .all(|d| !matches!(d.event, InputEvent::Pointer(_)))
    );
    assert_eq!(input.hide_at, None);
}

/// The `xkb-*` keys that vtium.conf sets keep their values, and the
/// system's names fill the keys left unset. The system's variant comes only
/// with the system's layout.
// covers: platform.system-layout
#[test]
fn vtium_confs_xkb_keys_win_over_the_systems_names() {
    let system = RuleNames {
        model: "pc105".to_owned(),
        layout: "de".to_owned(),
        variant: "nodeadkeys".to_owned(),
        options: "compose:ralt".to_owned(),
    };
    let set = KeymapConfig {
        layout: "us".to_owned(),
        options: "ctrl:nocaps".to_owned(),
        ..KeymapConfig::default()
    }
    .follow(&system);
    assert_eq!(
        (
            set.model.as_str(),
            set.layout.as_str(),
            set.variant.as_str(),
            set.options.as_str()
        ),
        ("pc105", "us", "", "ctrl:nocaps")
    );
    let unset = KeymapConfig::default().follow(&system);
    assert_eq!(
        (
            unset.model.as_str(),
            unset.layout.as_str(),
            unset.variant.as_str(),
            unset.options.as_str()
        ),
        ("pc105", "de", "nodeadkeys", "compose:ralt")
    );
    let variant_only = KeymapConfig {
        variant: "dvorak".to_owned(),
        ..KeymapConfig::default()
    }
    .follow(&system);
    assert_eq!(
        (variant_only.layout.as_str(), variant_only.variant.as_str()),
        ("de", "dvorak")
    );
}

/// What a key did, as the input reports it: pressed or repeated, or let
/// go.
#[derive(Debug, PartialEq, Eq)]
enum Did {
    Press(u32, Mods),
    Release(u32, Mods),
}

/// The keys' presses and releases, each with its keysym and modifiers.
fn presses_and_releases(input: &mut Input<Nodes>) -> Vec<Did> {
    input
        .events()
        .filter_map(|e| match e {
            InputEvent::Key(k) => Some(Did::Press(*k.keysyms.first()?, k.mods)),
            InputEvent::Release(k) => Some(Did::Release(*k.keysyms.first()?, k.mods)),
            InputEvent::Pointer(_) | InputEvent::Mods(_) => None,
        })
        .collect()
}

// ADR-1005 D8: a keyboard reports a release for every key whose press it
// reported, with the keysyms the press reported and the modifiers after
// the release, so the release of Shift holds no Shift.
// covers: input.kitty-keyboard
#[test]
fn a_release_is_reported_for_every_reported_press() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_A), release(KEY_A)]);
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_a, Mods::empty()),
            Did::Release(keysyms::KEY_a, Mods::empty()),
        ]
    );
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEFTSHIFT),
            press(KEY_A),
            release(KEY_LEFTSHIFT),
            release(KEY_A),
        ],
    );
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_Shift_L, Mods::SHIFT),
            Did::Press(keysyms::KEY_A, Mods::SHIFT),
            Did::Release(keysyms::KEY_Shift_L, Mods::empty()),
            Did::Release(keysyms::KEY_A, Mods::empty()),
        ]
    );
    // a release with no press reported before it is not reported
    feed(&mut input, &node, id, &[release(KEY_E)]);
    assert_eq!(presses_and_releases(&mut input), []);
}

// ADR-1005 D8: a key whose press went into a compose sequence reports no
// release, since no press was reported for it; the key that ends the
// sequence reports its release with the keysym the sequence made.
// covers: input.kitty-keyboard
#[test]
fn a_key_that_went_into_a_compose_sequence_reports_no_release() {
    let (mut input, node, id) = with_compose();
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_A), release(KEY_A), press(KEY_E), release(KEY_E)],
    );
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_x, Mods::empty()),
            Did::Release(keysyms::KEY_x, Mods::empty()),
        ]
    );
}

// A wake-up starts a fresh keyboard state, so a key held across a sleep
// reports no release: its press belongs to the state before.
#[test]
fn a_key_held_across_a_sleep_reports_no_release() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_A)]);
    presses_and_releases(&mut input);
    input.sleep();
    input.wake_up();
    feed(&mut input, &node, id, &[release(KEY_A)]);
    assert_eq!(presses_and_releases(&mut input), []);
}

// ADR-1005 D8: NumLock is a modifier while it is on, which the kitty
// keyboard protocol reports and mode 1035 reads.
// covers: DEC-1035
#[test]
fn num_lock_is_a_modifier_while_it_is_on() {
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_KP1), release(KEY_KP1)]);
    assert_eq!(
        presses_and_releases(&mut input).first(),
        Some(&Did::Press(keysyms::KEY_KP_End, Mods::empty()))
    );
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_NUMLOCK),
            release(KEY_NUMLOCK),
            press(KEY_KP1),
            release(KEY_KP1),
        ],
    );
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_Num_Lock, Mods::NUM),
            Did::Release(keysyms::KEY_Num_Lock, Mods::NUM),
            Did::Press(keysyms::KEY_KP_1, Mods::NUM),
            Did::Release(keysyms::KEY_KP_1, Mods::NUM),
        ]
    );
    assert_eq!(input.mods(), Mods::NUM);
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_NUMLOCK), release(KEY_NUMLOCK)],
    );
    assert_eq!(input.mods(), Mods::empty(), "NumLock off again");
}

/// The input awake with one keyboard at /dev/input/event0, under the
/// built-in keymap with each of `edits` made to its text.
fn with_edited_keymap(
    edits: &[(&str, &str)],
) -> (Input<Nodes>, Rc<RefCell<Node>>, super::DeviceId) {
    let mut text = String::from(include_str!("fallback.xkb"));
    for (from, to) in edits {
        assert!(text.contains(from), "the built-in keymap holds {from}");
        text = text.replace(from, to);
    }
    let keymap = Keymap::from_text(&text).unwrap_or_else(|| panic!("the keymap compiles"));
    let nodes = Nodes::default();
    let node = nodes.add("/dev/input/event0", fake::keyboard());
    let mut input = Input::new(nodes, keymap, InputConfig::default());
    input.wake_up();
    let id = input
        .add(Path::new("/dev/input/event0"))
        .unwrap_or_else(|| panic!("the keyboard is taken"));
    (input, node, id)
}

// ADR-1005 D8: in the built-in keymap, as in xkeyboard-config's default
// keymaps, Meta shares Alt's modifier and Hyper shares Super's, so a key
// reports Alt and Super alone, and never Meta beside Alt.
// covers: input.kitty-keyboard
#[test]
fn the_default_keymap_reports_neither_hyper_nor_meta() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEFTALT),
            press(KEY_A),
            release(KEY_A),
            release(KEY_LEFTALT),
            press(KEY_LEFTSHIFT),
            press(KEY_LEFTALT),
            release(KEY_LEFTALT),
            release(KEY_LEFTSHIFT),
            press(KEY_LEFTMETA),
        ],
    );
    let did = presses_and_releases(&mut input);
    assert_eq!(did.get(1), Some(&Did::Press(keysyms::KEY_a, Mods::ALT)));
    assert_eq!(
        did.get(5),
        Some(&Did::Press(
            keysyms::KEY_Meta_L,
            Mods::SHIFT.union(Mods::ALT)
        )),
        "Shift+Alt makes Meta_L, which sets Alt's modifier"
    );
    assert_eq!(
        did.last(),
        Some(&Did::Press(keysyms::KEY_Super_L, Mods::LOGO))
    );
}

// ADR-1005 D8: a keymap that gives Hyper a modifier of its own, Mod3,
// which neither Alt nor Super uses, reports Hyper while a Hyper key is
// held, and not Super.
// covers: input.kitty-keyboard
#[test]
fn a_keymap_that_gives_hyper_a_modifier_of_its_own_reports_it() {
    let (mut input, node, id) = with_edited_keymap(&[
        ("key <LWIN> { [ Super_L ] };", "key <LWIN> { [ Hyper_L ] };"),
        (
            "modifier_map Mod4 { <LWIN> };",
            "modifier_map Mod3 { <LWIN> };",
        ),
        (
            "modifier_map Mod4 { <HYPR> };",
            "modifier_map Mod3 { <HYPR> };",
        ),
    ]);
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEFTMETA),
            press(KEY_A),
            release(KEY_A),
            release(KEY_LEFTMETA),
        ],
    );
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_Hyper_L, Mods::HYPER),
            Did::Press(keysyms::KEY_a, Mods::HYPER),
            Did::Release(keysyms::KEY_a, Mods::HYPER),
            Did::Release(keysyms::KEY_Hyper_L, Mods::empty()),
        ]
    );
}

// ADR-1005 D8: a keymap that gives Meta a modifier of its own, Mod3,
// reports Meta while a Meta key is held, and not Alt.
// covers: input.kitty-keyboard
#[test]
fn a_keymap_that_gives_meta_a_modifier_of_its_own_reports_it() {
    let (mut input, node, id) = with_edited_keymap(&[
        (
            "key <LALT> { [ Alt_L, Meta_L ] };",
            "key <LALT> { [ Alt_L, Alt_L ] };",
        ),
        (
            "key <RALT> { type= \"TWO_LEVEL\", symbols[Group1]= [ Alt_R, Meta_R ] };",
            "key <RALT> { type= \"TWO_LEVEL\", symbols[Group1]= [ Alt_R, Alt_R ] };",
        ),
        ("key <LWIN> { [ Super_L ] };", "key <LWIN> { [ Meta_L ] };"),
        (
            "modifier_map Mod4 { <LWIN> };",
            "modifier_map Mod3 { <LWIN> };",
        ),
        (
            "modifier_map Mod1 { <META> };",
            "modifier_map Mod3 { <META> };",
        ),
    ]);
    feed(
        &mut input,
        &node,
        id,
        &[press(KEY_LEFTMETA), press(KEY_A), press(KEY_LEFTALT)],
    );
    assert_eq!(
        presses_and_releases(&mut input),
        [
            Did::Press(keysyms::KEY_Meta_L, Mods::META),
            Did::Press(keysyms::KEY_a, Mods::META),
            Did::Press(keysyms::KEY_Alt_L, Mods::META.union(Mods::ALT)),
        ]
    );
}

// ADR-1005 D8: a layout change while a key is held keeps that key's
// press, so its release is still reported. The kernel's key codes do not
// change with the layout, and the release carries the keysyms its press
// carried.
// covers: input.kitty-keyboard
#[test]
fn a_key_held_across_a_layout_change_reports_its_release() {
    const KEY_UP: u16 = 103;
    let (mut input, node, id) = with_keyboard();
    feed(&mut input, &node, id, &[press(KEY_UP)]);
    assert_eq!(
        presses_and_releases(&mut input),
        [Did::Press(keysyms::KEY_Up, Mods::empty())]
    );
    input.set_keymap(keymap());
    feed(&mut input, &node, id, &[release(KEY_UP)]);
    assert_eq!(
        presses_and_releases(&mut input),
        [Did::Release(keysyms::KEY_Up, Mods::empty())]
    );
}

// evdev's key codes for the key forms' tests, `linux/input-event-codes.h`:
// KEY_1, KEY_Q, KEY_C, the key the built-in keymap gives ISO_Level3_Shift,
// and KEY_102ND, the extra key of ISO keyboards.
const KEY_1: u16 = 2;
const KEY_Q: u16 = 16;
const KEY_C: u16 = 46;
const KEY_LEVEL3: u16 = 84;
const KEY_102ND: u16 = 86;

/// Each press's keysym, unshifted key and base layout key.
fn forms(input: &mut Input<Nodes>) -> Vec<(u32, u32, Option<char>)> {
    keys(input)
        .iter()
        .map(|k| (k.keysyms.first().copied().unwrap_or(0), k.unshifted, k.base))
        .collect()
}

// ADR-1005 D9: a key reports the keysym it makes at the first level of its
// layout, the unshifted key, and the character it types on the US PC-101
// layout, the base layout key, from the kernel's key code. Shift+1 makes
// exclam, and its unshifted key and base layout key are 1. The keypad and
// the extra key of ISO keyboards type no character on that layout.
// covers: input.kitty-keyboard
#[test]
fn a_key_reports_its_unshifted_key_and_base_layout_key() {
    let (mut input, node, id) = with_keyboard();
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_A),
            release(KEY_A),
            press(KEY_LEFTSHIFT),
            press(KEY_1),
            release(KEY_1),
            press(KEY_A),
            release(KEY_A),
            release(KEY_LEFTSHIFT),
            press(KEY_102ND),
            release(KEY_102ND),
            press(KEY_F2),
            release(KEY_F2),
        ],
    );
    assert_eq!(
        forms(&mut input),
        [
            (keysyms::KEY_a, keysyms::KEY_a, Some('a')),
            (keysyms::KEY_Shift_L, keysyms::KEY_Shift_L, None),
            (keysyms::KEY_exclam, keysyms::KEY_1, Some('1')),
            (keysyms::KEY_A, keysyms::KEY_a, Some('a')),
            (keysyms::KEY_less, keysyms::KEY_less, None),
            (keysyms::KEY_F2, keysyms::KEY_F2, None),
        ]
    );
    // a release carries its press's forms
    feed(&mut input, &node, id, &[press(KEY_1), release(KEY_1)]);
    let released: Vec<_> = input
        .events()
        .filter_map(|e| match e {
            InputEvent::Release(k) => Some((k.unshifted, k.base)),
            _ => None,
        })
        .collect();
    assert_eq!(released, [(keysyms::KEY_1, Some('1'))]);
}

// ADR-1005 D9: on a Russian layout, the key C makes Cyrillic_es and with
// Shift Cyrillic_ES. Its unshifted key is Cyrillic_es, given as the
// Unicode keysym of с, U+0441, and its base layout key is c, so that Ctrl+С
// can match a program's Ctrl+C.
// covers: input.kitty-keyboard
#[test]
fn a_non_latin_layout_reports_the_base_layout_key() {
    let (mut input, node, id) = with_edited_keymap(&[(
        "key <AB03> { [ c, C ] };",
        "key <AB03> { [ Cyrillic_es, Cyrillic_ES ] };",
    )]);
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_C),
            release(KEY_C),
            press(KEY_LEFTSHIFT),
            press(KEY_C),
            release(KEY_C),
            release(KEY_LEFTSHIFT),
        ],
    );
    let k = forms(&mut input);
    assert_eq!(
        k.first(),
        Some(&(keysyms::KEY_Cyrillic_es, 0x0100_0441, Some('c')))
    );
    assert_eq!(
        k.get(2),
        Some(&(keysyms::KEY_Cyrillic_ES, 0x0100_0441, Some('c')))
    );
}

// ADR-1005 D9: a key whose symbol AltGr chose reports no unshifted key, as
// the kitty keyboard protocol cannot encode AltGr. On a German layout
// AltGr+Q makes @; reported as Q, it could not be told from Q. Shift+Q
// still reports q.
// covers: input.kitty-keyboard
#[test]
fn a_symbol_altgr_chose_reports_no_unshifted_key() {
    let (mut input, node, id) = with_edited_keymap(&[(
        "key <AD01> { [ q, Q ] };",
        "key <AD01> { type= \"FOUR_LEVEL\", [ q, Q, at, Greek_OMEGA ] };",
    )]);
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_LEVEL3),
            press(KEY_Q),
            release(KEY_Q),
            release(KEY_LEVEL3),
            press(KEY_LEFTSHIFT),
            press(KEY_Q),
            release(KEY_Q),
            release(KEY_LEFTSHIFT),
        ],
    );
    let k = forms(&mut input);
    assert_eq!(
        k.get(1),
        Some(&(keysyms::KEY_at, keysyms::KEY_NoSymbol, Some('q')))
    );
    assert_eq!(k.get(3), Some(&(keysyms::KEY_Q, keysyms::KEY_q, Some('q'))));
}

// ADR-1005 D9: a key reports the keysym it makes at the level Shift alone
// chooses, the shifted key. With Caps Lock on, Shift+A makes a, and its
// shifted key is still A, as the kitty keyboard protocol's "Key codes"
// asks for the shifted key while Shift is held. A symbol AltGr chose
// reports no shifted key, as it reports no unshifted key.
// covers: input.kitty-keyboard
#[test]
fn a_key_reports_its_shifted_key() {
    let (mut input, node, id) = with_edited_keymap(&[(
        "key <AD01> { [ q, Q ] };",
        "key <AD01> { type= \"FOUR_LEVEL\", [ q, Q, at, Greek_OMEGA ] };",
    )]);
    feed(
        &mut input,
        &node,
        id,
        &[
            press(KEY_1),
            release(KEY_1),
            press(KEY_CAPSLOCK),
            release(KEY_CAPSLOCK),
            press(KEY_LEFTSHIFT),
            press(KEY_A),
            release(KEY_A),
            release(KEY_LEFTSHIFT),
            press(KEY_CAPSLOCK),
            release(KEY_CAPSLOCK),
            press(KEY_LEVEL3),
            press(KEY_Q),
            release(KEY_Q),
            release(KEY_LEVEL3),
        ],
    );
    let shifted: Vec<_> = keys(&mut input)
        .iter()
        .map(|k| (k.keysyms.first().copied().unwrap_or(0), k.shifted))
        .collect();
    assert_eq!(
        shifted.first(),
        Some(&(keysyms::KEY_1, keysyms::KEY_exclam))
    );
    assert!(
        shifted.contains(&(keysyms::KEY_a, keysyms::KEY_A)),
        "Caps Lock and Shift make a, whose shifted key is A: {shifted:x?}"
    );
    assert_eq!(
        shifted.last(),
        Some(&(keysyms::KEY_at, keysyms::KEY_NoSymbol))
    );
}

// ADR-1005 D9: the base layout key of every evdev key code the table
// names, against the names of linux/input-event-codes.h, and none for the
// codes around them.
// covers: input.kitty-keyboard
#[test]
fn the_base_layout_table_follows_the_kernels_key_names() {
    use super::keyboard::base_layout_key;
    const NAMED: [(&str, u32, char); 48] = [
        ("KEY_1", 2, '1'),
        ("KEY_2", 3, '2'),
        ("KEY_3", 4, '3'),
        ("KEY_4", 5, '4'),
        ("KEY_5", 6, '5'),
        ("KEY_6", 7, '6'),
        ("KEY_7", 8, '7'),
        ("KEY_8", 9, '8'),
        ("KEY_9", 10, '9'),
        ("KEY_0", 11, '0'),
        ("KEY_MINUS", 12, '-'),
        ("KEY_EQUAL", 13, '='),
        ("KEY_Q", 16, 'q'),
        ("KEY_W", 17, 'w'),
        ("KEY_E", 18, 'e'),
        ("KEY_R", 19, 'r'),
        ("KEY_T", 20, 't'),
        ("KEY_Y", 21, 'y'),
        ("KEY_U", 22, 'u'),
        ("KEY_I", 23, 'i'),
        ("KEY_O", 24, 'o'),
        ("KEY_P", 25, 'p'),
        ("KEY_LEFTBRACE", 26, '['),
        ("KEY_RIGHTBRACE", 27, ']'),
        ("KEY_A", 30, 'a'),
        ("KEY_S", 31, 's'),
        ("KEY_D", 32, 'd'),
        ("KEY_F", 33, 'f'),
        ("KEY_G", 34, 'g'),
        ("KEY_H", 35, 'h'),
        ("KEY_J", 36, 'j'),
        ("KEY_K", 37, 'k'),
        ("KEY_L", 38, 'l'),
        ("KEY_SEMICOLON", 39, ';'),
        ("KEY_APOSTROPHE", 40, '\''),
        ("KEY_GRAVE", 41, '`'),
        ("KEY_BACKSLASH", 43, '\\'),
        ("KEY_Z", 44, 'z'),
        ("KEY_X", 45, 'x'),
        ("KEY_C", 46, 'c'),
        ("KEY_V", 47, 'v'),
        ("KEY_B", 48, 'b'),
        ("KEY_N", 49, 'n'),
        ("KEY_M", 50, 'm'),
        ("KEY_COMMA", 51, ','),
        ("KEY_DOT", 52, '.'),
        ("KEY_SLASH", 53, '/'),
        ("KEY_SPACE", 57, ' '),
    ];
    for (name, code, c) in NAMED {
        assert_eq!(base_layout_key(code), Some(c), "{name}, {code}");
    }
    let named: Vec<u32> = NAMED.iter().map(|&(_, code, _)| code).collect();
    for code in 0..=255 {
        if !named.contains(&code) {
            assert_eq!(base_layout_key(code), None, "the key code {code}");
        }
    }
}
