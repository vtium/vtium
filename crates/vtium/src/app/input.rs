//! Input devices, the focus and the pointer: a keyboard or pointer comes and
//! goes, the focus moves between heads, each event is delivered to the head
//! it reaches, and held keys are released when their console stops
//! receiving them.

use std::path::Path;
use std::time::Instant;

use vtium_platform::input::{
    DeviceId, DeviceInput, InputEvent, KeyEvent, Mods as InputMods, PointerEvent,
};
use vtium_settings::Policy;
use vtium_term::{Key, KeyEvent as TermKeyEvent, Mods};

use crate::head::{Attached, Head};

use super::{App, held};

impl App {
    /// Mode 1004's focus (ADR-1401 D5): the shown console of each head a
    /// keyboard delivers to has it, lit or not, so several hold it at
    /// once. It holds only while vtium's VT is shown, and only while the
    /// head is not wholly paused by a hand-over of its displays (D16). So
    /// a VT release, under a seat manager the seat's disable, takes it
    /// from every head, and setBackground from each head it wholly
    /// pauses. Each change goes to a program that asked for it.
    pub(super) fn report_focus(&mut self) {
        // A keyboard that could not be opened, or went at a wake-up, is
        // gone from the seat's input; with no seat input, as a test drives
        // the seat, a device counts as open.
        let input = self.input.as_ref();
        let open = |id: Option<DeviceId>| {
            input.is_none_or(|i| id.is_some_and(|id| i.disposition(id).is_some()))
        };
        // A release under way counts as the VT gone. The VT stays ours
        // until the turn lets it go, once the displays are asleep, and the
        // release reports the focus-out before that.
        let shown = self.vt.is_active() && self.releasing.is_none();
        let paused = self.paused_heads();
        for (h, head) in self.heads.iter_mut().enumerate() {
            let focused = shown
                && !paused.contains(&h)
                && self
                    .routes
                    .keyboard_reaches(&self.vt_heads, h, self.focus, open);
            if focused != head.focused {
                head.focused = focused;
                if head.console.term.terminal.focus(focused) {
                    head.send_user_bytes();
                }
            }
        }
    }

    /// The focus, once a head has a display (ADR-1401 D5): it starts on
    /// the head holding the boot display, the connector lit at start, else
    /// the first connected one on the boot card, else on the other cards,
    /// in the kernel's order; else on the first head in the file with a
    /// display. Until the first key reaching a head or the first focus
    /// chord it starts again as each display lights, over every display lit
    /// since the start, one that left included, so a boot display lighting
    /// a turn late takes it whatever order the displays came in (the
    /// operator's answer on vtium/vtium#156) and a display leaving never
    /// moves it (D8); after, it moves only by the chord.
    pub(super) fn take_focus(&mut self) {
        let due = std::mem::take(&mut self.focus_due);
        if self.start_lit.is_empty() || self.focus.is_some() && (self.focus_settled || !due) {
            return;
        }
        // The display guard's display counts as lit; given no keyboard, the
        // anti-lockout guard moves nothing ([`App::guard`] runs it).
        let lit: Vec<vtium_settings::Identity> = self.start_lit.values().cloned().collect();
        let placement = self.vt_heads.placement(&self.rules, &lit, &[], None);
        self.focus = placement.focus.and_then(|p| self.vt_heads.head(p));
        self.bound_pointers();
        self.rehover();
    }

    /// An input device udev reported, placed by the first rule matching
    /// `identity` (ADR-1401 D3): opened to deliver, opened for the rescue
    /// key alone, or never opened.
    pub fn add_input(&mut self, node: &Path, identity: vtium_settings::Identity) {
        self.guard_due = true;
        let disposition = self
            .routes
            .add(&self.rules, &self.vt_heads, node, &identity);
        if let Some(id) = self
            .input
            .as_mut()
            .and_then(|i| i.add_with(node, disposition))
        {
            self.routes.opened(node, id);
            self.bound_pointers();
        }
    }

