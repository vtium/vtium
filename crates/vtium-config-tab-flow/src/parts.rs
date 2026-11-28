//! The parts PARTS offers, and a part's detail. A part is what a person
//! puts into a path: a display or an input, matched the way a rule matches
//! it; a look of the draft; or a terminal. Each of the four stages of a
//! path has a template, which is never placed in a path.
//!
//! The parts come from three sources ([`enumerate`]): every connected
//! device; every device or match the draft's rules or labels name; and
//! the special parts the draft uses, such as No input or Active terminal.
//! When the devices change, the tab enumerates again and [`merge`]s the
//! result into the parts it holds, by each part's match. A new device adds
//! a part. A part whose device left stays, marked not connected, and is
//! normal again when the device comes back.
//!
//! Enter on a display or an input opens its detail ([`draw_detail`]): its
//! label first, then the choices that narrow what it matches, from the
//! widest to the narrowest.
//!
//! References:
//! - ADR-1403 D7, "Parts from the devices present", "Templates and the
//!   part makers" and "Labels".

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::flow::{Device, Flow, Kind};
use vtium_config_widgets::canvas::{cells, fit};
use vtium_config_widgets::roles::Role;

use crate::inputs::{Sort, sort};
use crate::page::Model;
use crate::page::draw::Sheet;
use crate::page::layout::{Obj, Slot};
use crate::paths::{self, ANY_KEYBOARD, ANY_POINTER, Doc, Own, Piece};
use crate::words;

/// The four stages of a path, each with its template, in the order PARTS
/// shows their groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Display,
    Terminal,
    Input,
    Look,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Display, Stage::Terminal, Stage::Input, Stage::Look];

    /// The stage a part fills in a path.
    pub const fn of(piece: &Piece) -> Stage {
        match piece {
            Piece::Display(_) => Stage::Display,
            Piece::Input(_) | Piece::NoInput => Stage::Input,
            Piece::Look(_) => Stage::Look,
            Piece::Active | Piece::NoTerminal | Piece::Terminal(_) => Stage::Terminal,
        }
    }

    /// The template's title.
    pub const fn name(self) -> &'static str {
        match self {
            Stage::Display => "Display",
            Stage::Terminal => "Terminal",
            Stage::Input => "Input",
            Stage::Look => "Look",
        }
    }
}

/// One part of PARTS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub piece: Piece,
    /// Its name: the device's title, or the special part's name. A part
    /// whose device left keeps the name it had.
    pub title: String,
    /// A device it matches is connected, or it matches no device at all.
    pub connected: bool,
    /// The person made it from a template, so it keeps the name it was
    /// made with.
    pub made: bool,
}

/// The name of a part that matches no device: No input, Active terminal,
/// Nowhere, a new terminal's name, or a look's.
pub fn special_title(piece: &Piece) -> String {
    match piece {
        Piece::NoInput => words::NO_INPUT.to_owned(),
        Piece::NoTerminal => words::NO_TERMINAL.to_owned(),
        Piece::Active => words::ACTIVE_TERMINAL.to_owned(),
        Piece::Terminal(n) => n.clone(),
        Piece::Look(l) => l
            .clone()
            .unwrap_or_else(|| words::DEFAULT_LOOK_PART.to_owned()),
        Piece::Display(s) | Piece::Input(s) => s.clone(),
    }
}

/// A part's name, as [`paths::title`] gives a display's or an input's. A
/// connected device picked by its own match, which `own` holds, goes by
/// its own name.
pub fn title_of(flow: &Flow, own: &Own, piece: &Piece) -> String {
    let own = |kind: Kind, sel: &str| own.device(flow, kind, sel).map(|d| d.name.clone());
    match piece {
        Piece::Display(s) => {
            own(Kind::Display, s).unwrap_or_else(|| paths::title(flow, Kind::Display, s).0)
        }
        Piece::Input(s) => {
            own(Kind::Input, s).unwrap_or_else(|| paths::title(flow, Kind::Input, s).0)
        }
        _ => special_title(piece),
    }
}

