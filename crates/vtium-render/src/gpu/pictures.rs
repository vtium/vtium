//! Slot 1, pictures at another size (ADR-1303 D2).
//!
//! Two steps fill the slot, and both give the composition textures it
//! draws a picture from at its drawn size:
//!
//! - **GPU scaling**, the default: the texture holds the picture's own
//!   pixels, the part of the image its placement shows, uploaded once and
//!   kept by the image, the animation frame and the pixel revision. The
//!   picture shader scales them as it draws, with the CPU path's own filter
//!   in whole numbers (`glsl/picture.frag`), so its pixels are the CPU
//!   path's byte for byte.
//! - **The CPU copy**: the texture holds the CPU path's scaled copy,
//!   upright at the drawn size (`scale::turned_copy`), read one texel a
//!   pixel. Tests choose it with [`PictureStep::Copy`].
//!
//! A picture wider or taller than the largest texture is held in tiles
//! made by the tiler, overlapping by twice the filter's reach, and each
//! drawn pixel reads the tile that owns the source pixel under its centre.
//!
//! The textures count against ADR-1100 D5's bound for the display, the
//! bound the CPU path's copies keep, under the CPU path's eviction rule
//! ([`Copies`]). Where the bound has no room for a picture's own pixels, the
//! picture is drawn from the CPU copy, kept while it has room; where that
//! has no room either, the part the frame draws is scaled on the CPU for
//! that frame alone into the display's one scratch texture, as the CPU path
//! scales such a picture for each frame. A texture no frame placed for
//! [`GPU_IDLE_FRAMES`] frames is given back, and [`Pictures::release`]
//! gives back every one.

use vtium_image::Pixels;

use super::compose::target_size;
use super::gl::{
    Blend, Draw, DrawTarget, Error, ErrorKind, Filter, Format, Gl, Program, Quad, Size, Texture,
    Value,
};
use super::glyphs::rgba;
use super::tile::{Tile, meet, tiles};
use crate::bounds::{GPU_IDLE_FRAMES, PICTURE_CACHE_SCREENS};
use crate::picture::copies::{Copies, Name};
use crate::picture::scale::{self, Taps};
use crate::picture::{Key, PictureCounts};
use crate::target::{Orientation, Rect};

/// Which step draws a picture at another size (ADR-1303 D2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PictureStep {
    /// The GPU scales the picture's own pixels.
    #[default]
    Scale,
    /// The GPU draws the CPU path's scaled copy one texel a pixel, so the
    /// picture's pixels are the CPU path's by construction.
    Copy,
}

/// A picture of the frame as the composition draws it, in the upright
/// frame's pixels.
#[derive(Clone, Debug)]
pub(crate) struct Shown {
    /// What names its pixels, the crop it shows and its drawn size.
    pub(crate) key: Key,
    pub(crate) pixels: Pixels,
    /// The whole drawn picture's top left.
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) opaque: bool,
    /// It draws above the text (ADR-1100 D4).
    pub(crate) above: bool,
    /// Where this frame draws it: the parts of the cells it reaches that
    /// the frame draws.
    pub(crate) rects: Vec<Rect>,
}

/// What a texture holds: a picture's crop of `image_size` at `size`, its
/// own size for its own pixels or the drawn size for a CPU copy, in tiles
/// overlapping by `overlap`. Its image, frame and pixel revision come
/// first, so a frame's older revisions are one range of keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TexKey {
    image: u64,
    frame: u32,
    pixel_revision: u64,
    image_size: (u32, u32),
    src: (u32, u32, u32, u32),
    size: (u32, u32),
    overlap: u32,
}

impl Name for TexKey {
    fn older(&self) -> (TexKey, TexKey) {
        let first = TexKey {
            image: self.image,
            frame: self.frame,
            pixel_revision: 0,
            image_size: (0, 0),
            src: (0, 0, 0, 0),
            size: (0, 0),
            overlap: 0,
        };
        let end = TexKey {
            pixel_revision: self.pixel_revision,
            ..first
        };
        (first, end)
    }
}

impl TexKey {
    fn of(key: &Key, size: (u32, u32), overlap: u32) -> TexKey {
        TexKey {
            image: key.image,
            frame: key.frame,
            pixel_revision: key.pixel_revision,
            image_size: key.image_size,
            src: key.src,
            size,
            overlap,
        }
    }

