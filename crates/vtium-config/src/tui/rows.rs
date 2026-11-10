//! A try-out of the Flow view as rows (`VTIUM_CONFIG_FLOW=rows`): one row
//! per terminal (head), each reading left to right as the screens that
//! show it, the terminal with its look, and the keyboards and pointers
//! that type into it, every object in a bordered box and every connection
//! a line from border to border. Below a rule line, two trays hold what is
//! in no row: NOT IN A PATH YET, the devices connected that no rule places
//! on a terminal, and SET ASIDE, what vtium ignores or turns off.
//!
//! [`layout`] lays the whole flow out on a canvas as tall as it needs, as
//! plain data: boxes, wires and notes. [`State`] is where the highlight is
//! and which canvas row is at the top of the window, and keeps the
//! highlighted object in sight as the highlight moves and as the flow is
//! laid out again. [`super::rowsdraw`] draws the layout.
//!
//! The view only shows and navigates the flow. Every change it makes goes
//! through [`super::edit::Change`] into the draft, as the six-column view's
//! changes do.

use super::edit::{self, Change};
use super::flow::{Device, Flow, Item, Kind, Special, Target, general};
use super::input::Key;

/// The rows a box takes: its top border, its title, its detail and its
/// bottom border.
pub const BOX_ROWS: usize = 4;

/// The blank columns between two boxes of a tray.
const TRAY_GAP: usize = 2;

/// The blank columns between the two trays when they share a line.
const TRAYS_GAP: usize = 5;

/// One object on the overview. An object is named by what it is, not by
/// its place in the flow's lists, so the highlight stays on it when a
/// device comes or goes and the lists shift.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Obj {
    /// A display, by its name and its detail.
    Screen(String, String),
    /// A head, by its name.
    Terminal(String),
    /// An input device, by its name and its detail.
    Input(String, String),
    /// The box that folds every input that is neither a keyboard nor a
    /// pointer.
    Others,
}

/// What state a box is in, which its border's color shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// In a row: white.
    Placed,
    /// Connected, but no rule places it on a terminal: amber.
    Waiting,
    /// Ignored, turned off, or not plugged in: grey.
    Aside,
}

/// What sort of input device a device is, by the classes it offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Keyboard,
    Pointer,
    /// Neither a keyboard nor a pointer: a power button, a receiver's
    /// System Control node, an audio jack.
    Other,
    Screen,
}

/// The classes that make an input a pointer.
const POINTER_CLASSES: [&str; 6] = [
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
    "class:tablet",
];

/// The sort of the input `dev`.
pub fn sort(dev: &Device) -> Sort {
    if dev.keys.iter().any(|k| k == "class:keyboard") {
        Sort::Keyboard
    } else if dev
        .keys
        .iter()
        .any(|k| POINTER_CLASSES.contains(&k.as_str()))
    {
        Sort::Pointer
    } else {
        Sort::Other
    }
}

impl Sort {
    /// The word for one of this sort.
    pub const fn word(self) -> &'static str {
        match self {
            Sort::Keyboard => "keyboard",
            Sort::Pointer => "pointer",
            Sort::Other => "input",
            Sort::Screen => "screen",
        }
    }
}

/// A box on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boxed {
    pub obj: Obj,
    /// The flow's item the box stands for, which the route card and the
    /// changes name; `None` for the box of other inputs.
    pub item: Option<Item>,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub title: String,
    pub detail: String,
    pub mood: Mood,
}

impl Boxed {
    /// The row of the box's title, where its wire meets it.
    pub const fn title_row(&self) -> usize {
        self.y.saturating_add(1)
    }

    /// The column after the box's right border.
    pub const fn right(&self) -> usize {
        self.x.saturating_add(self.w)
    }

    /// The row after the box's bottom border.
    pub const fn bottom(&self) -> usize {
        self.y.saturating_add(BOX_ROWS)
    }
}

/// A line from one box to another: the cells it runs through as the
/// corners of its path, from the cell beside `from`'s border to the cell
/// beside `to`'s border, where its arrow is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: Obj,
    pub to: Obj,
    pub points: Vec<(usize, usize)>,
    /// A dotted line, which no device's rule draws: the roaming devices
    /// that type into this terminal when it is active.
    pub dashed: bool,
}

/// What a line of text on the canvas is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    /// A tray's title.
    Waiting,
    Aside,
    /// The grey italic words beside a dotted line.
    Hint,
    /// `+ add a terminal`.
    Add,
}

/// A line of text on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub x: usize,
    pub y: usize,
    pub text: String,
    pub kind: NoteKind,
}

/// The three columns of the rows: where each box of a column starts and
/// how wide it is, and where the lines that merge turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub screen_x: usize,
    pub screen_w: usize,
    pub term_x: usize,
    pub term_w: usize,
    pub dev_x: usize,
    pub dev_w: usize,
    /// The column where the lines from several screens merge.
    pub screen_join: usize,
    /// The column where the lines from several devices merge.
    pub dev_join: usize,
}

/// The columns at a screen `cols` wide: the boxes and the gaps between
/// them share the width as the mock-up at 120 columns does, 25, 11, 28,
/// 14 and 40 of its 118, with a margin of one column at each side.
pub fn geometry(cols: usize) -> Geometry {
    let room = cols.saturating_sub(2);
    let part = |n: usize| room.saturating_mul(n) / 118;
    let screen_w = part(25);
    let gap1 = part(11).max(5);
    let term_w = part(28);
    let gap2 = part(14).max(7);
    let screen_x = 1;
    let term_x = screen_x + screen_w + gap1;
    let dev_x = term_x + term_w + gap2;
    let dev_w = room.saturating_add(1).saturating_sub(dev_x);
    Geometry {
        screen_x,
        screen_w,
        term_x,
        term_w,
        dev_x,
        dev_w,
        screen_join: screen_x + screen_w + 1,
        dev_join: term_x + term_w + 2,
    }
}

/// Where a device sits.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Spot {
    /// On a head's row; `true` when it roams there as the active head.
    Row(usize, bool),
    /// In NOT IN A PATH YET, with what its box says.
    Waiting(String),
    /// In SET ASIDE, with what its box says.
    Aside(String),
    /// In the box of other inputs.
    Folded,
}

/// Whether the head `h` is isolated: as it writes `isolated`, else as the
/// policy leaves it, `no` under open and `yes` under closed.
fn isolated(flow: &Flow, h: usize) -> bool {
    let set = flow
        .heads
        .get(h)
        .and_then(|head| head.keys.iter().find(|(k, _)| k == "isolated"))
        .map(|(_, v)| v.as_str());
    match set {
        Some("yes") => true,
        Some("no") => false,
        _ => flow.policy == "closed",
    }
}

/// The head that is active, which roaming devices type into: the flow's
/// own when it names one, else the first head a screen shows that is not
/// isolated.
pub fn active(flow: &Flow) -> Option<usize> {
    if let Some(name) = &flow.active
        && let Some(h) = flow.heads.iter().position(|h| h.name == *name)
    {
        return Some(h);
    }
    flow.activatable().into_iter().find(|h| !isolated(flow, *h))
}