    /// The input device at `node` went.
    pub fn remove_input(&mut self, node: &Path) {
        self.routes.remove(node);
        self.guard_due = true;
        if let Some(input) = &mut self.input
            && let Some(id) = input.find(node)
        {
            input.remove(id);
        }
        // The keyboard's route is gone before the end of its modifiers is
        // drained, and that event finds no head, so the Ctrl it held is let
        // go here, and so are the keys it held (ADR-1005 D7).
        self.rehover();
        self.let_go_of_held_keys();
    }

    /// Each head hovers what its pointer is over with Ctrl as the
    /// keyboards delivering to it hold it now (ADR-1002, "What a link
    /// does"). The seat reads the keyboards, since a release sends no key,
    /// a keyboard that goes sends its modifiers' end after its route has
    /// gone, and a keyboard following the focus sends nothing as the focus
    /// moves.
    pub(super) fn rehover(&mut self) {
        for h in 0..self.heads.len() {
            let ctrl = self.input.as_ref().is_some_and(|input| {
                input
                    .mods_of(&self.routes.devices_of(&self.vt_heads, h, self.focus))
                    .contains(InputMods::CTRL)
            });
            if let Some(head) = self.heads.get_mut(h) {
                head.rehover(ctrl);
            }
        }
    }

    /// Natural scrolling, on or off (vtium/vtium#203): the wheel scrolls the
    /// view and reports to a program the other way, and the cursor keys
    /// mode 1007 sends reverse, while Ctrl's zoom keeps the wheel's own
    /// direction. vtium.conf's `natural-scrolling` sets it at the start.
    pub fn set_natural_scrolling(&mut self, on: bool) {
        self.natural_scrolling = on;
    }

    /// The name of the head holding the focus.
    pub fn focus_head(&self) -> Option<&str> {
        self.heads.get(self.focus?).map(Head::name)
    }

    /// The head the focus chord last moved the focus to, by its name,
    /// once (ADR-1401 D5).
    pub fn take_focus_move(&mut self) -> Option<String> {
        let h = self.focus_move.take()?;
        self.heads.get(h).map(|head| head.name().to_owned())
    }

    /// A key press or pointer event from no one device, which follows the
    /// focus.
    pub fn input_event(&mut self, event: InputEvent, now: Instant) {
        self.deliver(None, event, now);
        self.let_go_of_held_keys();
    }

    /// A key press or pointer event from the input device at `node`.
    pub fn device_input(&mut self, node: &Path, event: InputEvent, now: Instant) {
        self.deliver(Some(node), event, now);
        self.let_go_of_held_keys();
    }

    /// An event as the input drains it, from the device with the handle
    /// `e.device` or from none, which every turn delivers.
    pub fn drained_input(&mut self, e: DeviceInput, now: Instant) {
        // A device watched for the rescue key sends its presses alone.
        let node = e
            .device
            .and_then(|id| self.routes.node(id))
            .map(Path::to_owned);
        // Under closed an event of a device the seat no longer routes,
        // queued before it went, delivers nothing (D3, D5).
        if e.device.is_some() && node.is_none() && self.rules.policy == Policy::Closed {
            return;
        }
        self.deliver(node.as_deref(), e.event, now);
        self.let_go_of_held_keys();
    }

