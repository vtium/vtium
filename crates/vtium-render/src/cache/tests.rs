use super::*;
use vtium_font::{Psf, Style};

/// A 256-glyph 8x16 font without a table, glyph i's rows all i.
fn font() -> Font<Psf> {
    let mut b = vec![0x72, 0xb5, 0x4a, 0x86];
    for v in [0u32, 32, 0, 256, 16, 16, 8] {
        b.extend(v.to_le_bytes());
    }
    for i in 0..=255u8 {
        b.extend([i; 16]);
    }
    Font::new(Psf::parse(&b).unwrap())
}

fn get(c: &mut GlyphCache, f: &Font<Psf>, ch: char) -> bool {
    c.get(f, &[ch], Width::Narrow, Variant::default()).is_some()
}

#[test]
fn a_hit_draws_nothing_and_a_miss_draws_once() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    c.begin_frame();
    for ch in ['a', 'b', 'a', 'a', 'b'] {
        assert!(get(&mut c, &f, ch));
    }
    assert_eq!(
        c.take_counts(),
        CacheCounts {
            misses: 2,
            uncached: 0
        }
    );
    let bold = Variant::from(Style {
        bold: true,
        italic: false,
    });
    assert!(c.get(&f, &['a'], Width::Narrow, bold).is_some());
    assert!(c.get(&f, &['a'], Width::Wide, Variant::default()).is_some());
    assert!(
        c.get(&f, &['a', '\u{301}'], Width::Narrow, Variant::default())
            .is_some()
    );
    assert_eq!(c.take_counts().misses, 3);
}

// A line of double size draws each piece of a doubled glyph as a glyph
// of its own: one cell of the glyph with each pixel doubled, drawn by
// the font once. The test font's `a` is the byte 0x61 in every row,
// 01100001, so its left piece is 0110 doubled and its right piece 0001
// doubled.
// covers: text.line-size
#[test]
fn each_piece_of_a_doubled_glyph_is_a_glyph_of_its_own() {
    use vtium_term::LineSize;
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    let key = (&['a'][..], Width::Narrow, Variant::default());
    let line = |c: &mut GlyphCache, piece: Piece| {
        let mut out = [0u8; 8];
        c.get_piece(&f, key, piece)
            .unwrap()
            .mask_line(0, 0, &mut out);
        out.map(|a| u8::from(a == 255))
    };
    let left = Piece::of(LineSize::DoubleWidth, 0);
    let right = Piece::of(LineSize::DoubleWidth, 1);
    assert_eq!(line(&mut c, Piece::WHOLE), [0, 1, 1, 0, 0, 0, 0, 1]);
    assert_eq!(line(&mut c, left), [0, 0, 1, 1, 1, 1, 0, 0]);
    assert_eq!(line(&mut c, right), [0, 0, 0, 0, 0, 0, 1, 1]);
    assert_eq!(c.take_counts().misses, 3, "each piece drawn by the font");
    assert_eq!(line(&mut c, right), [0, 0, 0, 0, 0, 0, 1, 1]);
    assert_eq!(line(&mut c, left), [0, 0, 1, 1, 1, 1, 0, 0]);
    assert_eq!(c.take_counts().misses, 0, "each piece cached");
}

// covers: sgr.script
#[test]
fn each_script_is_a_glyph_of_its_own() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    let normal = Variant::default();
    let at = |script| Variant { script, ..normal };
    let mut drawn = Vec::new();
    for v in [normal, at(Script::Super), at(Script::Sub)] {
        let g = c.get(&f, &['a'], Width::Narrow, v).unwrap();
        // Stored has no equality of its own; its Debug shows its pixels
        drawn.push(format!("{g:?}"));
        assert_eq!(
            c.take_counts().misses,
            1,
            "{:?} drawn by the font",
            v.script
        );
    }
    // each is drawn once, and each its own pixels
    for v in [normal, at(Script::Super), at(Script::Sub)] {
        assert!(c.get(&f, &['a'], Width::Narrow, v).is_some());
    }
    assert_eq!(c.take_counts().misses, 0);
    let distinct: std::collections::BTreeSet<&String> = drawn.iter().collect();
    assert_eq!(distinct.len(), 3, "three glyphs of their own");
    // and each remembers its own reach
    let slots: Vec<_> = [normal, at(Script::Super), at(Script::Sub)]
        .iter()
        .map(|&v| reach_slot(&['a'], Width::Narrow, v))
        .collect();
    let distinct: std::collections::BTreeSet<usize> = slots.iter().flatten().copied().collect();
    assert_eq!(distinct.len(), 3, "three reach slots of their own");
}