/// The rule number of `dev`'s rule, counted from 1.
fn rule_no(dev: &Device) -> usize {
    dev.rule.unwrap_or_default().saturating_add(1)
}

/// Where the input `d` sits.
fn input_spot(flow: &Flow, d: usize, active: Option<usize>) -> Spot {
    let Some(dev) = flow.inputs.get(d) else {
        return Spot::Folded;
    };
    if !dev.found {
        return Spot::Aside(super::words::ROWS_UNPLUGGED.to_owned());
    }
    if sort(dev) == Sort::Other {
        return Spot::Folded;
    }
    let Some(rule) = dev.rule.and_then(|r| flow.rules.get(r)) else {
        return Spot::Waiting(super::words::ROWS_SPACE_IT.to_owned());
    };
    match flow.target(rule) {
        Target::Head(h) => Spot::Row(h, false),
        Target::Special(Special::Roaming) => match active {
            Some(h) => Spot::Row(h, true),
            None => Spot::Waiting(super::words::ROWS_NO_ACTIVE.to_owned()),
        },
        Target::Special(_) => Spot::Aside(super::words::rows_set_aside_by(rule_no(dev))),
        Target::Missing => {
            Spot::Waiting(super::words::rows_no_terminal(rule_no(dev), &rule.target))
        }
    }
}

/// Where the display `d` sits.
fn screen_spot(flow: &Flow, d: usize) -> Spot {
    let Some(dev) = flow.displays.get(d) else {
        return Spot::Folded;
    };
    if !dev.found {
        return Spot::Aside(super::words::ROWS_UNPLUGGED.to_owned());
    }
    let Some(rule) = dev.rule.and_then(|r| flow.rules.get(r)) else {
        return Spot::Waiting(super::words::ROWS_SPACE_IT.to_owned());
    };
    match flow.target(rule) {
        Target::Head(h) => Spot::Row(h, false),
        Target::Special(_) => Spot::Aside(super::words::rows_turned_off_by(rule_no(dev))),
        Target::Missing => {
            Spot::Waiting(super::words::rows_no_terminal(rule_no(dev), &rule.target))
        }
    }
}

/// What reaches the head `h`: the displays that show it, and the inputs
/// on its row, each with whether it roams there.
pub fn reaching(flow: &Flow, h: usize) -> (Vec<usize>, Vec<(usize, bool)>) {
    let active = active(flow);
    let screens = (0..flow.displays.len())
        .filter(|d| screen_spot(flow, *d) == Spot::Row(h, false))
        .collect();
    let inputs = (0..flow.inputs.len())
        .filter_map(|d| match input_spot(flow, d, active) {
            Spot::Row(on, roams) if on == h => Some((d, roams)),
            _ => None,
        })
        .collect();
    (screens, inputs)
}

/// The inputs the box of other inputs folds, by their index.
pub fn folded(flow: &Flow) -> Vec<usize> {
    let active = active(flow);
    (0..flow.inputs.len())
        .filter(|d| input_spot(flow, *d, active) == Spot::Folded)
        .collect()
}

/// The flow's item `obj` stands for now; `None` for the box of other
/// inputs, and for an object the flow no longer has.
pub fn item_of(flow: &Flow, obj: &Obj) -> Option<Item> {
    match obj {
        Obj::Screen(name, detail) => flow
            .displays
            .iter()
            .position(|d| d.name == *name && d.detail == *detail)
            .map(Item::Display),
        Obj::Input(name, detail) => flow
            .inputs
            .iter()
            .position(|d| d.name == *name && d.detail == *detail)
            .map(Item::Input),
        Obj::Terminal(name) => flow
            .heads
            .iter()
            .position(|h| h.name == *name)
            .map(Item::Head),
        Obj::Others => None,
    }
}

/// The object that stands for `item`, where one does.
pub fn obj_of(flow: &Flow, item: Item) -> Option<Obj> {
    match item {
        Item::Display(d) => flow
            .displays
            .get(d)
            .map(|x| Obj::Screen(x.name.clone(), x.detail.clone())),
        Item::Input(d) => flow
            .inputs
            .get(d)
            .map(|x| Obj::Input(x.name.clone(), x.detail.clone())),
        Item::Head(h) | Item::HeadLook(h) => {
            flow.heads.get(h).map(|x| Obj::Terminal(x.name.clone()))
        }
        _ => None,
    }
}

/// An input's port in a reader's words: `usb-0:3.3:1.0` reads `USB 3.3`;
/// a port of another kind is given as it is.
pub fn port_words(detail: &str) -> String {
    if let Some(at) = detail.rfind("usb-") {
        let rest = detail.get(at.saturating_add(4)..).unwrap_or_default();
        // the bus number before the first `:`, and the interface after the
        // second, say nothing a reader looks for
        let mut parts = rest.split(':');
        let first = parts.next().unwrap_or_default();
        let port = parts.next().unwrap_or(first);
        return format!("USB {port}");
    }
    if detail.contains("i8042") {
        return "built in".to_owned();
    }
    detail.to_owned()
}

/// What tells an input apart in its box: its port, and that it roams.
fn input_detail(dev: &Device, roams: bool) -> String {
    let port = port_words(&dev.detail);
    match (port.is_empty(), roams) {
        (true, true) => super::words::ROWS_ROAMS.to_owned(),
        (true, false) => String::new(),
        (false, true) => format!("{port} · {}", super::words::ROWS_ROAMS),
        (false, false) => port,
    }
}

/// The words the box of other inputs gives each of them, a kind of
/// input a reader knows, from the name it reports.
fn other_word(name: &str) -> String {
    let lower = name.to_lowercase();
    let word = if lower.contains("power button") {
        "power button"
    } else if lower.contains("sleep button") {
        "sleep button"
    } else if lower.contains("consumer control") || lower.contains("system control") {
        "media keys"
    } else if lower.contains("headphone") || lower.contains("mic") || lower.contains("jack") {
        "audio jacks"
    } else if lower.contains("pc speaker") {
        "PC speaker"
    } else if lower.contains("lid switch") {
        "lid switch"
    } else if lower.contains("video bus") {
        "display keys"
    } else {
        return name.to_owned();
    };
    word.to_owned()
}

/// What the box of other inputs says under its title: the kinds of input
/// it holds, each once.
pub fn others_detail(names: &[String]) -> String {
    let mut words: Vec<String> = Vec::new();
    for n in names {
        let w = other_word(n);
        if !words.contains(&w) {
            words.push(w);
        }
    }
    let power = words.iter().position(|w| w == "power button");
    let sleep = words.iter().position(|w| w == "sleep button");
    if let (Some(p), Some(s)) = (power, sleep) {
        if let Some(w) = words.get_mut(p) {
            "power and sleep buttons".clone_into(w);
        }
        words.remove(s);
    }
    words.join(", ")
}

