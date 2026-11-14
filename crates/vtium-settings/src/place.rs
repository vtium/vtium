//! Where each device lands: the first-match walk of one device (ADR-1401
//! D3) and the guards over a set of devices (D9), under the file's policy.
//!
//! Each vtium places devices for its own VT (D2). [`Config::walk_on`] and
//! [`Config::place_on`] take that VT and follow D3's four steps: a rule
//! whose head runs on other VTs only notes the head and reads on, and once
//! a head is noted only a rule whose head's `vt` names this VT places the
//! device; else it stays with the noted head, [`Place::Elsewhere`].
//! [`Config::walk`] and [`Config::place`] name no VT and read every head as
//! running, as vtium placed devices before heads had a `vt`.
//!
//! The walk reads only the device's own [`Identity`] and the rules, so a
//! hotplugged device lands where it would at start. The guards read the
//! set of devices present and never their order, so a placement is the
//! same however the devices were reported (docs/architecture.md,
//! "Determinism").
//!
//! The policy decides what the rules leave unsaid: where an input no rule
//! matches goes, whether a head that leaves `isolated` unset is isolated,
//! whether the anti-lockout guard and the rescue key are on, and what the
//! display guard may light (D3, D9). What survives of a file with errors
//! runs under open with the shipped rules' tail after its rules and no
//! head isolated, and under closed as it is written (D7).

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use crate::config::{Config, Target};
use crate::identity::{Class, Identity, Kind};
use crate::outcome::{Policy, SwitchKey};

/// The compiled-in configuration, whose input rules are the shipped rules'
/// tail.
static SHIPPED: LazyLock<Config> = LazyLock::new(Config::builtin);

/// Where a device lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// The head at this index of [`Config::heads`].
    Head(usize),
    /// The default head, `[head console]` of the compiled-in
    /// configuration, which vtium builds as its only head: under open, the
    /// display guard lights it when what survives of a file with errors
    /// runs no head, and [`Config::place_on`] puts every display in it when
    /// the file defines nothing ([`Config::uses_builtin`]). It is never a
    /// head of the file.
    DefaultHead,
    /// An input that roams, typing into the active head.
    Roaming,
    /// An input vtium never opens, but under open a keyboard watched for
    /// the VT keys alone, the rescue key among them.
    Ignore,
    /// A display set off.
    Off,
    /// The boot display, which the display guard lights under closed for a
    /// notice alone: no head holds it, and no input reaches a session
    /// through it.
    Notice,
    /// A device that stays with the head at this index of
    /// [`Config::heads`], which runs on other VTs only (ADR-1401 D3, step
    /// 4). On the VT placed for, a display stays dark and a keyboard types
    /// nowhere, though vtium watches a keyboard for the VT keys, under open
    /// and where its head is not isolated (D3, "Who switches VTs"). Only
    /// [`Config::walk_on`] and [`Config::place_on`] give it.
    Elsewhere(usize),
    /// Under open, a display that no path catches on the VT placed for, and
    /// that no rule turns off, which shows a notice that says why there is
    /// no terminal on it (ADR-1401 D3): it is configured for the head at
    /// this index of [`Config::heads`], whose `vt` names other VTs, or with
    /// `None` for no terminal. Only [`Config::place_on`] gives it; under
    /// closed such a display stays blank.
    Unused(Option<usize>),
}

impl Place {
    fn is_head(self) -> bool {
        matches!(self, Place::Head(_) | Place::DefaultHead)
    }
}

/// What placed a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    /// The rule on this line.
    Rule(usize),
    /// No rule matched: a display goes off, and an input roams under open
    /// and is ignored under closed.
    NoMatch,
    /// One of the shipped rules' input rules, which place what no surviving
    /// rule of a partly broken open file places (D7).
    Tail,
    /// The display guard lit the boot display.
    NeverDark,
    /// The anti-lockout guard made a keyboard roam.
    AntiLockout,
    /// The file defines nothing, so under open the built-in default placed
    /// the device on its VT: a display in the default head, and an input by
    /// the shipped rules' input rules (ADR-1401 D3).
    Builtin,
}

/// A device's place and what put it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub place: Place,
    pub by: By,
}