    /// An event from the device at `node`, or from none, for the head its
    /// rule names or the head holding the focus (ADR-1401 D5). A key press
    /// is the user's activity; a pointer event is when
    /// [`App::pointer_event`] says so.
    fn deliver(&mut self, node: Option<&Path>, event: InputEvent, now: Instant) {
        // A seat a crash loop stopped delivers nothing (D1), and under
        // closed neither does a device with no route, such as one whose
        // event was queued before it went: no rule placed it (D3, D5).
        let unknown = node.is_some_and(|n| !self.routes.knows(n));
        if self.stopped || unknown && self.rules.policy == Policy::Closed {
            return;
        }
        // A release goes to the console that received its press, and
        // nowhere else (ADR-1005 D7).
        if let InputEvent::Release(key) = &event {
            self.release(node, key);
            return;
        }
        // The shown VT's own key does nothing else (D9).
        if let (Some(node), InputEvent::Key(key)) = (node, &event)
            && !key.repeat
            && self.vt.is_own_key(key)
        {
            self.activity(now);
            self.rescue(node);
            return;
        }
        // A key of a keyboard that stays with a head on another VT reaches
        // no session: its VT keys switch the VT as every keyboard's do
        // (D3, "Who switches VTs"), Enter may switch to that head's VT
        // (D3, "Enter on a head's own keyboard"), and every key wakes the
        // notices.
        if let InputEvent::Key(key) = &event
            && let Some(head) = self.routes.elsewhere(node)
        {
            self.activity(now);
            let vt_keys = self.routes.vt_keys(&self.rules, node);
            if !(vt_keys && self.vt.key(key)) {
                self.key_elsewhere(head, node, key);
            }
            return;
        }
        // The head the device types into: its rule's, or the active head's
        // for a roaming device (D5). Whether its VT keys switch the VT is
        // the policy's and its own head's to say, wherever that head runs
        // (D3, "Who switches VTs").
        let asked = self.routes.head(&self.vt_heads, node, self.focus);
        let from = KeyFrom {
            vt_keys: self.routes.vt_keys(&self.rules, node),
            roaming: self.routes.roams(node),
        };
        // A head whose display never lit receives nothing (D8).
        let h = asked.filter(|&h| {
            self.heads
                .get(h)
                .is_some_and(|head| head.attached != Attached::NotYet)
        });
        match event {
            InputEvent::Key(key) => {
                // Only a key reaching a head settles the focus's start, and
                // ends its notices. An Esc that dismisses the kernel's
                // messages stays with vtium (ADR-1401 D15).
                self.focus_settled |= h.is_some();
                // A head a program holds the displays of keeps its notices
                // (ADR-1401 D16).
                let shown = h.filter(|&h| !self.wholly_paused(h));
                let kept = shown
                    .and_then(|h| self.heads.get_mut(h))
                    .is_some_and(|head| {
                        let dismissed = head.has_kernel() && dismisses(&key);
                        head.end_notices();
                        dismissed
                    });
                self.activity(now);
                if !kept {
                    self.key(h, node, &key, from, now);
                }
            }
            InputEvent::Pointer(p) => {
                let mods = match (&self.input, h) {
                    (Some(input), Some(h)) => {
                        input.mods_of(&self.routes.devices_of(&self.vt_heads, h, self.focus))
                    }
                    _ => InputMods::empty(),
                };
                self.pointer_to(h, p, mods, now);
            }
            // A keyboard's modifiers changed. Only the overlay follows
            // them, read from each head's own keyboards, and a modifier is
            // not the user's activity.
            InputEvent::Mods(_) => self.rehover(),
            InputEvent::Release(_) => {}
        }
    }

    /// The release of a key of the keyboard at `node`: sent to the console
    /// that received its press, through its encoder, which sends nothing
    /// for it unless the program asked for releases. A key whose press
    /// reached no encoder, as a key vtium took, is released nowhere
    /// (ADR-1005 D7).
    fn release(&mut self, node: Option<&Path>, key: &KeyEvent) {
        let Some(to) = self.held.released(node, key.keycode) else {
            return;
        };
        let Some(release) = term_key(key, TermKeyEvent::Release) else {
            return;
        };
        self.send_release(to, &release);
    }

    /// `release` to the console `to`, shown or hidden.
    fn send_release(&mut self, to: held::Console, release: &Key) {
        let console = self
            .heads
            .get_mut(to.head)
            .and_then(|head| head.console_mut(to.id));
        if let Some(console) = console
            && console.term.terminal.key(release)
        {
            console.send_replies();
        }
    }

