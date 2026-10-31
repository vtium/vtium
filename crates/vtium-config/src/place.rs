//! Where each device lands: the first-match walk of one device (ADR-1401
//! D3) and the guards over a set of devices (D9), under the file's policy.
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
    /// configuration, which the display guard lights under open when the
    /// file declares no head.
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
    /// Opened while vtium's VT is shown to watch for the rescue key alone;
    /// nothing else it sends is delivered or logged.
    RescueOnly,
    /// Never opened.
    Closed,
}

/// A layer 1 warning of D9, which `vtium check-config` and `vtiumctl
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
pub(crate) fn boot_display(devices: &[Identity]) -> Option<usize> {
    let pick = |keep: fn(&Identity) -> bool| {
        devices
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == Kind::Display && keep(d))
            .min_by(|(_, a), (_, b)| boot_order(a, b))
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

    /// Whether a head placed at `place` can be the active head: a head the
    /// file declares that is not isolated, or the default head of a file
    /// that declares none, which is never isolated (D5).
    fn can_be_active(&self, place: Place) -> bool {
        match place {
            Place::Head(i) => i < self.heads.len() && !self.isolated(i),
            // the display guard lights the default head under open alone
            Place::DefaultHead => self.heads.is_empty() && self.policy == Policy::Open,
            Place::Roaming | Place::Ignore | Place::Off | Place::Notice => false,
        }
    }

    /// Places one device by the first rule of its kind that matches it,
    /// reading nothing but `id` and the rules. With no match a display goes
    /// off; an input roams under open and is ignored under closed (D3),
    /// and what survives of an open file with errors walks the shipped
    /// rules' input rules instead, which roam the keyboard, pointer and key
    /// classes and ignore every other input (D7).
    pub fn walk(&self, id: &Identity) -> Placed {
        let first = self
            .rules
            .iter()
            .find(|r| r.kind == id.kind && r.matcher.matches(id));
        if let Some(rule) = first {
            return Placed {
                place: match rule.target {
                    Target::Head(i) => Place::Head(i),
                    Target::Roaming => Place::Roaming,
                    Target::Ignore => Place::Ignore,
                    Target::Off => Place::Off,
                },
                by: By::Rule(rule.line),
            };
        }
        match id.kind {
            Kind::Display => Placed {
                place: Place::Off,
                by: By::NoMatch,
            },
            Kind::Input if self.tail() => {
                let shipped = SHIPPED
                    .rules
                    .iter()
                    .find(|r| r.kind == Kind::Input && r.matcher.matches(id));
                Placed {
                    // the tail names no head, and what it would not place
                    // stays closed
                    place: match shipped.map(|r| r.target) {
                        Some(Target::Roaming) => Place::Roaming,
                        Some(Target::Head(_) | Target::Ignore | Target::Off) | None => {
                            Place::Ignore
                        }
                    },
                    by: By::Tail,
                }
            }
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
        let mut placed: Vec<Placed> = devices.iter().map(|d| self.walk(d)).collect();
        let lit_heads = |placed: &[Placed]| -> BTreeSet<Place> {
            devices
                .iter()
                .zip(placed)
                .filter(|(d, p)| d.kind == Kind::Display && p.place.is_head())
                .map(|(_, p)| p.place)
                .collect()
        };
        let mut never_dark = false;
        if lit_heads(&placed).is_empty() {
            // under open the first head, else the default head; under
            // closed a notice alone, for a file with heads or one that
            // failed, and nothing for a valid closed file with no head
            let place = match self.policy {
                Policy::Open if self.heads.is_empty() => Some(Place::DefaultHead),
                Policy::Open => Some(Place::Head(0)),
                Policy::Closed if self.heads.is_empty() && !self.survivors => None,
                Policy::Closed => Some(Place::Notice),
            };
            let boot = boot_display(devices).and_then(|i| placed.get_mut(i));
            if let (Some(place), Some(p)) = (place, boot) {
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
        let start = boot_display(devices)
            .and_then(|i| placed.get(i))
            .map(|p| p.place)
            .filter(|&place| lit.contains(&place) && self.can_be_active(place))
            .or_else(|| lit.iter().copied().find(|&p| self.can_be_active(p)));
        let mut focus = focus.filter(|&f| self.can_be_active(f)).or(start);
        let mut anti_lockout = false;
        if self.policy.unset(SwitchKey::AntiLockout) {
            let focus_lit = focus.is_some_and(|f| lit.contains(&f));
            let present = devices.iter().any(keyboard_class) || rescued.iter().any(keyboard_class);
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
                    if keyboard_class(d) && p.place != Place::Roaming {
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
        }
    }

    /// Whether `Ctrl-Alt-F<n>` for the shown VT, pressed on a keyboard
    /// placed so, makes it roam for the rest of the boot (D9): under open,
    /// from a head that is not isolated, the default head or `ignore`.
    /// Under closed the rescue key is off.
    pub fn rescue(&self, placed: &Placed) -> bool {
        self.policy.unset(SwitchKey::RescueKey)
            && match placed.place {
                Place::Head(i) => i < self.heads.len() && !self.isolated(i),
                Place::DefaultHead | Place::Ignore => true,
                Place::Roaming | Place::Off | Place::Notice => false,
            }
    }

    /// Whether vtium opens an input placed so: under open an ignored
    /// keyboard-class device is opened for the rescue key alone; every
    /// other ignored device, and under closed every ignored one, stays
    /// closed (D9).
    pub fn disposition(&self, placed: &Placed, id: &Identity) -> Disposition {
        match placed.place {
            Place::Ignore
                if self.policy.unset(SwitchKey::RescueKey) && id.classes.has(Class::Keyboard) =>
            {
                Disposition::RescueOnly
            }
            Place::Ignore | Place::Off | Place::Notice => Disposition::Closed,
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
    } else if let Some(path) = id.connector_path.as_deref().filter(|p| writable(p)) {
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