/// What a terminal's box says under its name: its look, and the look's
/// font set and size.
fn terminal_detail(flow: &Flow, h: usize) -> String {
    let Some(head) = flow.heads.get(h) else {
        return String::new();
    };
    let Some(look) = flow.look_of(h).and_then(|l| flow.looks.get(l)) else {
        return format!("look {}", head.look);
    };
    match (look.frit.is_empty(), look.points) {
        (true, 0) => format!("look {}", look.name),
        (true, p) => format!("look {} · {p} pt", look.name),
        (false, 0) => format!("look {} · {}", look.name, look.frit),
        (false, p) => format!("look {} · {} {p} pt", look.name, look.frit),
    }
}

/// The flow laid out on its canvas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub boxes: Vec<Boxed>,
    pub wires: Vec<Wire>,
    pub notes: Vec<Note>,
    /// The canvas row of the rule line above the trays.
    pub rule: usize,
    /// The canvas's height: the row after its last box.
    pub height: usize,
    pub geometry: Option<Geometry>,
    /// How many devices wait, by sort: keyboards, pointers and screens.
    pub waiting: Vec<Sort>,
}

impl Layout {
    /// The box of `obj`.
    pub fn find(&self, obj: &Obj) -> Option<&Boxed> {
        self.boxes.iter().find(|b| b.obj == *obj)
    }

    /// The boxes in reading order, across and then down.
    pub fn reading(&self) -> Vec<&Boxed> {
        let mut out: Vec<&Boxed> = self.boxes.iter().collect();
        out.sort_by_key(|b| (b.y, b.x));
        out
    }
}

/// A box on a terminal's row before it is placed.
struct Pending {
    obj: Obj,
    item: Item,
    title: String,
    detail: String,
}

/// The flow `flow` laid out at `cols` columns.
pub fn layout(flow: &Flow, cols: usize) -> Layout {
    let g = geometry(cols);
    let active = active(flow);
    let mut out = Layout {
        geometry: Some(g),
        ..Layout::default()
    };
    let n = flow.heads.len();
    let mut screens: Vec<Vec<Pending>> = (0..n).map(|_| Vec::new()).collect();
    let mut devices: Vec<Vec<Pending>> = (0..n).map(|_| Vec::new()).collect();
    let mut waiting: Vec<(Pending, Sort)> = Vec::new();
    let mut aside: Vec<Pending> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    let mut roaming = false;
    for (d, dev) in flow.displays.iter().enumerate() {
        let obj = Obj::Screen(dev.name.clone(), dev.detail.clone());
        let pending = |detail: String| Pending {
            obj: obj.clone(),
            item: Item::Display(d),
            title: dev.name.clone(),
            detail,
        };
        match screen_spot(flow, d) {
            Spot::Row(h, _) => {
                if let Some(row) = screens.get_mut(h) {
                    row.push(pending(dev.detail.clone()));
                }
            }
            Spot::Waiting(why) => waiting.push((pending(why), Sort::Screen)),
            Spot::Aside(why) => aside.push(pending(why)),
            Spot::Folded => {}
        }
    }
    for (d, dev) in flow.inputs.iter().enumerate() {
        let obj = Obj::Input(dev.name.clone(), dev.detail.clone());
        let pending = |detail: String| Pending {
            obj: obj.clone(),
            item: Item::Input(d),
            title: dev.name.clone(),
            detail,
        };
        match input_spot(flow, d, active) {
            Spot::Row(h, roams) => {
                roaming |= roams;
                if let Some(row) = devices.get_mut(h) {
                    row.push(pending(input_detail(dev, roams)));
                }
            }
            Spot::Waiting(why) => {
                let port = port_words(&dev.detail);
                let detail = if port.is_empty() {
                    why
                } else {
                    format!("{port} · {why}")
                };
                waiting.push((pending(detail), sort(dev)));
            }
            Spot::Aside(why) => aside.push(pending(why)),
            Spot::Folded => others.push(dev.name.clone()),
        }
    }
    // the rows, one per head
    let activatable = flow.activatable();
    let mut y = 0usize;
    for (h, head) in flow.heads.iter().enumerate() {
        let row_screens = screens.get(h).map_or(&[][..], Vec::as_slice);
        let row_devices = devices.get(h).map_or(&[][..], Vec::as_slice);
        let slots = row_screens.len().max(row_devices.len()).max(1);
        let term = Obj::Terminal(head.name.clone());
        let title = if active == Some(h) {
            format!("{}  · {}", head.name, super::words::ROWS_ACTIVE)
        } else {
            head.name.clone()
        };
        let term_box = Boxed {
            obj: term.clone(),
            item: Some(Item::Head(h)),
            x: g.term_x,
            y,
            w: g.term_w,
            title,
            detail: terminal_detail(flow, h),
            mood: Mood::Placed,
        };
        let join_row = term_box.title_row();
        out.boxes.push(term_box);
        for (k, p) in row_screens.iter().enumerate() {
            let b = Boxed {
                obj: p.obj.clone(),
                item: Some(p.item),
                x: g.screen_x,
                y: y.saturating_add(k.saturating_mul(BOX_ROWS)),
                w: g.screen_w,
                title: p.title.clone(),
                detail: p.detail.clone(),
                mood: Mood::Placed,
            };
            let start = (b.right(), b.title_row());
            let end = (g.term_x.saturating_sub(1), join_row);
            let points = if k == 0 {
                vec![start, end]
            } else {
                vec![
                    start,
                    (g.screen_join, b.title_row()),
                    (g.screen_join, join_row),
                    end,
                ]
            };
            out.wires.push(Wire {
                from: p.obj.clone(),
                to: term.clone(),
                points,
                dashed: false,
            });
            out.boxes.push(b);
        }
        for (k, p) in row_devices.iter().enumerate() {
            let b = Boxed {
                obj: p.obj.clone(),
                item: Some(p.item),
                x: g.dev_x,
                y: y.saturating_add(k.saturating_mul(BOX_ROWS)),
                w: g.dev_w,
                title: p.title.clone(),
                detail: p.detail.clone(),
                mood: Mood::Placed,
            };
            let start = (g.dev_x.saturating_sub(1), b.title_row());
            let end = (g.term_x.saturating_add(g.term_w), join_row);
            let points = if k == 0 {
                vec![start, end]
            } else {
                vec![
                    start,
                    (g.dev_join, b.title_row()),
                    (g.dev_join, join_row),
                    end,
                ]
            };
            out.wires.push(Wire {
                from: p.obj.clone(),
                to: term.clone(),
                points,
                dashed: false,
            });
            out.boxes.push(b);
        }
        // the roaming devices type here when this head is active: a dotted
        // line to the terminal, or, below the row's own devices, a dotted
        // branch off their line
        let hint = roaming && active != Some(h) && activatable.contains(&h);
        if hint {
            let slot = row_devices.len();
            let row = y
                .saturating_add(slot.saturating_mul(BOX_ROWS))
                .saturating_add(1);
            let start = (g.dev_x.saturating_sub(1), row);
            let points = if slot == 0 {
                vec![start, (g.term_x.saturating_add(g.term_w), join_row)]
            } else {
                // up to the row under the last device's line
                let last = y
                    .saturating_add(slot.saturating_sub(1).saturating_mul(BOX_ROWS))
                    .saturating_add(2);
                vec![start, (g.dev_join, row), (g.dev_join, last)]
            };
            out.wires.push(Wire {
                from: term.clone(),
                to: term.clone(),
                points,
                dashed: true,
            });
            let room = cols.saturating_sub(g.dev_x.saturating_add(2));
            let text = super::words::rows_roaming_hint(&head.name);
            for (i, line) in super::canvas::wrap(&text, room.max(1), BOX_ROWS.saturating_sub(1))
                .into_iter()
                .enumerate()
            {
                out.notes.push(Note {
                    x: g.dev_x.saturating_add(1),
                    y: row.saturating_add(i),
                    text: line,
                    kind: NoteKind::Hint,
                });
            }
        }
        let slots = slots.max(row_devices.len().saturating_add(usize::from(hint)));
        y = y
            .saturating_add(slots.saturating_mul(BOX_ROWS))
            .saturating_add(1);
    }
    out.notes.push(Note {
        x: g.term_x.saturating_add(2),
        y,
        text: super::words::ROWS_ADD_TERMINAL.to_owned(),
        kind: NoteKind::Add,
    });
    out.rule = y.saturating_add(2);
    // the trays
    let tray_box = |p: &Pending, mood: Mood| {
        let content = super::canvas::width(&p.title).max(super::canvas::width(&p.detail));
        Boxed {
            obj: p.obj.clone(),
            item: Some(p.item),
            x: 0,
            y: 0,
            w: content.saturating_add(4).min(cols.saturating_sub(2)),
            title: p.title.clone(),
            detail: p.detail.clone(),
            mood,
        }
    };
    let mut groups: Vec<(NoteKind, Vec<Boxed>)> = Vec::new();
    if !waiting.is_empty() {
        out.waiting = waiting.iter().map(|(_, s)| *s).collect();
        groups.push((
            NoteKind::Waiting,
            waiting
                .iter()
                .map(|(p, _)| tray_box(p, Mood::Waiting))
                .collect(),
        ));
    }
    let mut set_aside: Vec<Boxed> = Vec::new();
    if !others.is_empty() {
        let title = super::words::rows_other_inputs(others.len());
        let detail = others_detail(&others);
        let content = super::canvas::width(&title).max(super::canvas::width(&detail));
        set_aside.push(Boxed {
            obj: Obj::Others,
            item: None,
            x: 0,
            y: 0,
            w: content.saturating_add(4).min(cols.saturating_sub(2)),
            title,
            detail,
            mood: Mood::Aside,
        });
    }
    set_aside.extend(aside.iter().map(|p| tray_box(p, Mood::Aside)));
    if !set_aside.is_empty() {
        groups.push((NoteKind::Aside, set_aside));
    }
    // each line of the trays: a title row, then a row of boxes
    let line_rows = BOX_ROWS.saturating_add(2);
    let right_edge = cols.saturating_sub(1);
    let mut line = 0usize;
    let mut x = 1usize;
    let base = out.rule.saturating_add(1);
    let mut height = out.rule.saturating_add(1);
    for (kind, boxes) in groups {
        let mut first = true;
        for mut b in boxes {
            let gap = match (first, x > 1) {
                (_, false) => 0,
                (true, true) => TRAYS_GAP,
                (false, true) => TRAY_GAP,
            };
            if x > 1 && x.saturating_add(gap).saturating_add(b.w) > right_edge {
                line = line.saturating_add(1);
                x = 1;
            } else {
                x = x.saturating_add(gap);
            }
            let top = base.saturating_add(line.saturating_mul(line_rows));
            if first {
                let title = match kind {
                    NoteKind::Waiting => super::words::ROWS_WAITING,
                    _ => super::words::ROWS_ASIDE,
                };
                out.notes.push(Note {
                    x,
                    y: top,
                    text: title.to_owned(),
                    kind,
                });
                first = false;
            }
            b.x = x;
            b.y = top.saturating_add(1);
            x = x.saturating_add(b.w);
            height = height.max(b.bottom());
            out.boxes.push(b);
        }
    }
    out.height = height;
    out
}

