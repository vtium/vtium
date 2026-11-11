//! The Flow view's layout (ADR-1403 D7). The screen is one scrolling
//! document in three parts:
//!
//! - CONFIGURATION: each path of the configuration as a row of bordered boxes,
//!   the displays that show a terminal, the terminal with its font set, and
//!   the inputs that type into it, joined by lines from border to border;
//!   then the ROAMING strip, the inputs that type into whichever terminal is
//!   active, drawn once, which a short dotted line on each path's row
//!   points to.
//! - NEW PATH: the editor, a row of the same shape drawn dashed, where the
//!   person composes a path from pieces before adding it.
//! - PARTS: the palette of displays, inputs and font sets to pick from.
//!
//! [`layout`] lays the three parts out on a canvas as tall as they need,
//! as plain data: boxes, lines and notes. [`State`] is where the highlight
//! is, which canvas row is at the window's top, and the path in the
//! editor. [`super::flowdraw`] draws the layout, [`super::paths`] holds the
//! path model, and the keys are in `flowkeys`.

use super::flow::{Device, Flow, Kind};
use super::input::Key;
use super::parts::{Part, Stage};
use super::paths::{self, ANY_KEYBOARD, ANY_POINTER, Doc, Path, Piece};
use super::words;

/// The rows a box takes: its top border, its title, its detail and its
/// bottom border.
pub const BOX_ROWS: usize = 4;

/// The blank columns between two boxes of a group of pieces.
const GAP: usize = 2;

/// The widest a box grows to fit its text, borders and margins included,
/// before a line stretches it: 30 characters of text.
pub const BOX_MAX: usize = 34;

/// The width a box takes for its text: the longer of its two lines, with
/// a border and a blank on each side, up to [`BOX_MAX`].
pub fn natural(title: &str, detail: &str) -> usize {
    let text = super::canvas::width(title).max(super::canvas::width(detail));
    text.saturating_add(4).min(BOX_MAX)
}

/// Lays boxes `widths` wide out like words, from column `left` up to the
/// column `right`, which no box reaches: each box whole on a line, [`GAP`]
/// columns apart, and a box that does not fit on a line starting the next
/// one. A box wider than a line takes the line. Where no line holds more
/// than two boxes and the boxes take more than one line, the terminal is
/// narrow, so the boxes of each line stretch to share its width. Returns
/// each line's boxes, as their columns and widths.
pub fn flow_boxes(widths: &[usize], left: usize, right: usize) -> Vec<Vec<(usize, usize)>> {
    let room = right.saturating_sub(left).max(1);
    let mut lines: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut x = left;
    for w in widths {
        let w = (*w).min(room);
        let starts = lines
            .last()
            .is_none_or(|line| !line.is_empty() && x.saturating_add(GAP).saturating_add(w) > right);
        if starts {
            lines.push(Vec::new());
            x = left;
        } else {
            x = x.saturating_add(GAP);
        }
        if let Some(line) = lines.last_mut() {
            line.push((x, w));
        }
        x = x.saturating_add(w);
    }
    let narrow = lines.len() > 1 && lines.iter().all(|l| l.len() <= 2);
    if narrow {
        for line in &mut lines {
            let n = line.len();
            let gaps = GAP.saturating_mul(n.saturating_sub(1));
            let each = room.saturating_sub(gaps) / n.max(1);
            let mut x = left;
            for (k, b) in line.iter_mut().enumerate() {
                // the last box takes what division leaves over
                let w = if k.saturating_add(1) == n {
                    right.saturating_sub(x)
                } else {
                    each
                };
                *b = (x, w);
                x = x.saturating_add(w).saturating_add(GAP);
            }
        }
    }
    lines
}

/// Where a box is: in a path of the document, by the path's terminal; in
/// the editor; or in the palette.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Obj {
    Doc(String, Slot),
    New(Slot),
    Pal(Slot),
    /// The path of the active terminal, the roaming inputs': its terminal
    /// or one of its inputs.
    Roam(Slot),
    /// The path to no terminal: the displays that stay off and the inputs
    /// set aside, and its terminal's box.
    Nothing(Slot),
    /// The box that folds every input that is neither a keyboard nor a
    /// pointer, which cannot be picked.
    Others,
}

/// What a box holds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Display(String),
    Input(String),
    /// A path's terminal box.
    Terminal,
    Font(Option<String>),
    /// A part that matches no device: "no input", "the active terminal",
    /// "no terminal", or a new terminal by its name.
    Special(Piece),
    /// A stage's template in PARTS, which is never placed in a path.
    Template(Stage),
    /// A stage's empty place in NEW PATH.
    Empty(Stage),
}

impl Slot {
    /// The slot that holds `piece` in PARTS.
    pub fn of(piece: &Piece) -> Slot {
        match piece {
            Piece::Display(s) => Slot::Display(s.clone()),
            Piece::Input(s) => Slot::Input(s.clone()),
            Piece::Font(f) => Slot::Font(f.clone()),
            p => Slot::Special(p.clone()),
        }
    }

    /// The piece the slot holds; `None` for a terminal's box, a template
    /// and an empty place.
    pub fn piece(&self) -> Option<Piece> {
        match self {
            Slot::Display(s) => Some(Piece::Display(s.clone())),
            Slot::Input(s) => Some(Piece::Input(s.clone())),
            Slot::Font(f) => Some(Piece::Font(f.clone())),
            Slot::Special(p) => Some(p.clone()),
            Slot::Terminal | Slot::Template(_) | Slot::Empty(_) => None,
        }
    }
}

/// What state a box is in, which its border's color shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// In a path, or a piece anyone may pick: white.
    Placed,
    /// A device connected that no path holds: amber.
    Waiting,
    /// What cannot be picked: grey.
    Aside,
    /// Picked into the editor, in a color no state uses.
    Picked,
}

/// What sort of device a device is, by the classes it offers.
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
const POINTERS: [&str; 6] = [
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
    } else if dev.keys.iter().any(|k| POINTERS.contains(&k.as_str())) {
        Sort::Pointer
    } else {
        Sort::Other
    }
}

/// A box on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boxed {
    pub obj: Obj,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub title: String,
    pub detail: String,
    pub mood: Mood,
    /// Drawn with a dashed border: a box in the editor, not yet added.
    pub dashed: bool,
    /// Drawn as a template, with a double border: a stage's template in
    /// PARTS, or an empty place in NEW PATH.
    pub template: bool,
}

impl Boxed {
    /// The row of the box's title, where its line meets it.
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

/// A line from a piece to its terminal: the corners of its path, from the
/// cell beside the piece's border to the cell beside the terminal's,
/// where its arrow is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: Obj,
    pub to: Obj,
    pub points: Vec<(usize, usize)>,
    /// A dashed line, in the editor, or the dotted line that says the
    /// roaming inputs reach a terminal.
    pub dashed: bool,
    /// The dotted line from the words `roaming inputs` to a terminal of the
    /// configuration, which has no box at its start: it runs from the
    /// terminal's own box to itself.
    pub stub: bool,
}

/// What a line of text on the canvas is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    /// An area's heading on the canvas: NEW PATH, PARTS. CONFIGURATION's
    /// stays at the top of the screen while its area is in sight.
    Heading,
    /// The editor's words: what the new path needs, or that it is ready.
    Needs,
    Ready,
    /// Grey words.
    Grey,
    /// A group of pieces' title.
    Displays,
    Inputs,
    Fonts,
    Terminals,
    /// The words at the start of a roaming stub, grey and italic.
    Stub,
}

/// A line of text on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub x: usize,
    pub y: usize,
    pub text: String,
    pub kind: NoteKind,
}

/// The three columns of a path's row: where each box of a column starts
/// and how wide it is, and where the lines that merge turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub screen_x: usize,
    pub screen_w: usize,
    pub term_x: usize,
    pub term_w: usize,
    pub dev_x: usize,
    pub dev_w: usize,
    /// The column where the lines from several displays merge.
    pub screen_join: usize,
    /// The column where the lines from several inputs merge.
    pub dev_join: usize,
}

