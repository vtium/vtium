//! The Flow tab's layout and the queries its update and its view both
//! make: where the boxes stand on the document of paths, the window that
//! shows the document, the choices a part's layer offers, and the lines of a
//! terminal's card. Each function reads the tab's state, the shared part
//! and the size, and changes nothing. The Flow tab's update calls these
//! functions to move the highlight and to scroll, and the view in
//! [`super::flowdraw`] draws what they place.
//!
//! References:
//! - ADR-1206 D4, "Layout is a pure function": update calls the tab's
//!   layout functions and never a view.

use super::canvas::wrap;
use super::flow::Kind;
use super::flowview::{self, Ctx, Layout, Obj, Slot};
use super::model::{App, Shared, Size};
use super::paths::{self, Path, Piece};
use super::words;

/// The rows fixed at the top: the tab bar, the status line and the policy
/// line. Each section's title, and CONFIGURATION's column titles, scroll
/// with the document.
pub const HEAD_ROWS: usize = 3;

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;
/// Whether the document of paths is on the screen: the Flow view with
/// neither the file view nor Restore's list open over it.
pub fn shown(app: &App) -> bool {
    app.nav.view == super::model::View::Flow
        && app.flow.pager().is_none()
        && app.flow.restore().is_none()
}

/// The font sets the palette offers: every font the TUI lists that reads.
pub fn font_names(shared: &Shared) -> Vec<String> {
    shared
        .inputs
        .fonts
        .iter()
        .filter(|f| !matches!(f.state, super::model::FontState::Refused(_)))
        .map(|f| f.name.clone())
        .collect()
}

/// The document as the Flow tab's `state` and the draft hold it, laid
/// out at `cols` columns.
pub fn layout_of(state: &flowview::State, shared: &Shared, cols: usize) -> Layout {
    let flow = &shared.inputs.flow;
    let doc = shown_doc(state, paths::read(flow));
    flowview::layout(&Ctx {
        flow,
        doc: &doc,
        new: &state.new,
        parts: &state.parts,
        cols,
        default: paths::is_default(flow),
    })
}

/// The configuration as CONFIGURATION shows it: `doc` without what is
/// being revised in NEW PATH. The draft keeps it until the new path is
/// added or cleared, so adding it back unchanged changes nothing.
pub fn shown_doc(state: &flowview::State, doc: paths::Doc) -> paths::Doc {
    match state.revising.as_ref() {
        Some(flowview::Revising::Path(name)) => doc.revise(name).map_or(doc, |(rest, _)| rest),
        Some(flowview::Revising::Active) => doc.revise_active().map_or(doc, |(rest, _)| rest),
        Some(flowview::Revising::Nothing) => doc.revise_none().map_or(doc, |(rest, _)| rest),
        None => doc,
    }
}

/// The message under the window, wrapped.
pub fn message_rows(shared: &Shared, cols: usize) -> Vec<String> {
    shared
        .message
        .as_ref()
        .map(|m| wrap(m, cols.saturating_sub(2).max(1), MESSAGE_ROWS))
        .unwrap_or_default()
}

/// The window on the document: its first screen row, and how many rows it
/// has between the column titles and the message line and the `keys`
/// rows of the key line, in a terminal of `size`.
pub fn window(shared: &Shared, size: Size, keys: usize) -> (usize, usize) {
    let bottom = size
        .rows
        .saturating_sub(keys)
        .saturating_sub(message_rows(shared, size.cols).len());
    (HEAD_ROWS, bottom.saturating_sub(HEAD_ROWS))
}
/// The choices a part's layer offers, each its title, what it covers, and
/// its selector: what a display or an input part can match, from the
/// widest to the narrowest.
pub fn choices_for(shared: &Shared, obj: &Obj) -> Vec<(String, String, Option<String>)> {
    let flow = &shared.inputs.flow;
    let (Obj::Pal(slot) | Obj::New(slot)) = obj else {
        return Vec::new();
    };
    let kind = match slot {
        Slot::Display(_) => Kind::Display,
        Slot::Input(_) => Kind::Input,
        _ => return Vec::new(),
    };
    let Some(piece) = slot.piece() else {
        return Vec::new();
    };
    paths::narrow(flow, paths::sort_of(flow, &piece))
        .into_iter()
        .map(|(t, sel)| {
            let detail = paths::title(flow, kind, &sel).1;
            (t, detail, Some(sel))
        })
        .collect()
}

