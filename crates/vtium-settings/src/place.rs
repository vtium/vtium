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
    /// display guard lights it when the file declares no head, and on a VT
    /// where no head of the file runs every display goes to it
    /// ([`Config::uses_builtin_on`]). It is never a head of the file.
    DefaultHead,
    /// An input that roams, typing into the active head.
    Roaming,
    /// An input vtium never opens, but under open a keyboard watched for
    /// the rescue key alone.
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
    /// nowhere, though under open vtium watches a keyboard for the rescue
    /// key. Only [`Config::walk_on`] and [`Config::place_on`] give it.
    Elsewhere(usize),
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
    /// No head of the file runs on the VT placed for, so under open the
    /// built-in default placed the device: a display in the default head,
    /// and an input by the shipped rules' input rules (ADR-1401 D3).
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
    /// Why the VT placed for shows a notice in place of a terminal, when
    /// the display guard lit the boot display for a notice because of the
    /// heads' VTs. Only [`Config::place_on`] gives it.
    pub vt_notice: Option<VtNotice>,
}

/// Why a VT shows a notice on its boot display in place of a terminal
/// (ADR-1401 D3). On every VT it runs on, vtium shows either a terminal or
/// a notice that says why there is none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VtNotice {
    /// The heads that run on this VT and have no display, by their index
    /// in [`Config::heads`]. Empty when no head of the file runs on this VT.
    pub heads: Vec<usize>,
    /// Each connected display that stays with a head on another VT.
    pub tied: Vec<Tied>,
}

/// A display that stays with a head on another VT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tied {
    /// The display, by its place in the devices given.
    pub device: usize,
    /// The head it belongs to, by its index in [`Config::heads`].
    pub head: usize,
    /// The VTs that head runs on, its `vt`.
    pub vts: Vec<u16>,
}