    /// The texture of the picture's own pixels: its crop at its own size,
    /// overlapping by twice the reach of the filter that draws it at
    /// `drawn`.
    fn own(key: &Key) -> TexKey {
        let (_, _, sw, sh) = key.src;
        let (w, h) = key.size;
        let overlap = reach(sw, w).max(reach(sh, h)).saturating_mul(2);
        TexKey::of(key, (sw, sh), overlap)
    }

    /// The texture of the CPU path's copy at the drawn size, which is read
    /// one texel a pixel and needs no overlap. A picture drawn at its own
    /// size has one key for both.
    fn copy(key: &Key) -> TexKey {
        TexKey::of(key, key.size, 0)
    }
}

/// How many source pixels past the one under a drawn pixel's centre the
/// CPU path's filter reads, on either side, for `s` source pixels drawn as
/// `d` (`Taps::new`): `s / d + 1` where the picture shrinks, 1 where it
/// grows, none at its own size.
fn reach(s: u32, d: u32) -> u32 {
    match s.cmp(&d) {
        core::cmp::Ordering::Greater => s.checked_div(d).unwrap_or(0).saturating_add(1),
        core::cmp::Ordering::Less => 1,
        core::cmp::Ordering::Equal => 0,
    }
}

/// Whether the picture shader's 32-bit arithmetic holds `s` source pixels
/// drawn as `d` exactly: its largest product is about `2 * (s + 1) * d`,
/// and a weight times a drawn side must stay under 2^32. A picture past it
/// is drawn from the CPU copy.
fn shader_holds(s: u32, d: u32) -> bool {
    let (s, d) = (u64::from(s), u64::from(d));
    s.saturating_add(1)
        .saturating_mul(d)
        .saturating_mul(2)
        .saturating_add(d.saturating_mul(2))
        < 1 << 32
        && d < 1 << 15
        && s < 1 << 16
}

/// The first output of `d` whose centre lies on a source pixel at or past
/// `a`, of `s`: the drawn pixels a tile that owns the source from `a` draws
/// begin there.
fn drawn_from(a: u32, s: u32, d: u32) -> u32 {
    let (a, s, d) = (u64::from(a), u64::from(s), u64::from(d));
    let num = d.saturating_mul(2).saturating_mul(a);
    let o = if num <= s {
        0
    } else {
        num.saturating_sub(s).div_ceil(s.saturating_mul(2).max(1))
    };
    u32::try_from(o.min(d)).unwrap_or(u32::MAX)
}

/// How a shown picture draws this frame.
#[derive(Clone, Copy, Debug)]
enum Ready {
    /// It draws nothing this frame.
    Idle,
    /// From the kept textures of the key.
    Kept(TexKey),
    /// Scaled on the CPU into the scratch texture, for this frame alone.
    Scratch,
}

/// One picture's textures, each a tile of what it holds.
type Tiles = Vec<(Tile, Texture)>;

/// Slot 1 for one display.
#[derive(Debug)]
pub(crate) struct Pictures {
    step: PictureStep,
    cache: Copies<TexKey, Tiles>,
    /// The display's one scratch texture and its size, made the first time
    /// a picture needs it.
    scratch: Option<(Texture, Size)>,
    /// How each of the frame's pictures draws.
    ready: Vec<Ready>,
    /// The largest texture side the pictures use, when a test sets it
    /// smaller than the GPU's.
    largest: Option<u32>,
}

impl Pictures {
    pub(crate) fn new() -> Pictures {
        Pictures {
            step: PictureStep::default(),
            cache: Copies::new(0),
            scratch: None,
            ready: Vec::new(),
            largest: None,
        }
    }

    pub(crate) fn set_step(&mut self, step: PictureStep) {
        self.step = step;
    }

    pub(crate) fn set_largest(&mut self, side: Option<u32>) {
        self.largest = side;
    }

    pub(crate) fn counts(&self) -> PictureCounts {
        self.cache.counts()
    }

    fn largest<G: Gl>(&self, gl: &G) -> u32 {
        let t = gl.limits().texture;
        self.largest.map_or(t, |l| l.min(t))
    }

