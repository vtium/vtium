//! The GPU path's pictures on the stand-in GL (ADR-1303 D2, D9): each
//! picture's textures uploaded once and kept, the CPU copy step, the one
//! bound both paths share with its eviction and its fallbacks to the CPU
//! copy and the scratch texture, tiles past the largest texture, the
//! layers of a cell that pictures reach, the textures given back when idle,
//! and the release call. The stand-in draws no pixels; tier 2 judges the
//! pixels on llvmpipe against the CPU path's.
#![cfg(all(feature = "gpu", feature = "images"))]

mod common;

use common::{Screen, rgb, unifont};
use vtium_font::Unifont;
use vtium_image::{FramePlacement, LineKey, NewImage, Place, Protocol, Rows, Screens, Store};
use vtium_render::gpu::{
    Blend, Call, Config as GpuConfig, DrawTarget, Format, Gl, PictureStep, Pipeline, Quad, Size,
    StandIn, tiles,
};
use vtium_render::{Config, DrawState, GPU_IDLE_FRAMES, Rect, Renderer};
use vtium_term::Age;

/// 20 by 6 cells of 8x16, the grid filling the display: the pictures'
/// bound is two screens, 122,880 bytes.
const DISPLAY: Size = Size::new(160, 96);
const COLS: usize = 20;
const ROWS: usize = 6;

const PICTURE_INPUTS: [&str; 4] = ["target_size", "picture", "scale", "origin"];
const CELL_INPUTS: [&str; 5] = ["target_size", "masks", "colors", "sprites", "cell"];
const TINT_INPUTS: [&str; 2] = ["target_size", "under"];

/// A picture: its kitty id, its size and where it goes.
struct Pic {
    id: u32,
    size: (u32, u32),
    place: Place,
    z: i32,
}

fn pic(id: u32, size: (u32, u32), (col, row): (u32, u32), (cols, rows): (u32, u32)) -> Pic {
    Pic {
        id,
        size,
        place: Place {
            col,
            row,
            cols,
            rows,
            ..Place::default()
        },
        z: 1,
    }
}

/// The frame's placements of `pictures`, each opaque.
fn placements(pictures: &[Pic]) -> Vec<FramePlacement> {
    let keys: Vec<LineKey> = (1..=ROWS as u64).map(LineKey::new).collect();
    let view = Rows {
        screen: &keys,
        above: &[],
    };
    let screens = Screens {
        main: view,
        alt: Rows::default(),
    };
    let mut store = Store::new();
    store.set_limit(1 << 28);
    for p in pictures {
        let (w, h) = p.size;
        let pixels = (0..h)
            .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | (x * 7) << 16 | (y * 5) << 8 | x ^ y))
            .collect();
        let new = NewImage {
            protocol: Protocol::KITTY,
            width: w,
            height: h,
            pixels,
            id: p.id,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = store.add_image(new, None, screens).unwrap();
        let place = Place { z: p.z, ..p.place };
        store.place(image, place, screens, COLS).unwrap();
    }
    store.frame(view, COLS)
}

/// A display on the stand-in GL.
struct Gpu {
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    renderer: Renderer<Unifont>,
    screen: Screen,
    since: Age,
}

impl Gpu {
    fn new() -> Gpu {
        Gpu::with_limits(StandIn::LIMITS)
    }

    fn with_limits(limits: vtium_render::gpu::Limits) -> Gpu {
        let mut gl = StandIn::with_limits(limits);
        let pipeline = Pipeline::new(&mut gl, DISPLAY, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&DISPLAY).unwrap();
        Gpu {
            gl,
            pipeline,
            buffer,
            renderer: Renderer::new(unifont(), DISPLAY.width, DISPLAY.height, Config::default()),
            screen: Screen::new(COLS, ROWS, rgb("c0c0c0"), rgb("000000")),
            since: Age::NEVER,
        }
    }

    /// One frame showing `placements`, drawn whole when `whole`; the
    /// calls it made are left in the stand-in.
    fn frame(&mut self, placements: &[FramePlacement], whole: bool) {
        if whole {
            self.pipeline.redraw_whole();
        }
        self.gl.clear_calls();
        let frame = self
            .screen
            .frame(self.since)
            .with_placements(placements.to_vec());
        self.renderer
            .draw_gpu(
                &mut self.gl,
                &mut self.pipeline,
                &frame,
                &self.buffer,
                1,
                &DrawState::default(),
                &[],
            )
            .unwrap();
        self.since = self.screen.damage.checkpoint();
    }

    fn calls(&self) -> &[Call] {
        self.gl.calls()
    }