/// Whether the device `dev` holds every token of the match `sel`, as far
/// as vtium-config can tell from the keys it lists: a path takes the paths
/// below it, a name and a connector are globs, and a udev property, which
/// the list of keys does not carry, is taken to hold.
pub fn matches(dev: &Device, sel: &str) -> bool {
    sel.split_whitespace().all(|t| {
        if t == "*" {
            return true;
        }
        if t == ANY_POINTER {
            return sort(dev) == Sort::Pointer;
        }
        if t == ANY_KEYBOARD {
            return dev
                .keys
                .iter()
                .any(|k| k == "class:keyboard" || k == "class:key");
        }
        if let Some(pattern) = t.strip_prefix("name:") {
            return glob(pattern, &dev.name);
        }
        if t.starts_with("udev:") {
            return true;
        }
        dev.keys
            .iter()
            .any(|k| (t.starts_with("path:") && k.starts_with(t)) || glob(t, k))
    })
}

/// Whether `text` matches the glob `pattern`, as vtium matches a rule's
/// name or connector. A pattern with no `*` or `?` matches only itself, so
/// the comparison that most parts need skips the glob's work.
fn glob(pattern: &str, text: &str) -> bool {
    if pattern.contains(['*', '?']) {
        vtium_settings::glob(pattern, text)
    } else {
        pattern == text
    }
}

/// Whether the part `piece` is connected: a device it names is, or it
/// names no one device, as All keyboards or No input do.
pub fn connected(flow: &Flow, piece: &Piece) -> bool {
    let wide = |s: &str| s == "*" || s.starts_with("class:");
    let (kind, sel) = match piece {
        Piece::Display(s) if s != "*" => (Kind::Display, s),
        Piece::Input(s) if !wide(s) => (Kind::Input, s),
        _ => return true,
    };
    flow.devices(kind)
        .iter()
        .any(|d| d.found && matches(d, sel))
}

/// Whether the draft holds a `[look NAME]` section named `name` that a
/// terminal can name. A section named `default` is the default look, which
/// a terminal takes by naming no look.
fn holds_look(flow: &Flow, name: &str) -> bool {
    name != "default" && flow.looks.iter().any(|l| l.name == name)
}

/// The parts of the draft's flow `flow`, whose configuration is `doc`:
/// each connected display, keyboard and pointer, by the match it is picked
/// by; each display and input the configuration's paths name or label;
/// the special parts the paths use; and the looks they use that the draft
/// holds.
pub fn enumerate(flow: &Flow, own: &Own, doc: &Doc) -> Vec<Part> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut add = |p: Piece| {
        if !pieces.contains(&p) {
            pieces.push(p);
        }
    };
    for d in flow.displays.iter().filter(|d| d.found) {
        add(Piece::Display(paths::own_key(flow, Kind::Display, d)));
    }
    for d in doc.paths.iter().flat_map(|p| &p.displays).chain(&doc.off) {
        add(Piece::Display(d.clone()));
    }
    for d in flow
        .inputs
        .iter()
        .filter(|d| d.found && sort(d) != Sort::Other)
    {
        add(Piece::Input(paths::own_key(flow, Kind::Input, d)));
    }
    let named = doc.paths.iter().flat_map(|p| &p.inputs);
    for i in named.chain(&doc.roaming).chain(&doc.ignored) {
        add(Piece::Input(i.clone()));
    }
    // a labelled part comes back whether or not a rule uses it
    for (kind, sel, _) in &doc.labels {
        add(match kind {
            Kind::Display => Piece::Display(sel.clone()),
            Kind::Input => Piece::Input(sel.clone()),
        });
    }
    if doc.paths.iter().any(|p| p.no_input) {
        add(Piece::NoInput);
    }
    if !doc.roaming.is_empty() {
        add(Piece::Active);
    }
    if !doc.off.is_empty() || !doc.ignored.is_empty() {
        add(Piece::NoTerminal);
    }
    // a look line that names no section offers nothing: vtium draws that
    // terminal with the default look
    for p in &doc.paths {
        if p.look.as_deref().is_none_or(|l| holds_look(flow, l)) {
            add(Piece::Look(p.look.clone()));
        }
    }
    pieces
        .into_iter()
        .map(|piece| Part {
            title: title_of(flow, own, &piece),
            connected: connected(flow, &piece),
            piece,
            made: false,
        })
        .collect()
}