/// The box an arrow key moves to from `at`: Left and Right to the
/// nearest box on that side, the rows nearest first, else to the box
/// before or after it in reading order; Up and Down to the nearest box
/// above or below, those under the same columns first; Home and End to
/// the first and the last box.
pub fn step(layout: &Layout, at: &Obj, key: Key) -> Option<Obj> {
    let order = layout.reading();
    let here = layout.find(at)?;
    let pick = |b: Option<&&Boxed>| b.map(|b| b.obj.clone());
    // how far apart two boxes' rows are: 0 where they share a row
    let rows_apart = |b: &Boxed| {
        if b.bottom() <= here.y {
            here.y.saturating_sub(b.bottom())
        } else {
            b.y.saturating_sub(here.bottom())
        }
    };
    let overlaps = |b: &Boxed| b.x < here.right() && here.x < b.right();
    let centre = |b: &Boxed| b.x.saturating_add(b.w / 2);
    let at_order = order.iter().position(|b| b.obj == *at).unwrap_or(0);
    match key {
        Key::Right | Key::Left => {
            let right = key == Key::Right;
            let side = order
                .iter()
                .filter(|b| {
                    if right {
                        b.x >= here.right()
                    } else {
                        b.right() <= here.x
                    }
                })
                .min_by_key(|b| (rows_apart(b), b.x.abs_diff(here.x), b.y));
            if side.is_some() {
                return pick(side);
            }
            if right {
                pick(order.get(at_order.saturating_add(1)))
            } else {
                pick(at_order.checked_sub(1).and_then(|i| order.get(i)))
            }
        }
        Key::Down | Key::Up => {
            let down = key == Key::Down;
            let candidates: Vec<&&Boxed> = order
                .iter()
                .filter(|b| if down { b.y > here.y } else { b.y < here.y })
                .collect();
            let near = |b: &&&Boxed| {
                (
                    !overlaps(b),
                    b.y.abs_diff(here.y),
                    centre(b).abs_diff(centre(here)),
                )
            };
            pick(candidates.into_iter().min_by_key(near))
        }
        Key::Home => pick(order.first()),
        Key::End => pick(order.last()),
        _ => None,
    }
}

/// Where the highlight is, which canvas row is at the top of the window,
/// and what is open over the overview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The highlighted object; `None` until the first layout.
    pub at: Option<Obj>,
    /// The canvas row at the window's top.
    pub top: usize,
    /// How far below the window's top the highlighted box is, which a
    /// new layout keeps so the page does not jump.
    pub offset: isize,
    /// The canvas row the highlighted box was last on, to find its
    /// neighbour when it goes.
    pub last_y: usize,
    /// The six-column view is shown in place of the rows: `v Rules`.
    pub classic: bool,
    /// The object whose second layer is open.
    pub layer: Option<Obj>,
    /// On a device's or a screen's layer: the line and the choice under
    /// the cursor.
    pub line: usize,
    pub pick: usize,
    /// The object a path starts from, which Space on a terminal ends.
    pub path: Option<Obj>,
}

