//! The keys each console received and that are still held (ADR-1005 D7).
//!
//! A program that asked the kitty keyboard protocol for releases gets one
//! for every press it got. The binary records each press that reaches a
//! console's encoder, with the keyboard it came from. The key's real
//! release goes to the console that received the press, wherever the
//! keyboard types by then, and a release with no record goes nowhere, so a
//! key vtium took never reaches the program, pressed or released. A repeat
//! reaches the encoder only while its press's record stands, so a key vtium
//! took keeps its repeats too.
//!
//! When a console stops receiving a keyboard while one of its keys is held,
//! the binary sends that console a release for the key at once and drops
//! the record, so the key's real release later goes nowhere. That happens
//! when the VT is switched away, when a console chord shows another
//! console, when the focus chord moves the keyboard to another head, and
//! when the keyboard is unplugged. vtium-term encodes such a release only
//! while the program asked for releases, as it does every release.

use std::path::{Path, PathBuf};

use vtium_term::{Key, KeyEvent, Mods};

/// A press that reached a console's encoder, whose key is still held.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Held {
    /// The keyboard it came from, or none for a key from no one device.
    node: Option<PathBuf>,
    /// xkb's key code, which the release carries too.
    keycode: u32,
    /// The head the console belongs to, and the console's id.
    head: usize,
    console: u64,
    /// The press as the encoder received it.
    press: Key,
}

/// Where a key's release goes: the console of a head that received its
/// press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Console {
    pub(super) head: usize,
    pub(super) id: u64,
}

/// A release owed to a console that stopped receiving the key's keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Owed {
    pub(super) to: Console,
    pub(super) release: Key,
}

/// The presses each console received whose keys are still held, oldest
/// first.
#[derive(Debug, Default)]
pub(super) struct HeldKeys {
    held: Vec<Held>,
}

impl HeldKeys {
    /// A press of the key `keycode` of the keyboard at `node` reached the
    /// encoder of `to`. A record of the same key is replaced, as after a
    /// press whose release never came.
    pub(super) fn pressed(&mut self, node: Option<&Path>, keycode: u32, to: Console, press: Key) {
        self.held
            .retain(|h| !(h.node.as_deref() == node && h.keycode == keycode));
        self.held.push(Held {
            node: node.map(Path::to_owned),
            keycode,
            head: to.head,
            console: to.id,
            press,
        });
    }

    /// Whether the press of the key `keycode` of the keyboard at `node`
    /// reached an encoder and the key is still held there, so its repeats
    /// may follow it.
    pub(super) fn delivered(&self, node: Option<&Path>, keycode: u32) -> bool {
        self.held
            .iter()
            .any(|h| h.node.as_deref() == node && h.keycode == keycode)
    }

    /// The key `keycode` of the keyboard at `node` was let go: the console
    /// that received its press, which its release goes to, or `None` where
    /// no encoder received the press.
    pub(super) fn released(&mut self, node: Option<&Path>, keycode: u32) -> Option<Console> {
        let i = self
            .held
            .iter()
            .position(|h| h.node.as_deref() == node && h.keycode == keycode)?;
        let h = self.held.remove(i);
        Some(Console {
            head: h.head,
            id: h.console,
        })
    }

    /// The releases owed to the consoles that stopped receiving a
    /// keyboard: every key held whose console `receives` says no longer
    /// receives its keyboard, newest first, as a person lets go of the
    /// keys they pressed last first. Each release carries the modifiers
    /// that stay held once it is sent: those of the key pressed before it
    /// on the same keyboard, or the locks of its own press alone where no
    /// key was pressed before it, as the specification has a release carry
    /// the modifiers after it. The records go.
    pub(super) fn leave(&mut self, receives: impl Fn(Option<&Path>, Console) -> bool) -> Vec<Owed> {
        let (leaving, staying): (Vec<Held>, Vec<Held>) =
            std::mem::take(&mut self.held).into_iter().partition(|h| {
                !receives(
                    h.node.as_deref(),
                    Console {
                        head: h.head,
                        id: h.console,
                    },
                )
            });
        self.held = staying;
        let mut owed = Vec::with_capacity(leaving.len());
        for (i, h) in leaving.iter().enumerate().rev() {
            let before = leaving
                .get(..i)
                .and_then(|older| older.iter().rev().find(|o| o.node == h.node));
            let mods = match before {
                Some(o) => o.press.mods,
                None => locks(h.press.mods),
            };
            owed.push(Owed {
                to: Console {
                    head: h.head,
                    id: h.console,
                },
                release: Key {
                    event: KeyEvent::Release,
                    mods,
                    ..h.press
                },
            });
        }
        owed
    }

