use vtium_font::Unifont;

use super::*;
use crate::cache::Profile;
use crate::deco::SCRIPTS;
use crate::gpu::{Call, Limits, StandIn};

fn glyphs(gl: &mut StandIn, sprites: &Sprites) -> Glyphs {
    let spec = Spec {
        owner: 1,
        budgets: Budgets::for_profile(Profile::Performance, 1000, 800, 600),
        storage: Storage::default(),
        fallback: None,
        cell: Size::new(8, 16),
        sprites,
        bound: Bound::DRAWN,
    };
    Glyphs::new(gl, &spec).unwrap()
}

fn sprites_of(gl: &mut StandIn, cell: Size) -> (Sprites, Result<Glyphs, Error>) {
    let font = Font::new(Unifont::new(16));
    let sprites = Sprites::new(
        cell.width,
        cell.height,
        &SCRIPTS.map(|s| font.script_decorations(s)),
        Orientation::Normal,
        true,
        false,
    );
    let spec = Spec {
        owner: 1,
        budgets: Budgets::for_profile(Profile::Performance, 1000, 800, 600),
        storage: Storage::default(),
        fallback: None,
        cell,
        sprites: &sprites,
        bound: Bound::DRAWN,
    };
    let g = Glyphs::new(gl, &spec);
    (sprites, g)
}

// The sprites lie in a grid whose largest side is as small as the
// sprites allow, so a GPU whose largest texture is 64 holds the 24
// sprites of an 8x16 cell, the eight kinds of line in each script's
// set (ADR-1202 D8), six across and four down in 48x64, where a row of
// them would be 192 wide. Each line lies in its place in the grid, the
// normal set first, the first row left to right, then the next.
// covers: sgr.script
#[test]
fn the_sprites_lie_in_a_grid_that_fits_a_largest_texture_a_row_of_them_passes() {
    let limits = Limits {
        texture: 64,
        ..StandIn::LIMITS
    };
    let mut gl = StandIn::with_limits(limits);
    let cell = Size::new(8, 16);
    let (sprites, g) = sprites_of(&mut gl, cell);
    let g = g.unwrap();
    assert_eq!(g.sprite_columns(), 6);
    assert_eq!(
        gl.calls()[..2],
        [
            Call::CreateTexture(Format::R8, Size::new(48, 64)),
            Call::FillTexture(Size::new(48, 64).rect()),
        ]
    );
    let (_, size, texels) = gl.texture_pixels(1).unwrap();
    assert_eq!(size, Size::new(48, 64));
    for (k, line) in Line::all().enumerate() {
        assert_eq!(line.index(), k);
        let (col, row) = (k % 6, k / 6);
        for y in 0..16 {
            let mut want = [0u8; 8];
            sprites
                .get(line)
                .unwrap()
                .mask_line(0, i64::try_from(y).unwrap(), &mut want);
            let at = (row * 16 + y) * 48 + col * 8;
            assert_eq!(texels[at..at + 8], want, "{line:?} line {y}");
        }
    }
    // a square cell lies five across, the last row one short
    let mut gl = StandIn::new();
    let (_, g) = sprites_of(&mut gl, Size::new(16, 16));
    assert_eq!(g.unwrap().sprite_columns(), 5);
    assert_eq!(
        gl.calls()[0],
        Call::CreateTexture(Format::R8, Size::new(80, 80))
    );
}

// A cell whose sprites pass the largest texture even in the grid fails
// with an error that names the sprites, and makes no texture.
#[test]
fn sprites_past_the_largest_texture_fail_with_an_error_that_names_them() {
    let limits = Limits {
        texture: 60,
        ..StandIn::LIMITS
    };
    let mut gl = StandIn::with_limits(limits);
    let (_, g) = sprites_of(&mut gl, Size::new(16, 32));
    let e = g.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Limit);
    assert_eq!(
        e.detail,
        "the decoration sprites of a 16x32 cell take a 96x128 texture, \
         which passes the largest texture, 60"
    );
    assert!(gl.calls().is_empty(), "{:?}", gl.calls());
}

/// A glyph 8x16 whose ink reaches a pixel past each side of its
/// cell: a mask, or a color emoji drawn in one cell (ADR-1302 D5).
fn reaching(color: bool) -> Stored {
    let (format, bpp) = match color {
        true => (vtium_font::Format::Rgba, 4),
        false => (vtium_font::Format::Mask, 1),
    };
    let own = [255u8].repeat(128).repeat(bpp);
    let boxed = [255u8].repeat(160).repeat(bpp);
    let g = vtium_font::Glyph::from_pixels(8, 16, format, own)
        .unwrap()
        .with_ink((-1, 0), (10, 16), boxed)
        .unwrap();
    Stored::new(&g, 1, Orientation::Normal, false)
}

/// The sprites of an 8x16 cell.
fn sprites_816() -> Sprites {
    let font = Font::new(Unifont::new(16));
    Sprites::new(
        8,
        16,
        &SCRIPTS.map(|s| font.script_decorations(s)),
        Orientation::Normal,
        true,
        false,
    )
}