impl State {
    /// Puts the highlight back on its object in `layout`, or on the box
    /// nearest where it was when the object is gone, and scrolls so the
    /// highlighted box is at the same height in a window `window` rows
    /// tall, or as near it as keeps the box in sight.
    pub fn settle(&mut self, layout: &Layout, window: usize) {
        let found = self.at.as_ref().and_then(|o| layout.find(o));
        let b = match found {
            Some(b) => Some(b),
            None => {
                let last = self.last_y;
                layout
                    .reading()
                    .into_iter()
                    .min_by_key(|b| (b.y.abs_diff(last), b.x))
            }
        };
        let Some(b) = b else {
            self.at = None;
            self.top = 0;
            return;
        };
        self.at = Some(b.obj.clone());
        let y = b.y;
        let wanted = isize::try_from(y)
            .unwrap_or(isize::MAX)
            .saturating_sub(self.offset)
            .max(0);
        let mut top = usize::try_from(wanted).unwrap_or(0);
        let most = layout.height.saturating_sub(window);
        top = top.min(most);
        // the whole box in sight, its top first where the window is
        // shorter than a box
        if b.bottom() > top.saturating_add(window) {
            top = b.bottom().saturating_sub(window);
        }
        if y < top {
            top = y;
        }
        self.top = top;
        self.last_y = y;
        self.offset = isize::try_from(y)
            .unwrap_or(isize::MAX)
            .saturating_sub(isize::try_from(top).unwrap_or(0));
    }

    /// Moves the highlight to `to`, scrolling only as far as keeps it in
    /// sight, line by line.
    pub fn go(&mut self, layout: &Layout, to: Obj, window: usize) {
        if let Some(b) = layout.find(&to) {
            self.offset = isize::try_from(b.y)
                .unwrap_or(isize::MAX)
                .saturating_sub(isize::try_from(self.top).unwrap_or(0));
        }
        self.at = Some(to);
        self.settle(layout, window);
    }

    /// Scrolls by a window, down or up. When the highlighted box is
    /// scrolled away, the highlight moves to the box with the most rows in
    /// sight, the first in reading order going down and the last going up,
    /// and the window then shows that box whole.
    pub fn page(&mut self, layout: &Layout, down: bool, window: usize) {
        let most = layout.height.saturating_sub(window);
        self.top = if down {
            self.top.saturating_add(window).min(most)
        } else {
            self.top.saturating_sub(window)
        };
        let end = self.top.saturating_add(window);
        let rows_seen = |b: &Boxed| b.bottom().min(end).saturating_sub(b.y.max(self.top));
        let here = self.at.as_ref().and_then(|o| layout.find(o));
        if !here.is_some_and(|b| b.y >= self.top && b.bottom() <= end) {
            let mut order = layout.reading();
            if !down {
                order.reverse();
            }
            let best = order.iter().map(|b| rows_seen(b)).max().unwrap_or(0);
            if best > 0
                && let Some(b) = order.into_iter().find(|b| rows_seen(b) == best)
            {
                self.at = Some(b.obj.clone());
            }
        }
        if let Some(b) = self.at.as_ref().and_then(|o| layout.find(o)) {
            self.offset = isize::try_from(b.y)
                .unwrap_or(isize::MAX)
                .saturating_sub(isize::try_from(self.top).unwrap_or(0));
        }
        self.settle(layout, window);
    }
}

/// One choice on a device's or a screen's second layer: its words, a
/// note after them, whether it is what the draft says now, and the
/// changes that pick it, or why it cannot be picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub text: String,
    pub note: String,
    pub on: bool,
    pub changes: Result<Vec<Change>, String>,
}

/// One line of a second layer: its label and its choices, laid across
/// the line or down it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub choices: Vec<Choice>,
    pub down: bool,
}

/// The words a selector reads as on a layer: what a rule of it matches.
pub fn match_words(selector: &str, sort: Sort) -> String {
    let (key, value) = selector.split_once(':').unwrap_or((selector, ""));
    let one = sort.word();
    match key {
        "*" => format!(
            "any {}",
            if sort == Sort::Screen {
                "screen"
            } else {
                "input"
            }
        ),
        "class" => format!("any {value}"),
        "path" => format!("only this {one}, by its USB port"),
        "id" => format!("only this {one}, by its id, in any port"),
        "serial" => format!("only this {one}, by its serial number"),
        "name" => format!("only this {one}, by its name"),
        "bluetooth" => format!("only this {one}, by its Bluetooth address"),
        "edid" => {
            let serial = value.rsplit(':').next().unwrap_or(value);
            format!("only this screen, by its EDID serial {serial}")
        }
        "connector" => {
            let c = value.rsplit('/').next().unwrap_or(value);
            format!("only this screen, by its connector {c}")
        }
        "connector-path" => "only this screen, by its connector's path".to_owned(),
        "card" => format!("any screen on {value}"),
        _ => selector.to_owned(),
    }
}