    /// Whether no key is held on any console.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The consoles a key is held on, in press order.
    #[cfg(test)]
    pub(super) fn consoles(&self) -> Vec<Console> {
        self.held
            .iter()
            .map(|h| Console {
                head: h.head,
                id: h.console,
            })
            .collect()
    }
}

/// The locks of `mods`, Caps Lock and NumLock, which stay as they are when
/// vtium lets go of the keys held.
fn locks(mods: Mods) -> Mods {
    [Mods::LOCK, Mods::NUM]
        .into_iter()
        .filter(|&m| mods.contains(m))
        .fold(Mods::empty(), Mods::union)
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::panic,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]
mod tests {
    use super::*;

    const CONTROL_L: u32 = 0xffe3;
    const SHIFT_L: u32 = 0xffe1;

    fn press(keysym: u32, mods: Mods) -> Key {
        Key::press(keysym, 0, mods, None)
    }

    fn at(head: usize, id: u64) -> Console {
        Console { head, id }
    }

    fn release(keysym: u32, mods: Mods) -> Key {
        Key {
            event: KeyEvent::Release,
            ..press(keysym, mods)
        }
    }

    // ADR-1005 D7: a release goes to the console that received the press,
    // once; a release with no press recorded goes nowhere.
    // covers: input.kitty-keyboard
    #[test]
    fn a_release_goes_to_the_console_its_press_reached_once() {
        let kbd = Some(Path::new("/dev/input/event0"));
        let mut held = HeldKeys::default();
        held.pressed(kbd, 38, at(0, 1), press(0x61, Mods::empty()));
        assert!(held.delivered(kbd, 38));
        assert!(!held.delivered(None, 38), "another keyboard's key");
        assert_eq!(held.released(kbd, 38), Some(at(0, 1)));
        assert_eq!(held.released(kbd, 38), None, "once");
        assert!(!held.delivered(kbd, 38));
        assert_eq!(held.released(kbd, 39), None, "a key vtium took");
    }

    // ADR-1005 D7: a console that stops receiving a keyboard gets a
    // release for each of its keys still held, newest first, each with
    // the modifiers that stay held after it; the other keyboards' and the
    // other consoles' keys stay.
    // covers: input.kitty-keyboard
    #[test]
    fn a_console_that_stops_receiving_a_keyboard_gets_its_keys_released() {
        let kbd = Some(Path::new("/dev/input/event0"));
        let other = Some(Path::new("/dev/input/event1"));
        let mut held = HeldKeys::default();
        let ctrl_num = Mods::CTRL.union(Mods::NUM);
        held.pressed(kbd, 37, at(0, 1), press(CONTROL_L, ctrl_num));
        held.pressed(other, 50, at(0, 1), press(SHIFT_L, Mods::SHIFT));
        held.pressed(kbd, 38, at(0, 1), press(0x61, ctrl_num));
        let owed = held.leave(|node, _| node != kbd);
        assert_eq!(
            owed,
            [
                Owed {
                    to: at(0, 1),
                    release: release(0x61, ctrl_num),
                },
                Owed {
                    to: at(0, 1),
                    release: release(CONTROL_L, Mods::NUM),
                },
            ]
        );
        assert_eq!(held.consoles(), [at(0, 1)], "the other keyboard's Shift");
        assert!(held.delivered(other, 50));
        assert!(!held.delivered(kbd, 38));
        assert_eq!(held.leave(|_, _| true), [], "nothing more is owed");
    }

    /// The App's side, through its real key path: what reaches a console's
    /// encoder is recorded, and each way a console stops receiving the
    /// keyboard lets go of the keys held there.
    mod app {
        use std::path::{Path, PathBuf};
        use std::time::Instant;

        use vtium_platform::input::{InputEvent, KeyEvent, Mods};

        use crate::app::App;
        use crate::app::tests::{Still, no_fonts, seat, shown};

        const CONTROL_L: u32 = 0xffe3;
        const PAGE_UP: u32 = 0xff55;
        const F12: u32 = 0xffc9;
        const RETURN: u32 = 0xff0d;

        fn event(keycode: u32, keysym: u32, mods: Mods, repeat: bool) -> KeyEvent {
            KeyEvent {
                keycode,
                ascii: 0,
                unshifted: 0,
                shifted: 0,
                base: None,
                mods,
                keysyms: vec![keysym],
                codepoints: vec![char::from_u32(keysym).filter(|_| keysym < 0x100)],
                repeat,
            }
        }

        fn press(keycode: u32, keysym: u32, mods: Mods) -> InputEvent {
            InputEvent::Key(event(keycode, keysym, mods, false))
        }

        fn repeat(keycode: u32, keysym: u32, mods: Mods) -> InputEvent {
            InputEvent::Key(event(keycode, keysym, mods, true))
        }