/// A set of devices placed and guarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Each device's place, in the order the devices were given.
    pub placed: Vec<Placed>,
    /// Whether the display guard lit a display, for a head or for a notice.
    pub never_dark: bool,
    /// Whether the anti-lockout guard acted: made a keyboard roam, or moved
    /// the active head.
    pub anti_lockout: bool,
    /// The active head once the guards have run: a [`Place::Head`] that is
    /// not isolated or [`Place::DefaultHead`], or `None` with no head that
    /// can be the active head (D5).
    pub focus: Option<Place>,
}

/// Whether vtium opens an input (D9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Opened, and its events delivered.
    Deliver,
    /// Opened while vtium's VT is shown to watch for the VT keys alone,
    /// `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>`, the rescue key among them;
    /// nothing else it sends is delivered or logged.
    RescueOnly,
    /// Opened to watch for Enter, which switches to the VT of the head on
    /// another VT that it stays with, and for the rescue key (ADR-1401 D3,
    /// "Enter on a head's own keyboard"); any other key it sends only wakes
    /// the notices, and reaches no session.
    EnterAndRescue,
    /// Never opened.
    Closed,
}

/// A layer 1 warning of D9, which `vtium check-config` and `vtium config
/// check` give for the devices present; each names a head by its index or
/// a device by its place in the set given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lockout {
    /// The head claims pointers and no keyboard, while every keyboard is
    /// claimed by another head.
    PointersOnly { head: usize },
    /// Every keyboard is ignored.
    EveryKeyboardIgnored,
    /// The head's only keyboard is this one device.
    OneKeyboard { head: usize, device: usize },
}

/// The order the display guard picks the boot display by: the boot card
/// first, then the other cards in the kernel's order, then the kernel's
/// order of each card's connectors; the whole identity breaks a tie, so
/// the order of the set never does.
fn boot_order(a: &Identity, b: &Identity) -> Ordering {
    (!a.on_boot_card, a.card_order, a.order, a).cmp(&(!b.on_boot_card, b.card_order, b.order, b))
}

/// The boot display of `devices`, by its index: the connector lit at
/// start, else the first connected one in the kernel's order on the boot
/// card (D9), else, the team lead's decision of 2026-10-29 so that a
/// connected display always lights, the first connected one on the other
/// cards in the kernel's order.
///
/// A display that `placed` ties to a head on another VT is never the boot
/// display (ADR-1401 D3, step 4).
fn boot_display(devices: &[Identity], placed: &[Placed]) -> Option<usize> {
    boot_display_of(devices, placed, |p| !matches!(p.place, Place::Elsewhere(_)))
}

/// The display the display guard lights (D9): the [`boot_display`], else,
/// when a rule ties every connected display to a head of another VT, the
/// boot display among those, so that a VT never stays dark for what
/// survives of a file with errors.
fn guard_display(devices: &[Identity], placed: &[Placed]) -> Option<usize> {
    boot_display(devices, placed).or_else(|| boot_display_of(devices, placed, |_| true))
}

/// The boot display among the displays whose placement `among` keeps.
fn boot_display_of(
    devices: &[Identity],
    placed: &[Placed],
    among: impl Fn(&Placed) -> bool,
) -> Option<usize> {
    let pick = |keep: fn(&Identity) -> bool| {
        devices
            .iter()
            .zip(placed)
            .enumerate()
            .filter(|(_, (d, p))| d.kind == Kind::Display && among(p) && keep(d))
            .min_by(|(_, (a, _)), (_, (b, _))| boot_order(a, b))
            .map(|(i, _)| i)
    };
    // `boot_order` puts the boot card's connectors before every other card's
    pick(|d| d.lit_at_start).or_else(|| pick(|_| true))
}

/// Whether `id` is a keyboard-class input, which the anti-lockout guard
/// makes roam.
fn keyboard_class(id: &Identity) -> bool {
    id.kind == Kind::Input && id.classes.has(Class::Keyboard)
}

/// Whether `id` counts as a keyboard that reaches a head (D9): udev's
/// keyboard class, less the devices marked a token or a numpad.
fn counts(id: &Identity) -> bool {
    id.kind == Kind::Input && id.counts_as_keyboard()
}

impl Config {
    /// Whether the shipped rules' tail runs after the rules: for what
    /// survives of a file with errors under open (D7).
    fn tail(&self) -> bool {
        self.survivors && self.policy == Policy::Open
    }