/// The columns at a screen `cols` wide: the boxes and the gaps between
/// them share the width as the mock-up at 120 columns does, 25, 11, 28,
/// 14 and 40 of its 118, with a margin of one column at each side.
/// The fewest cells a terminal's box in a path takes.
const TERM_MIN_W: usize = 32;

pub fn geometry(cols: usize) -> Geometry {
    let room = cols.saturating_sub(2);
    let part = |n: usize| room.saturating_mul(n) / 118;
    // a box in a path shows its whole title: the displays' column holds
    // All displays; the terminals' column holds Active terminal, and is at
    // least TERM_MIN_W wide so that the start of its description shows
    let title = |t: &str| super::canvas::width(t).saturating_add(4);
    let screen_w = part(25).max(title(words::ANY_DISPLAY));
    let gap1 = part(11).max(5);
    let term_w = part(28).max(title(words::ACTIVE_TERMINAL)).max(TERM_MIN_W);
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

/// The words the box of other inputs gives each of them, a kind of input
/// a reader knows, from the name it reports.
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

/// The inputs the box of other inputs folds, by their index.
pub fn folded(flow: &Flow) -> Vec<usize> {
    (0..flow.inputs.len())
        .filter(|d| {
            flow.inputs
                .get(*d)
                .is_some_and(|x| x.found && sort(x) == Sort::Other)
        })
        .collect()
}

/// Whether the input selector `sel` names the device `dev`.
fn names(dev: &Device, sel: &str) -> bool {
    sel == "*"
        || dev.keys.iter().any(|k| k == sel)
        || (sel.starts_with("path:") && dev.keys.iter().any(|k| k.starts_with(sel)))
        || (sel == ANY_POINTER && sort(dev) == Sort::Pointer)
        || (sel == ANY_KEYBOARD
            && dev
                .keys
                .iter()
                .any(|k| paths::expand(ANY_KEYBOARD).contains(&k.as_str())))
}

/// The terminals of the configuration whose paths hold the device `dev`:
/// for a display, those that name it by one of its keys or take any
/// display; for an input, those whose parts match it.
pub fn holders(doc: &[Path], kind: Kind, dev: &Device) -> Vec<String> {
    doc.iter()
        .filter(|p| match kind {
            Kind::Display => p.displays.iter().any(|s| s == "*" || dev.keys.contains(s)),
            Kind::Input => p.inputs.iter().any(|s| names(dev, s)),
        })
        .filter_map(|p| p.name.clone())
        .collect()
}

/// A box's two lines for the part of `kind` that matches by `sel`: when
/// the person gave the part a label, the label as the title and the name
/// the system gives it under it; else `words` as they are.
fn labelled(ctx: &Ctx<'_>, kind: Kind, sel: &str, words: (String, String)) -> (String, String) {
    match ctx.doc.label(kind, sel) {
        Some(l) => (l.to_owned(), words.0),
        None => words,
    }
}

/// A font set's words on a terminal's second line.
pub fn font_words(font: Option<&str>) -> String {
    match font {
        Some(f) => format!("font set {f}"),
        None => words::DEFAULT_FONT_SET.to_owned(),
    }
}

/// What the view lays out: the draft's flow, its configuration as paths,
/// the path in the editor, the font sets to pick from, and the screen's
/// width.
pub struct Ctx<'a> {
    pub flow: &'a Flow,
    pub doc: &'a Doc,
    pub new: &'a Path,
    /// The parts PARTS shows, as the view holds them.
    pub parts: &'a [Part],
    pub cols: usize,
    /// The configuration is vtium's built-in default.
    pub default: bool,
}

/// The flow laid out on its canvas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub boxes: Vec<Boxed>,
    pub wires: Vec<Wire>,
    pub notes: Vec<Note>,
    /// The canvas rows of the rule lines between the parts.
    pub rules: Vec<usize>,
    /// The canvas's height: the row after its last box.
    pub height: usize,
    pub geometry: Option<Geometry>,
    /// The devices connected that no path holds, by sort.
    pub waiting: Vec<Sort>,
    /// The canvas row of each area's heading, NEW PATH's and PARTS', with
    /// its words; CONFIGURATION's area starts at the canvas's top.
    pub areas: Vec<(usize, &'static str)>,
}

impl Layout {
    /// The box of `obj`.
    pub fn find(&self, obj: &Obj) -> Option<&Boxed> {
        self.boxes.iter().find(|b| b.obj == *obj)
    }

    /// The heading of the area the canvas row `top` is in: the last area
    /// whose heading is above it, else CONFIGURATION.
    pub fn area_at(&self, top: usize) -> &'static str {
        self.areas
            .iter()
            .rev()
            .find(|(y, _)| *y < top)
            .map_or(words::FLOW_PATHS, |(_, a)| *a)
    }

    /// The boxes in reading order, across and then down.
    pub fn reading(&self) -> Vec<&Boxed> {
        let mut out: Vec<&Boxed> = self.boxes.iter().collect();
        out.sort_by_key(|b| (b.y, b.x));
        out
    }
}

/// Which row [`path_row`] lays out: a path of the configuration, by its
/// terminal's name; the editor's; or the path to no terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row<'a> {
    Doc(&'a str),
    New,
    Nothing,
}

/// A box of a path's row: in the editor dashed, and a template's look on
/// an empty place.
fn part_box(obj: Obj, x: usize, y: usize, w: usize, words: (String, String), mood: Mood) -> Boxed {
    let dashed = matches!(obj, Obj::New(_));
    let template = matches!(obj, Obj::New(Slot::Empty(_)));
    Boxed {
        obj,
        x,
        y,
        w,
        title: words.0,
        detail: words.1,
        mood,
        dashed: dashed && !template,
        template,
    }
}