    /// Each console that stopped receiving a keyboard while one of its
    /// keys was held gets a release for each such key (ADR-1005 D7): a
    /// console receives a keyboard while vtium's terminals are awake on
    /// the VT, the keyboard is still routed to the console's head on this
    /// VT, and the console is the one its head shows. So a VT switch, a
    /// console chord, a focus move and an unplugged keyboard each let go
    /// of the keys held, as the next call after them finds.
    pub(super) fn let_go_of_held_keys(&mut self) {
        let awake = self.awake && self.vt.is_active();
        let paused = self.paused_heads();
        let (heads, routes, vt_heads, focus) =
            (&self.heads, &self.routes, &self.vt_heads, self.focus);
        let owed = self.held.leave(|node, to| {
            awake
                && !paused.contains(&to.head)
                && node.is_none_or(|n| routes.knows(n))
                && routes.elsewhere(node).is_none()
                && routes.head(vt_heads, node, focus) == Some(to.head)
                && heads.get(to.head).is_some_and(|h| h.console.id == to.id)
        });
        for o in owed {
            self.send_release(o.to, &o.release);
        }
    }

    /// The rescue key, `Ctrl-Alt-F<n>` for vtium's own VT, pressed on the
    /// keyboard at `node` (ADR-1401 D9): it follows the focus for the rest
    /// of the boot, opened to deliver where it was watched for the rescue
    /// key alone, unless the `rescue-key` guard or its head's `rescue` is
    /// off. Nothing is written.
    fn rescue(&mut self, node: &Path) {
        let Some(id) = self.routes.rescue(&self.rules, node) else {
            return;
        };
        if let (Some(input), Some(id)) = (&mut self.input, id) {
            input.set_disposition(id, vtium_platform::input::Disposition::Deliver);
        }
        // The guards run at once, so the notice names the head the keyboard
        // types into after them, which the anti-lockout guard may move.
        self.guard_due = true;
        self.guard();
        let line = self.rescue_line(node);
        log::warn!("{line}");
        match self.focus {
            Some(h) => {
                if let Some(head) = self.heads.get_mut(h) {
                    head.add_notice(&line);
                }
            }
            None => self.notice(&line),
        }
    }

    /// What vtium says when the rescue key moved the keyboard at `node`
    /// (ADR-1401 D9): the head it types into now; on a VT where no head
    /// runs, that no terminal runs here and the VT keys that leave (D3);
    /// else that no head is active and the roaming chord that chooses one.
    pub(super) fn rescue_line(&self, node: &Path) -> String {
        match self.focus.and_then(|h| self.heads.get(h)) {
            Some(head) => crate::notice::rescue(node.display(), head.name()),
            None if self.heads().is_empty() => crate::notice::rescue_no_terminal(node.display()),
            None => crate::notice::rescue_no_head(node.display(), &self.bindings),
        }
    }

    /// A pointer event, with the modifiers held on the keyboards as it
    /// came, for the head holding the focus.
    pub fn pointer_event(&mut self, event: PointerEvent, mods: InputMods, now: Instant) {
        self.pointer_to(self.focus, event, mods, now);
    }