    /// Frees the textures the cache let go. Gives how many calls it made.
    fn free_gone<G: Gl>(&mut self, gl: &mut G) -> Result<usize, Error> {
        let mut calls = 0usize;
        let mut out = Ok(());
        for tiles in self.cache.take_gone() {
            for (_, t) in tiles {
                out = out.and(gl.free_texture(t));
                calls = calls.saturating_add(1);
            }
        }
        out.map(|()| calls)
    }

    /// Makes ready the textures `shown`, the frame's pictures in draw
    /// order, need, on a display whose upright frame is `frame`: every one
    /// the frame shows is placed, so none is evicted for another, and each
    /// one the frame draws is uploaded where it is not kept. Gives how many
    /// calls of the frame's batch of uploads that took.
    pub(crate) fn prepare<G: Gl>(
        &mut self,
        gl: &mut G,
        shown: &[Shown],
        frame: Size,
    ) -> Result<usize, Error> {
        let screen = u64::from(frame.width)
            .saturating_mul(u64::from(frame.height))
            .saturating_mul(4);
        let screens = u64::try_from(PICTURE_CACHE_SCREENS).unwrap_or(u64::MAX);
        self.cache.set_limit(screen.saturating_mul(screens));
        self.cache.begin_frame();
        self.cache.idle(GPU_IDLE_FRAMES);
        let mut calls = self.free_gone(gl)?;
        self.ready.clear();
        for s in shown {
            self.cache.place(&TexKey::own(&s.key));
            self.cache.place(&TexKey::copy(&s.key));
        }
        let largest = self.largest(gl);
        for s in shown {
            let ready = if s.rects.is_empty() {
                Ready::Idle
            } else {
                self.make_ready(gl, s, largest, &mut calls)?
            };
            self.ready.push(ready);
        }
        let scratch = self.ready.iter().any(|r| matches!(r, Ready::Scratch));
        if scratch && self.scratch.is_none() {
            let size = Size::new(frame.width.min(largest), frame.height.min(largest));
            let texture = gl.create_texture(Format::Rgba8, size)?;
            calls = calls.saturating_add(1);
            self.scratch = Some((texture, size));
        }
        Ok(calls)
    }

    /// The textures `s` draws from: kept ones, uploaded now where the bound
    /// has room, the picture's own pixels first under GPU scaling, then
    /// its CPU copy; or the scratch texture.
    fn make_ready<G: Gl>(
        &mut self,
        gl: &mut G,
        s: &Shown,
        largest: u32,
        calls: &mut usize,
    ) -> Result<Ready, Error> {
        let (_, _, sw, sh) = s.key.src;
        let (w, h) = s.key.size;
        let own = (self.step == PictureStep::Scale && shader_holds(sw, w) && shader_holds(sh, h))
            .then(|| TexKey::own(&s.key));
        let keys: Vec<TexKey> = own.into_iter().chain([TexKey::copy(&s.key)]).collect();
        if let Some(&k) = keys.iter().find(|k| self.cache.get(k).is_some()) {
            return Ok(Ready::Kept(k));
        }
        let side = Size::new(largest, largest);
        for k in keys {
            let Some(layout) = tiles(Size::new(k.size.0, k.size.1), side, k.overlap) else {
                continue;
            };
            let bytes = layout.iter().fold(0u64, |n, t| {
                n.saturating_add(t.held.area().saturating_mul(4))
            });
            let room = self.cache.room(&k, bytes);
            *calls = calls.saturating_add(self.free_gone(gl)?);
            if !room {
                continue;
            }
            let made = upload(gl, s, &k, layout)?;
            *calls = calls.saturating_add(made.len().saturating_mul(2));
            self.cache.keep(k, made, bytes);
            return Ok(Ready::Kept(k));
        }
        self.cache.uncached();
        Ok(Ready::Scratch)
    }

