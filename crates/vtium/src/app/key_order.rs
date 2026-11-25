//! The order in which a key is handled, and the heads' chords: the VT's
//! keys, the seat's power keys, the chords, the terminal's grabs, and then
//! the encoder.

use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use vtium_platform::input::{KeyEvent, Mods as InputMods};
use vtium_settings::Policy;
use vtium_term::KeyEvent as TermKeyEvent;

use crate::grab::{self, Grab};
use crate::head::Head;
use crate::head::chord::{self, HeadChord};
use crate::layout::{self, Zoom};
use crate::power;

use super::input::{KeyFrom, term_key};
use super::{App, after, held};

impl App {
    /// The key order: the VT's keys, the seat's, then the terminal of
    /// head `h`, the head the key's device delivers to: its grabs, DECARM,
    /// the encoder. Each key that passes the seat's clears the selection.
    ///
    /// The VT's keys switch the VT only `from` a keyboard whose VT keys
    /// count (ADR-1401 D3, "Who switches VTs"); from any other, the press
    /// goes on down the key order to the session as an ordinary key, as a
    /// serial terminal sends it (D5). The roaming chord is taken only from a roaming
    /// keyboard; from a claimed one it reaches the head's session as a key.
    /// A key that reaches the encoder is recorded as held on the console
    /// shown, with the keyboard at `node` it came from, so its release
    /// follows it there; a repeat reaches the encoder only while its
    /// press's record stands, so a key vtium took keeps its repeats
    /// (ADR-1005 D7).
    pub(super) fn key(
        &mut self,
        h: Option<usize>,
        node: Option<&Path>,
        key: &KeyEvent,
        from: KeyFrom,
        now: Instant,
    ) {
        if from.vt_keys && self.vt.key(key) || !self.awake {
            return;
        }
        let bits = key.mods.bits();
        let action = self.bindings.action(bits, &key.keysyms);
        if let Some(grab::Action::Power(power_key)) = action {
            // One action a press: a held chord's repeats would ask again,
            // and systemd takes seven Ctrl+Alt+Del in two seconds as an
            // immediate reboot.
            if key.repeat {
                return;
            }
            if let Some(child) = power::press(power_key, self.power.as_mut(), self.sleeping.get()) {
                let sleeping = Rc::clone(&self.sleeping);
                if let Err(e) = vtium_platform::power::watch(&self.handle, child, sleeping) {
                    log::error!("key-suspend: cannot watch the write to /sys/power/state: {e}");
                }
            }
            return;
        }
        // While a program holds every display of the head, the head takes
        // no key, console chord or grab: only the roaming chord, which
        // moves the roaming inputs to a head that draws (ADR-1401 D16).
        let paused = h.is_some_and(|h| self.wholly_paused(h));
        // The link hints key starts or ends link hints on the head the key's
        // device delivers to. While they are on, that head takes every key
        // after the power keys, one action a press, and none reaches the
        // program. Otherwise a key press ends vtium's note on the overlay
        // row and goes on as usual.
        //
        // References:
        // - ADR-1002, section 7: the keys read while link hints are on.
        if let Some(head) = h.filter(|_| !paused).and_then(|h| self.heads.get_mut(h)) {
            if action == Some(grab::Action::LinkHints) {
                if !key.repeat {
                    head.link_hints_key();
                }
                return;
            }
            if head.hints.is_some() {
                if !key.repeat {
                    let command = [InputMods::CTRL, InputMods::ALT, InputMods::LOGO]
                        .into_iter()
                        .any(|m| key.mods.contains(m));
                    head.hint_key(&key.keysyms, command, &mut self.selection);
                }
                return;
            }
            head.end_note();
        }
        if let Some(grab::Action::Head(chord)) = action
            && (from.roaming || chord.is_console())
            && !(paused && chord.is_console())
        {
            // One action a press, as the power keys.
            if !key.repeat {
                self.chord(h, chord, key.mods.contains(InputMods::CTRL), now);
            }
            return;
        }
        let Some(h) = h.filter(|_| !paused) else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        // What the displays show before the key, so that the key draws a
        // frame only when it changed that (`KeyShown`).
        let before = KeyShown::now(head);
        // A press of Ctrl shows the hovered link's URI, and a key pressed
        // without Ctrl hides it; letting go of Ctrl sends no key.
        head.hover(key.mods.contains(InputMods::CTRL));
        head.console.term.terminal.screen_mut().clear_selection();
        if action == Some(grab::Action::Paste) {
            // One paste a press, however long the key repeats.
            if !key.repeat {
                head.paste(self.selection.as_deref());
                head.send_user_bytes();
            }
            head.draw_now = true;
            return;
        }
        // The scroll keys and the prompt jumps are the main screen's: on
        // the alternate screen they reach the program (ADR-1005 D7,
        // ADR-1002, "What the marks are for").
        let alternate = head.console.term.terminal.screen().alternate();
        let grab = match action {
            Some(grab::Action::Grab(g)) if !(g.main_screen_only() && alternate) => Some(g),
            _ => None,
        };
        if let Some(g) = grab {
            let screen = head.console.term.terminal.screen_mut();
            let page = head.grid.1.saturating_sub(1).max(1);
            head.draw_now = true;
            match g {
                Grab::ScrollUp => screen.scroll_view_up(1),
                Grab::ScrollDown => screen.scroll_view_down(1),
                Grab::PageUp => screen.scroll_view_up(page),
                Grab::PageDown => screen.scroll_view_down(page),
                Grab::PromptUp | Grab::PromptDown => {
                    screen.jump_to_prompt(g == Grab::PromptUp);
                }
                Grab::ZoomIn => self.zoom(h, Zoom::In),
                Grab::ZoomOut => self.zoom(h, Zoom::Out),
                Grab::RotateCw | Grab::RotateCcw => {
                    head.orientation = layout::rotate(head.orientation, g == Grab::RotateCw);
                    self.relayout(h);
                    // A turn swaps which of a display's two densities the
                    // head's vtlook judges, so each display is checked
                    // against the vtlook's requirements again, as at a mode
                    // change.
                    #[cfg(feature = "filters")]
                    self.give_colors_again(h);
                }
                // One reset a press, of the console shown alone.
                Grab::Reset if !key.repeat => head.reset_shown(),
                Grab::Reset => {}
            }
            return;
        }
        // The key has already cleared the selection and changed the hovered
        // link, so each way out of this block reaches the frame decision
        // below, even when vtium drops the key itself: a repeat whose press
        // it never delivered, or a key with no keysym.
        'send: {
            if key.repeat
                && (!head.console.term.terminal.auto_repeat()
                    || !self.held.delivered(node, key.keycode))
            {
                break 'send;
            }
            let event = if key.repeat {
                TermKeyEvent::Repeat
            } else {
                TermKeyEvent::Press
            };
            let Some(pressed) = term_key(key, event) else {
                break 'send;
            };
            if !key.repeat {
                let to = held::Console {
                    head: h,
                    id: head.console.id,
                };
                self.held.pressed(node, key.keycode, to, pressed);
            }
            if head.console.term.terminal.key(&pressed) {
                // The key opens a window on each display: the program's
                // output draws at once until the display's next flip
                // completes.
                //
                // References:
                // - ADR-1401 D2: the key's window and when it closes.
                head.console.term.key_sent();
                head.send_user_bytes();
                head.console.term.terminal.screen_mut().reset_view();
                if self.hide_while_typing {
                    head.hide_pointer();
                }
            }
            let rate = crate::blink::cursor_period(&head.console.term.terminal);
            let blinks = head.console.term.terminal.cursor_blinks();
            if let Some(arm) = head.console.term.blink.key(rate, blinks) {
                head.cursor_due = Some((after(now, arm.first), arm.every));
            }
        }
        head.draw_now |= before.changed(head);
    }

    /// A head's chord (ADR-1401 D1, D5): a console chord shows another of
    /// head `h`'s consoles, the head the key's device delivers to, and
    /// under an open policy opens or closes one at `now`; the focus chord
    /// moves the focus to the next or the previous head with a lit
    /// display. A console chord leaves the console it shows hovering what
    /// the pointer is over, with `ctrl` as held now.
    pub(super) fn chord(&mut self, h: Option<usize>, chord: HeadChord, ctrl: bool, now: Instant) {
        let visible = self.awake;
        let back = match chord {
            HeadChord::NextHead => false,
            HeadChord::PreviousHead => true,
            HeadChord::NextConsole
            | HeadChord::PreviousConsole
            | HeadChord::OpenConsole
            | HeadChord::CloseConsole => {
                // setBackground and setForeground are judged against the
                // console shown when they came, before the chord shows
                // another (ADR-1401 D16). A setBackground that pauses the
                // head takes the chord with it, as a paused head takes no
                // chord.
                if let Some(h) = h {
                    self.display_requests(h);
                    if self.wholly_paused(h) {
                        return;
                    }
                }
                let changed = match (h, chord) {
                    // Under closed the count is fixed: the chord is taken,
                    // and does nothing (D1, D5).
                    (_, HeadChord::OpenConsole | HeadChord::CloseConsole)
                        if self.rules.policy == Policy::Closed =>
                    {
                        false
                    }
                    (Some(h), HeadChord::OpenConsole) => self.open_console(h, now),
                    (Some(h), HeadChord::CloseConsole) => {
                        let closed = self
                            .heads
                            .get_mut(h)
                            .and_then(|head| head.close_shown(visible))
                            .map(|console| console.id);
                        // Its session ended, and with it the hand-over it
                        // began (ADR-1401 D16).
                        if let Some(id) = closed {
                            self.hand_over_session_ended(h, id);
                        }
                        // And the switch it made (ADR-1201 D11).
                        if let Some(id) = closed.filter(|&id| {
                            self.heads
                                .get(h)
                                .is_some_and(|head| head.switched == Some(id))
                        }) {
                            self.return_font(h, id);
                        }
                        closed.is_some()
                    }
                    (h, _) => h
                        .and_then(|h| self.heads.get_mut(h))
                        .is_some_and(|head| head.console_chord(chord, visible)),
                };
                if let Some(h) = h.filter(|_| changed) {
                    self.relayout(h);
                    if let Some(head) = self.heads.get_mut(h) {
                        head.hover(ctrl);
                    }
                }
                return;
            }
        };
        self.focus_settled = true;
        // only to a lit head that can be the active head (D5), and never
        // to one a program holds every display of (ADR-1401 D16)
        let paused = self.paused_heads();
        let (heads, rules, vt_heads) = (self.heads(), &self.rules, &self.vt_heads);
        let lit = |h: usize| {
            heads.get(h).is_some_and(|head| !head.views.is_empty())
                && !vt_heads.isolated(rules, h)
                && !paused.contains(&h)
        };
        if let Some(to) = chord::step(self.focus, heads.len(), back, lit) {
            self.move_focus(to);
            // The keyboard that pressed the chord follows the focus, with
            // the Ctrl it holds for the chord.
            if let Some(head) = self.heads.get_mut(to).filter(|_| ctrl) {
                head.rehover(true);
            }
        }
    }
}

