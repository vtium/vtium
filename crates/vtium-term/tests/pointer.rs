//! OSC 22, the pointer's shape, as kitty's pointer-shapes specification
//! and xterm's ctlseqs define it. A program sets, pushes, pops and asks
//! about the shape, and the host reads the shape to draw with
//! `Terminal::pointer_shape`. Each screen keeps a stack of its own, and RIS
//! empties both.

mod common;

use common::term;
use vtium_term::{POINTER_QUERY_NAMES_MAX, POINTER_STACK_MAX, PointerShape, Terminal};

/// The shape a terminal fed `bytes` shows, checking that nothing was
/// replied.
fn shape(bytes: &[u8]) -> PointerShape {
    let mut t = term(8, 2, bytes);
    assert_eq!(t.take_replies(), b"", "{bytes:?} replied");
    t.pointer_shape()
}

/// What a terminal fed `bytes` replies.
fn reply(t: &mut Terminal, bytes: &[u8]) -> String {
    t.input(bytes);
    String::from_utf8(t.take_replies()).expect("a reply in ASCII")
}

// kitty's pointer-shapes: the prefix `=`, or none, sets the shape to the
// name, and an empty name resets the pointer to the default. ctlseqs: an
// empty name or one xterm does not know gives the default shape.
// covers: window.pointer-shape
#[test]
fn a_set_names_the_shape_with_or_without_its_prefix() {
    assert_eq!(shape(b""), PointerShape::Default);
    assert_eq!(shape(b"\x1b]22;pointer\x1b\\"), PointerShape::Pointer);
    assert_eq!(shape(b"\x1b]22;=text\x07"), PointerShape::Text);
    assert_eq!(
        shape(b"\x1b]22;=wait\x1b\\\x1b]22;\x1b\\"),
        PointerShape::Default
    );
    assert_eq!(
        shape(b"\x1b]22;=move\x1b\\\x1b]22;=\x1b\\"),
        PointerShape::Default
    );
    for s in PointerShape::ALL {
        let set = format!("\x1b]22;{}\x1b\\", s.name());
        assert_eq!(shape(set.as_bytes()), s, "{}", s.name());
    }
}

// A name vtium has no shape for gives the default shape, as ctlseqs says
// of xterm. A list falls back from name to name, as a CSS `cursor` list
// does, and the X cursor font's names that xterm takes name the same
// shapes. Names are matched exactly.
// covers: window.pointer-shape
#[test]
fn an_unknown_name_gives_the_default_shape() {
    assert_eq!(
        shape(b"\x1b]22;=pointer\x1b\\\x1b]22;=no-such-shape\x1b\\"),
        PointerShape::Default
    );
    assert_eq!(shape(b"\x1b]22;=Pointer\x1b\\"), PointerShape::Default);
    assert_eq!(shape(b"\x1b]22;=pointer \x1b\\"), PointerShape::Default);
    assert_eq!(
        shape(b"\x1b]22;=zoom-in,crosshair\x1b\\"),
        PointerShape::Crosshair
    );
    assert_eq!(shape(b"\x1b]22;=text,crosshair\x1b\\"), PointerShape::Text);
    for (x11, want) in [
        ("left_ptr", PointerShape::Default),
        ("xterm", PointerShape::Text),
        ("hand1", PointerShape::Pointer),
        ("hand2", PointerShape::Pointer),
        ("watch", PointerShape::Wait),
        ("fleur", PointerShape::Move),
        ("sb_h_double_arrow", PointerShape::EwResize),
        ("sb_v_double_arrow", PointerShape::NsResize),
    ] {
        let set = format!("\x1b]22;{x11}\x07");
        assert_eq!(shape(set.as_bytes()), want, "{x11}");
    }
}