/// Lays out the row of the path `p`, its terminal named `name`, from
/// canvas row `y`. A path of the configuration that roaming inputs reach
/// ends in a dotted line from the words `roaming inputs` to its terminal.
/// In the editor, each stage the path has no part for yet shows its empty
/// place, which Enter fills from that stage's template. A path with "no
/// input" shows it in its inputs' place, with no line, since nothing types
/// into its terminal. Returns the row after it.
fn path_row(
    ctx: &Ctx<'_>,
    out: &mut Layout,
    p: &Path,
    name: &str,
    y: usize,
    row: Row<'_>,
) -> usize {
    let g = geometry(ctx.cols);
    let obj = |slot: Slot| match row {
        Row::New => Obj::New(slot),
        Row::Nothing => Obj::Nothing(slot),
        Row::Doc(n) => Obj::Doc(n.to_owned(), slot),
    };
    let editor = row == Row::New;
    let empty_terminal = editor && p.name.is_none() && !p.active && !p.none;
    let term = obj(if empty_terminal {
        Slot::Empty(Stage::Terminal)
    } else {
        Slot::Terminal
    });
    let words = if empty_terminal {
        (
            Stage::Terminal.name().to_owned(),
            words::new_terminal_named(name),
        )
    } else if p.none {
        (
            words::NO_TERMINAL.to_owned(),
            words::NO_TERMINAL_DETAIL.to_owned(),
        )
    } else if p.active {
        (name.to_owned(), words::ACTIVE_DETAIL.to_owned())
    } else {
        (
            name.to_owned(),
            format!(
                "{} · {}",
                font_words(p.font.as_deref()),
                words::vt_words(&p.vts())
            ),
        )
    };
    let mood = if p.none { Mood::Aside } else { Mood::Placed };
    let term_box = part_box(term.clone(), g.term_x, y, g.term_w, words, mood);
    let join_row = term_box.title_row();
    out.boxes.push(term_box);
    // each side's boxes, line by line: in the editor they flow like words
    // across the side's column, and elsewhere each box takes a line of its
    // own across the whole column
    let side = |boxes: Vec<((String, String), Slot, Mood, bool)>, x: usize, w: usize| {
        let widths: Vec<usize> = boxes.iter().map(|((t, d), ..)| natural(t, d)).collect();
        let lines = if editor {
            flow_boxes(&widths, x, x.saturating_add(w))
        } else {
            widths.iter().map(|_| vec![(x, w)]).collect()
        };
        let mut boxes = boxes.into_iter();
        let mut out: Vec<Vec<(Boxed, bool)>> = Vec::new();
        for (n, line) in lines.into_iter().enumerate() {
            let row = y.saturating_add(n.saturating_mul(BOX_ROWS));
            let mut placed = Vec::new();
            for (bx, bw) in line {
                let Some((words, slot, mood, wired)) = boxes.next() else {
                    break;
                };
                placed.push((part_box(obj(slot), bx, row, bw, words, mood), wired));
            }
            out.push(placed);
        }
        out
    };
    let mut displays: Vec<((String, String), Slot, Mood, bool)> = p
        .displays
        .iter()
        .map(|s| {
            let words = labelled(
                ctx,
                Kind::Display,
                s,
                paths::title(ctx.flow, Kind::Display, s),
            );
            (words, Slot::Display(s.clone()), mood, true)
        })
        .collect();
    if editor && displays.is_empty() && !p.active {
        let words = (
            Stage::Display.name().to_owned(),
            words::EMPTY_PLACE.to_owned(),
        );
        displays.push((words, Slot::Empty(Stage::Display), mood, false));
    }
    let display_lines = side(displays, g.screen_x, g.screen_w);
    for (n, line) in display_lines.iter().enumerate() {
        // the box nearest the terminal, the line's last, takes the line to
        // it, and each box before it joins the box after it
        for (k, (b, wired)) in line.iter().enumerate() {
            if !wired {
                continue;
            }
            let start = (b.right(), b.title_row());
            let points = match line.get(k.saturating_add(1)) {
                Some((next, _)) => vec![start, (next.x.saturating_sub(1), b.title_row())],
                None if n == 0 => vec![start, (g.term_x.saturating_sub(1), join_row)],
                None => vec![
                    start,
                    (g.screen_join, b.title_row()),
                    (g.screen_join, join_row),
                    (g.term_x.saturating_sub(1), join_row),
                ],
            };
            let to = line
                .get(k.saturating_add(1))
                .map_or_else(|| term.clone(), |(next, _)| next.obj.clone());
            out.wires.push(Wire {
                from: b.obj.clone(),
                to,
                points,
                dashed: b.dashed,
                stub: false,
            });
        }
    }
    let mut inputs: Vec<((String, String), Slot, Mood, bool)> = p
        .inputs
        .iter()
        .map(|s| {
            let words = labelled(ctx, Kind::Input, s, paths::title(ctx.flow, Kind::Input, s));
            (words, Slot::Input(s.clone()), mood, true)
        })
        .collect();
    if p.no_input {
        let words = (
            words::NO_INPUT.to_owned(),
            words::NO_INPUT_DETAIL.to_owned(),
        );
        inputs.push((words, Slot::Special(Piece::NoInput), Mood::Aside, false));
    } else if editor && inputs.is_empty() {
        let words = (
            Stage::Input.name().to_owned(),
            words::EMPTY_PLACE.to_owned(),
        );
        inputs.push((words, Slot::Empty(Stage::Input), mood, false));
    }
    let input_lines = side(inputs, g.dev_x, g.dev_w);
    // the line from an input to the terminal: straight from the first
    // line's first box, and merging into it from the other lines'
    let line_from = |row: usize, n: usize| {
        let start = (g.dev_x.saturating_sub(1), row);
        let end = (g.term_x.saturating_add(g.term_w), join_row);
        if n == 0 {
            vec![start, end]
        } else {
            vec![start, (g.dev_join, row), (g.dev_join, join_row), end]
        }
    };
    for (n, line) in input_lines.iter().enumerate() {
        // the line's first box takes the line to the terminal, and each box
        // after it joins the box before it
        for (k, (b, wired)) in line.iter().enumerate() {
            if !wired {
                continue;
            }
            let (points, to) = match k.checked_sub(1).and_then(|i| line.get(i)) {
                Some((before, _)) => (
                    vec![
                        (b.x.saturating_sub(1), b.title_row()),
                        (before.right(), b.title_row()),
                    ],
                    before.obj.clone(),
                ),
                None => (line_from(b.title_row(), n), term.clone()),
            };
            out.wires.push(Wire {
                from: b.obj.clone(),
                to,
                points,
                dashed: b.dashed,
                stub: false,
            });
        }
    }
    let k = input_lines.len();
    let d = display_lines.len();
    for line in display_lines.into_iter().chain(input_lines) {
        out.boxes.extend(line.into_iter().map(|(b, _)| b));
    }
    let mut bottom = y.saturating_add(k.saturating_mul(BOX_ROWS));
    // roaming inputs reach a terminal of the configuration that has no
    // "no input"
    if matches!(row, Row::Doc(_)) && !p.no_input && !ctx.doc.roaming.is_empty() {
        // on the terminal's title row when the path has no input of its
        // own, else just under its last input
        let row = if k == 0 {
            join_row
        } else {
            y.saturating_add(k.saturating_mul(BOX_ROWS))
        };
        out.notes.push(Note {
            x: g.dev_x,
            y: row,
            text: super::canvas::fit(words::FLOW_ROAMING_STUB, g.dev_w),
            kind: NoteKind::Stub,
        });
        out.wires.push(Wire {
            from: term.clone(),
            to: term.clone(),
            points: line_from(row, k),
            dashed: true,
            stub: true,
        });
        bottom = bottom.max(row.saturating_add(1));
    }
    let displays = y.saturating_add(d.saturating_mul(BOX_ROWS));
    bottom.max(displays).max(y.saturating_add(BOX_ROWS))
}

/// Lays out the path of the active terminal from canvas row `y`: the
/// heading ROAMING and the sentence that says what roaming means where a
/// path's displays are, since the active terminal is one of the other
/// terminals at a time and has none of its own; the active terminal's
/// box, dashed because it changes as the person switches terminals; and
/// each roaming input's box, with a dashed line to it. Returns the row
/// after it.
fn roaming_row(ctx: &Ctx<'_>, out: &mut Layout, y: usize) -> usize {
    let g = geometry(ctx.cols);
    out.notes.push(Note {
        x: 1,
        y: y.saturating_add(1),
        text: words::FLOW_ROAMING.to_owned(),
        kind: NoteKind::Inputs,
    });
    let room = g.term_x.saturating_sub(3).max(1);
    let mut bottom = y.saturating_add(BOX_ROWS);
    for (i, line) in super::canvas::wrap(words::FLOW_ROAMING_ABOUT, room, 3)
        .into_iter()
        .enumerate()
    {
        let row = y.saturating_add(2).saturating_add(i);
        out.notes.push(Note {
            x: 1,
            y: row,
            text: line,
            kind: NoteKind::Grey,
        });
        bottom = bottom.max(row.saturating_add(1));
    }
    let term = Obj::Roam(Slot::Terminal);
    let term_box = Boxed {
        obj: term.clone(),
        x: g.term_x,
        y,
        w: g.term_w,
        title: words::ACTIVE_TERMINAL.to_owned(),
        detail: words::ACTIVE_DETAIL.to_owned(),
        mood: Mood::Placed,
        dashed: true,
        template: false,
    };
    let join_row = term_box.title_row();
    out.boxes.push(term_box);
    for (k, sel) in ctx.doc.roaming.iter().enumerate() {
        let (title, detail) = labelled(
            ctx,
            Kind::Input,
            sel,
            paths::title(ctx.flow, Kind::Input, sel),
        );
        let b = Boxed {
            obj: Obj::Roam(Slot::Input(sel.clone())),
            x: g.dev_x,
            y: y.saturating_add(k.saturating_mul(BOX_ROWS)),
            w: g.dev_w,
            title,
            detail,
            mood: Mood::Placed,
            dashed: false,
            template: false,
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
            from: b.obj.clone(),
            to: term.clone(),
            points,
            dashed: true,
            stub: false,
        });
        bottom = bottom.max(b.bottom());
        out.boxes.push(b);
    }
    bottom
}