    /// A pointer event for head `h`, with the modifiers held on the
    /// keyboards delivering to it. A move, a button or the wheel is the
    /// user's activity; the end of a frame and the hide are vtium's own
    /// events, so they neither wake the displays nor restart the blank's
    /// countdown.
    fn pointer_to(&mut self, h: Option<usize>, event: PointerEvent, mods: InputMods, now: Instant) {
        match event {
            // While the terminals are not awake, a VT the user released or
            // displays blanked, the pointer's action reaches no program and
            // selects, pastes or zooms nothing, as a key reaches none. It
            // may end a blank,
            // which wakes the terminals first.
            PointerEvent::Moved { .. } | PointerEvent::Button { .. } | PointerEvent::Wheel(_) => {
                self.activity(now);
                if !self.awake {
                    return;
                }
            }
            PointerEvent::Sync if !self.awake => return,
            PointerEvent::Sync => {}
            // The seat's pointers hide together, on every head: the hide
            // comes from no one device. While the VT is another's, as a
            // released fake VT is with its pointers still open, the mouse
            // moving in the other session arms the hide. That hide clears no
            // selection, so the selection stays for the user's return. It
            // still hides the arrow, which stays hidden at the return until
            // the pointer moves: the input keeps adding up the moves made in
            // the other session, so an arrow at the old place would jump at
            // the next move (the README's "A fake VT").
            PointerEvent::Hide => {
                let ours = self.vt.is_active();
                for head in &mut self.heads {
                    if ours {
                        head.pointer_event(event, Mods::empty(), false, &mut self.selection);
                    } else {
                        head.hide_pointer();
                    }
                }
                return;
            }
        }
        // While a program holds every display of the head, its pointer
        // events reach no console (ADR-1401 D16).
        let Some(h) = h.filter(|&h| !self.wholly_paused(h)) else {
            return;
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let zoom = head.pointer_event(
            event,
            Mods::from_bits(mods.bits()),
            self.natural_scrolling,
            &mut self.selection,
        );
        if let Some(dir) = zoom {
            self.zoom(h, dir);
        }
    }

    /// The focus moves to head `h`, by the focus chord or the anti-lockout
    /// guard, the only moves after its start (ADR-1401 D5): every head's
    /// on-screen display shows.
    pub(super) fn move_focus(&mut self, h: usize) {
        self.focus = Some(h);
        // The keyboards following the focus now reach head `h`, and no
        // longer the head that lost it, which keeps the hover its pointer
        // shows with Ctrl as its own keyboards hold it.
        self.rehover();
        self.focus_move = Some(h);
        if self.osd.shows_on_change() {
            for head in self
                .heads
                .iter_mut()
                .filter(|head| !head.views.is_empty() && !head.notice_only)
            {
                head.show_osd();
            }
        }
        self.bound_pointers();
    }
}

/// Whether `key` is the Esc that dismisses the kernel's messages
/// (ADR-1401 D15): Esc pressed, with Shift and Lock at most.
fn dismisses(key: &KeyEvent) -> bool {
    const ESCAPE: u32 = 0xff1b;
    let held = [InputMods::CTRL, InputMods::ALT, InputMods::LOGO];
    !key.repeat && key.keysyms.contains(&ESCAPE) && !held.iter().any(|&m| key.mods.contains(m))
}

/// What the device a key came from may do (ADR-1401 D5).
#[derive(Clone, Copy, Debug)]
pub(super) struct KeyFrom {
    /// The VT's keys switch the VT: every keyboard's under open, and under
    /// closed a keyboard's that a path places, but never those of a
    /// keyboard a rule sends to an isolated head (ADR-1401 D3).
    pub(super) vt_keys: bool,
    /// It roams, so it takes the roaming chord.
    pub(super) roaming: bool,
}

/// vtium-term's description of the key event `key` from vtium-platform,
/// as `event`: its first keysym and that keysym's character, the ASCII
/// layout's keysym, the unshifted key, the shifted key, the base layout key
/// and the modifiers, whose bits the two crates share (ADR-1005, D1 and
/// D2). `None` for an event with no keysym.
pub fn term_key(key: &KeyEvent, event: TermKeyEvent) -> Option<Key> {
    let &keysym = key.keysyms.first()?;
    let unicode = key.codepoints.first().copied().flatten();
    let mods = Mods::from_bits(key.mods.bits());
    Some(Key {
        event,
        unshifted: key.unshifted,
        shifted: key.shifted,
        base: key.base,
        ..Key::press(keysym, key.ascii, mods, unicode)
    })
}