// kitty's pointer-shapes: a push adds every name to the stack, the last
// on top and shown; a pop removes the top entry, and the names after `<`
// are ignored; a pop of an empty stack does nothing. A set changes the
// shape on top.
// covers: window.pointer-shape
#[test]
fn push_and_pop_keep_a_stack_of_shapes() {
    let mut t = term(8, 2, b"\x1b]22;>text,pointer\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Pointer);
    t.input(b"\x1b]22;=wait\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Wait);
    t.input(b"\x1b]22;<move\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Text);
    t.input(b"\x1b]22;<\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    t.input(b"\x1b]22;<\x1b\\\x1b]22;<\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    assert_eq!(t.take_replies(), b"");
    // a name vtium has no shape for pushes the default shape, which a pop
    // takes off again
    t.input(b"\x1b]22;>crosshair,no-such-shape\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    t.input(b"\x1b]22;<\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Crosshair);
}

// kitty's pointer-shapes: when the stack is full, a push evicts the entry
// at the bottom. vtium keeps 16 entries, the specification's minimum.
// covers: window.pointer-shape
#[test]
fn a_full_stack_drops_its_bottom_entry() {
    assert_eq!(POINTER_STACK_MAX, 16);
    let mut t = term(8, 2, b"\x1b]22;>text\x1b\\");
    for _ in 1..POINTER_STACK_MAX {
        t.input(b"\x1b]22;>move\x1b\\");
    }
    t.input(b"\x1b]22;>wait\x1b\\");
    for _ in 1..POINTER_STACK_MAX {
        assert_ne!(t.pointer_shape(), PointerShape::Default);
        t.input(b"\x1b]22;<\x1b\\");
    }
    // text was evicted, so the last pop empties the stack
    assert_eq!(t.pointer_shape(), PointerShape::Move);
    t.input(b"\x1b]22;<\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    assert_eq!(
        reply(&mut t, b"\x1b]22;?__current__\x1b\\"),
        "\x1b]22;0\x1b\\"
    );
}

// kitty's pointer-shapes: `?__current__` answers the current shape's name,
// or 0 when the stack is empty; `__default__` and `__grabbed__` answer the
// shapes used by default and while the mouse is grabbed; any other name
// answers 1 when the terminal supports it and 0 when it does not, one
// answer for each name, separated by commas.
// covers: window.pointer-shape
#[test]
fn a_query_answers_each_name_in_turn() {
    let mut t = term(8, 2, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]22;?__current__\x1b\\"),
        "\x1b]22;0\x1b\\"
    );
    assert_eq!(
        reply(
            &mut t,
            b"\x1b]22;?pointer,crosshair,no-such-name,wait\x1b\\"
        ),
        "\x1b]22;1,1,0,1\x1b\\"
    );
    assert_eq!(
        reply(&mut t, b"\x1b]22;?__default__,__grabbed__\x07"),
        "\x1b]22;default,default\x07"
    );
    // the alias's shape is named by its own name, never by the alias
    t.input(b"\x1b]22;sb_h_double_arrow\x1b\\");
    assert_eq!(
        reply(&mut t, b"\x1b]22;?__current__,hand2,grab\x1b\\"),
        "\x1b]22;ew-resize,1,0\x1b\\"
    );
    assert_eq!(reply(&mut t, b"\x1b]22;?\x1b\\"), "\x1b]22;0\x1b\\");
    // a query changes nothing
    assert_eq!(t.pointer_shape(), PointerShape::EwResize);
}

// A query that names more than 64 names gets answers for the first 64.
// covers: window.pointer-shape
#[test]
fn a_long_query_is_answered_for_its_first_names() {
    let names = vec!["text"; POINTER_QUERY_NAMES_MAX + 10].join(",");
    let mut t = term(8, 2, b"");
    let got = reply(&mut t, format!("\x1b]22;?{names}\x1b\\").as_bytes());
    let want = vec!["1"; POINTER_QUERY_NAMES_MAX].join(",");
    assert_eq!(got, format!("\x1b]22;{want}\x1b\\"));
}

// kitty's pointer-shapes: the main and the alternate screen keep separate
// stacks, and a reset empties both.
// covers: window.pointer-shape
#[test]
fn each_screen_keeps_its_own_stack_and_ris_empties_both() {
    let mut t = term(8, 2, b"\x1b]22;>text\x1b\\");
    t.input(b"\x1b[?1049h");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    t.input(b"\x1b]22;>pointer\x1b\\");
    assert_eq!(t.pointer_shape(), PointerShape::Pointer);
    t.input(b"\x1b[?1049l");
    assert_eq!(t.pointer_shape(), PointerShape::Text);
    t.input(b"\x1b[?1049h");
    assert_eq!(t.pointer_shape(), PointerShape::Pointer);
    t.input(b"\x1bc");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    t.input(b"\x1b[?1049h");
    assert_eq!(t.pointer_shape(), PointerShape::Default);
    assert_eq!(
        reply(&mut t, b"\x1b]22;?__current__\x1b\\"),
        "\x1b]22;0\x1b\\"
    );
}

// vtium-parse drops a string cut short whole, so a set that a CAN ends
// changes nothing.
#[test]
fn a_string_cut_short_changes_nothing() {
    assert_eq!(shape(b"\x1b]22;=pointer\x18"), PointerShape::Default);
}