/// The lines of the second layer of the device `item`, a display or an
/// input: where it goes, and how its rule matches it.
pub fn device_lines(flow: &Flow, item: Item) -> Vec<Line> {
    let (kind, dev) = match item {
        Item::Display(d) => (Kind::Display, flow.displays.get(d)),
        Item::Input(d) => (Kind::Input, flow.inputs.get(d)),
        _ => (Kind::Input, None),
    };
    let Some(dev) = dev else {
        return Vec::new();
    };
    let s = match kind {
        Kind::Display => Sort::Screen,
        Kind::Input => sort(dev),
    };
    let rule = dev.rule.and_then(|r| flow.rules.get(r).map(|x| (r, x)));
    // a device a rule names that is not plugged in can only be forgotten
    if !dev.found {
        let mut out = Vec::new();
        if let Some((r, x)) = rule {
            out.push(Line {
                label: super::words::ROWS_RULE.to_owned(),
                choices: vec![Choice {
                    text: format!("{} -> {}", x.selector, x.target),
                    note: super::words::rows_rule_note(r.saturating_add(1)),
                    on: true,
                    changes: Ok(Vec::new()),
                }],
                down: false,
            });
            out.push(Line {
                label: String::new(),
                choices: vec![Choice {
                    text: super::words::rows_forget(r.saturating_add(1)),
                    note: String::new(),
                    on: false,
                    changes: Ok(vec![Change::RuleRm(r.saturating_add(1))]),
                }],
                down: false,
            });
        }
        return out;
    }
    let now = rule.map(|(_, x)| flow.target(x));
    let connect = |target: Item| edit::connect(flow, &[item, target]);
    let mut goes: Vec<Choice> = flow
        .heads
        .iter()
        .enumerate()
        .map(|(h, head)| Choice {
            text: head.name.clone(),
            note: String::new(),
            on: now == Some(Target::Head(h)),
            changes: connect(Item::Head(h)),
        })
        .collect();
    let specials: &[(Special, &str)] = match kind {
        Kind::Display => &[(Special::Off, super::words::ROWS_TURNED_OFF)],
        Kind::Input => &[
            (Special::Roaming, super::words::ROWS_THE_ACTIVE),
            (Special::Ignore, super::words::ROWS_SET_ASIDE),
        ],
    };
    for (special, words) in specials {
        goes.push(Choice {
            text: (*words).to_owned(),
            note: String::new(),
            on: now == Some(Target::Special(*special)),
            changes: connect(Item::Special(*special)),
        });
    }
    let label = match kind {
        Kind::Display => super::words::ROWS_SHOWS,
        Kind::Input => super::words::ROWS_TYPES_INTO,
    };
    let mut out = vec![Line {
        label: label.to_owned(),
        choices: goes,
        down: false,
    }];
    // how its rule matches it, and the narrower keys it offers
    let mut matched: Vec<Choice> = Vec::new();
    if let Some((r, x)) = rule {
        let shared = general(&x.selector);
        matched.push(Choice {
            text: match_words(&x.selector, s),
            note: if shared {
                super::words::rows_shared(r.saturating_add(1), s.word())
            } else {
                super::words::rows_rule_note(r.saturating_add(1))
            },
            on: true,
            changes: Ok(Vec::new()),
        });
        let narrow: &[&str] = match kind {
            Kind::Display => &["edid:", "connector:"],
            Kind::Input => &["path:", "id:"],
        };
        for prefix in narrow {
            let Some(key) = dev.keys.iter().find(|k| k.starts_with(prefix)) else {
                continue;
            };
            if *key == x.selector {
                continue;
            }
            let word = match kind {
                Kind::Display => "display",
                Kind::Input => "input",
            };
            let changes = if shared {
                Change::RuleAdd(format!("{word} {key} -> {}", x.target))
            } else {
                Change::RuleSet {
                    number: r.saturating_add(1),
                    selector: Some(key.clone()),
                    target: None,
                }
            };
            matched.push(Choice {
                text: match_words(key, s),
                note: String::new(),
                on: false,
                changes: Ok(vec![changes]),
            });
        }
    } else {
        matched.push(Choice {
            text: super::words::ROWS_NO_RULE.to_owned(),
            note: String::new(),
            on: true,
            changes: Ok(Vec::new()),
        });
    }
    out.push(Line {
        label: super::words::ROWS_MATCHED_AS.to_owned(),
        choices: matched,
        down: true,
    });
    out
}