    /// Whether the head at `head` of [`Config::heads`] is isolated (D5): as
    /// it writes `isolated`, else as the policy leaves it, `no` under open
    /// and `yes` under closed. What survives of an open file with errors
    /// isolates no head, so the rescue key and the active head always have
    /// somewhere to land (D7). A head the file does not declare is not.
    pub fn isolated(&self, head: usize) -> bool {
        let unset = self.policy.unset(SwitchKey::Isolated);
        !self.tail()
            && self
                .heads
                .get(head)
                .is_some_and(|h| h.isolated.unwrap_or(unset))
    }

    /// Whether Enter, pressed on a keyboard that stays with the head at
    /// `head` on a VT where that head does not run, switches to the head's
    /// VT (ADR-1401 D3, "Enter on a head's own keyboard"): under open, for
    /// a head that writes `switch-on-enter = yes` and is not isolated.
    pub fn switch_on_enter(&self, head: usize) -> bool {
        self.policy == Policy::Open
            && !self.isolated(head)
            && self.heads.get(head).is_some_and(|h| h.switch_on_enter)
    }

    /// Whether the head at `head` runs on `vt`, where `None` names no VT
    /// and every head runs (ADR-1401 D2).
    fn runs(&self, head: usize, vt: Option<u16>) -> bool {
        vt.is_none_or(|vt| self.heads.get(head).is_some_and(|h| h.runs_on(vt)))
    }

