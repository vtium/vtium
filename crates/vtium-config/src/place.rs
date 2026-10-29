//! Where each device lands: the first-match walk of one device (ADR-1401
//! D3) and the guards over a set of devices (D9).
//!
//! The walk reads only the device's own [`Identity`] and the rules, so a
//! hotplugged device lands where it would at start. The guards read the
//! set of devices present and never their order, so a placement is the
//! same however the devices were reported (docs/architecture.md,
//! "Determinism").

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::config::{Config, Target};
use crate::identity::{Class, Identity, Kind};

/// Where a device lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// The head at this index of [`Config::heads`].
    Head(usize),
    /// The default head, `[head console]` of the compiled-in
    /// configuration, which the display guard lights when the file
    /// declares no head.
    DefaultHead,
    /// An input that follows the focus.
    Focus,
    /// An input vtium never opens, but a keyboard watched for the rescue
    /// key alone.
    Ignore,
    /// A display set off.
    Dark,
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
    /// No rule matched: a display goes dark, an input follows the focus.
    NoMatch,
    /// The display guard lit the boot display.
    NeverDark,
    /// The anti-lockout guard moved a keyboard to the focus.
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
    /// Whether the display guard lit a display.
    pub never_dark: bool,
    /// Whether the anti-lockout guard acted: moved a keyboard, or the
    /// focus.
    pub anti_lockout: bool,
    /// The head holding the focus once the guards have run: a
    /// [`Place::Head`] or [`Place::DefaultHead`], or `None` with no head
    /// to rest on.
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
/// first, then the card, then the kernel's order of its connectors; the
/// whole identity breaks a tie, so the order of the set never does.
fn boot_order(a: &Identity, b: &Identity) -> Ordering {
    (!a.on_boot_card, &a.card, a.order, a).cmp(&(!b.on_boot_card, &b.card, b.order, b))
}

/// The boot display of `devices`, by its index: the connector lit at
/// start, else the first connected one in the kernel's order on the boot
/// card (D9).
pub(crate) fn boot_display(devices: &[Identity]) -> Option<usize> {
    let pick = |keep: fn(&Identity) -> bool| {
        devices
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == Kind::Display && keep(d))
            .min_by(|(_, a), (_, b)| boot_order(a, b))
            .map(|(i, _)| i)
    };
    pick(|d| d.lit_at_start).or_else(|| pick(|d| d.on_boot_card))
}

impl Config {
    /// Places one device by the first rule of its kind that matches it,
    /// reading nothing but `id` and the rules. No match sends a display
    /// dark and an input to the focus.
    pub fn walk(&self, id: &Identity) -> Placed {
        let first = self
            .rules
            .iter()
            .find(|r| r.kind == id.kind && r.matcher.matches(id));
        match first {
            Some(rule) => Placed {
                place: match rule.target {
                    Target::Head(i) => Place::Head(i),
                    Target::Focus => Place::Focus,
                    Target::Ignore => Place::Ignore,
                    Target::Dark => Place::Dark,
                },
                by: By::Rule(rule.line),
            },
            None => Placed {
                place: match id.kind {
                    Kind::Display => Place::Dark,
                    Kind::Input => Place::Focus,
                },
                by: By::NoMatch,
            },
        }
    }

    /// Places every device of a set, then runs the guards over the result,
    /// as vtium does at start and after every device added or removed
    /// (D9): the display guard first, then the anti-lockout guard.
    ///
    /// `focus` is the head holding the focus, or `None` before the focus
    /// has a head, when it starts where D5 starts it: on the head holding
    /// the boot display, else the first head in the file with a lit
    /// display. A device following the focus reaches a lit head only when
    /// the focus's head is lit. When no keyboard reaches a lit head, the
    /// anti-lockout guard sends every keyboard-class device to the focus
    /// and moves the focus by the same rule as its start; otherwise the
    /// focus stays where it is, lit or not.
    pub fn place(&self, devices: &[Identity], focus: Option<Place>) -> Placement {
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
        if self.guards.never_dark && lit_heads(&placed).is_empty() {
            let boot = boot_display(devices).and_then(|i| placed.get_mut(i));
            if let Some(p) = boot {
                let place = if self.heads.is_empty() {
                    Place::DefaultHead
                } else {
                    Place::Head(0)
                };
                *p = Placed {
                    place,
                    by: By::NeverDark,
                };
                never_dark = true;
            }
        }
        let lit = lit_heads(&placed);
        // the boot display's head, else the first lit head in the file
        let start = boot_display(devices)
            .and_then(|i| placed.get(i))
            .map(|p| p.place)
            .filter(|place| lit.contains(place))
            .or_else(|| lit.first().copied());
        let mut focus = focus.or(start);
        let mut anti_lockout = false;
        if self.guards.anti_lockout {
            let reaches = devices.iter().zip(&placed).any(|(d, p)| {
                d.kind == Kind::Input
                    && d.counts_as_keyboard()
                    && match p.place {
                        Place::Focus => focus.is_some_and(|f| lit.contains(&f)),
                        place => lit.contains(&place),
                    }
            });
            if !reaches {
                for (d, p) in devices.iter().zip(placed.iter_mut()) {
                    let keyboard = d.kind == Kind::Input && d.classes.has(Class::Keyboard);
                    if keyboard && p.place != Place::Focus {
                        *p = Placed {
                            place: Place::Focus,
                            by: By::AntiLockout,
                        };
                        anti_lockout = true;
                    }
                }
                if start.is_some() && start != focus {
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
    /// placed so, moves it to the focus for the rest of the boot (D9): with
    /// the `rescue-key` guard on, from a head whose `rescue` is on, or from
    /// `ignore`.
    pub fn rescue(&self, placed: &Placed) -> bool {
        self.guards.rescue_key
            && match placed.place {
                Place::Head(i) => self.heads.get(i).is_some_and(|h| h.rescue),
                Place::DefaultHead | Place::Ignore => true,
                Place::Focus | Place::Dark => false,
            }
    }

    /// Whether vtium opens an input placed so: an ignored keyboard-class
    /// device is opened for the rescue key alone while that guard is on,
    /// and every other ignored device stays closed (D9).
    pub fn disposition(&self, placed: &Placed, id: &Identity) -> Disposition {
        match placed.place {
            Place::Ignore if self.guards.rescue_key && id.classes.has(Class::Keyboard) => {
                Disposition::RescueOnly
            }
            Place::Ignore | Place::Dark => Disposition::Closed,
            Place::Head(_) | Place::DefaultHead | Place::Focus => Disposition::Deliver,
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
