//! `Face::builtins`: the face the console draws its notices and focus mark
//! with (ADR-1401 D5, D7). Every face gives none unless it says otherwise,
//! and a boxed face, as the console holds its faces, gives what the face
//! inside it gives.

use vtium_font::{Cell, Face, Glyph, Style, Unifont};

/// A face of nothing, in a cell 9 wide, that gives `inner` as its
/// built-in fonts, as the console's face over a font set's glyphs does.
struct Over {
    inner: Unifont,
}

impl Face for Over {
    fn cell(&self) -> Cell {
        Cell {
            width: 9,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, _: char) -> bool {
        false
    }
    fn draw(&self, _: &[char], _: Style) -> Option<Glyph> {
        None
    }
    fn builtins(&self) -> Option<Box<dyn Face + '_>> {
        Some(Box::new(Inner(&self.inner)))
    }
}

/// `Over`'s inner face through a reference, so the face it gives
/// borrows it.
struct Inner<'a>(&'a Unifont);

impl Face for Inner<'_> {
    fn cell(&self) -> Cell {
        self.0.cell()
    }
    fn size_step(&self) -> u32 {
        self.0.size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        self.0.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.0.draw(cluster, style)
    }
}

#[test]
fn a_face_gives_no_builtins_unless_it_says_so() {
    assert!(Unifont::new(16).builtins().is_none(), "the default");
    let boxed: Box<dyn Face + Send> = Box::new(Unifont::new(16));
    assert!(boxed.builtins().is_none(), "boxed, the default still");
}

#[test]
fn a_boxed_face_gives_the_builtins_the_face_inside_gives() {
    let over: Box<dyn Face> = Box::new(Over {
        inner: Unifont::new(16),
    });
    let builtins = over.builtins().expect("the box passes the face's own on");
    assert_eq!(
        builtins.cell(),
        Cell {
            width: 8,
            height: 16
        }
    );
    assert!(builtins.draw(&['A'], Style::default()).is_some());
}
