//! The Flow page laid out as plain data: boxes, the lines between them,
//! and lines of text, on one document as tall as it needs, at the width of
//! the viewer. The document holds three sections from the top:
//!
//! - CONFIGURATION: each path as a row of boxes under three column titles,
//!   its displays, its terminal and its inputs, joined by lines from
//!   border to border; then ROAMING, the inputs that type into whichever
//!   terminal is active, and the path to Nowhere; then a line that counts
//!   the connected devices in no path yet.
//! - NEW PATH: the path being built, in the same shape, with dashed boxes
//!   and an empty place for each stage it has no part for yet.
//! - PARTS: the parts to pick, in four groups, each led by its template.
//!   Boxes flow like words, and a box that does not fit on a line starts
//!   the next one.
//!
//! [`layout`] lays the document out, [`super::draw`] draws it, and both
//! the tab's update and its draw call [`layout`], so a key and a click
//! land on the boxes the page shows.
//!
//! References:
//! - ADR-1403 D7, "One document with three sections", "A path", "Boxes
//!   that reflow" and "Colors, borders and lines".
//! - ADR-1206 D4, "Layout is a pure function": update and view both call
//!   the tab's layout function.

use vtium_config_core::flow::{Flow, Kind};
use vtium_config_widgets::canvas::{fit, width, wrap};

use crate::inputs::{self, Sort};
use crate::parts::{Part, Stage};
use crate::paths::{self, Doc, Own, Path, Piece};
use crate::words;

/// The rows a box takes: its top border, its title, its second line and
/// its bottom border.
pub const BOX_ROWS: usize = 4;

/// The blank columns between two boxes of a group of parts.
const GAP: usize = 2;

/// The widest a box grows to fit its text, borders and margins included,
/// before a line stretches it: 30 characters of text.
const BOX_MAX: usize = 34;

/// The fewest cells a terminal's box in a path takes, so that the start
/// of its second line shows.
const TERM_MIN_W: usize = 32;

/// The column of NEW PATH's status, after its heading.
const STATUS_X: usize = 14;

/// The column of the policy, after CONFIGURATION's heading.
const POLICY_X: usize = 17;

/// The width a box takes for its text: the longer of its two lines, with
/// a border and a blank on each side, up to [`BOX_MAX`].
fn natural(title: &str, detail: &str) -> usize {
    width(title)
        .max(width(detail))
        .saturating_add(4)
        .min(BOX_MAX)
}

/// Lays boxes `widths` wide out like words, from column `left` up to the
/// column `right`, which no box reaches: each box whole on a line, `GAP`
/// columns apart, and a box that does not fit on a line starting the next
/// one. A box wider than a line takes the line. Where no line holds more
/// than two boxes and the boxes take more than one line, the viewer is
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

/// Where a box is: in a path of the configuration, by the path's terminal;
/// in NEW PATH; or in PARTS.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Obj {
    Doc(String, Slot),
    New(Slot),
    Pal(Slot),
    /// The path of the active terminal, the roaming inputs': its terminal
    /// or one of its inputs.
    Roam(Slot),
    /// The path to Nowhere: the displays that stay off and the inputs set
    /// aside, and its terminal's box.
    Nothing(Slot),
    /// The box that folds every input that is neither a keyboard nor a
    /// pointer.
    Others,
}

/// What a box holds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Display(String),
    Input(String),
    /// A path's terminal box.
    Terminal,
    /// A look in PARTS, by its name; `None` for the default look.
    Look(Option<String>),
    /// A part that matches no device: No input, Active terminal, Nowhere,
    /// or a new terminal by its name.
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
            Piece::Look(f) => Slot::Look(f.clone()),
            p => Slot::Special(p.clone()),
        }
    }

    /// The stage the slot belongs to, whose kind colors its title.
    pub fn stage(&self) -> Stage {
        match self {
            Slot::Display(_) => Stage::Display,
            Slot::Terminal => Stage::Terminal,
            Slot::Look(_) => Stage::Look,
            Slot::Input(_) => Stage::Input,
            Slot::Special(p) => Stage::of(p),
            Slot::Template(st) | Slot::Empty(st) => *st,
        }
    }
}