/// One line of a terminal's card: what a path sets that allocates no
/// display, input or font, and in the editor, each part it joins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardLine {
    Name,
    Font,
    Size,
    Setting(&'static str),
    Part(Piece),
}

/// The settings a card offers, without `isolated`, which decides where
/// roaming keyboards go.
const CARD_SETTINGS: [&str; 4] = ["consoles", "login", "when-display-lost", "switch-on-enter"];

/// The path a terminal's card shows: an added path's, or the editor's
/// with the name it will take.
pub fn card_path(state: &flowview::State, shared: &Shared, obj: &Obj) -> Option<Path> {
    let flow = &shared.inputs.flow;
    let doc = paths::read(flow);
    match obj {
        Obj::Doc(name, Slot::Terminal) => doc.path(name).cloned(),
        Obj::New(Slot::Terminal) => {
            let mut p = state.new.clone();
            if p.name.is_none() {
                p.name = Some(paths::new_name(flow, &doc));
            }
            Some(p)
        }
        _ => None,
    }
}

/// The card's lines for `path`; the editor's also list its parts.
pub fn card_lines(path: &Path, editor: bool) -> Vec<CardLine> {
    let mut out = vec![CardLine::Name, CardLine::Font, CardLine::Size];
    // the VTs change only in NEW PATH, where adding the path checks each
    // VT for a clash
    //
    // References:
    // - ADR-1403 D7, "Composing and adding a path": adding a path checks
    //   each display's VT for a clash.
    if editor {
        out.push(CardLine::Setting("vt"));
    }
    out.extend(CARD_SETTINGS.iter().map(|k| CardLine::Setting(k)));
    if editor {
        out.extend(
            path.displays
                .iter()
                .map(|d| CardLine::Part(Piece::Display(d.clone()))),
        );
        out.extend(
            path.inputs
                .iter()
                .map(|i| CardLine::Part(Piece::Input(i.clone()))),
        );
    }
    out
}

/// A line's choices, each its words and its value; none for the name,
/// which the person types. The login's second choice, Other…, starts
/// typing a command ([`card_types`]).
pub fn card_choices(shared: &Shared, line: &CardLine) -> Vec<(String, String)> {
    let own = |v: &str| (v.to_owned(), v.to_owned());
    match line {
        CardLine::Name => Vec::new(),
        CardLine::Setting("login") => vec![
            (
                words::login_default(&paths::login_in_effect(&shared.inputs.flow)),
                String::new(),
            ),
            (words::LOGIN_OTHER.to_owned(), String::new()),
        ],
        CardLine::Font => std::iter::once((words::DEFAULT_FONT_SET.to_owned(), String::new()))
            .chain(font_names(shared).iter().map(|f| own(f)))
            .collect(),
        CardLine::Size => std::iter::once((words::AUTOMATIC_SIZE.to_owned(), String::new()))
            .chain(super::model::SIZES.iter().map(|s| own(&s.to_string())))
            .collect(),
        CardLine::Setting("consoles") => (1..=6).map(|n| own(&n.to_string())).collect(),
        CardLine::Setting("vt") => {
            let mut out = vec![(words::EVERY_VT.to_owned(), String::new())];
            out.extend((1u16..=12).map(|n| (words::vt_words(&[n]), n.to_string())));
            out
        }
        CardLine::Setting("when-display-lost") => vec![
            (words::KEEP_SESSIONS.to_owned(), String::new()),
            (words::END_SESSIONS.to_owned(), "hangup".to_owned()),
        ],
        CardLine::Setting("switch-on-enter") => vec![
            (words::SWITCH_OFF.to_owned(), String::new()),
            (words::SWITCH_ON.to_owned(), "yes".to_owned()),
        ],
        CardLine::Setting(_) => Vec::new(),
        CardLine::Part(_) => vec![(words::DISCONNECT.to_owned(), String::new())],
    }
}