/// Whether vtium opens an input (D9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Opened, and its events delivered.
    Deliver,
    /// Opened while vtium's VT is shown to watch for the rescue key alone;
    /// nothing else it sends is delivered or logged.
    RescueOnly,
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
/// With `tied_dark`, a display that `placed` ties to a head on another VT
/// is never the boot display, since it stays dark on this one (ADR-1401
/// D3, step 4).
fn boot_display(devices: &[Identity], placed: &[Placed], tied_dark: bool) -> Option<usize> {
    let pick = |keep: fn(&Identity) -> bool| {
        devices
            .iter()
            .zip(placed)
            .enumerate()
            .filter(|(_, (d, p))| {
                d.kind == Kind::Display
                    && !(tied_dark && matches!(p.place, Place::Elsewhere(_)))
                    && keep(d)
            })
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
    /// `vt`, and each head with no `vt` (ADR-1401 D2). When it is empty and
    /// [`Config::uses_builtin_on`] holds, vtium builds the default head
    /// alone.
    pub fn heads_on_vt(&self, vt: u16) -> Vec<usize> {
        self.heads_on(Some(vt)).collect()
    }

    /// Whether the vtium on VT `vt` runs the built-in default: under open,
    /// when the file declares heads and none of them runs on `vt`. vtium
    /// then builds the default head alone, [`Place::DefaultHead`], and
    /// [`Config::walk_on`] places every display in it and every input by
    /// the shipped rules' input rules, so the VT shows a terminal.
    /// Under closed such a VT shows the closed policy's notice on its boot
    /// display instead ([`Config::place_on`]). A file that declares no head
    /// keeps the display guard's default head (the maintainer, 2026-11-10;
    /// ADR-1401 D3).
    pub fn uses_builtin_on(&self, vt: u16) -> bool {
        self.builtin_at(Some(vt))
    }

    /// [`Config::uses_builtin_on`] for `vt`, where `None` names no VT and
    /// every head runs.
    fn builtin_at(&self, vt: Option<u16>) -> bool {
        self.policy == Policy::Open && !self.heads.is_empty() && self.heads_on(vt).next().is_none()
    }

    /// Whether the file declares heads and none of them runs on `vt`.
    fn empty_vt(&self, vt: Option<u16>) -> bool {
        !self.heads.is_empty() && self.heads_on(vt).next().is_none()
    }

    /// Whether a head placed at `place` can be the active head on `vt`: a
    /// head the file declares that runs on `vt` and is not isolated, or the
    /// default head where no declared head runs on `vt`, which is never
    /// isolated (D5).
    fn can_be_active(&self, place: Place, vt: Option<u16>) -> bool {
        match place {
            Place::Head(i) => i < self.heads.len() && self.runs(i, vt) && !self.isolated(i),
            // the default head runs under open alone, where no head of the file
            // runs
            Place::DefaultHead => self.heads_on(vt).next().is_none() && self.policy == Policy::Open,
            Place::Roaming | Place::Ignore | Place::Off | Place::Notice | Place::Elsewhere(_) => {
                false
            }
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
    /// On a VT where [`Config::uses_builtin_on`] holds, the built-in
    /// default places the device instead, by [`By::Builtin`].
    pub fn walk_on(&self, id: &Identity, vt: u16) -> Placed {
        self.walk_at(id, Some(vt))
    }

    /// [`Config::walk_on`] for `vt`, where `None` names no VT and every
    /// head runs.
    fn walk_at(&self, id: &Identity, vt: Option<u16>) -> Placed {
        if self.builtin_at(vt) {
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
    /// D3). Each device walks the rules by [`Config::walk_on`], and the
    /// guards take only the heads that run on `vt`: the display guard
    /// lights the boot display for the first of them, or for the default
    /// head when none runs on `vt`, and only one of them can be the active
    /// head. A device that stays with a head on another VT,
    /// [`Place::Elsewhere`], is left where it is: the display guard never
    /// lights such a display, and the anti-lockout guard never makes such
    /// a keyboard roam, so it types nowhere on `vt` (ADR-1403 D7, path
    /// tests 5 to 7). Under open the rescue key still moves it.
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
        let mut vt_notice = None;
        if lit_heads(&placed).is_empty() {
            // each display that stays with a head on another VT
            let tied: Vec<Tied> = devices
                .iter()
                .zip(&placed)
                .enumerate()
                .filter_map(|(device, (d, p))| match p.place {
                    Place::Elsewhere(head) if d.kind == Kind::Display => Some(Tied {
                        device,
                        head,
                        vts: self
                            .heads
                            .get(head)
                            .map(|h| h.vts.clone())
                            .unwrap_or_default(),
                    }),
                    _ => None,
                })
                .collect();
            let displays = devices.iter().filter(|d| d.kind == Kind::Display).count();
            // this VT's heads have no display because every display belongs
            // to a head on another VT
            let all_tied = !tied.is_empty() && tied.len() == displays;
            // under open the first head of this VT, else the default head;
            // under closed a notice alone, for a file with heads, on this VT
            // or not, or one that failed, and nothing for a valid closed
            // file with no head; and a notice alone, under either policy,
            // where every display is tied to another VT
            let first = self.heads_on(vt).next();
            let place = match self.policy {
                Policy::Open if all_tied => Some(Place::Notice),
                Policy::Open => Some(first.map_or(Place::DefaultHead, Place::Head)),
                Policy::Closed if self.heads.is_empty() && !self.survivors => None,
                Policy::Closed => Some(Place::Notice),
            };
            // the notice is the one exception to step 4 for a display: a
            // display tied to another VT's head can show it, when this VT
            // has no head or every display is tied
            let tied_dark = !(self.empty_vt(vt) || all_tied);
            let boot = boot_display(devices, &placed, tied_dark).and_then(|i| placed.get_mut(i));
            if let (Some(place), Some(p)) = (place, boot) {
                *p = Placed {
                    place,
                    by: By::NeverDark,
                };
                never_dark = true;
                // the notice says why when the heads' VTs are the reason:
                // no head runs here, or a display belongs to another VT
                if place == Place::Notice && (self.empty_vt(vt) || !tied.is_empty()) {
                    vt_notice = Some(VtNotice {
                        heads: self.heads_on(vt).collect(),
                        tied,
                    });
                }
            }
        }
        let lit = lit_heads(&placed);
        // the boot display's head, else the first lit head in the file, of
        // the heads that can be the active head
        let start = boot_display(devices, &placed, true)
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
        Placement {
            placed,
            never_dark,
            anti_lockout,
            focus,
            vt_notice,
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
                Place::Roaming | Place::Off | Place::Notice => false,
            }
    }

    /// Whether vtium opens an input placed so: under open an ignored
    /// keyboard-class device, or one that stays with a head on another VT,
    /// is opened for the rescue key alone; every other such device, and
    /// under closed every one, stays closed (D3, D9).
    pub fn disposition(&self, placed: &Placed, id: &Identity) -> Disposition {
        match placed.place {
            Place::Ignore | Place::Elsewhere(_)
                if self.policy.unset(SwitchKey::RescueKey) && id.classes.has(Class::Keyboard) =>
            {
                Disposition::RescueOnly
            }
            Place::Ignore | Place::Elsewhere(_) | Place::Off | Place::Notice => Disposition::Closed,
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