/// Merges the parts `now`, enumerated again from `flow`, into the parts
/// `known`, by each part's match: a part is added only for what is new,
/// and every part held is marked connected or not by `flow`. A part keeps
/// its place and, while its device is away, its name. A look's part leaves
/// once the draft no longer holds that look, because a terminal can name
/// only a look the draft holds. A part that `now` holds takes what `now`
/// worked out for it, so only the parts the flow no longer makes are
/// worked out again.
pub fn merge(known: &mut Vec<Part>, flow: &Flow, own: &Own, now: &[Part]) {
    known.retain(|k| match &k.piece {
        Piece::Look(Some(l)) => holds_look(flow, l),
        _ => true,
    });
    for k in known.iter_mut() {
        match now.iter().find(|n| n.piece == k.piece) {
            Some(n) => {
                k.connected = n.connected;
                if !k.made {
                    k.title.clone_from(&n.title);
                }
            }
            None => {
                k.connected = connected(flow, &k.piece);
                if k.connected && !k.made {
                    k.title = title_of(flow, own, &k.piece);
                }
            }
        }
    }
    for n in now {
        if !known.iter().any(|k| k.piece == n.piece) {
            known.push(n.clone());
        }
    }
}

/// Makes the part `piece` with the name `title`, unless a part with the
/// same match is held already, and says which part it is, by its place in
/// `known`.
pub fn make(known: &mut Vec<Part>, flow: &Flow, piece: Piece, title: String) -> usize {
    if let Some(at) = known.iter().position(|k| k.piece == piece) {
        return at;
    }
    known.push(Part {
        connected: connected(flow, &piece),
        piece,
        title,
        made: true,
    });
    known.len().saturating_sub(1)
}

/// The part a detail is about: a display or an input in PARTS or in NEW
/// PATH, as its kind and its match.
pub fn detail_target(obj: &Obj) -> Option<(Kind, &str)> {
    match obj {
        Obj::Pal(Slot::Display(s)) | Obj::New(Slot::Display(s)) => Some((Kind::Display, s)),
        Obj::Pal(Slot::Input(s)) | Obj::New(Slot::Input(s)) => Some((Kind::Input, s)),
        _ => None,
    }
}

/// The choices a part's detail offers, each its title, what it covers and
/// its match: what a display or an input part can match, from the widest
/// to the narrowest.
pub fn choices(flow: &Flow, obj: &Obj) -> Vec<(String, String, String)> {
    let Some((kind, sel)) = detail_target(obj) else {
        return Vec::new();
    };
    let piece = match kind {
        Kind::Display => Piece::Display(sel.to_owned()),
        Kind::Input => Piece::Input(sel.to_owned()),
    };
    paths::narrow(flow, paths::sort_of(flow, &piece))
        .into_iter()
        .map(|(t, s)| {
            let detail = paths::title(flow, kind, &s).1;
            (t, detail, s)
        })
        .collect()
}

/// The rows of a detail's screen above its first choice: the breadcrumb,
/// a blank row, the part's heading, its label, a blank row and the
/// choices' heading.
pub const CHOICES_TOP: usize = 6;

/// The first choice the detail shows when its viewer has `rows` rows and
/// the cursor is on `line`, where line 0 is the label and line 1 the first
/// choice: the window of choices scrolls just enough to show the cursor.
pub fn first_shown(line: usize, rows: usize) -> usize {
    let room = rows.saturating_sub(CHOICES_TOP).max(1);
    line.saturating_sub(room)
}