/// The choice of `line` that the path holds, if any: for the login,
/// Default while the head sets none and Other… while it sets a command.
pub fn card_chosen(shared: &Shared, path: &Path, line: &CardLine) -> Option<usize> {
    let value = card_value(path, line);
    match line {
        CardLine::Setting("login") => Some(usize::from(!value.is_empty())),
        _ => card_choices(shared, line)
            .iter()
            .position(|(_, v)| *v == value),
    }
}

/// Whether Enter on the choice `pick` of `line` starts typing a value: on
/// the name, which has no choices, and on the login's Other….
pub fn card_types(shared: &Shared, line: &CardLine, pick: usize) -> bool {
    match line {
        CardLine::Setting("login") => pick == 1,
        _ => card_choices(shared, line).is_empty(),
    }
}

/// What a line sets, which the card shows under its lines while the
/// cursor is on it. Switch on Enter also says why it does nothing for
/// `path`, as vtium decides: under policy = closed, on an isolated head,
/// on a head that runs on every VT, and on one whose VTs are all past 12,
/// which the VT keys cannot reach. On a terminal that runs on every VT,
/// it names the control that gives it a VT: `e` on an added path's card,
/// and the line VTs on the `editor`'s card in NEW PATH.
///
/// References:
/// - ADR-1401 D3, "Enter on a head's own keyboard (`switch-on-enter`)":
///   when vtium switches to a head's VT on Enter, and when it does not.
pub fn card_about(shared: &Shared, path: &Path, line: &CardLine, editor: bool) -> Option<String> {
    match line {
        CardLine::Setting("login") => Some(words::LOGIN_ABOUT.to_owned()),
        CardLine::Setting("switch-on-enter") => {
            let vts = path.vts();
            let why = if shared.inputs.flow.policy == "closed" {
                Some(words::SWITCH_CLOSED)
            } else if path.no_input || path.setting("isolated") == "yes" {
                Some(words::SWITCH_ISOLATED)
            } else if vts.is_empty() && editor {
                Some(words::SWITCH_EVERY_VT_NEW)
            } else if vts.is_empty() {
                Some(words::SWITCH_EVERY_VT)
            } else if vts.iter().all(|v| *v > 12) {
                Some(words::SWITCH_PAST_12)
            } else {
                None
            };
            Some(match why {
                Some(why) => format!("{} {why}", words::SWITCH_ABOUT),
                None => words::SWITCH_ABOUT.to_owned(),
            })
        }
        _ => None,
    }
}

/// A line's value as the path holds it, empty for the default.
pub fn card_value(path: &Path, line: &CardLine) -> String {
    match line {
        CardLine::Name => path.name.clone().unwrap_or_default(),
        CardLine::Font => path.font.clone().unwrap_or_default(),
        CardLine::Size => path.size.map(|s| s.to_string()).unwrap_or_default(),
        CardLine::Setting("consoles") => Some(path.setting("consoles"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "1".to_owned()),
        CardLine::Setting(k) => path.setting(k),
        CardLine::Part(_) => String::new(),
    }
}

/// The part a part's detail labels: its kind and its match. Only a
/// display or an input part, in PARTS or in NEW PATH, takes a label.
pub fn label_target(obj: &Obj) -> Option<(Kind, String)> {
    match obj {
        Obj::Pal(Slot::Display(s)) | Obj::New(Slot::Display(s)) => Some((Kind::Display, s.clone())),
        Obj::Pal(Slot::Input(s)) | Obj::New(Slot::Input(s)) => Some((Kind::Input, s.clone())),
        _ => None,
    }
}