    /// The sizes of the RGBA8 textures the frame made.
    fn made(&self) -> Vec<Size> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::CreateTexture(Format::Rgba8, s) => Some(*s),
                _ => None,
            })
            .collect()
    }

    fn freed(&self) -> usize {
        self.calls()
            .iter()
            .filter(|c| matches!(c, Call::FreeTexture))
            .count()
    }

    /// Each draw of the frame through the program of `inputs`: its quads
    /// and its blend, in order.
    fn draws(&self, inputs: &[&str]) -> Vec<(Vec<Quad>, Blend)> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::Draw {
                    quads,
                    blend,
                    inputs: i,
                } if i == inputs => Some((quads.clone(), *blend)),
                _ => None,
            })
            .collect()
    }

    fn fills(&self) -> usize {
        self.calls()
            .iter()
            .filter(|c| matches!(c, Call::FillTexture(_)))
            .count()
    }
}

/// How many times the quads cover each pixel of the display.
fn cover(quads: &[Quad]) -> Vec<u32> {
    let mut n = vec![0u32; (DISPLAY.width * DISPLAY.height) as usize];
    for q in quads {
        for y in q.dst[1]..q.dst[1] + q.dst[3] {
            for x in q.dst[0]..q.dst[0] + q.dst[2] {
                n[(y as u32 * DISPLAY.width + x as u32) as usize] += 1;
            }
        }
    }
    n
}

// ADR-1303 D2: GPU scaling uploads the picture's own pixels once, keyed
// by the image, the frame and the pixel revision, and keeps them
#[test]
fn a_picture_at_another_size_is_uploaded_once_at_its_own_size_and_kept() {
    let mut g = Gpu::new();
    // 48x48 drawn over 4 by 2 cells, 32x32: shrunk by 1.5
    let p = placements(&[pic(1, (48, 48), (2, 1), (4, 2))]);
    g.frame(&p, false);
    assert_eq!(g.made(), [Size::new(48, 48)]);
    let draws = g.draws(&PICTURE_INPUTS);
    assert_eq!(draws.len(), 1);
    // opaque, it replaces what lies beneath, over its 32x32
    assert_eq!(draws[0].1, Blend::Replace);
    let n = cover(&draws[0].0);
    assert_eq!(n.iter().sum::<u32>(), 32 * 32);
    assert_eq!(n[(16 * 160 + 16) as usize], 1);
    // drawn whole again, nothing is uploaded
    g.frame(&p, true);
    assert_eq!(g.made(), []);
    assert_eq!(g.draws(&PICTURE_INPUTS).len(), 1);
    let c = g.pipeline.picture_counts();
    assert_eq!(
        (c.kept, c.uncached, c.copies, c.bytes),
        (1, 0, 1, 48 * 48 * 4)
    );
}

// ADR-1303 D2: the CPU copy step uploads the CPU path's copy at the drawn
// size, read one texel a pixel
#[test]
fn the_copy_step_uploads_the_cpu_paths_copy_at_the_drawn_size() {
    let mut g = Gpu::new();
    g.pipeline.set_picture_step(PictureStep::Copy);
    let p = placements(&[pic(1, (48, 48), (2, 1), (4, 2))]);
    g.frame(&p, false);
    assert_eq!(g.made(), [Size::new(32, 32)]);
    g.frame(&p, true);
    assert_eq!(g.made(), []);
}

// ADR-1303 D2: where the bound has no room for a picture's own pixels, it
// is drawn from the CPU copy at its drawn size, kept, so the next frame
// that draws it uploads nothing
#[test]
fn a_picture_whose_own_pixels_pass_the_bound_is_drawn_from_its_kept_copy() {
    let mut g = Gpu::new();
    // 160,000 bytes of its own, past the bound of 122,880; drawn 32x32
    let p = placements(&[pic(1, (200, 200), (2, 1), (4, 2))]);
    g.frame(&p, false);
    assert_eq!(g.made(), [Size::new(32, 32)]);
    g.frame(&p, true);
    assert_eq!(g.made(), []);
    let c = g.pipeline.picture_counts();
    assert_eq!((c.kept, c.uncached, c.bytes), (1, 0, 32 * 32 * 4));
}