    /// Draws picture `i` of the frame, `s`, into the upright frame's
    /// `tiles` through `program`, the picture shader: from each of its
    /// textures, the drawn pixels whose source pixel that texture owns.
    pub(crate) fn draw<G: Gl>(
        &self,
        gl: &mut G,
        i: usize,
        s: &Shown,
        frame: &[(Tile, DrawTarget)],
        program: &Program,
    ) -> Result<(), Error> {
        let ready = self.ready.get(i).copied().unwrap_or(Ready::Idle);
        let (w, h) = s.key.size;
        match ready {
            Ready::Idle => Ok(()),
            Ready::Kept(k) => {
                let held = self
                    .cache
                    .get(&k)
                    .ok_or_else(|| Error::new(ErrorKind::Misuse, "a picture's texture is gone"))?;
                let (sw, sh) = k.size;
                for (tile, texture) in held {
                    let o = tile.owned;
                    let x0 = drawn_from(o.x, sw, w);
                    let x1 = drawn_from(o.x.saturating_add(o.width), sw, w);
                    let y0 = drawn_from(o.y, sh, h);
                    let y1 = drawn_from(o.y.saturating_add(o.height), sh, h);
                    let part = (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0));
                    let pass = Pass {
                        texture,
                        scale: [sw, sh, w, h],
                        first: (tile.held.x, tile.held.y),
                    };
                    draw_part(gl, s, part, &pass, frame, program)?;
                }
                Ok(())
            }
            Ready::Scratch => self.draw_scratch(gl, s, frame, program),
        }
    }

    /// Draws `s` from the scratch texture: the part of it the frame draws,
    /// scaled on the CPU a piece the scratch's size at a time, each piece
    /// drawn before the next is uploaded.
    fn draw_scratch<G: Gl>(
        &self,
        gl: &mut G,
        s: &Shown,
        frame: &[(Tile, DrawTarget)],
        program: &Program,
    ) -> Result<(), Error> {
        let Some((texture, side)) = &self.scratch else {
            return Err(Error::new(ErrorKind::Misuse, "no scratch texture"));
        };
        let (w, h) = s.key.size;
        let whole = Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        // the part the frame draws, in the picture's pixels
        let Some(part) = s
            .rects
            .iter()
            .filter_map(|r| in_picture(*r, s.x, s.y).and_then(|r| meet(r, whole)))
            .reduce(bounds)
        else {
            return Ok(());
        };
        let Some(pieces) = tiles(Size::new(part.width, part.height), *side, 0) else {
            return Ok(());
        };
        let (iw, _) = s.key.image_size;
        let (cx, cy, sw, sh) = s.key.src;
        let src = scale::Source {
            pixels: s.pixels.as_slice(),
            stride: usize::try_from(iw).unwrap_or(0),
            x: cx,
            y: cy,
        };
        for piece in pieces {
            let p = Rect {
                x: part.x.saturating_add(piece.held.x),
                y: part.y.saturating_add(piece.held.y),
                ..piece.held
            };
            let (u1, v1) = (p.x.saturating_add(p.width), p.y.saturating_add(p.height));
            let tx = Taps::new(sw, w, p.x, u1);
            let ty = Taps::new(sh, h, p.y, v1);
            let mut bytes =
                Vec::with_capacity(usize::try_from(p.area().saturating_mul(4)).unwrap_or(0));
            for v in p.y..v1 {
                for u in p.x..u1 {
                    bytes.extend_from_slice(&rgba(scale::eval(&src, &tx, &ty, u, v)));
                }
            }
            let at = Rect {
                x: 0,
                y: 0,
                width: p.width,
                height: p.height,
            };
            let stride = usize::try_from(p.width).unwrap_or(0).saturating_mul(4);
            gl.fill_texture(texture, at, &bytes, stride)?;
            let pass = Pass {
                texture,
                scale: [w, h, w, h],
                first: (p.x, p.y),
            };
            draw_part(gl, s, (p.x, p.y, p.width, p.height), &pass, frame, program)?;
        }
        Ok(())
    }

    /// Gives back every picture texture and the scratch texture.
    pub(crate) fn release<G: Gl>(&mut self, gl: &mut G) -> Result<(), Error> {
        self.cache.clear();
        self.ready.clear();
        let mut out = self.free_gone(gl).map(|_| ());
        if let Some((t, _)) = self.scratch.take() {
            out = out.and(gl.free_texture(t));
        }
        out
    }
}