/// What state a box is in, which its border shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// In a path, or a part anyone may pick.
    Placed,
    /// A device connected that no path holds.
    Waiting,
    /// What cannot be picked.
    Aside,
    /// Picked into NEW PATH.
    Picked,
}

/// A box on the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boxed {
    pub obj: Obj,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub title: String,
    pub detail: String,
    pub mood: Mood,
    /// Drawn with a dashed border: a box in NEW PATH, not yet added, and
    /// the active terminal, which stands for a terminal that changes.
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

/// A line from a part to its terminal: the corners of its path, from the
/// cell beside the part's border to the cell beside the terminal's, where
/// its arrow is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: Obj,
    pub to: Obj,
    pub points: Vec<(usize, usize)>,
    /// A dotted line: in NEW PATH, a roaming input's, or the one that says
    /// the roaming inputs reach a terminal.
    pub dashed: bool,
    /// The dotted line from the words `roaming inputs` to a terminal,
    /// which has no box at its start: it runs from the terminal to itself.
    pub stub: bool,
}

/// What a line of text on the document is, which its role follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    /// A section's heading: CONFIGURATION, NEW PATH, PARTS.
    Heading,
    /// NEW PATH's status while the path is not complete, and the count of
    /// devices in no path yet.
    Needs,
    /// NEW PATH's status once the path is complete.
    Ready,
    /// Words of less weight.
    Grey,
    /// A column's or a group's title, in its kind's role.
    Title(Stage),
    /// The words at the start of a roaming line.
    Stub,
}

/// A line of text on the document.
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

/// The columns at a viewer `cols` wide: the boxes and the gaps between
/// them share the width as the figure at 120 columns does, 25, 11, 28, 14
/// and 40 of its 118, with a margin of one column at each side. A display
/// box holds All displays whole, and a terminal box Active terminal.
pub fn geometry(cols: usize) -> Geometry {
    let room = cols.saturating_sub(2);
    let part = |n: usize| room.saturating_mul(n) / 118;
    let title = |t: &str| width(t).saturating_add(4);
    let screen_w = part(25).max(title(words::ANY_DISPLAY));
    let gap1 = part(11).max(5);
    let term_w = part(28).max(title(words::ACTIVE_TERMINAL)).max(TERM_MIN_W);
    let gap2 = part(14).max(7);
    let screen_x = 1;
    let term_x = screen_x + screen_w + gap1;
    let dev_x = term_x + term_w + gap2;
    let dev_w = room.saturating_add(1).saturating_sub(dev_x).max(1);
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

/// The look a terminal draws with, on its second line and on a look's box.
pub fn look_words(look: Option<&str>) -> String {
    look.map_or_else(|| words::DEFAULT_LOOK_PART.to_owned(), words::look_words)
}

/// What the page lays out: the draft's flow, its configuration as paths,
/// NEW PATH, the parts PARTS shows, and the viewer's width.
pub struct Ctx<'a> {
    pub flow: &'a Flow,
    pub doc: &'a Doc,
    pub new: &'a Path,
    pub parts: &'a [Part],
    /// The match each connected device is picked by.
    pub own: &'a Own,
    pub cols: usize,
    /// The configuration is vtium's built-in default.
    pub default: bool,
    /// vtium-config follows devices as they come and go.
    pub hotplug: bool,
}

/// The page laid out as the document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub boxes: Vec<Boxed>,
    pub wires: Vec<Wire>,
    pub notes: Vec<Note>,
    /// The document's rows of the rules between the sections.
    pub rules: Vec<usize>,
    /// The document's height: the row after its last box.
    pub height: usize,
    pub geometry: Option<Geometry>,
    /// The devices connected that no path holds, by sort.
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

    /// The box whose cells hold the document's cell `x`, `y`.
    pub fn at(&self, x: usize, y: usize) -> Option<&Boxed> {
        self.boxes
            .iter()
            .find(|b| x >= b.x && x < b.right() && y >= b.y && y < b.bottom())
    }
}

/// The second line of an empty place in a column `w` cells wide: its
/// words, or its key alone where the box cannot hold them, so the key's
/// brackets stay whole.
fn empty_place(w: usize) -> String {
    let words = if natural("", words::EMPTY_PLACE) <= w {
        words::EMPTY_PLACE
    } else {
        words::EMPTY_PLACE_SHORT
    };
    words.to_owned()
}