/// What a head's displays show before a key, to tell whether the key
/// changed it. A key can clear the selection, return the view from the
/// scrollback, change the link the pointer hovers, hide the pointer and
/// show a blinking cursor again, and each of these needs a frame. A key
/// that changes none of them draws no frame of its own: the program's
/// echo draws the next one. A frame drawn for such a key would still have
/// damage on a display with two or three buffers, because the buffer it
/// goes into lacks the frames drawn since that buffer was last shown. The
/// display would then present the same picture again, a frame early, and
/// the echo would wait for that frame's flip.
struct KeyShown {
    /// The screen's cells, its cursor and the pointer's shape.
    screen: crate::term::Shown,
    /// Whether the pointer shows.
    pointer: bool,
    /// Whether the cursor is in the shown phase of its blink.
    cursor: bool,
}

impl KeyShown {
    fn now(head: &mut Head) -> KeyShown {
        KeyShown {
            screen: crate::term::Shown::now(&mut head.console.term.terminal),
            pointer: head.pointer.visible,
            cursor: head.console.term.blink.cursor_shown(),
        }
    }

    /// Whether the key changed what `head`'s displays show.
    fn changed(&self, head: &Head) -> bool {
        self.screen.changed(&head.console.term.terminal)
            || self.pointer != head.pointer.visible
            || self.cursor != head.console.term.blink.cursor_shown()
    }
}
