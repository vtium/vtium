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
use super::{Button, Input, InputConfig, InputEvent, KeyEvent, Keymap, Leds, Mods, PointerEvent};
use crate::ioctl::{
    ABS_X, ABS_Y, BTN_LEFT, BTN_RIGHT, BTN_TOUCH, EV_ABS, EV_KEY, EV_REL, EV_SYN, LED_CAPSL,
    LED_NUML, LED_SCROLLL, REL_WHEEL, REL_X, REL_Y,
};

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
            InputEvent::Pointer(_) => None,
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
fn the_repeat_settings_take_the_c_references_defaults_and_bounds() {
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
            InputEvent::Key(_) => None,
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
