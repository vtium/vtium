//! OSC 22, the pointer's shape: a program names the shape the pointer
//! takes over its window, as kitty's pointer-shapes specification and
//! xterm's ctlseqs define the string.
//!
//! The string is `OSC 22 ; Pt ST`. Pt's first byte chooses what it does,
//! and the rest is a list of shape names separated by commas:
//!
//! - `=` or no prefix sets the shape. The first name vtium has a shape for
//!   wins, as a CSS `cursor` list falls back from name to name. When vtium
//!   has none of them, or Pt is empty, the pointer takes vtium's default
//!   shape, the arrow. xterm likewise falls back to its own default shape,
//!   which is an I-beam. A set replaces the shape on top of the screen's
//!   stack, or starts the stack when it is empty.
//! - `>` pushes each name in turn, so the last one is on top and shows. A
//!   name vtium has no shape for pushes the default shape. An empty name,
//!   as in `>` alone or between two commas, pushes nothing, so a program
//!   that pops once for each name it pushed empties what it pushed. A push
//!   onto a full stack drops the entry at the bottom.
//! - `<` pops the top entry, and an empty stack stays empty. Names after
//!   `<` are ignored.
//! - `?` asks about each name in turn, and the reply is
//!   `OSC 22 ; answers ST` with one answer for each name, separated by
//!   commas. `__current__` answers the name of the shape on top of the
//!   stack, or `0` when the stack is empty. `__default__` and
//!   `__grabbed__` answer `default`, because vtium shows the default shape
//!   whether or not a program tracks the mouse. Any other name answers `1`
//!   when vtium has a shape for it and `0` when it does not. An empty name
//!   is a name vtium has no shape for, so it answers `0`: the reply keeps
//!   one answer in each place the query has a name, and `?` alone answers
//!   `0`.
//!
//! The main screen and the alternate screen keep a stack each, and RIS
//! empties both. The host reads the shape to draw with
//! [`Terminal::pointer_shape`].
//!
//! A reply never carries the request's text. `__current__` answers with
//! the name of a shape from vtium's own fixed set, and every other answer
//! is `0`, `1` or `default`.
//!
//! References:
//! - docs/architecture.md, "Determinism": the reply rule the query follows.
//! - ADR-1000, the row `window.pointer-shape`: vtium supports OSC 22.

use vtium_parse::StringEnd;

use super::State;
use super::reply::{Query, osc_end};
use crate::Terminal;

/// Entries one screen's stack keeps. The specification asks for at least
/// 16 and lets the terminal choose more, and 16 already holds every shape
/// of vtium's set with room to spare.
pub const POINTER_STACK_MAX: usize = 16;

/// Names one query is answered for. A query that names more gets answers
/// for its first 64 names only, so that a long string cannot make a reply
/// many times longer than any program needs.
pub const POINTER_QUERY_NAMES_MAX: usize = 64;

/// A shape the pointer can take: vtium's fixed set, each of which vtium
/// draws a picture of its own for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PointerShape {
    /// The arrow, or the character `pointer-char` names.
    #[default]
    Default,
    /// The text cursor, an I-beam, for text that can be selected.
    Text,
    /// A hand with a raised finger, for a link or a button.
    Pointer,
    /// A thin cross, for picking a point or a cell.
    Crosshair,
    /// An hourglass: the program is busy and takes no input.
    Wait,
    /// The arrow with a small hourglass: the program is busy and still
    /// takes input.
    Progress,
    /// A circle with a slash: the action is not allowed here.
    NotAllowed,
    /// A cross of four arrows: something can be moved.
    Move,
    /// A double arrow, left and right: an edge or a column can be resized
    /// sideways, such as a split between two panes.
    EwResize,
    /// A double arrow, up and down: an edge or a row can be resized up or
    /// down.
    NsResize,
}

impl PointerShape {
    /// Every shape, in the order the tests and the screens draw them.
    pub const ALL: [PointerShape; 10] = [
        PointerShape::Default,
        PointerShape::Text,
        PointerShape::Pointer,
        PointerShape::Crosshair,
        PointerShape::Wait,
        PointerShape::Progress,
        PointerShape::NotAllowed,
        PointerShape::Move,
        PointerShape::EwResize,
        PointerShape::NsResize,
    ];

