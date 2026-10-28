//! The deferred pipeline's glyphs (ADR-1301 D2): the per-boot memory
//! store of what the helper generated, and the face that draws a cell from
//! it, falling back per character to the built-in font (#141).
//!
//! The lookup for a cell is the frit's generated glyph, then the built-in
//! font for the cell size, then [`vtium_font::Font`]'s replacement chain
//! through this same face: U+25A1 from the frit, else from the built-in.
//! Nothing here is written to disk. The store holds only the cells the
//! displays draw at ([`Generated::retain`]), within a limit taken from the
//! machine's memory ([`store_limit`]): a fill stops at it, and a request
//! past it evicts the sets least recently on the screen
//! ([`Generated::evict`]), whose glyphs are asked for again when the
//! screen shows them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use vtium_font::set::{Set, SetFace};
use vtium_font::sprite::Range;
use vtium_font::{Cell, Decorations, Face, Glyph, RangeCoverage, SpritePolicy, Style};

const GIB: u64 = 1 << 30;

/// The store's bytes on a machine with `ram` bytes, as the scrollback takes
/// its own (vtium-term's `sb_size_cap`): 16 MiB below 2 GiB, 64 MiB from 2
/// GiB, 128 MiB from 4 GiB.
pub fn store_limit(ram: u64) -> u64 {
    match ram {
        r if r >= 4 * GIB => 128 << 20,
        r if r >= 2 * GIB => 64 << 20,
        _ => 16 << 20,
    }
}

/// One set the helper sent: its cell, the face it draws through, the
/// codepoints it maps, its bytes, and when the screen last showed one of
/// them.
#[derive(Debug)]
struct Entry {
    cell: Cell,
    face: SetFace,
    runs: Vec<(u32, u32)>,
    bytes: u64,
    used: u64,
}

/// The helper's glyphs this boot.
#[derive(Debug, Default)]
pub struct Generated {
    entries: Vec<Entry>,
    /// Counts [`Generated::touch`]es: the store's clock.
    tick: u64,
}

/// The store, shared between the console and its displays' faces.
pub type Shared = Rc<RefCell<Generated>>;

impl Generated {
    /// Adds a checked set of glyphs at `cell`, mapping `runs` and taking
    /// `bytes`; the face it draws through, for the caller to ask what it
    /// holds.
    pub fn add(
        &mut self,
        cell: Cell,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    ) -> Option<SetFace> {
        let face = SetFace::new(set, cell.height)?;
        self.entries.push(Entry {
            cell,
            face: face.clone(),
            runs,
            bytes,
            used: self.tick,
        });
        Some(face)
    }

    /// The bytes held.
    pub fn bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }

    /// Drops the set least recently on the screen: its cell and the
    /// codepoints it mapped. `None` when the store is empty.
    pub fn evict(&mut self) -> Option<(Cell, Vec<(u32, u32)>)> {
        let (i, _) = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.used)?;
        let e = self.entries.remove(i);
        Some((e.cell, e.runs))
    }

    /// The screen shows `chars` at `cell`: the sets that hold them are the
    /// most recently used.
    pub fn touch(&mut self, cell: Cell, chars: &[char]) {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        for e in self.entries.iter_mut().filter(|e| e.cell == cell) {
            if chars.iter().any(|&c| e.face.has_glyph(c)) {
                e.used = tick;
            }
        }
    }

    /// Drops the glyphs of every cell but `cells`.
    pub fn retain(&mut self, cells: &[Cell]) {
        self.entries.retain(|e| cells.contains(&e.cell));
    }

    /// Whether any glyph at `cell` is held.
    pub fn holds(&self, cell: Cell) -> bool {
        self.entries.iter().any(|e| e.cell == cell)
    }

    fn face(&self, cell: Cell, c: char) -> Option<&SetFace> {
        self.entries
            .iter()
            .find(|e| e.cell == cell && e.face.has_glyph(c))
            .map(|e| &e.face)
    }
}

/// A display's face while the font is a frit: the generated glyph where
/// the helper sent one, else the built-in font's, at the built-in font's
/// cell. Its glyphs come at their drawn size, so the renderer stores a
/// generated glyph and a built-in one alike.
pub struct DeferredFace {
    builtin: Box<dyn Face>,
    generated: Shared,
}

impl DeferredFace {
    pub fn new(builtin: Box<dyn Face>, generated: Shared) -> DeferredFace {
        DeferredFace { builtin, generated }
    }

    fn generated_has(&self, c: char) -> bool {
        self.generated
            .borrow()
            .face(self.builtin.cell(), c)
            .is_some()
    }
}

impl Face for DeferredFace {
    fn cell(&self) -> Cell {
        self.builtin.cell()
    }