/// A piece of the palette, before it is placed.
struct PalPiece {
    slot: Slot,
    title: String,
    detail: String,
    mood: Mood,
    template: bool,
}

/// What a part's box says under its title, and the state its border
/// shows: grey when its device is not connected or a path holds it as a
/// display can be held once, amber for a connected device no path holds,
/// white for the rest. A connected device no path holds is counted in
/// `waiting`.
fn part_state(ctx: &Ctx<'_>, part: &Part, waiting: &mut Vec<Sort>) -> (String, Mood) {
    let (flow, doc) = (ctx.flow, ctx.doc);
    if !part.connected {
        return (words::NOT_PLUGGED.to_owned(), Mood::Aside);
    }
    let holding = |sel: &str, kind: Kind| -> Vec<String> {
        doc.paths
            .iter()
            .filter(|p| match kind {
                Kind::Display => p.displays.iter().any(|s| s == sel),
                Kind::Input => p.inputs.iter().any(|s| s == sel),
            })
            .filter_map(|p| p.name.clone())
            .collect()
    };
    let device = |kind: Kind, sel: &str| {
        flow.devices(kind)
            .iter()
            .find(|d| d.found && paths::own_key(flow, kind, d) == sel)
    };
    match &part.piece {
        Piece::Display(sel) => {
            let dev = device(Kind::Display, sel);
            let held = dev.map_or_else(
                || holding(sel, Kind::Display),
                |d| holders(&doc.paths, Kind::Display, d),
            );
            let off = doc
                .off
                .iter()
                .any(|o| o == sel || dev.is_some_and(|d| d.keys.contains(o)));
            match (held.first(), dev) {
                (Some(h), _) => (words::in_terminal(h), Mood::Aside),
                _ if off => (words::TURNED_OFF.to_owned(), Mood::Aside),
                (None, Some(d)) => {
                    waiting.push(Sort::Screen);
                    (
                        format!("{} · {}", d.detail, words::IN_NO_PATH),
                        Mood::Waiting,
                    )
                }
                (None, None) => (paths::title(flow, Kind::Display, sel).1, Mood::Placed),
            }
        }
        Piece::Input(sel) => {
            let Some(d) = device(Kind::Input, sel) else {
                let held = holding(sel, Kind::Input);
                let detail = match held.as_slice() {
                    [one] => words::in_terminal(one),
                    [] if doc.roaming.contains(sel) => words::ROAMING_WORD.to_owned(),
                    [] if doc.ignored.contains(sel) => words::SET_ASIDE.to_owned(),
                    [] => paths::title(flow, Kind::Input, sel).1,
                    many => words::roams_among(many.len()),
                };
                return (detail, Mood::Placed);
            };
            let held = holders(&doc.paths, Kind::Input, d);
            let port = port_words(&d.detail);
            let roams = doc.roaming.iter().any(|r| names(d, r));
            let aside = doc.ignored.iter().any(|r| r != "*" && names(d, r));
            match held.as_slice() {
                [] if roams => (format!("{port} · {}", words::ROAMING_WORD), Mood::Placed),
                [] if aside => (format!("{port} · {}", words::SET_ASIDE), Mood::Aside),
                [] => {
                    waiting.push(sort(d));
                    (format!("{port} · {}", words::IN_NO_PATH), Mood::Waiting)
                }
                [one] => (
                    format!("{port} · {}", words::in_terminal(one)),
                    Mood::Placed,
                ),
                many => (
                    format!("{port} · {}", words::roams_among(many.len())),
                    Mood::Placed,
                ),
            }
        }
        Piece::Font(f) => {
            let users: Vec<String> = doc
                .paths
                .iter()
                .filter(|p| p.font == *f)
                .filter_map(|p| p.name.clone())
                .collect();
            let detail = match (users.first(), f) {
                (Some(u), _) => words::in_terminal(u),
                (None, None) => words::DEFAULT_FONT_SET_DETAIL.to_owned(),
                (None, Some(_)) => words::FONT_SET.to_owned(),
            };
            (detail, Mood::Placed)
        }
        Piece::NoInput => (words::NO_INPUT_DETAIL.to_owned(), Mood::Placed),
        Piece::Active => (words::ACTIVE_DETAIL.to_owned(), Mood::Placed),
        Piece::NoTerminal => (words::NO_TERMINAL_DETAIL.to_owned(), Mood::Placed),
        Piece::Terminal(_) => (words::NEW_TERMINAL_DETAIL.to_owned(), Mood::Placed),
    }
}

/// The palette's four groups, one per stage: its template first, then
/// each part of that stage, with its words and its state.
fn palette(ctx: &Ctx<'_>, waiting: &mut Vec<Sort>) -> Vec<(NoteKind, &'static str, Vec<PalPiece>)> {
    // the default font set is the absence of one, so it shows picked in a
    // path that has parts and no font set
    let picked = |piece: &Piece| match piece {
        Piece::Font(None) => ctx.new.font.is_none() && !ctx.new.is_empty(),
        p => ctx.new.has(p),
    };
    Stage::ALL
        .into_iter()
        .map(|stage| {
            let mut pieces = vec![PalPiece {
                slot: Slot::Template(stage),
                title: stage.name().to_owned(),
                detail: words::template_add(stage.name()),
                mood: Mood::Placed,
                template: true,
            }];
            for part in ctx.parts.iter().filter(|p| Stage::of(&p.piece) == stage) {
                let (detail, mood) = part_state(ctx, part, waiting);
                // a labelled part shows its label, and its own name under it
                let (title, detail) = match &part.piece {
                    Piece::Display(s) => {
                        labelled(ctx, Kind::Display, s, (part.title.clone(), detail))
                    }
                    Piece::Input(s) => labelled(ctx, Kind::Input, s, (part.title.clone(), detail)),
                    _ => (part.title.clone(), detail),
                };
                let on = picked(&part.piece);
                pieces.push(PalPiece {
                    slot: Slot::of(&part.piece),
                    title: if on { format!("✓ {title}") } else { title },
                    detail,
                    mood: if on { Mood::Picked } else { mood },
                    template: false,
                });
            }
            let (kind, heading) = match stage {
                Stage::Display => (NoteKind::Displays, words::FLOW_DISPLAYS),
                Stage::Terminal => (NoteKind::Terminals, words::FLOW_TERMINALS),
                Stage::Input => (NoteKind::Inputs, words::FLOW_INPUTS),
                Stage::Font => (NoteKind::Fonts, words::FLOW_FONT_SETS),
            };
            (kind, heading, pieces)
        })
        .collect()
}