    /// The heads that run on `vt`, by index, in the file's order.
    fn heads_on(&self, vt: Option<u16>) -> impl Iterator<Item = usize> + '_ {
        (0..self.heads.len()).filter(move |&i| self.runs(i, vt))
    }

    /// The heads the vtium on VT `vt` builds, by their index in
    /// [`Config::heads`], in the file's order: each head whose `vt` names
    /// `vt`, and each head with no `vt` (ADR-1401 D2). When
    /// [`Config::uses_builtin`] holds, vtium builds the default head alone
    /// instead.
    pub fn heads_on_vt(&self, vt: u16) -> Vec<usize> {
        self.heads_on(Some(vt)).collect()
    }

    /// Whether vtium runs the built-in default terminal: under open, when
    /// the file defines nothing, no head and no rule (the maintainer's
    /// table, ADR-1401 D3). The answer is the same on every VT, and where
    /// vtium has no VT number. vtium then builds the default head alone,
    /// [`Place::DefaultHead`], and [`Config::walk_on`] places every display
    /// in it and every input by the shipped rules' input rules. A file that
    /// defines paths never runs it, even on a VT where none of its heads
    /// runs: there each display shows its notice under open
    /// ([`Place::Unused`]) and stays blank under closed.
    pub fn uses_builtin(&self) -> bool {
        self.policy == Policy::Open
            && !self.survivors
            && self.heads.is_empty()
            && self.rules.is_empty()
    }

    /// Whether the vtium on VT `vt` builds the default head, where `None`
    /// names no VT number and every head runs: where it runs the built-in
    /// default terminal ([`Config::uses_builtin`]), and, for what survives
    /// of a file with errors under open, where no head of the file runs on
    /// `vt`. The display guard then lights the boot display in the default
    /// head, with its login (ADR-1401 D9, the maintainer, 2026-11-12).
    pub fn default_head_on(&self, vt: Option<u16>) -> bool {
        self.uses_builtin() || self.tail() && self.heads_on(vt).next().is_none()
    }

    /// Whether a head placed at `place` can be the active head on `vt`: a
    /// head the file declares that runs on `vt` and is not isolated, or the
    /// default head where no declared head runs on `vt`, which is never
    /// isolated (D5).
    fn can_be_active(&self, place: Place, vt: Option<u16>) -> bool {
        match place {
            Place::Head(i) => i < self.heads.len() && self.runs(i, vt) && !self.isolated(i),
            // the default head runs under open alone, where no head of the
            // file runs on `vt`
            Place::DefaultHead => self.policy == Policy::Open && self.heads_on(vt).next().is_none(),
            Place::Roaming
            | Place::Ignore
            | Place::Off
            | Place::Notice
            | Place::Elsewhere(_)
            | Place::Unused(_) => false,
        }
    }

    /// Where the shipped rules' input rules place the input `id`, with the
    /// file's `mouse` shorthand applied: roaming or ignored. The rules name
    /// no head, and what they would not place stays closed.
    fn shipped_input(&self, id: &Identity) -> Place {
        let tail = self.tail.as_deref().unwrap_or(&SHIPPED.rules);
        let shipped = tail
            .iter()
            .find(|r| r.kind == Kind::Input && r.matcher.matches(id));
        match shipped.map(|r| r.target) {
            Some(Target::Roaming) => Place::Roaming,
            Some(Target::Head(_) | Target::Ignore | Target::Off) | None => Place::Ignore,
        }
    }

    /// Places one device by the first rule of its kind that matches it,
    /// reading nothing but `id` and the rules, with every head running, as
    /// [`Config::walk_on`] places for a VT. With no match a display goes
    /// off; an input roams under open and is ignored under closed (D3),
    /// and what survives of an open file with errors walks the shipped
    /// rules' input rules instead, which roam the keyboard, pointer and key
    /// classes and ignore every other input (D7).
    pub fn walk(&self, id: &Identity) -> Placed {
        self.walk_at(id, None)
    }

    /// Places one device for the vtium on VT `vt` by the four steps of
    /// ADR-1401 D3, reading nothing but `id`, the rules and `vt`:
    ///
    /// 1. A rule that does not match the device is read past.
    /// 2. A rule that matches, and whose target runs on `vt`, places the
    ///    device. Once step 3 has noted a head, only a rule whose head's
    ///    `vt` names `vt` can: a head with no `vt`, `roaming`, `ignore` and
    ///    `off` leave the device alone, and the walk reads on.
    /// 3. A rule that matches, but whose head's `vt` names other VTs only,
    ///    notes that the device belongs to that head, and the walk reads on.
    /// 4. When the rules end with no rule placing the device, a device with
    ///    a noted head stays with the first head noted,
    ///    [`Place::Elsewhere`], by the rule that noted it. One with none
    ///    falls to the policy, as [`Config::walk`] says.
    ///
    /// Where [`Config::uses_builtin`] holds, the built-in default places
    /// the device instead, by [`By::Builtin`].
    pub fn walk_on(&self, id: &Identity, vt: u16) -> Placed {
        self.walk_at(id, Some(vt))
    }

    /// [`Config::walk_on`] for `vt`, where `None` names no VT and every
    /// head runs.
    fn walk_at(&self, id: &Identity, vt: Option<u16>) -> Placed {
        if self.uses_builtin() {
            return Placed {
                place: match id.kind {
                    Kind::Display => Place::DefaultHead,
                    Kind::Input => self.shipped_input(id),
                },
                by: By::Builtin,
            };
        }
        // the first head step 3 noted, with the line of its rule
        let mut noted: Option<(usize, usize)> = None;
        for rule in self
            .rules
            .iter()
            .filter(|r| r.kind == id.kind && r.matcher.matches(id))
        {
            let place = match rule.target {
                Target::Head(i) if !self.runs(i, vt) => {
                    noted.get_or_insert((i, rule.line));
                    continue;
                }
                Target::Head(i) => Place::Head(i),
                Target::Roaming => Place::Roaming,
                Target::Ignore => Place::Ignore,
                Target::Off => Place::Off,
            };
            // the exception in step 2: a device tied to a head on another
            // VT goes only to a head whose `vt` names this one
            let names_vt =
                |i: usize| vt.is_some_and(|vt| self.heads.get(i).is_some_and(|h| h.names_vt(vt)));
            if noted.is_some() && !matches!(place, Place::Head(i) if names_vt(i)) {
                continue;
            }
            return Placed {
                place,
                by: By::Rule(rule.line),
            };
        }
        if let Some((head, line)) = noted {
            return Placed {
                place: Place::Elsewhere(head),
                by: By::Rule(line),
            };
        }
        match id.kind {
            Kind::Display => Placed {
                place: Place::Off,
                by: By::NoMatch,
            },
            Kind::Input if self.tail() => Placed {
                place: self.shipped_input(id),
                by: By::Tail,
            },
            Kind::Input => Placed {
                place: match self.policy {
                    Policy::Open => Place::Roaming,
                    Policy::Closed => Place::Ignore,
                },
                by: By::NoMatch,
            },
        }
    }

    /// Places every device of a set, then runs the guards over the result,
    /// as vtium does at start and after every device added or removed
    /// (D9). [`Config::place_rescued`] with no keyboard the rescue key
    /// moved.
    pub fn place(&self, devices: &[Identity], focus: Option<Place>) -> Placement {
        self.place_rescued(devices, &[], focus)
    }

    /// [`Config::place`] for the vtium on VT `vt`: each device walks the
    /// rules by [`Config::walk_on`], and the guards act for `vt`, as
    /// [`Config::place_rescued_on`] says.
    pub fn place_on(&self, devices: &[Identity], focus: Option<Place>, vt: u16) -> Placement {
        self.place_rescued_on(devices, &[], focus, vt)
    }

    /// [`Config::place_rescued`] for the vtium on VT `vt` (ADR-1401 D2,
    /// D3). Each device walks the rules by [`Config::walk_on`], and only a
    /// head that runs on `vt` can be the active head. A device that stays
    /// with a head on another VT, [`Place::Elsewhere`], is left where it
    /// is: the anti-lockout guard never makes such a keyboard roam, so it
    /// types nowhere on `vt` (ADR-1403 D7, path tests 5 to 7). Under open
    /// the rescue key still moves it.
    ///
    /// The table and the display guard act as [`Config::place_rescued`]
    /// says.
    pub fn place_rescued_on(
        &self,
        devices: &[Identity],
        rescued: &[Identity],
        focus: Option<Place>,
        vt: u16,
    ) -> Placement {
        self.place_at(devices, rescued, focus, Some(vt))
    }

    /// Places every device of `devices`, then runs the guards over the
    /// result: the display guard first, then the anti-lockout guard (D9).
    /// `rescued` are the keyboards the rescue key moved, which roam
    /// whatever the rules say and are not placed again.
    ///
    /// For a valid file, the maintainer's table replaces the display guard
    /// (D3, D9), with or without a VT number (the maintainer, 2026-11-13).
    /// Under open, each display that no path catches, and that no rule
    /// turns off, shows a notice that says why, [`Place::Unused`]: a
    /// display tied to a head on another VT names that head, and one no
    /// rule claims names none. Under closed, such a display stays blank. A
    /// file that defines nothing runs the built-in default under open
    /// ([`Config::uses_builtin`]). Only what survives of a file with errors
    /// runs the display guard (D7): when the rules light no display, it
    /// lights the boot display, under open for the first head that runs,
    /// else the default head, and under closed for a notice alone.
    ///
    /// `focus` is the active head, or `None` before there is one. The
    /// active head starts where D5 starts it: on the head holding the boot
    /// display, else the first head in the file with a lit display, taking
    /// only a head that can be the active head. A `focus` that cannot be
    /// one starts again the same way. A roaming device reaches a lit head
    /// only when the active head is lit.
    ///
    /// Under open, when no keyboard reaches a lit head, the anti-lockout
    /// guard makes every keyboard-class device roam and moves the active
    /// head by the same rule as its start; while no lit head can be the
    /// active head it has nothing to move them to and acts not. Otherwise
    /// the active head stays where it is, lit or not. With no keyboard present there is none to strand. Under
    /// closed the guard is off.
    pub fn place_rescued(
        &self,
        devices: &[Identity],
        rescued: &[Identity],
        focus: Option<Place>,
    ) -> Placement {
        self.place_at(devices, rescued, focus, None)
    }

    /// [`Config::place_rescued_on`] for `vt`, where `None` names no VT and
    /// every head runs.
    fn place_at(
        &self,
        devices: &[Identity],
        rescued: &[Identity],
        focus: Option<Place>,
        vt: Option<u16>,
    ) -> Placement {
        let mut placed: Vec<Placed> = devices.iter().map(|d| self.walk_at(d, vt)).collect();
        let lit_heads = |placed: &[Placed]| -> BTreeSet<Place> {
            devices
                .iter()
                .zip(placed)
                .filter(|(d, p)| d.kind == Kind::Display && p.place.is_head())
                .map(|(_, p)| p.place)
                .collect()
        };
        let mut never_dark = false;
        // the maintainer's table replaces the display guard for a valid
        // file, on a VT or not; what survives of a file with errors keeps
        // it (D7)
        let table = !self.survivors;
        if !table && lit_heads(&placed).is_empty() {
            // under open the first head of this VT, else the default head;
            // under closed a notice alone
            let place = match self.policy {
                Policy::Open => self
                    .heads_on(vt)
                    .next()
                    .map_or(Place::DefaultHead, Place::Head),
                Policy::Closed => Place::Notice,
            };
            let boot = guard_display(devices, &placed).and_then(|i| placed.get_mut(i));
            if let Some(p) = boot {
                *p = Placed {
                    place,
                    by: By::NeverDark,
                };
                never_dark = true;
            }
        }
        let lit = lit_heads(&placed);
        // the boot display's head, else the first lit head in the file, of
        // the heads that can be the active head
        let start = boot_display(devices, &placed)
            .and_then(|i| placed.get(i))
            .map(|p| p.place)
            .filter(|&place| lit.contains(&place) && self.can_be_active(place, vt))
            .or_else(|| lit.iter().copied().find(|&p| self.can_be_active(p, vt)));
        let mut focus = focus.filter(|&f| self.can_be_active(f, vt)).or(start);
        let mut anti_lockout = false;
        if self.policy.unset(SwitchKey::AntiLockout) {
            let focus_lit = focus.is_some_and(|f| lit.contains(&f));
            // a keyboard that stays with a head on another VT is that
            // head's, and the guard neither counts nor moves it
            let movable = |(d, p): (&Identity, &Placed)| {
                keyboard_class(d) && !matches!(p.place, Place::Elsewhere(_))
            };
            let present =
                devices.iter().zip(&placed).any(movable) || rescued.iter().any(keyboard_class);
            let reaches = rescued.iter().any(counts) && focus_lit
                || devices.iter().zip(&placed).any(|(d, p)| {
                    counts(d)
                        && match p.place {
                            Place::Roaming => focus_lit,
                            place => lit.contains(&place),
                        }
                });
            // with no lit head that can be the active head there is none to
            // move the keyboards to, and a head's own keyboards keep typing
            // into it unlit (D8)
            if present && !reaches && start.is_some() {
                for (d, p) in devices.iter().zip(placed.iter_mut()) {
                    if movable((d, p)) && p.place != Place::Roaming {
                        *p = Placed {
                            place: Place::Roaming,
                            by: By::AntiLockout,
                        };
                        anti_lockout = true;
                    }
                }
                if start != focus {
                    focus = start;
                    anti_lockout = true;
                }
            }
        }
        if table && self.policy == Policy::Open {
            // under open, each display that no path catches, and that no
            // rule turns off, shows a notice that says why; under closed it
            // stays blank (ADR-1401 D3)
            for (d, p) in devices.iter().zip(placed.iter_mut()) {
                if d.kind != Kind::Display {
                    continue;
                }
                match (p.place, p.by) {
                    (Place::Elsewhere(head), _) => p.place = Place::Unused(Some(head)),
                    (Place::Off, By::NoMatch) => p.place = Place::Unused(None),
                    _ => {}
                }
            }
        }
        Placement {
            placed,
            never_dark,
            anti_lockout,
            focus,
        }
    }

    /// Whether `Ctrl-Alt-F<n>` for the shown VT, pressed on a keyboard
    /// placed so, makes it roam for the rest of the boot (D9): under open,
    /// from a head that is not isolated, the default head, `ignore`, or a
    /// head on another VT that the keyboard stays with.
    /// Under closed the rescue key is off.
    pub fn rescue(&self, placed: &Placed) -> bool {
        self.policy.unset(SwitchKey::RescueKey)
            && match placed.place {
                Place::Head(i) => i < self.heads.len() && !self.isolated(i),
                // a keyboard that types nowhere on this VT, as an ignored
                // one does (ADR-1401 D3, step 4)
                Place::DefaultHead | Place::Ignore | Place::Elsewhere(_) => true,
                Place::Roaming | Place::Off | Place::Notice | Place::Unused(_) => false,
            }
    }

    /// Whether vtium opens an input placed so. A keyboard-class device that
    /// stays with a head on another VT is opened for the VT keys alone, the
    /// rescue key among them, under open, and under closed where that head
    /// is not isolated, since a path places it and its VT keys switch the VT
    /// (D3, "Who switches VTs"); one whose head switches on Enter is opened
    /// for Enter too. Under open an ignored keyboard-class device is opened
    /// for the VT keys alone. Every other such device, and under closed an
    /// ignored one, stays closed (D3, D9).
    pub fn disposition(&self, placed: &Placed, id: &Identity) -> Disposition {
        let keyboard = id.classes.has(Class::Keyboard);
        let open = self.policy.unset(SwitchKey::RescueKey);
        match placed.place {
            Place::Elsewhere(i) if keyboard && open && self.switch_on_enter(i) => {
                Disposition::EnterAndRescue
            }
            Place::Elsewhere(i) if keyboard && (open || !self.isolated(i)) => {
                Disposition::RescueOnly
            }
            Place::Ignore if keyboard && open => Disposition::RescueOnly,
            Place::Ignore | Place::Elsewhere(_) | Place::Off | Place::Notice | Place::Unused(_) => {
                Disposition::Closed
            }
            Place::Head(_) | Place::DefaultHead | Place::Roaming => Disposition::Deliver,
        }
    }

    /// The layer 1 warnings of D9 for `devices` as the rules alone place
    /// them, before any guard acts: a head that claims only pointers while
    /// every keyboard is claimed elsewhere, every keyboard ignored, and a
    /// head whose only keyboard is one device.
    pub fn lockout(&self, devices: &[Identity]) -> Vec<Lockout> {
        let placed: Vec<Placed> = devices.iter().map(|d| self.walk(d)).collect();
        let keyboards: Vec<(usize, Place)> = devices
            .iter()
            .zip(&placed)
            .enumerate()
            .filter(|(_, (d, _))| d.kind == Kind::Input && d.counts_as_keyboard())
            .map(|(i, (_, p))| (i, p.place))
            .collect();
        let mut out = Vec::new();
        for head in 0..self.heads.len() {
            let here = Place::Head(head);
            let mine: Vec<usize> = keyboards
                .iter()
                .filter(|(_, p)| *p == here)
                .map(|(i, _)| *i)
                .collect();
            let pointers = devices
                .iter()
                .zip(&placed)
                .any(|(d, p)| d.kind == Kind::Input && p.place == here && d.classes.any_pointer());
            let all_elsewhere = !keyboards.is_empty()
                && keyboards
                    .iter()
                    .all(|(_, p)| matches!(p, Place::Head(_)) && *p != here);
            if pointers && mine.is_empty() && all_elsewhere {
                out.push(Lockout::PointersOnly { head });
            }
            if let [device] = mine.as_slice() {
                out.push(Lockout::OneKeyboard {
                    head,
                    device: *device,
                });
            }
        }
        if !keyboards.is_empty() && keyboards.iter().all(|(_, p)| *p == Place::Ignore) {
            out.push(Lockout::EveryKeyboardIgnored);
        }
        out
    }

    /// The lines of the rules that match no device of `devices`, which a
    /// start skips with a log line (D3).
    pub fn idle_rules(&self, devices: &[Identity]) -> Vec<usize> {
        self.rules
            .iter()
            .filter(|r| {
                !devices
                    .iter()
                    .any(|d| d.kind == r.kind && r.matcher.matches(d))
            })
            .map(|r| r.line)
            .collect()
    }
}