    /// The shape's name in kitty's specification, which follows CSS's
    /// `cursor` property. A query's reply names a shape by it.
    pub const fn name(self) -> &'static str {
        match self {
            PointerShape::Default => "default",
            PointerShape::Text => "text",
            PointerShape::Pointer => "pointer",
            PointerShape::Crosshair => "crosshair",
            PointerShape::Wait => "wait",
            PointerShape::Progress => "progress",
            PointerShape::NotAllowed => "not-allowed",
            PointerShape::Move => "move",
            PointerShape::EwResize => "ew-resize",
            PointerShape::NsResize => "ns-resize",
        }
    }

    /// The shape a name stands for, if vtium has one: the specification's
    /// own name, or one of the X cursor font's names that xterm's OSC 22
    /// takes and that means the same shape. The match is exact, so
    /// `Pointer` or `pointer ` names nothing.
    pub fn from_name(name: &[u8]) -> Option<PointerShape> {
        let found = PointerShape::ALL
            .into_iter()
            .find(|s| s.name().as_bytes() == name);
        found.or(match name {
            b"left_ptr" => Some(PointerShape::Default),
            b"xterm" => Some(PointerShape::Text),
            b"hand1" | b"hand2" => Some(PointerShape::Pointer),
            b"watch" => Some(PointerShape::Wait),
            b"fleur" => Some(PointerShape::Move),
            b"sb_h_double_arrow" => Some(PointerShape::EwResize),
            b"sb_v_double_arrow" => Some(PointerShape::NsResize),
            _ => None,
        })
    }
}

/// The shape stacks, one for the main screen and one for the alternate
/// screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Stacks([Vec<PointerShape>; 2]);

impl Stacks {
    /// The stack of the screen at `slot`: 0 is the main screen, 1 the
    /// alternate one.
    fn at(&mut self, slot: usize) -> Option<&mut Vec<PointerShape>> {
        self.0.get_mut(slot)
    }

    /// The shape on top of the stack at `slot`, or `None` when that stack
    /// is empty.
    fn top(&self, slot: usize) -> Option<PointerShape> {
        self.0.get(slot).and_then(|s| s.last().copied())
    }
}

/// Pushes `shape` on `stack`, dropping the bottom entry when the stack is
/// full.
fn push(stack: &mut Vec<PointerShape>, shape: PointerShape) {
    if stack.len() >= POINTER_STACK_MAX {
        stack.remove(0);
    }
    stack.push(shape);
}

impl State {
    /// The slot of the screen in use: 0 for the main screen, 1 for the
    /// alternate one.
    fn pointer_slot(&self) -> usize {
        usize::from(self.screen.alternate())
    }

    /// OSC 22, `args` being the string after `22;`.
    pub(super) fn osc_pointer(&mut self, args: &[u8], end: StringEnd) {
        let slot = self.pointer_slot();
        match args.split_first() {
            Some((b'?', names)) => self.reply_pointer(names, slot, end),
            Some((b'>', names)) => {
                if let Some(stack) = self.pointer.at(slot) {
                    for name in names.split(|&b| b == b',').filter(|n| !n.is_empty()) {
                        push(stack, PointerShape::from_name(name).unwrap_or_default());
                    }
                }
            }
            Some((b'<', _)) => {
                if let Some(stack) = self.pointer.at(slot) {
                    stack.pop();
                }
            }
            _ => {
                let names = args.strip_prefix(b"=").unwrap_or(args);
                let shape = names
                    .split(|&b| b == b',')
                    .find_map(PointerShape::from_name)
                    .unwrap_or_default();
                if let Some(stack) = self.pointer.at(slot) {
                    stack.pop();
                    stack.push(shape);
                }
            }
        }
    }

    /// The reply to `OSC 22 ; ? names`, written from vtium's own shapes
    /// and fixed answers only.
    fn reply_pointer(&mut self, names: &[u8], slot: usize, end: StringEnd) {
        let answers: Vec<&str> = names
            .split(|&b| b == b',')
            .take(POINTER_QUERY_NAMES_MAX)
            .map(|name| match name {
                b"__current__" => self.pointer.top(slot).map_or("0", PointerShape::name),
                b"__default__" | b"__grabbed__" => PointerShape::Default.name(),
                _ if PointerShape::from_name(name).is_some() => "1",
                _ => "0",
            })
            .collect();
        let reply = format!("\x1b]22;{}{}", answers.join(","), osc_end(end));
        self.reply(Query::PointerShape, reply.as_bytes());
    }

    /// RIS's part: both stacks empty.
    pub(super) fn reset_pointer(&mut self) {
        self.pointer = Stacks::default();
    }
}

impl Terminal {
    /// The shape a program asked the pointer to take over this terminal,
    /// with OSC 22: the shape on top of the stack of the screen in use, or
    /// the default shape when that stack is empty.
    pub fn pointer_shape(&self) -> PointerShape {
        self.state
            .pointer
            .top(self.state.pointer_slot())
            .unwrap_or_default()
    }
}