/// Draws a part's detail into `area`: the breadcrumb, the part's kind and
/// name, its label with what the label is for, and the choices that narrow
/// it, with `line` under the cursor. A choice the part holds now, in NEW
/// PATH, is marked ●, and the others ○.
pub fn draw_detail(
    model: &Model,
    obj: &Obj,
    line: usize,
    buf: &mut Buffer,
    area: Rect,
    color: bool,
) {
    let Some((kind, sel)) = detail_target(obj) else {
        return;
    };
    let (flow, doc) = (&model.flow, &model.doc);
    let width = usize::from(area.width);
    let (title, _) = paths::title(flow, kind, sel);
    let name = match obj {
        Obj::Pal(_) => title_of(
            flow,
            &model.own,
            &match kind {
                Kind::Display => Piece::Display(sel.to_owned()),
                Kind::Input => Piece::Input(sel.to_owned()),
            },
        ),
        _ => title,
    };
    let (part, heading, about, role) = match kind {
        Kind::Display => (
            words::DISPLAY_PART,
            words::CHOOSE_DISPLAY,
            words::DISPLAY_LABEL_ABOUT,
            Role::Display,
        ),
        Kind::Input => (
            words::INPUT_PART,
            words::CHOOSE_INPUT,
            words::INPUT_LABEL_ABOUT,
            Role::Input,
        ),
    };
    let mut s = Sheet::new(buf, area, color);
    s.crumb(&name);
    let x = s.put(1, 2, part, Role::Heading);
    s.put_bold(x.saturating_add(2), 2, &name, role);
    // the label's line, highlighted whole while the cursor is on it
    if line == 0 {
        s.highlight(3);
    }
    let label_role = if line == 0 {
        Role::RowHighlight
    } else {
        Role::Row
    };
    s.put(
        3,
        3,
        words::LABEL_FIELD,
        if line == 0 { label_role } else { Role::Dim },
    );
    let shown = doc.label(kind, sel).unwrap_or(words::NO_LABEL);
    let x = s.put(13, 3, shown, label_role);
    let about_x = x.saturating_add(4).max(40.min(width / 2));
    s.put(
        about_x,
        3,
        &fit(about, width.saturating_sub(about_x).saturating_sub(1)),
        if line == 0 { label_role } else { Role::Dim },
    );
    let x = s.put(1, 5, heading, Role::Heading).saturating_add(2);
    s.put(
        x,
        5,
        &fit(
            words::CHOOSE_ABOUT,
            width.saturating_sub(x).saturating_sub(1),
        ),
        Role::Dim,
    );
    let list = choices(flow, obj);
    if list.is_empty() {
        s.put(3, CHOICES_TOP, words::NO_CHOICES, Role::Dim);
        return;
    }
    let now = match obj {
        Obj::New(_) => Some(sel),
        _ => None,
    };
    let title_w = list
        .iter()
        .map(|(t, ..)| cells(t))
        .max()
        .unwrap_or(0)
        .min(width / 2);
    let rows = usize::from(area.height);
    let first = first_shown(line, rows);
    for (i, (t, detail, value)) in list.iter().enumerate().skip(first) {
        let y = CHOICES_TOP.saturating_add(i.saturating_sub(first));
        if y >= rows {
            break;
        }
        let cursor = line == i.saturating_add(1);
        if cursor {
            s.highlight(y);
        }
        let row = if cursor {
            Role::RowHighlight
        } else {
            Role::Row
        };
        let on = now == Some(value.as_str());
        let mark = if on { "●" } else { "○" };
        s.put(
            3,
            y,
            mark,
            if cursor {
                row
            } else if on {
                Role::Active
            } else {
                Role::Dim
            },
        );
        s.put(5, y, &fit(t, title_w), row);
        let dx = 5usize.saturating_add(title_w).saturating_add(3);
        s.put(
            dx,
            y,
            &fit(detail, width.saturating_sub(dx).saturating_sub(1)),
            if cursor { row } else { Role::Dim },
        );
    }
}