// ADR-1303 D2: where the drawn copy has no room either, the part the frame
// draws is scaled for that frame alone into the display's one scratch
// texture, which every such picture shares
#[test]
fn pictures_whose_copies_pass_the_bound_share_one_scratch_texture() {
    let mut g = Gpu::new();
    // each drawn over 40 by 12 cells, 320x192 and 245,760 bytes
    let p = placements(&[
        pic(1, (200, 200), (0, 0), (40, 12)),
        pic(2, (210, 210), (1, 1), (40, 12)),
    ]);
    g.frame(&p, false);
    // one scratch texture, the upright frame's size
    assert_eq!(g.made(), [DISPLAY]);
    let first = g.fills();
    assert!(first >= 2, "each picture filled the scratch: {first}");
    g.frame(&p, true);
    assert_eq!(g.made(), []);
    assert!(g.fills() >= 2);
    let c = g.pipeline.picture_counts();
    assert_eq!((c.kept, c.uncached, c.copies), (0, 4, 0));
}

// ADR-1303 D2, ADR-1100 D5: the cache makes room as the CPU path's does,
// evicting what no placement of the frame shows, least recently placed
// first, and frees each evicted texture in GL; a texture the frame shows
// is never evicted
#[test]
fn eviction_frees_the_least_recently_placed_texture_the_frame_does_not_show() {
    let mut g = Gpu::new();
    // 40,000 bytes each of their own
    let a = pic(1, (100, 100), (0, 0), (4, 2));
    let b = pic(2, (100, 100), (5, 0), (4, 2));
    // 57,600: room once one of the others goes
    let c = pic(3, (120, 120), (10, 0), (4, 2));
    let all = placements(&[a, b, c]);
    let (pa, pb, pc) = (all[0].clone(), all[1].clone(), all[2].clone());
    g.frame(&[pa.clone(), pb.clone()], false);
    assert_eq!(g.made().len(), 2);
    // b shown again with c, so a goes, not b
    g.frame(&[pb.clone(), pc.clone()], true);
    assert_eq!(g.made(), [Size::new(120, 120)]);
    assert_eq!(g.freed(), 1);
    assert_eq!(g.pipeline.picture_counts().evicted, 1);
    g.frame(&[pb], true);
    assert_eq!(g.made(), [], "b was kept");
    g.frame(&[pa], true);
    assert_eq!(g.made(), [Size::new(100, 100)], "a was evicted");
}

// ADR-1303 D2: a picture past the largest texture is held in tiles that
// overlap by twice the filter's reach, and each drawn pixel is drawn once,
// from the tile that owns the source pixel under its centre
#[test]
fn a_picture_past_the_largest_texture_is_tiled_with_twice_the_reach_and_drawn_once() {
    let mut g = Gpu::new();
    g.pipeline.limit_picture_textures(Some(64));
    // 150x100 fitted into 10 by 3 cells, 80x48: drawn 72x48, about 2.1:1,
    // so the filter reaches 3 source pixels and the tiles overlap by 6
    let p = placements(&[pic(1, (150, 100), (1, 1), (10, 3))]);
    g.frame(&p, false);
    let want: Vec<Size> = tiles(Size::new(150, 100), Size::new(64, 64), 6)
        .unwrap()
        .iter()
        .map(|t| Size::new(t.held.width, t.held.height))
        .collect();
    assert!(want.len() >= 4, "{want:?}");
    let mut made = g.made();
    made.sort_by_key(|s| (s.width, s.height));
    let mut want_sorted = want.clone();
    want_sorted.sort_by_key(|s| (s.width, s.height));
    assert_eq!(made, want_sorted);
    let draws = g.draws(&PICTURE_INPUTS);
    assert_eq!(draws.len(), want.len(), "a draw for each tile");
    let quads: Vec<Quad> = draws.into_iter().flat_map(|(q, _)| q).collect();
    let n = cover(&quads);
    assert!(n.iter().all(|&k| k <= 1), "a pixel drawn twice");
    assert_eq!(n.iter().sum::<u32>(), 72 * 48);
}