#[test]
fn forgetting_a_character_drops_each_glyph_whose_text_holds_it() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    let cluster = |ch: char| [ch, '\u{301}'];
    let texts = [['A'].as_slice(), &['B'], &cluster('A'), &cluster('B')];
    c.begin_frame();
    for text in texts {
        assert!(c.get(&f, text, Width::Narrow, Variant::default()).is_some());
    }
    assert_eq!(c.take_counts().misses, 4);
    c.forget(&['A']);
    c.begin_frame();
    let mut misses = Vec::new();
    for text in texts {
        assert!(c.get(&f, text, Width::Narrow, Variant::default()).is_some());
        misses.push(c.take_counts());
    }
    let miss = CacheCounts {
        misses: 1,
        uncached: 0,
    };
    let hit = CacheCounts::default();
    assert_eq!(misses, [miss, hit, miss, hit]);
}

#[test]
fn forgetting_a_combining_mark_drops_each_cluster_that_holds_it() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    let texts = [
        ['A'].as_slice(),
        &['B'],
        &['A', '\u{301}'],
        &['B', '\u{301}'],
        &['A', '\u{300}'],
    ];
    c.begin_frame();
    for text in texts {
        assert!(c.get(&f, text, Width::Narrow, Variant::default()).is_some());
    }
    assert_eq!(c.take_counts().misses, 5);
    c.forget(&['\u{301}']);
    c.begin_frame();
    let mut misses = Vec::new();
    for text in texts {
        assert!(c.get(&f, text, Width::Narrow, Variant::default()).is_some());
        misses.push(c.take_counts());
    }
    let miss = CacheCounts {
        misses: 1,
        uncached: 0,
    };
    let hit = CacheCounts::default();
    assert_eq!(misses, [hit, hit, miss, miss, hit]);
}

#[test]
fn a_full_pool_evicts_only_glyphs_older_than_the_frame() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(2), Storage::default(), Orientation::Normal);
    c.begin_frame();
    for ch in ['a', 'b', 'c'] {
        assert!(get(&mut c, &f, ch));
    }
    // 'c' would have evicted a glyph this frame drew
    assert_eq!(
        c.take_counts(),
        CacheCounts {
            misses: 3,
            uncached: 1
        }
    );
    c.begin_frame();
    assert!(get(&mut c, &f, 'b'));
    assert!(get(&mut c, &f, 'c'));
    assert_eq!(
        c.take_counts(),
        CacheCounts {
            misses: 1,
            uncached: 0
        }
    );
    // 'a' went, 'b' stayed
    assert!(get(&mut c, &f, 'b'));
    assert_eq!(c.take_counts().misses, 0);
    c.begin_frame();
    assert!(get(&mut c, &f, 'a'));
    assert_eq!(c.take_counts().misses, 1);
}

#[test]
fn a_cache_of_nothing_draws_every_glyph() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(0), Storage::default(), Orientation::Normal);
    c.begin_frame();
    for _ in 0..3 {
        assert!(get(&mut c, &f, 'a'));
    }
    assert_eq!(
        c.take_counts(),
        CacheCounts {
            misses: 3,
            uncached: 3
        }
    );
    assert_eq!(c.pixel_bytes(), 0);
}

#[test]
fn a_byte_ceiling_bounds_a_pool() {
    let f = font();
    let budgets = Budgets {
        mask_bytes: 40,
        ..Budgets::glyphs(100)
    };
    let mut c = GlyphCache::new(budgets, Storage::default(), Orientation::Normal);
    // each glyph is 16 bytes at 1 bit
    for (i, ch) in ('A'..='Z').enumerate() {
        c.begin_frame();
        assert!(get(&mut c, &f, ch), "{i}");
        assert!(c.pixel_bytes() <= 40);
    }
    assert_eq!(c.pixel_bytes(), 32);
    let wide = Budgets {
        mask_bytes: 8,
        ..Budgets::glyphs(100)
    };
    let mut c = GlyphCache::new(wide, Storage::default(), Orientation::Normal);
    c.begin_frame();
    assert!(get(&mut c, &f, 'a'));
    assert_eq!(c.take_counts().uncached, 1);
}

// Many inserts and evictions through a small table keep every glyph
// findable: the backward shift never strands an entry.
#[test]
fn the_table_survives_churn() {
    let f = font();
    let mut c = GlyphCache::new(Budgets::glyphs(37), Storage::default(), Orientation::Normal);
    for round in 0..50u32 {
        c.begin_frame();
        for k in 0..40u32 {
            let ch = char::from_u32(0x20 + (round * 7 + k * 13) % 200).unwrap();
            assert!(get(&mut c, &f, ch));
        }
        c.take_counts();
        for k in 0..37u32 {
            let ch = char::from_u32(0x20 + (round * 7 + k * 13) % 200).unwrap();
            c.begin_frame();
            let before = c.take_counts();
            assert_eq!(before.misses, 0);
            assert!(get(&mut c, &f, ch));
            assert_eq!(c.take_counts().misses, 0, "round {round} glyph {k}");
        }
    }
    assert!(c.live <= 37);
}