/// The one sentence that says what choosing `target` does for the device
/// of sort `s`.
pub fn what_changes(flow: &Flow, s: Sort, target: &str) -> String {
    let verb = match s {
        Sort::Keyboard => "types into",
        Sort::Screen => "shows",
        _ => "works in",
    };
    let this = format!("This {}", s.word());
    if target == super::words::ROWS_THE_ACTIVE {
        let active = active(flow);
        let now = active
            .and_then(|h| flow.heads.get(h))
            .map(|h| h.name.clone());
        let others: Vec<String> = flow
            .activatable()
            .into_iter()
            .filter(|h| Some(*h) != active)
            .filter_map(|h| flow.heads.get(h).map(|x| x.name.clone()))
            .collect();
        return super::words::rows_roams_sentence(&this, verb, now.as_deref(), &others);
    }
    if target == super::words::ROWS_SET_ASIDE {
        return super::words::rows_ignored_sentence(s.word());
    }
    if target == super::words::ROWS_TURNED_OFF {
        return super::words::ROWS_OFF_SENTENCE.to_owned();
    }
    format!("{this} {verb} {target} only, whichever terminal is active.")
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use super::super::flow::{Device, Head, Look, Rule, RuleState};
    use super::*;

    fn dev(name: &str, detail: &str, keys: &[&str], found: bool, rule: Option<usize>) -> Device {
        Device {
            name: name.into(),
            detail: detail.into(),
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            found,
            rule,
        }
    }

    fn rule(kind: Kind, selector: &str, target: &str) -> Rule {
        Rule {
            kind,
            line: 1,
            selector: selector.into(),
            target: target.into(),
            state: RuleState::Matches,
        }
    }

    /// Two terminals, a keyboard and a mouse that roam, a keyboard that
    /// waits, two inputs that fold into one box, and a keyboard a rule
    /// names that is not plugged in.
    pub(crate) fn flow() -> Flow {
        use Kind::{Display, Input};
        Flow {
            displays: vec![
                dev(
                    "DP-8",
                    "LARGE00001",
                    &["edid:AAA:0x0001:LARGE00001", "connector:card1/DP-8"],
                    true,
                    Some(0),
                ),
                dev(
                    "HDMI-A-2",
                    "SMALL00001",
                    &["edid:BBB:0x0002:SMALL00001"],
                    true,
                    Some(1),
                ),
            ],
            inputs: vec![
                dev("046d:c31c:0", "", &[], false, Some(2)),
                dev(
                    "Logitech USB Receiver",
                    "pci-0000:15:00.0-usb-0:3.3:1.0",
                    &[
                        "path:pci-0000:15:00.0-usb-0:3.3:1.0",
                        "id:046d:c31c",
                        "class:keyboard",
                    ],
                    true,
                    Some(3),
                ),
                dev(
                    "Logitech USB Receiver Mouse",
                    "pci-0000:15:00.0-usb-0:3.3:1.1",
                    &["path:pci-0000:15:00.0-usb-0:3.3:1.1", "class:mouse"],
                    true,
                    Some(4),
                ),
                dev(
                    "Power Button",
                    "LNXPWRBN/button/input0",
                    &["name:Power Button", "class:key"],
                    true,
                    Some(5),
                ),
                dev(
                    "Sleep Button",
                    "LNXSLPBN/button/input0",
                    &["name:Sleep Button", "class:key"],
                    true,
                    Some(5),
                ),
                dev(
                    "8BitDo Keyboard",
                    "pci-0000:15:00.0-usb-0:3.3.1:1.0",
                    &[
                        "path:pci-0000:15:00.0-usb-0:3.3.1:1.0",
                        "id:2dc8:5200",
                        "class:keyboard",
                    ],
                    true,
                    None,
                ),
            ],
            rules: vec![
                rule(Display, "edid:AAA:0x0001:LARGE00001", "desk"),
                rule(Display, "edid:BBB:0x0002:SMALL00001", "panel"),
                rule(Input, "serial:046d:c31c:0", "desk"),
                rule(Input, "class:keyboard", "roaming"),
                rule(Input, "class:mouse", "roaming"),
                rule(Input, "*", "ignore"),
            ],
            heads: vec![
                Head {
                    name: "desk".into(),
                    consoles: 1,
                    look: "big".into(),
                    keys: Vec::new(),
                },
                Head {
                    name: "panel".into(),
                    consoles: 1,
                    look: "small".into(),
                    keys: Vec::new(),
                },
            ],
            looks: vec![
                Look {
                    name: "big".into(),
                    frit: "monoid".into(),
                    points: 14,
                    keys: Vec::new(),
                    fonts: Vec::new(),
                    problem: None,
                },
                Look {
                    name: "small".into(),
                    frit: "dejavu".into(),
                    points: 12,
                    keys: Vec::new(),
                    fonts: Vec::new(),
                    problem: None,
                },
            ],
            active: None,
            policy: "open".into(),
            read: true,
            ..Flow::default()
        }
    }

    fn input(f: &Flow, i: usize) -> Obj {
        Obj::Input(f.inputs[i].name.clone(), f.inputs[i].detail.clone())
    }

    fn screen(f: &Flow, i: usize) -> Obj {
        Obj::Screen(f.displays[i].name.clone(), f.displays[i].detail.clone())
    }

    fn at<'a>(l: &'a Layout, o: &Obj) -> &'a Boxed {
        l.find(o)
            .unwrap_or_else(|| panic!("no box for {o:?} in {l:#?}"))
    }

    #[test]
    fn each_object_sits_in_its_row_and_column() {
        let f = flow();
        for cols in [80, 100, 120] {
            let l = layout(&f, cols);
            let g = geometry(cols);
            let desk = at(&l, &Obj::Terminal("desk".into()));
            let panel = at(&l, &Obj::Terminal("panel".into()));
            assert_eq!((desk.x, panel.x), (g.term_x, g.term_x));
            assert!(panel.y > desk.bottom(), "{cols}: the rows stack");
            assert_eq!(desk.title, "desk  · active");
            assert_eq!(desk.detail, "look big · monoid 14 pt");
            // each screen on its terminal's row, in the screens' column
            let dp = at(&l, &screen(&f, 0));
            let hdmi = at(&l, &screen(&f, 1));
            assert_eq!((dp.x, dp.y), (g.screen_x, desk.y));
            assert_eq!((hdmi.x, hdmi.y), (g.screen_x, panel.y));
            // the roaming keyboard and mouse on the active terminal's row
            let kbd = at(&l, &input(&f, 1));
            let mouse = at(&l, &input(&f, 2));
            assert_eq!((kbd.x, kbd.y), (g.dev_x, desk.y));
            assert_eq!((mouse.x, mouse.y), (g.dev_x, kbd.bottom()));
            assert_eq!(kbd.detail, "USB 3.3 · roams");
            assert!(panel.y > mouse.bottom());
            // on the other row, a dotted line and the words that say so
            let hint = l
                .notes
                .iter()
                .find(|n| n.kind == NoteKind::Hint)
                .expect("a hint");
            assert_eq!(hint.y, panel.title_row());
            assert!(hint.text.starts_with("roaming keyboards"), "{}", hint.text);
            assert!(
                l.wires
                    .iter()
                    .any(|w| w.dashed && w.points.iter().all(|p| p.1 == panel.title_row()))
            );
            // below the rule, the keyboard that waits, and what is set aside
            let wait = at(&l, &input(&f, 5));
            assert_eq!(wait.mood, Mood::Waiting);
            assert!(wait.y > l.rule);
            let others = at(&l, &Obj::Others);
            assert_eq!(others.mood, Mood::Aside);
            assert_eq!(others.title, "2 other inputs");
            assert_eq!(others.detail, "power and sleep buttons");
            let gone = at(&l, &input(&f, 0));
            assert_eq!(gone.mood, Mood::Aside);
            assert!(gone.detail.contains("unplugged"), "{}", gone.detail);
            assert_eq!(l.waiting, [Sort::Keyboard]);
            for n in &l.notes {
                if matches!(n.kind, NoteKind::Waiting | NoteKind::Aside) {
                    assert!(n.y > l.rule);
                }
            }
        }
    }

    #[test]
    fn no_box_overlaps_another_or_the_screen_s_edge() {
        let mut f = flow();
        // a crowded tray wraps to a second line
        for i in 0..6 {
            f.inputs.push(dev(
                &format!("Spare keyboard number {i}"),
                &format!("pci-0-usb-0:{i}"),
                &["class:keyboard"],
                true,
                None,
            ));
        }
        for cols in [80, 100, 120] {
            let l = layout(&f, cols);
            for (i, a) in l.boxes.iter().enumerate() {
                assert!(a.right() < cols, "{cols}: {a:?} runs off the screen");
                assert!(a.bottom() <= l.height);
                for b in l.boxes.iter().skip(i + 1) {
                    let apart = a.right() <= b.x
                        || b.right() <= a.x
                        || a.bottom() <= b.y
                        || b.bottom() <= a.y;
                    assert!(apart, "{cols}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn every_wire_runs_from_border_to_border() {
        let mut f = flow();
        // a second screen on desk, whose line merges into the first's
        f.displays.push(dev(
            "DP-9",
            "LARGE00002",
            &["edid:AAA:0x0001:LARGE00002"],
            true,
            Some(0),
        ));
        for cols in [80, 100, 120] {
            let l = layout(&f, cols);
            let solid: Vec<&Wire> = l.wires.iter().filter(|w| !w.dashed).collect();
            // two screens and two devices on desk, one screen on panel
            assert_eq!(solid.len(), 5, "{cols}");
            for w in solid {
                let from = at(&l, &w.from);
                let to = at(&l, &w.to);
                let first = w.points[0];
                let last = *w.points.last().expect("a point");
                assert_eq!(first.1, from.title_row(), "{cols}: {w:?}");
                assert_eq!(last.1, to.title_row(), "{cols}: {w:?}");
                // the first cell beside the source's border, the arrow's
                // beside the terminal's
                if from.x < to.x {
                    assert_eq!(first.0, from.right(), "{cols}: {w:?} leaves a gap");
                    assert_eq!(last.0 + 1, to.x, "{cols}: {w:?} stops short");
                } else {
                    assert_eq!(first.0 + 1, from.x, "{cols}: {w:?} leaves a gap");
                    assert_eq!(last.0, to.right(), "{cols}: {w:?} stops short");
                }
                // each turn is a right angle: one coordinate changes
                for pair in w.points.windows(2) {
                    assert!(pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1, "{w:?}");
                }
            }
        }
    }

    #[test]
    fn the_roaming_devices_move_to_the_active_terminal_s_row() {
        let mut f = flow();
        f.active = Some("panel".into());
        let l = layout(&f, 100);
        let panel = at(&l, &Obj::Terminal("panel".into()));
        assert_eq!(panel.title, "panel  · active");
        assert_eq!(at(&l, &input(&f, 1)).y, panel.y);
        let desk = at(&l, &Obj::Terminal("desk".into()));
        let hint: Vec<&Note> = l
            .notes
            .iter()
            .filter(|n| n.kind == NoteKind::Hint)
            .collect();
        assert_eq!(hint[0].y, desk.title_row());
        let words: Vec<&str> = hint.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(words.join(" "), "roaming keyboards, when desk is active");
    }

    #[test]
    fn a_row_with_a_device_of_its_own_still_shows_the_roaming_hint_below_it() {
        let mut f = flow();
        // the waiting keyboard now types into panel
        f.rules.push(rule(Kind::Input, "path:x", "panel"));
        f.inputs[5].rule = Some(f.rules.len() - 1);
        let l = layout(&f, 100);
        let panel = at(&l, &Obj::Terminal("panel".into()));
        let kbd = at(&l, &input(&f, 5));
        assert_eq!(kbd.y, panel.y);
        let hint = l
            .notes
            .iter()
            .find(|n| n.kind == NoteKind::Hint)
            .expect("a hint");
        assert_eq!(hint.y, kbd.bottom() + 1, "the hint sits in the slot below");
        let dotted = l.wires.iter().find(|w| w.dashed).expect("a dotted line");
        // it runs from the device column to the line's trunk, and up to the
        // row under the keyboard's line
        let g = geometry(100);
        assert_eq!(dotted.points[0], (g.dev_x - 1, hint.y));
        assert_eq!(
            *dotted.points.last().expect("a point"),
            (g.dev_join, kbd.title_row() + 1)
        );
        assert!(
            l.find(&Obj::Terminal("desk".into()))
                .expect("desk")
                .bottom()
                < panel.y
        );
        assert!(
            panel.y + 2 * BOX_ROWS <= l.rule,
            "the row is two boxes tall"
        );
    }

    #[test]
    fn the_arrows_move_across_and_down() {
        let f = flow();
        let l = layout(&f, 120);
        let desk = Obj::Terminal("desk".into());
        assert_eq!(step(&l, &desk, Key::Left), Some(screen(&f, 0)));
        assert_eq!(step(&l, &desk, Key::Right), Some(input(&f, 1)));
        assert_eq!(
            step(&l, &desk, Key::Down),
            Some(Obj::Terminal("panel".into()))
        );
        assert_eq!(step(&l, &input(&f, 1), Key::Down), Some(input(&f, 2)));
        // from the mouse below the keyboard, Left goes to its terminal
        assert_eq!(step(&l, &input(&f, 2), Key::Left), Some(desk.clone()));
        assert_eq!(step(&l, &screen(&f, 0), Key::Up), None);
        assert_eq!(step(&l, &desk, Key::Home), Some(screen(&f, 0)));
    }

    #[test]
    fn the_highlighted_box_stays_in_sight_as_it_moves() {
        let f = flow();
        let l = layout(&f, 80);
        let window = 8;
        let mut s = State::default();
        s.settle(&l, window);
        assert_eq!(s.at, Some(screen(&f, 0)));
        assert_eq!(s.top, 0);
        // down to the tray: the window scrolls just enough to show it
        let mut at = s.at.clone().expect("a highlight");
        for _ in 0..10 {
            let Some(next) = step(&l, &at, Key::Down) else {
                break;
            };
            s.go(&l, next.clone(), window);
            let b = at_box(&l, &next);
            assert!(b.y >= s.top && b.bottom() <= s.top + window, "{s:?} {b:?}");
            at = next;
        }
        let b = at_box(&l, &at);
        assert!(b.y > l.rule, "the highlight reached the trays: {b:?}");
        assert_eq!(s.top, b.bottom() - window, "scrolled line by line, no more");
        // back up one box: the window does not move while it is in sight
        let top = s.top;
        let up = step(&l, &at, Key::Up).expect("a box above");
        s.go(&l, up.clone(), window);
        let ub = at_box(&l, &up);
        if ub.y >= top {
            assert_eq!(s.top, top);
        } else {
            assert_eq!(s.top, ub.y);
        }
        // a page up and a page down move the window, and keep the
        // highlighted box whole in it
        let before = s.top;
        s.page(&l, false, window);
        assert!(s.top < before, "{s:?}");
        let b = at_box(&l, s.at.as_ref().expect("a highlight"));
        assert!(b.y >= s.top && b.bottom() <= s.top + window, "{s:?} {b:?}");
        let before = s.top;
        s.page(&l, true, window);
        assert!(s.top > before, "{s:?}");
        let b = at_box(&l, s.at.as_ref().expect("a highlight"));
        assert!(b.y >= s.top && b.bottom() <= s.top + window, "{s:?} {b:?}");
    }

    fn at_box<'a>(l: &'a Layout, o: &Obj) -> &'a Boxed {
        at(l, o)
    }

    #[test]
    fn a_new_layout_keeps_the_highlight_where_it_was_on_the_page() {
        let mut f = flow();
        let window = 9;
        let l = layout(&f, 80);
        let mut s = State::default();
        let mouse = input(&f, 2);
        s.go(&l, Obj::Terminal("panel".into()), window);
        s.go(&l, mouse.clone(), window);
        let before = at_box(&l, &mouse).y - s.top;
        // the keyboard above it is unplugged: it drops out of the row, the
        // mouse moves up the canvas, and stays at its height in the window
        f.inputs[1].found = false;
        let l2 = layout(&f, 80);
        s.settle(&l2, window);
        assert_eq!(s.at, Some(mouse.clone()));
        assert_eq!(at_box(&l2, &mouse).y - s.top, before);
        // the mouse itself goes: the highlight moves to the box nearest it
        f.inputs.remove(2);
        let l3 = layout(&f, 80);
        s.settle(&l3, window);
        assert!(s.at.is_some());
        assert_ne!(s.at, Some(mouse));
    }

    #[test]
    fn a_waiting_keyboard_offers_each_terminal_and_says_what_each_does() {
        let f = flow();
        let lines = device_lines(&f, Item::Input(5));
        let names: Vec<&str> = lines[0].choices.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            names,
            ["desk", "panel", "the active terminal (roams)", "set aside"]
        );
        assert!(lines[0].choices.iter().all(|c| !c.on));
        assert_eq!(
            lines[0].choices[0].changes,
            Ok(vec![Change::RuleAdd(
                "input path:pci-0000:15:00.0-usb-0:3.3.1:1.0 -> desk".into()
            )])
        );
        let kbd = device_lines(&f, Item::Input(1));
        assert!(kbd[0].choices[2].on, "the keyboard roams");
        assert_eq!(kbd[1].choices[0].text, "any keyboard");
        assert_eq!(
            kbd[1].choices[1].text,
            "only this keyboard, by its USB port"
        );
        assert_eq!(
            what_changes(&f, Sort::Keyboard, "the active terminal (roams)"),
            "This keyboard types into whichever terminal is active: desk now, and panel after Ctrl+Super+Up or Ctrl+Super+Down."
        );
    }

    #[test]
    fn a_port_reads_as_a_reader_says_it() {
        assert_eq!(port_words("pci-0000:15:00.0-usb-0:3.3:1.0"), "USB 3.3");
        assert_eq!(port_words("usb-0:3.3"), "USB 3.3");
        assert_eq!(port_words("platform-i8042-serio-0"), "built in");
    }
}