/// Which row [`path_row`] lays out: a path of the configuration, by its
/// terminal's name; NEW PATH's; or the path to Nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row<'a> {
    Doc(&'a str),
    New,
    Nothing,
}

/// A box of a path's row: in NEW PATH dashed, and a template's double
/// border on an empty place.
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

/// A box's two lines for the part of `kind` that matches by `sel`: when
/// the person gave the part a label, the label as the title and the name
/// the system gives it under it; else `words` as they are.
fn labelled(ctx: &Ctx<'_>, kind: Kind, sel: &str, words: (String, String)) -> (String, String) {
    match ctx.doc.label(kind, sel) {
        Some(l) => (l.to_owned(), words.0),
        None => words,
    }
}

/// Whether vtium isolates the terminal of the path `p`, so no roaming
/// input reaches it.
fn isolated(flow: &Flow, p: &Path) -> bool {
    p.name
        .as_deref()
        .and_then(|name| flow.heads.iter().find(|h| h.name == name))
        .is_some_and(|h| h.isolated)
}

/// The words of a path's terminal box.
fn terminal_words(ctx: &Ctx<'_>, p: &Path, name: &str, empty: bool) -> (String, String) {
    if empty {
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
        // a look line that names a look the draft does not hold is marked
        // missing
        let look = match p.look.as_deref() {
            Some(l) if !ctx.flow.looks.iter().any(|x| x.name == l) => {
                words::look_words(&words::missing_look(l))
            }
            l => look_words(l),
        };
        (
            name.to_owned(),
            format!("{look} · {}", words::vt_words(&p.vts())),
        )
    }
}

/// One side of a path's row, its boxes line by line: in NEW PATH they flow
/// like words across the side's column, and elsewhere each box takes a
/// line of its own across the whole column.
type Side = Vec<Vec<(Boxed, bool)>>;

/// The boxes of one side of a path's row from the document's row `y`.
fn side(
    boxes: Vec<((String, String), Obj, Mood, bool)>,
    x: usize,
    w: usize,
    y: usize,
    editor: bool,
) -> Side {
    let widths: Vec<usize> = boxes.iter().map(|((t, d), ..)| natural(t, d)).collect();
    let lines = if editor {
        flow_boxes(&widths, x, x.saturating_add(w))
    } else {
        widths.iter().map(|_| vec![(x, w)]).collect()
    };
    let mut boxes = boxes.into_iter();
    let mut out: Side = Vec::new();
    for (n, line) in lines.into_iter().enumerate() {
        let row = y.saturating_add(n.saturating_mul(BOX_ROWS));
        let mut placed = Vec::new();
        for (bx, bw) in line {
            let Some((words, obj, mood, wired)) = boxes.next() else {
                break;
            };
            placed.push((part_box(obj, bx, row, bw, words, mood), wired));
        }
        out.push(placed);
    }
    out
}