/// The coverage of each pixel of the piece `piece` of `text` that the
/// cache `c` draws in `f`, row by row.
fn piece_pixels<F: Face>(c: &mut GlyphCache, f: &Font<F>, text: &[char], piece: Piece) -> Vec<u8> {
    let cell = f.cell();
    let g = c
        .get_piece(f, (text, Width::Narrow, Variant::default()), piece)
        .unwrap();
    assert!(!g.is_rgba(), "the piece is a color glyph");
    let (w, h) = (cell.width as usize, cell.height as usize);
    let mut out = vec![0u8; w.checked_mul(h).unwrap()];
    for (y, row) in out.chunks_mut(w).enumerate() {
        g.mask_line(0, i64::try_from(y).unwrap(), row);
    }
    out
}

// On a 1-bit display a light shade is its 1-bit ordered pattern, one
// pixel in four lit, on a line of double size as on a line of the
// normal size: the sprite drawn at the doubled cell takes its pattern
// too, so no pixel is a gray that the cut at half would light or blank
// whole. The operator decided the pattern on 2026-10-28.
// covers: text.line-size
#[test]
fn a_shade_on_a_line_of_double_size_keeps_its_1_bit_pattern() {
    use crate::mono::{Depth, Fallback};
    use vtium_term::LineSize;
    let f = font();
    let shade = ['\u{2591}'];
    assert!(!f.face().has_glyph(shade[0]), "the shade is a sprite");
    let mut c = GlyphCache::new(Budgets::glyphs(16), Storage::default(), Orientation::Normal)
        .with_fallback(Fallback::new(Depth::One, f.cell()));
    for size in [
        LineSize::Single,
        LineSize::DoubleWidth,
        LineSize::DoubleHeightTop,
        LineSize::DoubleHeightBottom,
    ] {
        for column in [0, 1] {
            let px = piece_pixels(&mut c, &f, &shade, Piece::of(size, column));
            assert!(
                px.iter().all(|&p| p == 0 || p == 0xff),
                "{size:?} {column}: {px:?}"
            );
            let lit = px.iter().filter(|&&p| p == 0xff).count();
            assert_eq!(lit, px.len() / 4, "{size:?} {column}");
        }
    }
}

/// A face of unifont's 8×16 cell that draws ☺ as a color glyph, at the
/// normal size and at each double size, as the font helper renders a
/// color font's glyphs.
struct Smiley;

impl Face for Smiley {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, c: char) -> bool {
        c == '\u{263a}'
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        (cluster == ['\u{263a}'])
            .then(|| vtium_font::Glyph::rgba(8, 16))
            .flatten()
    }
    fn doubled(
        &self,
        cluster: &[char],
        _: Variant,
        doubled: vtium_font::Doubled,
    ) -> Option<vtium_font::Glyph> {
        let cell = doubled.cell(self.cell())?;
        (cluster == ['\u{263a}'])
            .then(|| vtium_font::Glyph::rgba(cell.width, cell.height))
            .flatten()
    }
}

// On a gray display a color glyph is unifont's bitmap of the same
// character, the operator's decision on vtium/vtium#135, on a line of
// double size as on a line of the normal size: the color glyph the
// face holds at the double size is not drawn, and each piece is
// unifont's bitmap with each pixel doubled.
// covers: text.line-size
#[test]
fn a_color_glyph_on_a_line_of_double_size_takes_unifonts_bitmap_off_color() {
    use crate::mono::{Depth, Fallback};
    use vtium_term::LineSize;
    let f = Font::new(Smiley);
    let smiley = ['\u{263a}'];
    let unifont = Font::new(vtium_font::Unifont::new(16));
    let mut want = GlyphCache::new(Budgets::glyphs(16), Storage::default(), Orientation::Normal);
    for depth in [Depth::Gray(4), Depth::One] {
        let mut c = GlyphCache::new(Budgets::glyphs(16), Storage::default(), Orientation::Normal)
            .with_fallback(Fallback::new(depth, f.cell()));
        for size in [
            LineSize::DoubleWidth,
            LineSize::DoubleHeightTop,
            LineSize::DoubleHeightBottom,
        ] {
            for column in [0, 1] {
                let piece = Piece::of(size, column);
                assert_eq!(
                    piece_pixels(&mut c, &f, &smiley, piece),
                    piece_pixels(&mut want, &unifont, &smiley, piece),
                    "{depth:?} {size:?} {column}"
                );
            }
        }
    }
}