/// The document laid out at the context's width: the paths, the editor,
/// and the palette.
pub fn layout(ctx: &Ctx<'_>) -> Layout {
    let g = geometry(ctx.cols);
    let mut out = Layout {
        geometry: Some(g),
        ..Layout::default()
    };
    let note = |out: &mut Layout, x: usize, y: usize, text: String, kind: NoteKind| {
        out.notes.push(Note { x, y, text, kind });
    };
    // CONFIGURATION: its heading and its column titles scroll with it
    note(
        &mut out,
        1,
        0,
        words::FLOW_PATHS.to_owned(),
        NoteKind::Heading,
    );
    for (x, w, title, kind) in [
        (
            g.screen_x,
            g.screen_w,
            words::FLOW_DISPLAYS,
            NoteKind::Displays,
        ),
        (
            g.term_x,
            g.term_w,
            words::FLOW_TERMINALS,
            NoteKind::Terminals,
        ),
        (g.dev_x, g.dev_w, words::FLOW_INPUTS, NoteKind::Inputs),
    ] {
        note(&mut out, x, 1, super::canvas::fit(title, w), kind);
    }
    let mut y = 1usize;
    let grey = |out: &mut Layout, y: &mut usize, text: String| {
        *y = y.saturating_add(1);
        note(out, 1, *y, text, NoteKind::Grey);
    };
    if ctx.doc.paths.is_empty() {
        grey(&mut out, &mut y, words::NO_PATHS.to_owned());
    }
    if ctx.default {
        let room = ctx.cols.saturating_sub(2).max(1);
        for line in super::canvas::wrap(words::BUILT_IN_DEFAULT, room, 2) {
            grey(&mut out, &mut y, line);
        }
    }
    if ctx.doc.unfit > 0 {
        grey(&mut out, &mut y, words::unfit_rules(ctx.doc.unfit));
    }
    // the first path's row right under the column titles, a blank row
    // between the others
    let mut gap = 1usize;
    if y == 1 {
        gap = 0;
        y = 2;
    }
    for p in &ctx.doc.paths {
        let name = p.name.clone().unwrap_or_default();
        y = path_row(
            ctx,
            &mut out,
            p,
            &name,
            y.saturating_add(gap),
            Row::Doc(&name),
        );
        gap = 1;
    }
    if !ctx.doc.roaming.is_empty() {
        y = roaming_row(ctx, &mut out, y.saturating_add(1));
    }
    if !ctx.doc.off.is_empty() || !ctx.doc.ignored.is_empty() {
        let none = Path {
            displays: ctx.doc.off.clone(),
            inputs: ctx.doc.ignored.clone(),
            none: true,
            ..Path::default()
        };
        y = path_row(
            ctx,
            &mut out,
            &none,
            words::NO_TERMINAL,
            y.saturating_add(1),
            Row::Nothing,
        );
    }
    // NEW PATH
    y = y.saturating_add(1);
    out.rules.push(y);
    y = y.saturating_add(1);
    out.areas.push((y, words::FLOW_NEW_PATH));
    note(
        &mut out,
        1,
        y,
        words::FLOW_NEW_PATH.to_owned(),
        NoteKind::Heading,
    );
    let kind = if ctx.new.complete(ctx.doc) {
        NoteKind::Ready
    } else {
        NoteKind::Needs
    };
    let room = ctx.cols.saturating_sub(16);
    note(
        &mut out,
        14,
        y,
        super::canvas::fit(ctx.new.status(ctx.doc), room),
        kind,
    );
    let name = match &ctx.new.name {
        _ if ctx.new.active => words::ACTIVE_TERMINAL.to_owned(),
        _ if ctx.new.none => words::NO_TERMINAL.to_owned(),
        Some(n) => n.clone(),
        None => paths::new_name(ctx.flow, ctx.doc),
    };
    y = path_row(ctx, &mut out, ctx.new, &name, y.saturating_add(1), Row::New);
    // PARTS
    y = y.saturating_add(1);
    out.rules.push(y);
    y = y.saturating_add(1);
    out.areas.push((y, words::FLOW_PIECES));
    note(
        &mut out,
        1,
        y,
        words::FLOW_PIECES.to_owned(),
        NoteKind::Heading,
    );
    let mut waiting = Vec::new();
    let mut groups = palette(ctx, &mut waiting);
    let others: Vec<String> = folded(ctx.flow)
        .into_iter()
        .filter_map(|d| ctx.flow.inputs.get(d).map(|x| x.name.clone()))
        .collect();
    let mut height = y.saturating_add(1);
    for (kind, title, pieces) in groups.drain(..) {
        y = y.saturating_add(1);
        note(&mut out, 1, y, title.to_owned(), kind);
        y = y.saturating_add(1);
        let mut boxes: Vec<Boxed> = pieces
            .into_iter()
            .map(|p| Boxed {
                obj: Obj::Pal(p.slot),
                x: 0,
                y: 0,
                w: natural(&p.title, &p.detail),
                title: p.title,
                detail: p.detail,
                mood: p.mood,
                dashed: false,
                template: p.template,
            })
            .collect();
        if kind == NoteKind::Inputs && !others.is_empty() {
            let title = words::flow_other_inputs(others.len());
            let detail = others_detail(&others);
            boxes.push(Boxed {
                obj: Obj::Others,
                x: 0,
                y: 0,
                w: natural(&title, &detail),
                title,
                detail,
                mood: Mood::Aside,
                dashed: false,
                template: false,
            });
        }
        // the boxes flow like words, the template first
        let widths: Vec<usize> = boxes.iter().map(|b| b.w).collect();
        let lines = flow_boxes(&widths, 1, ctx.cols.saturating_sub(1));
        let mut boxes = boxes.into_iter();
        for line in lines {
            for (x, w) in line {
                let Some(mut b) = boxes.next() else { break };
                b.x = x;
                b.y = y;
                b.w = w;
                height = height.max(b.bottom());
                out.boxes.push(b);
            }
            y = y.saturating_add(BOX_ROWS);
        }
    }
    out.waiting = waiting;
    out.height = height;
    out
}

/// The box an arrow key moves to from `at`: Right and Left to the box
/// after or before it in reading order, along a line and then on to the
/// next, so Right from the first box reaches every box; Home and End to
/// the first and the last box. Up and Down go to the line above or below,
/// to its box nearest in column. In CONFIGURATION, whose boxes stand in
/// three columns, they keep to the column first, so Down from a terminal
/// reaches the next terminal.
pub fn step(layout: &Layout, at: &Obj, key: Key) -> Option<Obj> {
    let order = layout.reading();
    let here = layout.find(at)?;
    let pick = |b: Option<&&Boxed>| b.map(|b| b.obj.clone());
    let overlaps = |b: &Boxed| b.x < here.right() && here.x < b.right();
    let centre = |b: &Boxed| b.x.saturating_add(b.w / 2);
    let at_order = order.iter().position(|b| b.obj == *at).unwrap_or(0);
    match key {
        Key::Right => pick(order.get(at_order.saturating_add(1))),
        Key::Left => pick(at_order.checked_sub(1).and_then(|i| order.get(i))),
        Key::Down | Key::Up => {
            let down = key == Key::Down;
            let candidates: Vec<&&Boxed> = order
                .iter()
                .filter(|b| if down { b.y > here.y } else { b.y < here.y })
                .collect();
            let config = |b: &Boxed| matches!(b.obj, Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_));
            if config(here)
                && let Some(b) = candidates
                    .iter()
                    .filter(|b| config(b) && overlaps(b))
                    .min_by_key(|b| (b.y.abs_diff(here.y), centre(b).abs_diff(centre(here))))
            {
                return Some(b.obj.clone());
            }
            let line = candidates.iter().map(|b| b.y.abs_diff(here.y)).min();
            pick(
                candidates
                    .into_iter()
                    .filter(|b| Some(b.y.abs_diff(here.y)) == line)
                    .min_by_key(|b| centre(b).abs_diff(centre(here))),
            )
        }
        Key::Home => pick(order.first()),
        Key::End => pick(order.last()),
        _ => None,
    }
}

/// Where the highlight is, which canvas row is at the top of the window,
/// what is open over the document, and the path in the editor.
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
    /// The object whose second layer is open.
    pub layer: Option<Obj>,
    /// On a layer: the line under the cursor, and on a card, the choice
    /// under it on that line.
    pub line: usize,
    pub pick: usize,
    /// The path in the editor.
    pub new: Path,
    /// A value being typed on a card's line, the name or the login,
    /// while it is.
    pub typing: Option<String>,
    /// The box to highlight once a change in flight is written and the
    /// box exists: an added path's terminal.
    pub want: Option<Obj>,
    /// What NEW PATH holds for revising, which CONFIGURATION leaves out
    /// while the draft still has it.
    pub revising: Option<Revising>,
    /// The parts PARTS shows, kept while the view is open: a part whose
    /// device left stays, grey, until the view closes.
    pub parts: Vec<Part>,
    /// A filter's picker, open over a template's layer.
    pub picker: Option<Picker>,
}

/// A filter's picker: the stage and the match key it lists the values of,
/// and whether the template's layer was opened from NEW PATH, so the new
/// part also goes into the new path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picker {
    pub stage: Stage,
    pub key: &'static str,
    pub from_new: bool,
}

/// What is being revised in NEW PATH.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Revising {
    /// The path of the terminal of this name.
    Path(String),
    /// The path of the active terminal: the roaming inputs.
    Active,
    /// The path to no terminal.
    Nothing,
}

