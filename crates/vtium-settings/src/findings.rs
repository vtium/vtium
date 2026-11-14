//! What a check of vtium.conf's sections finds beyond their errors, from what
//! survives of it (ADR-1401 D3, D5, D7, D9, D10): the warnings that need
//! vtium.conf's caps and the frits installed, and the remarks about what
//! runs on the devices present. `vtium check-config` reports them, and
//! the start can log the same.

use std::path::Path;

use crate::config::{Config, Target};
use crate::error::{Fate, Quoted, Remark, Warning};
use crate::identity::{Identity, Kind};
use crate::outcome::Policy;
use crate::parse::Parsed;
use crate::place::{By, Place};

/// What the warnings are measured against: vtium.conf's
/// `head-consoles-max` and `consoles-max`, and the frits installed.
pub struct Against<'a> {
    pub head_cap: u32,
    pub total: u32,
    /// The directory the frits are in, which a warning names.
    pub frits: &'a Path,
    /// Whether the frit of this name is installed there.
    pub has_frit: &'a dyn Fn(&str) -> bool,
}

/// The warnings of `parsed` and those measured `against` vtium.conf and
/// the frits, by line: a head's `consoles` past its cap or past the total
/// (D10), and a look whose `font-name` names a frit that is not installed
/// (D7; ADR-1403 D5). A head past its cap
/// takes one warning, the cap's, in place of the parser's warning past
/// [`CONSOLES_MAX`](crate::CONSOLES_MAX) on the same line.
pub fn warnings(parsed: &Parsed, against: &Against) -> Vec<Warning> {
    let config = &parsed.config;
    let mut out = Vec::new();
    let mut spare = usize::try_from(against.total)
        .unwrap_or(usize::MAX)
        .saturating_sub(config.heads.len());
    for h in &config.heads {
        let head = || Quoted::new(&h.name);
        let asked = h.consoles.max(1);
        if asked > against.head_cap {
            out.push(Warning::PastHeadCap {
                line: h.consoles_line,
                head: head(),
                cap: against.head_cap,
            });
        }
        // each head keeps its first console; the others are given in the
        // file's order while the total lasts
        let more = usize::try_from(asked.min(against.head_cap).saturating_sub(1)).unwrap_or(0);
        let given = more.min(spare);
        if given < more {
            out.push(Warning::PastTotal {
                line: h.consoles_line,
                head: head(),
                gets: u32::try_from(given.saturating_add(1)).unwrap_or(u32::MAX),
                total: against.total,
            });
        }
        spare = spare.saturating_sub(given);
    }
    for look in &config.looks {
        let frit = look
            .settings
            .iter()
            .find(|s| s.key.name == crate::keys::FONT_NAME.name);
        if let Some(frit) = frit.filter(|f| !(against.has_frit)(&f.value)) {
            out.push(Warning::MissingFrit {
                line: frit.line,
                look: Quoted::new(&look.name),
                frit: Quoted::new(&frit.value),
                dir: Quoted::path(against.frits),
            });
        }
    }
    let capped: Vec<usize> = out
        .iter()
        .filter_map(|w| match w {
            Warning::PastHeadCap { line, .. } => Some(*line),
            _ => None,
        })
        .collect();
    out.extend(
        parsed
            .warnings
            .iter()
            .filter(|w| !matches!(w, Warning::ConsolesCapped { line } if capped.contains(line)))
            .cloned(),
    );
    out.sort_by_key(Warning::line);
    out
}

/// A device as it reports itself: its name, else its path, else its
/// connector.
fn reported(id: &Identity) -> Quoted {
    let shown = id
        .name
        .as_deref()
        .or(id.path.as_deref())
        .or(id.connector.as_deref())
        .unwrap_or_default();
    Quoted::new(shown)
}

/// The remarks about `config`, each with the line it names, `None` for
/// the file as a whole: a head no rule names, which the display guard
/// still lights when it is the first head of what survives of an open file
/// with errors (D3, D7, D9); every head isolated
/// (D5, D9); and on `devices`, the devices present, when given: no head
/// that can be the active head with a display, under closed no keyboard
/// reaching any head, and each device that no rule places and no guard
/// moves, with what the policy gives it (D9). The lockout comes first, so
/// it is seen first.
pub fn remarks(config: &Config, devices: Option<&[Identity]>) -> Vec<(Option<usize>, Remark)> {
    let mut out = Vec::new();
    let every_isolated =
        !config.heads.is_empty() && (0..config.heads.len()).all(|i| config.isolated(i));
    if let Some(devices) = devices {
        let placement = config.place(devices, None);
        let keyboards: Vec<Place> = devices
            .iter()
            .zip(&placement.placed)
            .filter(|(d, _)| d.kind == Kind::Input && d.counts_as_keyboard())
            .map(|(_, p)| p.place)
            .collect();
        let reaches = |p: &Place| match p {
            Place::Head(_) | Place::DefaultHead => true,
            Place::Roaming => placement.focus.is_some(),
            // `place` names no VT, so it never ties a device elsewhere
            Place::Ignore | Place::Off | Place::Notice | Place::Elsewhere(_) | Place::Unused(_) => {
                false
            }
        };
        if config.policy == Policy::Closed
            && !keyboards.is_empty()
            && !keyboards.iter().any(reaches)
        {
            out.push((None, Remark::ClosedLockout));
        }
        if !config.heads.is_empty() && !every_isolated && placement.focus.is_none() {
            out.push((None, Remark::NoActiveHead));
        }
        // as the guards leave them: a display the display guard lights is
        // not dark
        for (id, placed) in devices.iter().zip(&placement.placed) {
            let fate = match (id.kind, placed.by, placed.place) {
                // `place` names no VT, so the built-in default never places
                (_, By::Rule(_) | By::NeverDark | By::AntiLockout | By::Builtin, _) => None,
                (Kind::Display, By::NoMatch | By::Tail, _) => Some(Fate::Dark),
                (Kind::Input, By::NoMatch, Place::Roaming) if placement.focus.is_some() => {
                    Some(Fate::Roams)
                }
                (Kind::Input, By::NoMatch, Place::Roaming) => Some(Fate::RoamsNowhere),
                (Kind::Input, By::NoMatch, _) => Some(Fate::Ignored),
                (Kind::Input, By::Tail, Place::Roaming) if placement.focus.is_some() => {
                    Some(Fate::TailRoams)
                }
                (Kind::Input, By::Tail, Place::Roaming) => Some(Fate::TailRoamsNowhere),
                (Kind::Input, By::Tail, _) => Some(Fate::TailIgnored),
            };
            if let Some(fate) = fate {
                out.push((
                    None,
                    Remark::Fell {
                        device: reported(id),
                        fate,
                    },
                ));
            }
        }
    }
    if every_isolated {
        out.push((None, Remark::EveryHeadIsolated));
    }
    for (i, h) in config.heads.iter().enumerate() {
        let named = config
            .rules
            .iter()
            .any(|r| matches!(r.target, Target::Head(t) if t == i));
        if !named {
            let head = Quoted::new(&h.name);
            // for what survives of an open file with errors, the display
            // guard gives the first head the boot display while no head has
            // one; for a valid file the table leaves it without one (D3)
            let remark = if i == 0 && config.survivors && config.policy == Policy::Open {
                Remark::UnnamedLitByGuard { head }
            } else {
                Remark::Unnamed { head }
            };
            out.push((Some(h.line), remark));
        }
    }
    out
}