    fn size_step(&self) -> u32 {
        self.builtin.size_step()
    }

    fn has_glyph(&self, c: char) -> bool {
        self.generated_has(c) || self.builtin.has_glyph(c)
    }

    /// The built-in font's, so the sprites draw the same ranges before the
    /// generated glyphs arrive and after.
    fn coverage(&self, range: Range) -> RangeCoverage {
        self.builtin.coverage(range)
    }

    fn sprite_policy(&self) -> SpritePolicy {
        self.builtin.sprite_policy()
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        let &first = cluster.first()?;
        let generated = self.generated.borrow();
        match generated.face(self.builtin.cell(), first) {
            Some(face) => face.draw(cluster, style),
            None => self.builtin.draw(cluster, style),
        }
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.builtin.last_resort(style)
    }

    fn decorations(&self) -> Decorations {
        self.builtin.decorations()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
    use vtium_font::{Font, Unifont, Width};

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    /// A set at `cell` whose every glyph in `chars` is solid.
    pub(crate) fn solid(cell: Cell, chars: &[char]) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        for &c in chars {
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    fn face(generated: &Shared) -> DeferredFace {
        DeferredFace::new(Box::new(Unifont::new(16)), Rc::clone(generated))
    }

    fn covered(g: &Glyph) -> usize {
        (0..g.height())
            .flat_map(|y| (0..g.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| g.pixel(x, y).unwrap()[0] > 0)
            .count()
    }

    #[test]
    fn the_built_in_font_draws_until_the_glyphs_come() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(a, unifont.draw(&['A'], Style::default()).unwrap());
        generated
            .borrow_mut()
            .add(CELL, solid(CELL, &['A']), vec![(0x41, 1)], 1);
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(covered(&a), 8 * 16);
    }

    // #141: a codepoint the frit lacks draws in the built-in font, one the
    // built-in lacks too in the replacement glyph
    #[test]
    fn a_character_the_frit_lacks_draws_in_the_built_in_font() {
        let generated = Shared::default();
        generated
            .borrow_mut()
            .add(CELL, solid(CELL, &['A']), vec![(0x41, 1)], 1);
        let f = face(&generated);
        let unifont = Unifont::new(16);
        assert_eq!(
            f.draw(&['B'], Style::default()),
            unifont.draw(&['B'], Style::default())
        );
        assert!(f.has_glyph('A') && f.has_glyph('B'));
        // U+10FFFD is in neither: the font's replacement chain draws its
        // U+25A1, from the frit once the frit has one
        let font = Font::new(face(&generated));
        let lacking = font.glyph(&['\u{10fffd}'], Width::Narrow, Style::default());
        let unifont_font = Font::new(Unifont::new(16));
        assert_eq!(
            lacking,
            unifont_font.glyph(&['\u{10fffd}'], Width::Narrow, Style::default())
        );
        generated
            .borrow_mut()
            .add(CELL, solid(CELL, &['\u{25a1}']), vec![(0x25a1, 1)], 1);
        let lacking = font
            .glyph(&['\u{10fffd}'], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(covered(&lacking), 8 * 16);
    }

    #[test]
    fn the_store_limit_follows_the_machines_memory() {
        assert_eq!(store_limit(512 << 20), 16 << 20);
        assert_eq!(store_limit(2 << 30), 64 << 20);
        assert_eq!(store_limit(32 << 30), 128 << 20);
    }

    #[test]
    fn the_set_least_recently_on_the_screen_is_evicted_first() {
        let mut g = Generated::default();
        g.add(CELL, solid(CELL, &['A']), vec![(0x41, 1)], 10);
        g.add(CELL, solid(CELL, &['B']), vec![(0x42, 1)], 20);
        assert_eq!(g.bytes(), 30);
        // A is on the screen, B is not
        g.touch(CELL, &['A']);
        assert_eq!(g.evict(), Some((CELL, vec![(0x42, 1)])));
        assert_eq!(g.bytes(), 10);
        assert_eq!(g.evict(), Some((CELL, vec![(0x41, 1)])));
        assert_eq!(g.evict(), None);
    }

    #[test]
    fn glyphs_at_another_cell_are_not_drawn_and_retain_drops_them() {
        let generated = Shared::default();
        let other = Cell {
            width: 16,
            height: 32,
        };
        generated
            .borrow_mut()
            .add(other, solid(other, &['A']), vec![(0x41, 1)], 1);
        let f = face(&generated);
        assert_eq!(
            f.draw(&['A'], Style::default()),
            Unifont::new(16).draw(&['A'], Style::default())
        );
        assert!(generated.borrow().holds(other));
        generated.borrow_mut().retain(&[CELL]);
        assert!(!generated.borrow().holds(other));
    }
}