/// Lays out the row of the path `p`, its terminal named `name`, from the
/// document's row `y`. A path of the configuration that roaming inputs
/// reach ends in a dotted line from the words `roaming inputs` to its
/// terminal. In NEW PATH, each stage the path has no part for yet shows
/// its empty place. A path with No input shows it in its inputs' place,
/// with no line, since nothing types into its terminal. Returns the row
/// after it.
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
    let mood = if p.none { Mood::Aside } else { Mood::Placed };
    let term_box = part_box(
        term.clone(),
        g.term_x,
        y,
        g.term_w,
        terminal_words(ctx, p, name, empty_terminal),
        mood,
    );
    let join_row = term_box.title_row();
    out.boxes.push(term_box);
    let mut displays: Vec<((String, String), Obj, Mood, bool)> = p
        .displays
        .iter()
        .map(|s| {
            let words = labelled(
                ctx,
                Kind::Display,
                s,
                paths::title(ctx.flow, Kind::Display, s),
            );
            (words, obj(Slot::Display(s.clone())), mood, true)
        })
        .collect();
    if editor && displays.is_empty() && !p.active {
        let words = (Stage::Display.name().to_owned(), empty_place(g.screen_w));
        displays.push((words, obj(Slot::Empty(Stage::Display)), mood, false));
    }
    let display_lines = side(displays, g.screen_x, g.screen_w, y, editor);
    for (n, line) in display_lines.iter().enumerate() {
        // the box nearest the terminal, the line's last, takes the line to
        // it, and each box before it joins the box after it
        for (k, (b, wired)) in line.iter().enumerate() {
            if !wired {
                continue;
            }
            let start = (b.right(), b.title_row());
            let next = line.get(k.saturating_add(1));
            let points = match next {
                Some((next, _)) => vec![start, (next.x.saturating_sub(1), b.title_row())],
                None if n == 0 => vec![start, (g.term_x.saturating_sub(1), join_row)],
                None => vec![
                    start,
                    (g.screen_join, b.title_row()),
                    (g.screen_join, join_row),
                    (g.term_x.saturating_sub(1), join_row),
                ],
            };
            let to = next.map_or_else(|| term.clone(), |(next, _)| next.obj.clone());
            out.wires.push(Wire {
                from: b.obj.clone(),
                to,
                points,
                dashed: b.dashed,
                stub: false,
            });
        }
    }
    let mut inputs: Vec<((String, String), Obj, Mood, bool)> = p
        .inputs
        .iter()
        .map(|s| {
            let words = labelled(ctx, Kind::Input, s, paths::title(ctx.flow, Kind::Input, s));
            (words, obj(Slot::Input(s.clone())), mood, true)
        })
        .collect();
    if p.no_input {
        let words = (
            words::NO_INPUT.to_owned(),
            words::NO_INPUT_DETAIL.to_owned(),
        );
        inputs.push((
            words,
            obj(Slot::Special(Piece::NoInput)),
            Mood::Aside,
            false,
        ));
    } else if editor && inputs.is_empty() {
        let words = (Stage::Input.name().to_owned(), empty_place(g.dev_w));
        inputs.push((words, obj(Slot::Empty(Stage::Input)), mood, false));
    }
    let input_lines = side(inputs, g.dev_x, g.dev_w, y, editor);
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
    // No input and that vtium does not isolate
    if matches!(row, Row::Doc(_))
        && !p.no_input
        && !ctx.doc.roaming.is_empty()
        && !isolated(ctx.flow, p)
    {
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
            text: fit(words::ROAMING_STUB, g.dev_w),
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

/// Lays out the path of the active terminal from the document's row `y`:
/// the heading ROAMING and the sentence that says what roaming means where
/// a path's displays are, since the active terminal is one of the other
/// terminals at a time and has none of its own; the active terminal's
/// box, dashed because it changes as the person switches terminals; and
/// each roaming input's box, with a dotted line to it. Returns the row
/// after it.
fn roaming_row(ctx: &Ctx<'_>, out: &mut Layout, y: usize) -> usize {
    let g = geometry(ctx.cols);
    out.notes.push(Note {
        x: 1,
        y: y.saturating_add(1),
        text: words::ROAMING.to_owned(),
        kind: NoteKind::Title(Stage::Input),
    });
    let room = g.term_x.saturating_sub(3).max(1);
    let mut bottom = y.saturating_add(BOX_ROWS);
    for (i, line) in wrap(words::ROAMING_ABOUT, room, 3).into_iter().enumerate() {
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

/// The page laid out at the context's width: CONFIGURATION, NEW PATH and
/// PARTS.
pub fn layout(ctx: &Ctx<'_>) -> Layout {
    let g = geometry(ctx.cols);
    let mut out = Layout {
        geometry: Some(g),
        ..Layout::default()
    };
    let note = |out: &mut Layout, x: usize, y: usize, text: String, kind: NoteKind| {
        out.notes.push(Note { x, y, text, kind });
    };
    // CONFIGURATION, with the policy beside its heading, and its column
    // titles; both scroll with the document
    note(
        &mut out,
        1,
        0,
        words::CONFIGURATION.to_owned(),
        NoteKind::Heading,
    );
    let room = ctx.cols.saturating_sub(POLICY_X).saturating_sub(1);
    note(
        &mut out,
        POLICY_X,
        0,
        fit(words::policy(&ctx.flow.policy), room),
        NoteKind::Grey,
    );
    for (x, w, title, stage) in [
        (g.screen_x, g.screen_w, words::DISPLAYS, Stage::Display),
        (g.term_x, g.term_w, words::TERMINALS, Stage::Terminal),
        (g.dev_x, g.dev_w, words::INPUTS, Stage::Input),
    ] {
        note(&mut out, x, 1, fit(title, w), NoteKind::Title(stage));
    }
    let line_room = ctx.cols.saturating_sub(2).max(1);
    let mut y = 1usize;
    let grey = |out: &mut Layout, y: &mut usize, text: String| {
        *y = y.saturating_add(1);
        note(out, 1, *y, fit(&text, line_room), NoteKind::Grey);
    };
    if ctx.doc.paths.is_empty() {
        grey(&mut out, &mut y, words::NO_PATHS.to_owned());
    }
    if ctx.default {
        grey(&mut out, &mut y, words::BUILT_IN_DEFAULT.to_owned());
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
    // the connected devices in no path yet, counted from PARTS, and
    // whether vtium-config follows devices as they come and go
    let mut waiting = Vec::new();
    let groups = palette(ctx, &mut waiting);
    let count = |s: Sort| waiting.iter().filter(|w| **w == s).count();
    let kind = if waiting.is_empty() {
        NoteKind::Grey
    } else {
        NoteKind::Needs
    };
    note(
        &mut out,
        1,
        y,
        fit(
            &words::waiting(
                count(Sort::Keyboard),
                count(Sort::Pointer),
                count(Sort::Screen),
            ),
            line_room,
        ),
        kind,
    );
    if !ctx.hotplug {
        y = y.saturating_add(1);
        note(
            &mut out,
            1,
            y,
            fit(words::NO_HOTPLUG, line_room),
            NoteKind::Grey,
        );
    }
    // NEW PATH
    y = y.saturating_add(1);
    out.rules.push(y);
    y = y.saturating_add(1);
    note(
        &mut out,
        1,
        y,
        words::NEW_PATH.to_owned(),
        NoteKind::Heading,
    );
    let kind = if ctx.new.complete(ctx.doc) {
        NoteKind::Ready
    } else {
        NoteKind::Needs
    };
    note(
        &mut out,
        STATUS_X,
        y,
        fit(
            ctx.new.status(ctx.doc),
            ctx.cols.saturating_sub(STATUS_X).saturating_sub(1),
        ),
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
    note(&mut out, 1, y, words::PARTS.to_owned(), NoteKind::Heading);
    let mut height = y.saturating_add(1);
    for (stage, title, boxes) in groups {
        y = y.saturating_add(1);
        note(&mut out, 1, y, title.to_owned(), NoteKind::Title(stage));
        y = y.saturating_add(1);
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

/// What a part's box says under its title, its state first so a narrow
/// box still shows it, and the state its border shows: set aside when its device is not connected or a path to Nowhere
/// holds it, waiting for a connected device no path holds, placed for the
/// rest. A connected device no path holds is counted in `waiting`.
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
    let device = |kind: Kind, sel: &str| ctx.own.device(flow, kind, sel);
    match &part.piece {
        Piece::Display(sel) => {
            let dev = device(Kind::Display, sel);
            let held = dev.map_or_else(
                || holding(sel, Kind::Display),
                |d| inputs::holders(&doc.paths, Kind::Display, d),
            );
            let off = doc
                .off
                .iter()
                .any(|o| o == sel || dev.is_some_and(|d| d.keys.contains(o)));
            match (held.first(), dev) {
                (Some(h), _) => (words::in_terminal(h), Mood::Placed),
                _ if off => (words::TURNED_OFF.to_owned(), Mood::Aside),
                (None, Some(d)) => {
                    waiting.push(Sort::Screen);
                    let detail = if d.detail.is_empty() {
                        words::IN_NO_PATH.to_owned()
                    } else {
                        format!("{} · {}", words::IN_NO_PATH, d.detail)
                    };
                    (detail, Mood::Waiting)
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
            let held = inputs::holders(&doc.paths, Kind::Input, d);
            let port = inputs::port_words(&d.detail);
            let roams = doc.roaming.iter().any(|r| inputs::names(d, r));
            let aside = doc.ignored.iter().any(|r| r != "*" && inputs::names(d, r));
            match held.as_slice() {
                [] if roams => (format!("{} · {port}", words::ROAMING_WORD), Mood::Placed),
                [] if aside => (format!("{} · {port}", words::SET_ASIDE), Mood::Aside),
                [] => {
                    waiting.push(inputs::sort(d));
                    (format!("{} · {port}", words::IN_NO_PATH), Mood::Waiting)
                }
                [one] => (
                    format!("{} · {port}", words::in_terminal(one)),
                    Mood::Placed,
                ),
                many => (
                    format!("{} · {port}", words::roams_among(many.len())),
                    Mood::Placed,
                ),
            }
        }
        Piece::Look(l) => {
            let users: Vec<String> = doc
                .paths
                .iter()
                .filter(|p| p.look == *l)
                .filter_map(|p| p.name.clone())
                .collect();
            let detail = match (users.first(), l) {
                (Some(u), _) => words::in_terminal(u),
                (None, None) => words::DEFAULT_LOOK_DETAIL.to_owned(),
                (None, Some(_)) => words::LOOK.to_owned(),
            };
            (detail, Mood::Placed)
        }
        Piece::NoInput => (words::NO_INPUT_DETAIL.to_owned(), Mood::Placed),
        Piece::Active => (words::ACTIVE_DETAIL.to_owned(), Mood::Placed),
        Piece::NoTerminal => (words::NO_TERMINAL_DETAIL.to_owned(), Mood::Placed),
        Piece::Terminal(_) => (words::NEW_TERMINAL_DETAIL.to_owned(), Mood::Placed),
    }
}

/// PARTS' four groups, one for each stage: its template first, then each
/// part of that stage with its words and its state, and at the end of the
/// inputs the box of the other inputs. A box's place comes later, when
/// the group flows.
fn palette(ctx: &Ctx<'_>, waiting: &mut Vec<Sort>) -> Vec<(Stage, &'static str, Vec<Boxed>)> {
    // the default look is the absence of one, so it shows picked in a
    // path that has parts and no look
    let picked = |piece: &Piece| match piece {
        Piece::Look(None) => ctx.new.look.is_none() && !ctx.new.is_empty(),
        p => ctx.new.has(p),
    };
    let boxed = |obj: Obj, title: String, detail: String, mood: Mood, template: bool| Boxed {
        obj,
        x: 0,
        y: 0,
        w: natural(&title, &detail),
        title,
        detail,
        mood,
        dashed: false,
        template,
    };
    let others: Vec<String> = inputs::folded(ctx.flow)
        .into_iter()
        .filter_map(|d| ctx.flow.inputs.get(d).map(|x| x.name.clone()))
        .collect();
    Stage::ALL
        .into_iter()
        .map(|stage| {
            let mut boxes = vec![boxed(
                Obj::Pal(Slot::Template(stage)),
                stage.name().to_owned(),
                words::template_add(stage.name()),
                Mood::Placed,
                true,
            )];
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
                boxes.push(boxed(
                    Obj::Pal(Slot::of(&part.piece)),
                    if on { format!("✓ {title}") } else { title },
                    detail,
                    if on { Mood::Picked } else { mood },
                    false,
                ));
            }
            if stage == Stage::Input && !others.is_empty() {
                boxes.push(boxed(
                    Obj::Others,
                    words::other_inputs(others.len()),
                    inputs::others_detail(&others),
                    Mood::Aside,
                    false,
                ));
            }
            let heading = match stage {
                Stage::Display => words::DISPLAYS,
                Stage::Terminal => words::TERMINALS,
                Stage::Input => words::INPUTS,
                Stage::Look => words::LOOKS,
            };
            (stage, heading, boxes)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_flow_like_words_and_stretch_when_the_viewer_is_narrow() {
        let lines = flow_boxes(&[10, 10, 10], 1, 40);
        assert_eq!(lines, [vec![(1, 10), (13, 10), (25, 10)]]);
        // two lines of at most two boxes each: they share the line's width
        let lines = flow_boxes(&[10, 10, 10], 1, 25);
        assert_eq!(lines, [vec![(1, 11), (14, 11)], vec![(1, 24)]]);
    }

    #[test]
    fn the_columns_keep_their_titles_whole_at_80_columns() {
        let g = geometry(80);
        assert!(g.screen_w >= width(words::ANY_DISPLAY) + 4);
        assert!(g.term_w >= TERM_MIN_W);
        assert!(g.dev_x + g.dev_w <= 79, "{g:?}");
    }
}