// ADR-1302 D5, D10: a color emoji whose box the color boxes have no
// room for under the ceiling is drawn cut at its cell, with a note
// for the log, as a mask is when the boxes are full; the frame goes on.
// The ceiling holds the sprites and one texture of boxes, 98,304 bytes
#[test]
fn a_color_emoji_past_the_ceiling_is_drawn_cut_at_its_cell() {
    let sprites = sprites_816();
    let mut gl = StandIn::new();
    let mut g = glyphs(&mut gl, &sprites);
    g.set_ceiling(g.bytes() + 98_304);
    g.begin_frame();
    let style = Variant::default();
    let mask = g.put(&mut gl, (&['a'], Width::Narrow, style), reaching(false));
    assert!(mask.unwrap().is_some_and(|p| p.boxed.is_some()));
    let _ = g.take_notes();
    let emoji = g.put(&mut gl, (&['b'], Width::Narrow, style), reaching(true));
    assert_eq!(emoji.unwrap(), None, "no box: drawn cut at its cell");
    assert_eq!(g.take_notes().len(), 1);
    assert!(g.bytes() <= g.ceiling);
}

// ADR-1302 D10: the boxes and the color boxes grow together, and the
// ceiling counts both: once a color emoji has a box, a ceiling with
// room for two more texture's worth of boxes, 2 x 98,304 bytes, holds
// the textures to it while 127 masks whose ink passes their cells ask
// for boxes, though the R8 growth alone would fit
#[test]
fn the_ceiling_counts_the_color_boxes_growth() {
    let sprites = sprites_816();
    let mut gl = StandIn::new();
    let mut g = glyphs(&mut gl, &sprites);
    g.begin_frame();
    let style = Variant::default();
    let emoji = g.put(&mut gl, (&['@'], Width::Narrow, style), reaching(true));
    assert!(emoji.unwrap().is_some_and(|p| p.color && p.boxed.is_some()));
    let ceiling = g.bytes() + 2 * 98_304;
    assert_eq!(ceiling, 691_200);
    g.set_ceiling(ceiling);
    for c in ('A'..).take(127) {
        let _ = g
            .put(&mut gl, (&[c], Width::Narrow, style), reaching(false))
            .unwrap();
        assert!(g.bytes() <= ceiling, "{} bytes at {c}", g.bytes());
    }
}

// the issue on GPU texture garbage collection: past the ceiling a pool
// takes no new texture, a glyph it has no slot for is drawn blank, and
// a slot given back is taken again
#[test]
fn past_the_ceiling_a_pool_takes_no_texture_and_reuses_a_slot_given_back() {
    let font = Font::new(Unifont::new(16));
    let sprites = Sprites::new(
        8,
        16,
        &SCRIPTS.map(|s| font.script_decorations(s)),
        Orientation::Normal,
        true,
        false,
    );
    let mut gl = StandIn::new();
    let mut g = glyphs(&mut gl, &sprites);
    // the sprites and one texture of 64 slots, 512x16 at a byte a pixel
    let ceiling = g.bytes() + 512 * 16;
    g.set_ceiling(ceiling);
    g.begin_frame();
    let style = Variant::default();
    let chars: Vec<char> = ('A'..).take(65).collect();
    for &c in &chars[..64] {
        assert!(
            g.glyph(&mut gl, &font, (&[c], Width::Narrow, style), Piece::WHOLE)
                .unwrap()
                .is_some()
        );
    }
    let textures = |gl: &StandIn| {
        gl.calls()
            .iter()
            .filter(|c| matches!(c, Call::CreateTexture(..)))
            .count()
    };
    let made = textures(&gl);
    // the 65th has no slot and no room for a texture: drawn blank
    let last = [chars[64]];
    assert_eq!(
        g.glyph(&mut gl, &font, (&last, Width::Narrow, style), Piece::WHOLE)
            .unwrap(),
        None
    );
    assert_eq!(textures(&gl), made);
    assert_eq!(g.bytes(), ceiling);
    // the first blank glyph gives one note for the log; there is none
    // in the same frame, a frame later, or one frame short of
    // GPU_IDLE_FRAMES, and there is one again at GPU_IDLE_FRAMES
    assert_eq!(g.take_notes().len(), 1);
    let blank_at = |g: &mut Glyphs, gl: &mut StandIn, frames: u64| {
        for _ in 0..frames {
            g.give_back(gl).unwrap();
        }
        let drawn = g.glyph(gl, &font, (&last, Width::Narrow, style), Piece::WHOLE);
        assert_eq!(drawn.unwrap(), None);
        g.take_notes().len()
    };
    assert_eq!(blank_at(&mut g, &mut gl, 0), 0);
    assert_eq!(blank_at(&mut g, &mut gl, 1), 0);
    assert_eq!(blank_at(&mut g, &mut gl, GPU_IDLE_FRAMES - 2), 0);
    assert_eq!(blank_at(&mut g, &mut gl, 1), 1);
    // a head that never takes the notes holds at most one
    for _ in 0..3 {
        for _ in 0..GPU_IDLE_FRAMES {
            g.give_back(&mut gl).unwrap();
        }
        let drawn = g.glyph(&mut gl, &font, (&last, Width::Narrow, style), Piece::WHOLE);
        assert_eq!(drawn.unwrap(), None);
    }
    assert_eq!(g.take_notes().len(), 1);
    g.end_frame();
    // a glyph forgotten gives its slot back, which the 65th then takes
    g.forget(&['A']);
    g.begin_frame();
    assert!(
        g.glyph(&mut gl, &font, (&last, Width::Narrow, style), Piece::WHOLE)
            .unwrap()
            .is_some()
    );
    assert_eq!(textures(&gl), made);
}