// ADR-1100 D4: a cell that pictures reach draws its background, the
// pictures below the text, its text layer blended over them, the
// pictures above the text, and the selection's tint, in that order
#[test]
fn a_cell_under_pictures_draws_its_layers_in_order() {
    let mut g = Gpu::new();
    let st = common::style(rgb("ffffff"), rgb("203040"));
    g.screen.text(0, 0, "layers", st);
    let below = Pic {
        z: -5,
        ..pic(1, (48, 32), (0, 0), (6, 2))
    };
    let above = pic(2, (16, 16), (2, 0), (2, 1));
    g.frame(&placements(&[below, above]), false);
    let order: Vec<(&str, Blend)> = g
        .calls()
        .iter()
        .filter_map(|c| match c {
            Call::Draw { inputs, blend, .. } if inputs[..] == PICTURE_INPUTS => {
                Some(("picture", *blend))
            }
            Call::Draw { inputs, blend, .. } if inputs[..] == CELL_INPUTS => Some(("cell", *blend)),
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        [
            ("cell", Blend::Replace),
            ("picture", Blend::Replace),
            ("cell", Blend::Over),
            ("picture", Blend::Replace),
        ]
    );
}

// ADR-1100 D4: the selection tints a cell and its pictures with the CPU
// path's arithmetic, read from a copy of the cell, since no draw reads
// its own target
#[test]
fn a_selected_cell_under_a_picture_is_copied_out_and_tinted_back() {
    let mut g = Gpu::new();
    let p = placements(&[pic(1, (32, 32), (0, 0), (4, 2))]);
    g.frame(&p, false);
    assert!(g.draws(&TINT_INPUTS).is_empty());
    let frame = g
        .screen
        .frame(Age::NEVER)
        .with_selection(std::iter::once(0..2).collect());
    g.pipeline.redraw_whole();
    g.gl.clear_calls();
    g.renderer
        .draw_gpu(
            &mut g.gl,
            &mut g.pipeline,
            &frame.with_placements(p),
            &g.buffer,
            1,
            &DrawState::default(),
            &[],
        )
        .unwrap();
    let tints = g.draws(&TINT_INPUTS);
    assert_eq!(tints.len(), 2, "a copy out and a tint back");
    // the copy's quads copy, the tint's tint at SELECTION_TINT
    assert!(tints[0].0.iter().all(|q| q.params[0] >> 24 == 0));
    assert!(
        tints[1]
            .0
            .iter()
            .all(|q| q.params[0] >> 24 == 1
                && q.params[1] == u32::from(vtium_render::SELECTION_TINT))
    );
    // the copy is the frame's own, freed at its end
    let made = g
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::CreateFrame(_)))
        .count();
    let freed = g
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::FreeTarget))
        .count();
    assert_eq!((made, freed), (1, 1));
}

// the issue on GPU texture garbage collection: a picture no frame placed
// for GPU_IDLE_FRAMES frames gives its textures back, counted in frames
#[test]
fn a_picture_texture_no_frame_shows_is_given_back_after_the_idle_frames() {
    let mut g = Gpu::new();
    let p = placements(&[pic(1, (48, 48), (2, 1), (4, 2))]);
    g.frame(&p, false);
    let live = g.gl.live().0;
    for _ in 1..GPU_IDLE_FRAMES {
        g.frame(&[], false);
        assert_eq!(g.freed(), 0);
    }
    g.frame(&[], false);
    assert_eq!(g.freed(), 1);
    assert_eq!(g.gl.live().0, live - 1);
    assert_eq!(g.pipeline.picture_counts().copies, 0);
}

// the issue on GPU texture garbage collection: a glyph pool gives back
// its last texture once no glyph has held a slot of it for the idle frames
#[test]
fn a_glyph_pools_emptied_texture_is_given_back_after_the_idle_frames() {
    let mut g = Gpu::new();
    let st = common::style(rgb("ffffff"), rgb("000000"));
    // 70 glyphs: a pool's first texture holds 64
    let chars: Vec<char> = ('!'..).take(70).collect();
    for (i, &c) in chars.iter().enumerate() {
        g.screen.put(i % COLS, i / COLS, c, false, st);
    }
    g.frame(&[], false);
    let full = g.pipeline.glyph_texture_bytes().unwrap();
    // the screen now shows one glyph, and the font forgets the rest
    for i in 0..chars.len() {
        g.screen.put(i % COLS, i / COLS, ' ', false, st);
    }
    g.screen.put(0, 0, '!', false, st);
    g.renderer.forget_glyphs(&chars);
    g.frame(&[], false);
    for _ in 1..GPU_IDLE_FRAMES {
        g.frame(&[], false);
        assert_eq!(g.freed(), 0);
    }
    g.frame(&[], false);
    assert_eq!(g.freed(), 1);
    // the second texture held 128 slots of 8x16, in one row
    assert_eq!(g.pipeline.glyph_texture_bytes(), Some(full - 128 * 8 * 16));
}