        fn release(keycode: u32, keysym: u32, mods: Mods) -> InputEvent {
            InputEvent::Release(event(keycode, keysym, mods, false))
        }

        /// A seat with one head on one display, shown, the focus on it.
        fn one_head() -> (vtium_platform::event_loop::Loop<App>, App) {
            let (lp, mut app) = seat(None, no_fonts());
            app.add_video(
                PathBuf::from("/dev/mem0"),
                Box::new(Still::new(&[(1, "A")])),
            );
            shown(&mut app);
            app.focus = Some(0);
            (lp, app)
        }

        /// Whether a key the program received opened a key's window on
        /// the head's displays (ADR-1401 D2) since the last flip.
        fn sent(app: &App) -> bool {
            let head = app.heads.first().unwrap_or_else(|| panic!("a head"));
            head.views.values().all(|v| v.after_key(&head.console.term))
        }

        // ADR-1005 D7: a key the encoder receives is held on the console
        // shown until its release, which goes there; a key vtium took is
        // never recorded, so neither its release nor a repeat that no
        // longer matches its binding reaches the program.
        // covers: input.kitty-keyboard
        #[test]
        fn a_key_vtium_took_never_reaches_the_program_pressed_or_released() {
            let (_lp, mut app) = one_head();
            let t = Instant::now();
            let shift = Mods::SHIFT;
            app.input_event(press(112, PAGE_UP, shift), t);
            assert!(app.held.is_empty(), "Shift+Page Up scrolls the view");
            // Shift let go while Page Up is held: its repeat matches no
            // binding, and still never reaches the program.
            app.input_event(repeat(112, PAGE_UP, Mods::empty()), t);
            app.input_event(release(112, PAGE_UP, Mods::empty()), t);
            assert!(!sent(&app), "the program got nothing");
            app.input_event(press(38, 0x61, Mods::empty()), t);
            assert_eq!(app.held.consoles().len(), 1, "a reached the encoder");
            assert!(sent(&app));
            app.input_event(release(38, 0x61, Mods::empty()), t);
            assert!(app.held.is_empty(), "its release went to the console");
        }

        // ADR-1005 D7: a VT switched away lets go of every key held.
        // covers: input.kitty-keyboard
        #[test]
        fn a_vt_switch_lets_go_of_the_keys_held() {
            let (_lp, mut app) = one_head();
            let t = Instant::now();
            let ctrl = Mods::CTRL;
            app.input_event(press(37, CONTROL_L, ctrl), t);
            app.input_event(press(38, 0x61, ctrl), t);
            assert_eq!(app.held.consoles().len(), 2);
            // The fake VT's own switch key, Ctrl+Super+F12.
            app.input_event(press(96, F12, ctrl.union(Mods::LOGO)), t);
            app.turn(t);
            assert!(!app.vt.is_active(), "the VT was given up");
            assert!(app.held.is_empty(), "both keys let go");
        }

        // ADR-1005 D7: a console chord that shows another console lets
        // go of the keys held on the console it hides.
        // covers: input.kitty-keyboard
        #[test]
        fn a_console_chord_lets_go_of_the_keys_held() {
            let (_lp, mut app) = one_head();
            let t = Instant::now();
            app.input_event(press(38, 0x61, Mods::empty()), t);
            let first = app.heads.first().map(|h| h.console.id);
            assert_eq!(app.held.consoles().first().map(|c| Some(c.id)), Some(first));
            let ctrl_super = Mods::CTRL.union(Mods::LOGO);
            app.input_event(press(36, RETURN, ctrl_super), t);
            assert_ne!(
                app.heads.first().map(|h| h.console.id),
                first,
                "Ctrl+Super+Return opened and showed a console"
            );
            assert!(app.held.is_empty(), "a let go on the hidden console");
        }

        // ADR-1005 D7: a keyboard unplugged lets go of its keys held, and
        // another keyboard's stay.
        // covers: input.kitty-keyboard
        #[test]
        fn an_unplugged_keyboard_lets_go_of_its_keys_held() {
            let (_lp, mut app) = one_head();
            let t = Instant::now();
            let node = Path::new("/dev/input/event0");
            let keyboard =
                vtium_settings::Identity::input().with_class(vtium_settings::Class::Keyboard);
            app.add_input(node, keyboard);
            app.device_input(node, press(38, 0x61, Mods::empty()), t);
            app.input_event(press(39, 0x73, Mods::empty()), t);
            assert_eq!(app.held.consoles().len(), 2);
            app.remove_input(node);
            assert_eq!(app.held.consoles().len(), 1, "the other keyboard's s");
            app.device_input(node, release(38, 0x61, Mods::empty()), t);
            assert_eq!(app.held.consoles().len(), 1);
        }
    }
}