/// Whether `value` can be written as a token's value.
fn writable(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '#')
}

/// The rule line that would send display `id` to `target`, ready to paste
/// into the log line of a display no rule claims (D3): by its EDID, the
/// recommended key, else its `PATH`, else its card and connector (D4).
/// `None` when the display offers no value a token can hold.
pub fn rule_for(id: &Identity, target: &str) -> Option<String> {
    if id.kind != Kind::Display {
        return None;
    }
    let edid = id.edid.as_ref().filter(|e| {
        e.maker.len() == 3 && e.maker.bytes().all(|b| b.is_ascii_uppercase()) && writable(&e.serial)
    });
    let token = if let Some(e) = edid {
        format!("edid:{}:{:#06x}:{}", e.maker, e.product, e.serial)
    } else if let Some(path) = id
        .connector_path
        .as_deref()
        .filter(|p| writable(p) && !p.contains(['*', '?']))
    {
        format!("connector-path:{path}")
    } else {
        let card = id
            .card
            .as_deref()
            .filter(|c| writable(c) && !c.contains(['*', '?']))?;
        let connector = id
            .connector
            .as_deref()
            .filter(|c| writable(c) && !c.contains(['/', '*', '?']))?;
        format!("connector:{card}/{connector}")
    };
    Some(format!("display {token} -> {target}"))
}