/// A face of an 8×16 cell that holds `A` rendered at each double size,
/// as the font helper renders an outline font's glyphs, and counts how
/// often the cache asks it for one.
struct Counted(std::rc::Rc<core::cell::Cell<u32>>);

impl Face for Counted {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, c: char) -> bool {
        c == 'A'
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        (cluster == ['A'])
            .then(|| vtium_font::Glyph::mask(8, 16))
            .flatten()
    }
    fn doubled(
        &self,
        cluster: &[char],
        _: Variant,
        doubled: vtium_font::Doubled,
    ) -> Option<vtium_font::Glyph> {
        self.0.set(self.0.get().saturating_add(1));
        let cell = doubled.cell(self.cell())?;
        (cluster == ['A'])
            .then(|| vtium_font::Glyph::mask(cell.width, cell.height))
            .flatten()
    }
}

// The pieces of one cell on a line of double size are cut from one
// glyph drawn at the double size, which the face renders once for all
// of them: once for the two pieces of a line of double width, and once
// for the four pieces of a double-height pair, whose top and bottom
// pieces cut one glyph. An arrival makes the face render it again.
// covers: text.line-size
#[test]
fn the_pieces_of_a_cell_of_double_size_are_cut_from_one_drawn_glyph() {
    use vtium_term::LineSize;
    let asked = std::rc::Rc::new(core::cell::Cell::new(0));
    let f = Font::new(Counted(asked.clone()));
    let mut c = GlyphCache::new(Budgets::glyphs(16), Storage::default(), Orientation::Normal);
    let key = (&['A'][..], Width::Narrow, Variant::default());
    let mut pieces = |sizes: &[LineSize]| {
        asked.set(0);
        for &size in sizes {
            for column in [0, 1] {
                assert!(c.get_piece(&f, key, Piece::of(size, column)).is_some());
            }
        }
        c.forget(&['A']);
        asked.get()
    };
    assert_eq!(pieces(&[LineSize::DoubleWidth]), 1);
    let pair = [LineSize::DoubleHeightTop, LineSize::DoubleHeightBottom];
    assert_eq!(pieces(&pair), 1);
    assert_eq!(pieces(&pair), 1, "drawn again after an arrival");
}

/// A face that draws nothing at all.
struct Blank;

impl Face for Blank {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, _: char) -> bool {
        false
    }
    fn draw(&self, _: &[char], _: Style) -> Option<vtium_font::Glyph> {
        None
    }
}

// The font is asked once for a glyph it draws nothing for, until the
// glyph is forgotten
#[test]
fn a_glyph_the_font_draws_nothing_for_is_asked_for_once_until_forgotten() {
    let f = Font::new(Blank);
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    for _ in 0..3 {
        c.begin_frame();
        assert!(
            c.get(&f, &['a'], Width::Narrow, Variant::default())
                .is_none()
        );
    }
    assert_eq!(c.take_counts().misses, 1);
    // bold is another glyph
    let bold = Variant::from(Style {
        bold: true,
        italic: false,
    });
    assert!(c.get(&f, &['a'], Width::Narrow, bold).is_none());
    assert_eq!(c.take_counts().misses, 1);
    c.forget(&['a']);
    assert!(
        c.get(&f, &['a'], Width::Narrow, Variant::default())
            .is_none()
    );
    assert_eq!(c.take_counts().misses, 1);
}

/// A face that draws only `?`, once it has arrived.
struct Late(std::rc::Rc<core::cell::Cell<bool>>);

impl Face for Late {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, c: char) -> bool {
        c == '?' && self.0.get()
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        (cluster == ['?'] && self.0.get())
            .then(|| vtium_font::Glyph::mask(8, 16))
            .flatten()
    }
}

// a glyph the font cannot draw falls back to U+25A1, then `?`, then the
// last resort, so any arrival can give a remembered failure a glyph
#[test]
fn an_arrival_forgets_every_remembered_failure() {
    let arrived = std::rc::Rc::new(core::cell::Cell::new(false));
    let f = Font::new(Late(arrived.clone()));
    let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
    assert!(
        c.get(&f, &['a'], Width::Narrow, Variant::default())
            .is_none()
    );
    arrived.set(true);
    c.forget(&['?']);
    c.begin_frame();
    assert!(
        c.get(&f, &['a'], Width::Narrow, Variant::default())
            .is_some()
    );
}