// the issue on GPU texture garbage collection: the release call frees the
// glyph and picture textures, keeps the programs, and the next frame makes
// them again and draws whole
#[test]
fn a_release_frees_the_glyph_and_picture_textures_and_the_next_frame_draws_whole() {
    let mut g = Gpu::new();
    // the upright frame's texture, before any glyph or picture
    let frame_only = g.gl.live().0;
    let st = common::style(rgb("ffffff"), rgb("000000"));
    g.screen.text(0, 5, "text", st);
    let p = placements(&[pic(1, (48, 48), (2, 1), (4, 2))]);
    g.frame(&p, false);
    let (textures, targets, programs) = g.gl.live();
    assert!(textures > frame_only);
    g.pipeline.release(&mut g.gl).unwrap();
    // the upright frame, the buffer and the programs stay
    assert_eq!(g.gl.live(), (frame_only, targets, programs));
    g.gl.clear_calls();
    let frame = g.screen.frame(g.since).with_placements(p);
    let drawn = g
        .renderer
        .draw_gpu(
            &mut g.gl,
            &mut g.pipeline,
            &frame,
            &g.buffer,
            1,
            &DrawState::default(),
            &[],
        )
        .unwrap();
    assert_eq!(
        drawn.damage.to_vec(),
        [Rect {
            x: 0,
            y: 0,
            width: DISPLAY.width,
            height: DISPLAY.height
        }]
    );
    assert!(!g.calls().iter().any(|c| matches!(c, Call::Compile(_))));
    assert!(g.made().contains(&Size::new(48, 48)));
    assert_eq!(g.gl.live().0, textures);
}

// ADR-1303 D2, D3: on a GL whose largest texture is 64, with no limit of
// the renderer's own, a picture's textures and the upright frame's tiles
// stay within it, and each picture and tint quad draws into its tile in
// the tile's own pixels, every pixel of the frame once
#[test]
fn pictures_and_tints_draw_tile_local_across_an_upright_frame_of_several_tiles() {
    let limits = vtium_render::gpu::Limits {
        texture: 64,
        ..StandIn::LIMITS
    };
    let mut g = Gpu::with_limits(limits);
    // 150x100 fitted into 10 by 3 cells from cell 1, 2: drawn 72x48 at
    // 12, 32, across the tiles' seams at x 64 and y 64
    let p = placements(&[pic(1, (150, 100), (1, 2), (10, 3))]);
    // cells 6 to 9 of rows 3 and 4 selected, across both seams
    let rows: Vec<std::ops::Range<usize>> = (0..ROWS)
        .map(|r| if r == 3 || r == 4 { 6..10 } else { 0..0 })
        .collect();
    let frame = g.screen.frame(Age::NEVER).with_selection(rows);
    g.gl.clear_calls();
    g.renderer
        .draw_gpu(
            &mut g.gl,
            &mut g.pipeline,
            &frame.with_placements(p),
            &g.buffer,
            1,
            &DrawState::default(),
            &[],
        )
        .unwrap();
    let over = |s: &Size| s.width > 64 || s.height > 64;
    assert!(!g.made().iter().any(over), "{:?}", g.made());
    for c in g.calls() {
        if let Call::CreateFrame(s) = c {
            assert!(!over(s), "{s:?}");
        }
    }
    // the upright frame's tiles, in the order the pipeline made them
    let layout = tiles(DISPLAY, Size::new(64, 64), 0).unwrap();
    assert!(layout.len() > 4, "{layout:?}");
    let origin = |name: u32| {
        let k = g.gl.frame_names().iter().position(|&n| n == name)?;
        layout.get(k).map(|t| t.held)
    };
    // each draw of `inputs` into a tile, mapped back into the frame
    let cover = |inputs: &[&str], want: u32| {
        let mut n = vec![0u32; (DISPLAY.width * DISPLAY.height) as usize];
        let draws = g.calls().iter().filter(|c| matches!(c, Call::Draw { .. }));
        for (c, &target) in draws.zip(g.gl.draw_targets()) {
            let Call::Draw {
                quads, inputs: i, ..
            } = c
            else {
                continue;
            };
            let Some(held) = (i[..] == *inputs).then(|| origin(target)).flatten() else {
                continue;
            };
            for q in quads {
                // tile-local: inside the tile's own pixels
                assert!(
                    q.dst[0] >= 0
                        && q.dst[1] >= 0
                        && (q.dst[0] + q.dst[2]) as u32 <= held.width
                        && (q.dst[1] + q.dst[3]) as u32 <= held.height,
                    "{inputs:?} quad {:?} past its tile {held:?}",
                    q.dst
                );
                for y in q.dst[1]..q.dst[1] + q.dst[3] {
                    for x in q.dst[0]..q.dst[0] + q.dst[2] {
                        let (fx, fy) = (x as u32 + held.x, y as u32 + held.y);
                        n[(fy * DISPLAY.width + fx) as usize] += 1;
                    }
                }
            }
        }
        assert!(n.iter().all(|&k| k <= 1), "{inputs:?}: a pixel drawn twice");
        assert_eq!(n.iter().sum::<u32>(), want, "{inputs:?}");
    };
    cover(&PICTURE_INPUTS, 72 * 48);
    // the tint's draws back into the tiles: 2 rows of 4 cells of 8x16
    cover(&TINT_INPUTS, 2 * 4 * 8 * 16);
}