impl State {
    /// Puts the highlight back on its object in `layout`, or on the box
    /// nearest where it was when the object is gone, and scrolls so the
    /// highlighted box is at the same height in a window `window` rows
    /// tall, or as near it as keeps the box in sight.
    pub fn settle(&mut self, layout: &Layout, window: usize) {
        if let Some(w) = self.want.take() {
            if layout.find(&w).is_some() {
                self.at = Some(w);
            } else {
                self.want = Some(w);
            }
        }
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
        // on the first layout the window starts at the canvas's top, so
        // the headings above the first box show
        if self.at.is_none() {
            self.offset = isize::try_from(b.y).unwrap_or(0);
        }
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

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use super::super::paths::tests::{EDID_A, EDID_B, KBD_PATH, flow};
    use super::*;

    pub(crate) fn lay(f: &Flow, new: &Path, cols: usize) -> Layout {
        let doc = paths::read(f);
        layout(&Ctx {
            flow: f,
            doc: &doc,
            new,
            parts: &super::super::parts::enumerate(f, &doc),
            cols,
            default: paths::is_default(f),
        })
    }

    // the round-6 renders cut "Whichever terminal is a…": a box in a path
    // shows its whole title at the minimum width and at 120 columns
    #[test]
    fn a_path_s_terminal_box_never_cuts_its_title() {
        let f = flow();
        for cols in [80, 120] {
            let layout = lay(&f, &Path::default(), cols);
            let roam = layout
                .boxes
                .iter()
                .find(|b| matches!(b.obj, Obj::Roam(Slot::Terminal)))
                .expect("the roaming path's terminal");
            assert_eq!(roam.title, words::ACTIVE_TERMINAL);
            let text = roam.w.saturating_sub(4);
            assert!(
                super::super::canvas::width(&roam.title) <= text,
                "{cols}: {} in {}",
                roam.title,
                roam.w
            );
        }
    }

    fn at<'a>(l: &'a Layout, o: &Obj) -> &'a Boxed {
        l.find(o).expect("a box for the object")
    }

    fn doc(name: &str, slot: Slot) -> Obj {
        Obj::Doc(name.to_owned(), slot)
    }

    #[test]
    fn the_paths_the_editor_and_the_pieces_stack_in_that_order() {
        let f = flow();
        for cols in [80, 100, 120] {
            let l = lay(&f, &Path::default(), cols);
            let g = geometry(cols);
            let desk = at(&l, &doc("desk", Slot::Terminal));
            let panel = at(&l, &doc("panel", Slot::Terminal));
            assert_eq!((desk.x, panel.x), (g.term_x, g.term_x));
            assert_eq!(desk.detail, "font set monoid · every VT");
            assert_eq!(panel.detail, "default font set · every VT");
            let dp = at(&l, &doc("desk", Slot::Display(EDID_A.into())));
            assert_eq!(
                (dp.x, dp.y, dp.title.as_str()),
                (g.screen_x, desk.y, "DP-8")
            );
            // the roaming receiver once, in ROAMING below the paths, in the
            // inputs' column, and on no path's row
            let roams = at(&l, &Obj::Roam(Slot::Input("id:046d:c31c".into())));
            assert_eq!(roams.title, "Logitech USB Receiver (by id)");
            assert_eq!((roams.x, roams.w), (g.dev_x, g.dev_w));
            assert!(roams.y > panel.bottom(), "{roams:?}");
            assert!(
                l.find(&doc("desk", Slot::Input("id:046d:c31c".into())))
                    .is_none()
            );
            assert!(
                l.notes
                    .iter()
                    .any(|n| n.text == "ROAMING" && n.y == roams.title_row())
            );
            // each path's row ends in a dotted stub from the words
            // `roaming inputs` to its terminal
            for t in [desk, panel] {
                let stub = l
                    .wires
                    .iter()
                    .find(|w| w.stub && w.to == t.obj)
                    .expect("a stub");
                assert!(stub.dashed);
                let (x, y) = stub.points[0];
                assert_eq!(x + 1, g.dev_x);
                assert!(y < roams.y, "{stub:?}");
                let words = l
                    .notes
                    .iter()
                    .find(|n| n.kind == NoteKind::Stub && n.y == y)
                    .expect("its words");
                assert_eq!((words.x, words.text.as_str()), (g.dev_x, "roaming inputs"));
                assert_eq!(
                    *stub.points.last().expect("an end"),
                    (t.right(), t.title_row())
                );
            }
            let gone = at(&l, &doc("desk", Slot::Input("serial:046d:c52b:0".into())));
            assert_eq!(gone.detail, "Not connected");
            // the catch-all to ignore, as the path to no terminal, below
            // the roaming inputs, grey, with no roaming stub
            let none = at(&l, &Obj::Nothing(Slot::Terminal));
            assert_eq!((none.title.as_str(), none.mood), ("Nowhere", Mood::Aside));
            assert!(none.y > roams.y);
            let every = at(&l, &Obj::Nothing(Slot::Input("*".into())));
            assert_eq!(every.title, "All input devices");
            assert!(
                l.wires
                    .iter()
                    .any(|w| w.from == every.obj && w.to == none.obj && !w.dashed)
            );
            assert!(!l.wires.iter().any(|w| w.stub && w.to == none.obj));
            // the editor below the paths: each stage an empty place drawn as
            // its template, the terminal's saying the name it will take
            let new = at(&l, &Obj::New(Slot::Empty(Stage::Terminal)));
            assert!(new.template && !new.dashed && !desk.template);
            assert_eq!(
                (new.title.as_str(), new.detail.as_str()),
                ("Terminal", "new: terminal-1")
            );
            for st in [Stage::Display, Stage::Input] {
                let e = at(&l, &Obj::New(Slot::Empty(st)));
                assert_eq!(
                    (e.y, e.template, e.title.as_str()),
                    (new.y, true, st.name())
                );
            }
            assert!(new.y > none.bottom() && new.y > l.rules[0]);
            // the pieces below the editor, a display in a path grey and one
            // nobody has amber
            assert!(l.rules[1] > new.bottom());
            let dp = at(&l, &Obj::Pal(Slot::Display(EDID_A.into())));
            assert_eq!(
                (dp.mood, dp.detail.as_str()),
                (Mood::Aside, "in the terminal desk")
            );
            let kbd = at(&l, &Obj::Pal(Slot::Input(KBD_PATH.into())));
            assert_eq!(kbd.mood, Mood::Placed);
            assert_eq!(kbd.detail, "USB 3.3 · roaming");
            let bitdo = at(&l, &Obj::Pal(Slot::Input("id:2dc8:5200".into())));
            assert_eq!(bitdo.mood, Mood::Waiting);
            assert!(dp.y > l.rules[1]);
            // each group opens with its template, drawn double
            for st in Stage::ALL {
                let t = at(&l, &Obj::Pal(Slot::Template(st)));
                assert!(t.template && t.x == 1, "{t:?}");
                assert_eq!(t.title, st.name());
            }
            // the parts the draft uses, and no standing "every" part
            for o in [
                Obj::Pal(Slot::Input("id:046d:c31c".into())),
                Obj::Pal(Slot::Input("serial:046d:c52b:0".into())),
                Obj::Pal(Slot::Input("*".into())),
                Obj::Pal(Slot::Special(Piece::Active)),
                Obj::Pal(Slot::Special(Piece::NoTerminal)),
                Obj::Pal(Slot::Font(None)),
                Obj::Pal(Slot::Font(Some("monoid".into()))),
                Obj::Others,
            ] {
                at(&l, &o);
            }
            for o in [
                Obj::Pal(Slot::Display("*".into())),
                Obj::Pal(Slot::Input("class:keyboard".into())),
                Obj::Pal(Slot::Input(ANY_POINTER.into())),
            ] {
                assert!(l.find(&o).is_none(), "{o:?}");
            }
            let unplugged = at(&l, &Obj::Pal(Slot::Input("serial:046d:c52b:0".into())));
            assert_eq!(
                (unplugged.mood, unplugged.detail.as_str()),
                (Mood::Aside, "Not connected")
            );
            assert_eq!(at(&l, &Obj::Others).title, "1 other input");
            assert_eq!(l.waiting, [Sort::Keyboard]);
            let headings: Vec<&str> = l
                .notes
                .iter()
                .filter(|n| n.kind == NoteKind::Heading)
                .map(|n| n.text.as_str())
                .collect();
            assert_eq!(headings, ["CONFIGURATION", "NEW PATH", "PARTS"]);
            // each area's heading above its area, on the canvas
            assert_eq!(l.area_at(0), "CONFIGURATION");
            let (new_at, _) = l.areas[0];
            let (parts_at, _) = l.areas[1];
            assert!(new_at < new.y && new_at > l.rules[0]);
            assert!(parts_at < dp.y && parts_at > l.rules[1]);
            assert_eq!(l.area_at(new_at), "CONFIGURATION");
            assert_eq!(l.area_at(new_at + 1), "NEW PATH");
            assert_eq!(l.area_at(parts_at + 1), "PARTS");
        }
    }

    #[test]
    fn a_picked_piece_is_marked_in_the_palette_and_drawn_in_the_editor() {
        let f = flow();
        let mut new = Path::default();
        new.pick(Piece::Input("id:2dc8:5200".into()));
        new.pick(Piece::Font(Some("dejavu".into())));
        let l = lay(&f, &new, 100);
        let p = at(&l, &Obj::Pal(Slot::Input("id:2dc8:5200".into())));
        assert_eq!(p.mood, Mood::Picked);
        assert_eq!(p.title, "✓ 8BitDo Keyboard");
        let e = at(&l, &Obj::New(Slot::Input("id:2dc8:5200".into())));
        assert!(e.dashed);
        assert_eq!(e.title, "8BitDo Keyboard (by id)");
        assert_eq!(
            at(&l, &Obj::New(Slot::Empty(Stage::Terminal))).detail,
            "new: terminal-1"
        );
        assert_eq!(at(&l, &Obj::Pal(Slot::Font(None))).mood, Mood::Placed);
        let status = l
            .notes
            .iter()
            .find(|n| n.kind == NoteKind::Needs)
            .expect("words");
        assert_eq!(status.text, words::PATH_NEEDS_DISPLAY);
    }

    #[test]
    fn no_box_overlaps_another_or_the_screen_s_edge() {
        let mut f = flow();
        for i in 0..6 {
            f.inputs.push(Device {
                name: format!("Spare keyboard number {i}"),
                detail: format!("pci-0-usb-0:{i}"),
                keys: vec![format!("path:pci-0-usb-0:{i}"), "class:keyboard".into()],
                found: true,
                rule: None,
            });
        }
        let mut new = Path::default();
        new.pick(Piece::Input(KBD_PATH.into()));
        new.pick(Piece::Input("id:2dc8:5200".into()));
        new.pick(Piece::Display("*".into()));
        for cols in [80, 100, 120] {
            let l = lay(&f, &new, cols);
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
        // a second display on desk, whose line merges into the first's
        f.displays.push(Device {
            name: "DP-9".into(),
            detail: "LARGE00002".into(),
            keys: vec!["edid:AAA:0x0001:LARGE00002".into()],
            found: true,
            rule: None,
        });
        f.rules[0].selector = EDID_A.into();
        let mut new = Path::default();
        new.pick(Piece::Input(KBD_PATH.into()));
        new.pick(Piece::Input("id:2dc8:5200".into()));
        new.pick(Piece::Display("edid:AAA:0x0001:LARGE00002".into()));
        new.pick(Piece::Display("*".into()));
        for cols in [80, 100, 120] {
            let l = lay(&f, &new, cols);
            // desk 1+1, panel 1+0, the active terminal 0+1, no terminal
            // 0+1, the editor 2+2, and a roaming stub on each path of the
            // configuration
            assert_eq!(l.wires.len(), 11, "{cols}");
            assert_eq!(l.wires.iter().filter(|w| w.dashed).count(), 7);
            assert_eq!(l.wires.iter().filter(|w| w.stub).count(), 2);
            for w in l.wires.iter().filter(|w| !w.stub) {
                let from = at(&l, &w.from);
                let to = at(&l, &w.to);
                let first = w.points[0];
                let last = *w.points.last().expect("a point");
                assert_eq!(first.1, from.title_row(), "{cols}: {w:?}");
                assert_eq!(last.1, to.title_row(), "{cols}: {w:?}");
                if from.x < to.x {
                    assert_eq!(first.0, from.right(), "{cols}: {w:?} leaves a gap");
                    assert_eq!(last.0 + 1, to.x, "{cols}: {w:?} stops short");
                } else {
                    assert_eq!(first.0 + 1, from.x, "{cols}: {w:?} leaves a gap");
                    assert_eq!(last.0, to.right(), "{cols}: {w:?} stops short");
                }
                for pair in w.points.windows(2) {
                    assert!(pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1, "{w:?}");
                }
            }
        }
    }

    #[test]
    fn the_arrows_move_across_and_down() {
        let f = flow();
        let l = lay(&f, &Path::default(), 120);
        let desk = doc("desk", Slot::Terminal);
        assert_eq!(
            step(&l, &desk, Key::Left),
            Some(doc("desk", Slot::Display(EDID_A.into())))
        );
        assert_eq!(
            step(&l, &desk, Key::Right),
            Some(doc("desk", Slot::Input("serial:046d:c52b:0".into())))
        );
        assert_eq!(
            step(&l, &desk, Key::Down),
            Some(doc("panel", Slot::Terminal))
        );
        // below the paths, the active terminal's path, the path to no
        // terminal, then NEW PATH's empty terminal place
        assert_eq!(
            step(&l, &doc("panel", Slot::Terminal), Key::Down),
            Some(Obj::Roam(Slot::Terminal))
        );
        assert_eq!(
            step(&l, &Obj::Roam(Slot::Terminal), Key::Down),
            Some(Obj::Nothing(Slot::Terminal))
        );
        assert_eq!(
            step(&l, &Obj::Nothing(Slot::Terminal), Key::Down),
            Some(Obj::New(Slot::Empty(Stage::Terminal)))
        );
        assert_eq!(
            step(&l, &desk, Key::Home),
            Some(doc("desk", Slot::Display(EDID_A.into())))
        );
        let _ = EDID_B;
    }

    #[test]
    fn right_from_the_first_box_reaches_every_box_in_reading_order() {
        let f = flow();
        let mut new = Path::default();
        new.pick(Piece::Input(KBD_PATH.into()));
        for cols in [80, 100, 120] {
            for editor in [Path::default(), new.clone()] {
                let l = lay(&f, &editor, cols);
                let mut at_now = step(&l, &l.reading()[0].obj, Key::Home).expect("a first box");
                let mut seen = vec![at_now.clone()];
                while let Some(next) = step(&l, &at_now, Key::Right) {
                    assert!(!seen.contains(&next), "{cols}: Right came back to {next:?}");
                    seen.push(next.clone());
                    at_now = next;
                }
                assert_eq!(seen.len(), l.boxes.len(), "{cols}: Right skipped a box");
            }
        }
    }

    #[test]
    fn the_highlighted_box_stays_in_sight_as_it_moves() {
        let f = flow();
        let l = lay(&f, &Path::default(), 80);
        let window = 8;
        let mut s = State::default();
        s.settle(&l, window);
        assert_eq!(s.top, 0);
        let mut at_now = s.at.clone().expect("a highlight");
        for _ in 0..12 {
            let Some(next) = step(&l, &at_now, Key::Down) else {
                break;
            };
            let before = s.top;
            s.go(&l, next.clone(), window);
            let b = at(&l, &next);
            assert!(b.y >= s.top && b.bottom() <= s.top + window, "{s:?} {b:?}");
            // the window moves only as far as it must
            if b.bottom() > before + window {
                assert_eq!(s.top, b.bottom() - window);
            }
            at_now = next;
        }
        assert!(matches!(at_now, Obj::Pal(_) | Obj::Others), "{at_now:?}");
        let before = s.top;
        s.page(&l, false, window);
        assert!(s.top < before, "{s:?}");
        let b = at(&l, s.at.as_ref().expect("a highlight"));
        assert!(b.y >= s.top && b.bottom() <= s.top + window, "{s:?} {b:?}");
    }

    #[test]
    fn a_new_layout_keeps_the_highlight_where_it_was_on_the_page() {
        let f = flow();
        let window = 9;
        let mut new = Path::default();
        let l = lay(&f, &new, 80);
        let mut s = State::default();
        let bitdo = Obj::Pal(Slot::Input("id:2dc8:5200".into()));
        s.go(&l, bitdo.clone(), window);
        let before = at(&l, &bitdo).y - s.top;
        // picking two pieces makes the editor's row taller, and the palette
        // moves down the canvas; the box stays at its height in the window
        new.pick(Piece::Input(KBD_PATH.into()));
        new.pick(Piece::Input("id:2dc8:5200".into()));
        let l2 = lay(&f, &new, 80);
        assert!(at(&l2, &bitdo).y > at(&l, &bitdo).y);
        s.settle(&l2, window);
        assert_eq!(s.at, Some(bitdo.clone()));
        assert_eq!(at(&l2, &bitdo).y - s.top, before);
    }

    #[test]
    fn a_port_reads_as_a_reader_says_it() {
        assert_eq!(port_words("pci-0000:15:00.0-usb-0:3.3:1.0"), "USB 3.3");
        assert_eq!(port_words("usb-0:3.3"), "USB 3.3");
        assert_eq!(port_words("platform-i8042-serio-0"), "built in");
    }
    #[test]
    fn boxes_flow_like_words_whole_and_stretch_when_a_line_holds_two() {
        // a line breaks before the box that does not fit, and keeps the
        // boxes' own widths while one line holds three
        assert_eq!(
            flow_boxes(&[10, 10, 10, 30], 1, 40),
            [vec![(1, 10), (13, 10), (25, 10)], vec![(1, 30)]]
        );
        assert_eq!(
            flow_boxes(&[10, 10, 10, 10], 1, 50),
            [vec![(1, 10), (13, 10), (25, 10), (37, 10)]]
        );
        // where no line holds more than two, the boxes share each line
        assert_eq!(
            flow_boxes(&[10, 10, 10], 1, 25),
            [vec![(1, 11), (14, 11)], vec![(1, 24)]]
        );
        // a box wider than the line takes the line, whole
        assert_eq!(flow_boxes(&[50, 8], 1, 25), [vec![(1, 24)], vec![(1, 24)]]);
        assert_eq!(flow_boxes(&[], 1, 25), Vec::<Vec<(usize, usize)>>::new());
    }

    /// The flow with six spare keyboards, so PARTS' inputs wrap.
    fn spares() -> Flow {
        let mut f = flow();
        for i in 0..6 {
            f.inputs.push(Device {
                name: format!("Spare keyboard {i}"),
                detail: format!("pci-0-usb-0:{i}"),
                keys: vec![format!("path:pci-0-usb-0:{i}"), "class:keyboard".into()],
                found: true,
                rule: None,
            });
        }
        f
    }

    /// PARTS' boxes of `stage`'s group, line by line, each line left to
    /// right.
    fn group_lines(l: &Layout, stage: Stage) -> Vec<Vec<Boxed>> {
        let mut lines: Vec<Vec<Boxed>> = Vec::new();
        for b in l.reading() {
            let of = match &b.obj {
                Obj::Pal(Slot::Display(_)) => Stage::Display,
                Obj::Pal(Slot::Input(_)) | Obj::Others => Stage::Input,
                Obj::Pal(Slot::Font(_)) => Stage::Font,
                Obj::Pal(Slot::Template(s)) => *s,
                Obj::Pal(Slot::Special(p)) => Stage::of(p),
                _ => continue,
            };
            if of != stage {
                continue;
            }
            match lines.last_mut() {
                Some(line) if line[0].y == b.y => line.push(b.clone()),
                _ => lines.push(vec![b.clone()]),
            }
        }
        lines
    }

    #[test]
    fn parts_reflow_to_every_width_with_each_box_whole() {
        let f = spares();
        for cols in [60, 80, 100, 120, 160] {
            let l = lay(&f, &Path::default(), cols);
            let right = cols - 1;
            let mut stretched = 0;
            for stage in Stage::ALL {
                let lines = group_lines(&l, stage);
                assert!(!lines.is_empty(), "{cols}: {stage:?}");
                // the template comes first
                assert_eq!(lines[0][0].obj, Obj::Pal(Slot::Template(stage)));
                let narrow = lines.iter().all(|line| line.len() <= 2);
                for (n, line) in lines.iter().enumerate() {
                    for pair in line.windows(2) {
                        assert_eq!(pair[0].right() + GAP, pair[1].x, "{cols}: {line:?}");
                    }
                    let first = &line[0];
                    let last = line.last().expect("a box");
                    assert_eq!(first.x, 1, "{cols}: a line starts at the margin");
                    assert!(last.right() <= right, "{cols}: {last:?} runs off");
                    if narrow && lines.len() > 1 {
                        assert_eq!(last.right(), right, "{cols}: {line:?} does not stretch");
                        stretched += 1;
                    } else {
                        for b in line {
                            assert_eq!(b.w, natural(&b.title, &b.detail).min(right - 1));
                        }
                    }
                    // the next line starts with the box that did not fit here
                    if let Some(next) = lines.get(n + 1) {
                        assert_eq!(next[0].y, first.y + BOX_ROWS);
                        let wanted = natural(&next[0].title, &next[0].detail);
                        assert!(last.right() + GAP + wanted > right, "{cols}: {next:?} fits");
                    }
                }
            }
            if cols == 60 {
                assert!(stretched > 0, "at 60 columns some lines stretch");
            }
        }
    }

    #[test]
    fn the_arrows_follow_wrapped_lines_in_reading_order_and_by_column() {
        let f = spares();
        for cols in [60, 80] {
            let l = lay(&f, &Path::default(), cols);
            let lines = group_lines(&l, Stage::Input);
            assert!(lines.len() >= 3, "{cols}: {lines:?}");
            for pair in lines.windows(2) {
                let (this, next) = (&pair[0], &pair[1]);
                let last = this.last().expect("a box");
                assert_eq!(step(&l, &last.obj, Key::Right), Some(next[0].obj.clone()));
                assert_eq!(step(&l, &next[0].obj, Key::Left), Some(last.obj.clone()));
                let centre = |b: &Boxed| b.x + b.w / 2;
                for b in this {
                    let want = next
                        .iter()
                        .min_by_key(|c| centre(c).abs_diff(centre(b)))
                        .expect("a box below");
                    assert_eq!(
                        step(&l, &b.obj, Key::Down),
                        Some(want.obj.clone()),
                        "{cols}"
                    );
                }
                for b in next {
                    let want = this
                        .iter()
                        .min_by_key(|c| centre(c).abs_diff(centre(b)))
                        .expect("a box above");
                    assert_eq!(step(&l, &b.obj, Key::Up), Some(want.obj.clone()), "{cols}");
                }
            }
        }
    }

    #[test]
    fn each_section_s_title_scrolls_with_its_section() {
        let f = flow();
        let l = lay(&f, &Path::default(), 80);
        let note = |text: &str| {
            l.notes
                .iter()
                .find(|n| n.text == text)
                .expect("the title")
                .y
        };
        // CONFIGURATION and its column titles sit at the canvas's top
        assert_eq!(note("CONFIGURATION"), 0);
        assert_eq!(note(words::FLOW_DISPLAYS), 1);
        assert_eq!(note(words::FLOW_TERMINALS), 1);
        let first = l.reading()[0].y;
        assert!(first > 1, "the first box is under the titles");
        assert!(note("NEW PATH") > first && note("PARTS") > note("NEW PATH"));
    }
}