/// What one draw of a picture samples: a texture, the source and drawn
/// sizes the shader scales between, and the source pixel its first texel
/// holds.
struct Pass<'a> {
    texture: &'a Texture,
    scale: [u32; 4],
    first: (u32, u32),
}

/// `r`, in the upright frame, in the pixels of a picture whose top left is
/// at `x`, `y`: the part of it at or right of and below the picture's top
/// left.
fn in_picture(r: Rect, x: i64, y: i64) -> Option<Rect> {
    let x0 = i64::from(r.x).saturating_sub(x);
    let y0 = i64::from(r.y).saturating_sub(y);
    let x1 = x0.saturating_add(i64::from(r.width));
    let y1 = y0.saturating_add(i64::from(r.height));
    let (x0, y0) = (x0.max(0), y0.max(0));
    let to = |v: i64| u32::try_from(v).ok();
    (x1 > x0 && y1 > y0).then_some(())?;
    Some(Rect {
        x: to(x0)?,
        y: to(y0)?,
        width: to(x1.saturating_sub(x0))?,
        height: to(y1.saturating_sub(y0))?,
    })
}

/// The smallest rectangle holding both.
fn bounds(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

fn side(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// Draws the part `(x, y, width, height)` of picture `s`, in its own drawn
/// pixels, where the frame draws it, through `pass`, into each tile of the
/// upright frame it meets.
fn draw_part<G: Gl>(
    gl: &mut G,
    s: &Shown,
    (px, py, pw, ph): (u32, u32, u32, u32),
    pass: &Pass<'_>,
    frame: &[(Tile, DrawTarget)],
    program: &Program,
) -> Result<(), Error> {
    if pw == 0 || ph == 0 {
        return Ok(());
    }
    // the part in the upright frame, cut to the frame's top left
    let Some(part) = shift(px, py, pw, ph, s.x, s.y) else {
        return Ok(());
    };
    let rects: Vec<Rect> = s.rects.iter().filter_map(|&r| meet(r, part)).collect();
    if rects.is_empty() {
        return Ok(());
    }
    let opaque = u32::from(s.opaque);
    let first = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    for (tile, target) in frame {
        let (tx, ty) = (i64::from(tile.held.x), i64::from(tile.held.y));
        let quads: Vec<Quad> = rects
            .iter()
            .filter(|&&r| meet(r, tile.held).is_some())
            .map(|r| Quad {
                dst: [
                    side(i64::from(r.x).saturating_sub(tx)),
                    side(i64::from(r.y).saturating_sub(ty)),
                    side(i64::from(r.width)),
                    side(i64::from(r.height)),
                ],
                src: [0; 4],
                params: [opaque, 0, 0, 0],
            })
            .collect();
        if quads.is_empty() {
            continue;
        }
        gl.draw(&Draw {
            target,
            program,
            inputs: &[
                ("target_size", target_size(target)),
                ("picture", Value::Texture(pass.texture, Filter::Nearest)),
                ("scale", Value::UVec4(pass.scale)),
                (
                    "origin",
                    Value::IVec4([
                        side(s.x.saturating_sub(tx)),
                        side(s.y.saturating_sub(ty)),
                        first(pass.first.0),
                        first(pass.first.1),
                    ]),
                ),
            ],
            quads: &quads,
            blend: if s.opaque {
                Blend::Replace
            } else {
                Blend::Over
            },
            scissor: None,
        })?;
    }
    Ok(())
}

/// The rectangle `x`, `y`, `w`, `h` moved by `dx`, `dy`, the part of it at
/// or right of and below the origin.
fn shift(x: u32, y: u32, w: u32, h: u32, dx: i64, dy: i64) -> Option<Rect> {
    let x0 = i64::from(x).saturating_add(dx);
    let y0 = i64::from(y).saturating_add(dy);
    let x1 = x0.saturating_add(i64::from(w));
    let y1 = y0.saturating_add(i64::from(h));
    let (x0, y0) = (x0.max(0), y0.max(0));
    let to = |v: i64| u32::try_from(v).ok();
    (x1 > x0 && y1 > y0).then_some(())?;
    Some(Rect {
        x: to(x0)?,
        y: to(y0)?,
        width: to(x1.saturating_sub(x0))?,
        height: to(y1.saturating_sub(y0))?,
    })
}

/// Makes and fills `key`'s textures for `s`, one for each tile of
/// `layout`: the picture's own pixels, or the CPU path's copy at the drawn
/// size. A call that fails frees what it made.
fn upload<G: Gl>(gl: &mut G, s: &Shown, key: &TexKey, layout: Vec<Tile>) -> Result<Tiles, Error> {
    let (iw, _) = s.key.image_size;
    let (cx, cy, sw, sh) = s.key.src;
    let image = scale::Source {
        pixels: s.pixels.as_slice(),
        stride: usize::try_from(iw).unwrap_or(0),
        x: cx,
        y: cy,
    };
    // the CPU copy, upright at the drawn size, where the texture holds it
    let copy = (key.size != (sw, sh))
        .then(|| scale::turned_copy(&image, (sw, sh), key.size, Orientation::Normal));
    let (pixels, stride, x0, y0) = match &copy {
        Some(c) => (c.as_slice(), key.size.0, 0, 0),
        None => (s.pixels.as_slice(), iw, cx, cy),
    };
    let mut made: Tiles = Vec::with_capacity(layout.len());
    for tile in layout {
        let r = tile.held;
        let mut bytes =
            Vec::with_capacity(usize::try_from(r.area().saturating_mul(4)).unwrap_or(0));
        for y in r.y..r.y.saturating_add(r.height) {
            let row =
                usize::try_from(u64::from(y0.saturating_add(y)).saturating_mul(u64::from(stride)))
                    .unwrap_or(usize::MAX)
                    .saturating_add(usize::try_from(x0.saturating_add(r.x)).unwrap_or(usize::MAX));
            let line = pixels
                .get(row..row.saturating_add(usize::try_from(r.width).unwrap_or(0)))
                .unwrap_or_default();
            for &p in line {
                bytes.extend_from_slice(&rgba(p));
            }
        }
        let size = Size::new(r.width, r.height);
        let texture = match gl.create_texture(Format::Rgba8, size) {
            Ok(t) => t,
            Err(e) => {
                free(gl, made);
                return Err(e);
            }
        };
        let stride = usize::try_from(r.width).unwrap_or(0).saturating_mul(4);
        if let Err(e) = gl.fill_texture(&texture, size.rect(), &bytes, stride) {
            let _ = gl.free_texture(texture);
            free(gl, made);
            return Err(e);
        }
        made.push((tile, texture));
    }
    Ok(made)
}

/// Frees `tiles`, after an error that matters more than theirs.
fn free<G: Gl>(gl: &mut G, tiles: Tiles) {
    for (_, t) in tiles {
        let _ = gl.free_texture(t);
    }
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// The source pixel under drawn pixel `o`'s centre, as the CPU path's
    /// filter centres its taps.
    fn under(o: u32, s: u32, d: u32) -> u32 {
        ((2 * u64::from(o) + 1) * u64::from(s) / (2 * u64::from(d))) as u32
    }

    // each drawn pixel is drawn from the tile that owns the source pixel
    // under its centre, so the tiles' drawn parts cover the picture once
    #[test]
    fn each_drawn_pixel_is_drawn_from_the_tile_that_owns_its_centre() {
        for (s, d) in [(100, 37), (37, 100), (64, 64), (1000, 9), (7, 3000)] {
            for a in 0..=s {
                let from = drawn_from(a, s, d);
                for o in 0..d {
                    assert_eq!(
                        o >= from,
                        under(o, s, d) >= a,
                        "{s}->{d} at {a}, output {o}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_overlap_is_twice_the_filters_reach() {
        assert_eq!(reach(300, 100), 4);
        assert_eq!(reach(100, 300), 1);
        assert_eq!(reach(100, 100), 0);
        let key = Key {
            image: 1,
            frame: 1,
            pixel_revision: 1,
            image_size: (900, 400),
            src: (0, 0, 900, 400),
            size: (200, 800),
            turn: 0,
        };
        // 900 across shrinks to 200: a reach of 5; 400 down grows: 1
        assert_eq!(TexKey::own(&key).overlap, 10);
        assert_eq!(TexKey::copy(&key).overlap, 0);
    }
}
