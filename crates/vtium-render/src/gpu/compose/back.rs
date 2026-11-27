//! The renderer's back end on the GPU path: [`GpuBack`] turns the border's
//! parts into clears and each cell job into one quad through the cell
//! shader.

use vtium_font::{Face, Font};
use vtium_term::Rgb;

use crate::beside::shown_at;
use crate::blend::pixel;
use crate::compose::{Area, Composer, Job};
use crate::deco::{LINES, Line, Sprites, magnification};
use crate::gpu::beside::SHIFT;
use crate::gpu::gl::{Error, Gl, Quad};
use crate::gpu::glyphs::{Glyphs, Place};
use crate::gpu::tile::meet;
use crate::gpu::turn::Turn;
use crate::reach::Reach;
use crate::renderer::Back;
use crate::renderer::double::Piece;
use crate::target::Rect;

use super::composition::dst_rect;
use super::{COLOR, Cells, DOWN, GAP, MARKED, RECEIVES};

/// The renderer's [`Back`] on the GPU path: the border's parts as clears,
/// and each cell job as one quad through the cell shader, its glyph from
/// the pools, uploaded when the pools lack it. A cell that pictures reach
/// is taken apart into ADR-1100 D4's layers: its background as a cell, the
/// parts of each picture over it, its text layer, the selection's tint and
/// vtium's mark. The front end gives each in the display's pixels, and
/// `turn` maps it into the upright frame's.
///
/// The renderer can draw a cell twice in one frame: once with its row, and
/// again once the row below has sent it ink (ADR-1306 D5). The CPU path
/// draws in order, so the second drawing covers the first. The GPU path
/// draws its quads in batches, each batch whole, so the cell's second
/// drawing replaces its first here, in every list, and each cell is drawn
/// once.
pub(crate) struct GpuBack<'a, G: Gl> {
    gl: &'a mut G,
    glyphs: &'a mut Glyphs,
    turn: Turn,
    /// The first grid row the frame has not drawn yet. The renderer draws
    /// rows from the top, so a row above it is a row drawn again.
    next_row: usize,
    pub(crate) clear: Vec<Rect>,
    pub(crate) cells: Vec<Cells>,
    pub(crate) error: Option<Error>,
    /// The pictures meeting the cell being drawn.
    #[cfg(feature = "images")]
    meeting: Vec<usize>,
    /// Where each of the frame's pictures draws, by its place in the
    /// frame's order, in the upright frame.
    #[cfg(feature = "images")]
    pub(crate) picture_rects: Vec<Vec<Rect>>,
    #[cfg(feature = "images")]
    pub(crate) text: Vec<Cells>,
    #[cfg(feature = "images")]
    pub(crate) tints: Vec<(Rect, u32)>,
    #[cfg(feature = "images")]
    pub(crate) marks: Vec<Cells>,
    /// The bars of bar cursors, drawn after everything else.
    pub(crate) bars: Vec<Cells>,
}

impl<'a, G: Gl> GpuBack<'a, G> {
    pub(crate) fn new(gl: &'a mut G, glyphs: &'a mut Glyphs, turn: Turn) -> GpuBack<'a, G> {
        glyphs.begin_frame();
        GpuBack {
            gl,
            glyphs,
            turn,
            next_row: 0,
            clear: Vec::new(),
            cells: Vec::new(),
            error: None,
            #[cfg(feature = "images")]
            meeting: Vec::new(),
            #[cfg(feature = "images")]
            picture_rects: Vec::new(),
            #[cfg(feature = "images")]
            text: Vec::new(),
            #[cfg(feature = "images")]
            tints: Vec::new(),
            #[cfg(feature = "images")]
            marks: Vec::new(),
            bars: Vec::new(),
        }
    }

    /// Forgets the quads and pictures the frame has drawn so far, for a
    /// frame drawn again whole.
    pub(crate) fn restart(&mut self) {
        self.next_row = 0;
        self.cells.clear();
        self.bars.clear();
        #[cfg(feature = "images")]
        {
            self.picture_rects.clear();
            self.text.clear();
            self.tints.clear();
            self.marks.clear();
        }
    }

    /// Takes the cell `job` of grid row `row`, whose glyph lies at
    /// `place`, apart into its layers when pictures reach it, as the CPU
    /// path's `pictured` composes it: no picture under a block cursor, none
    /// that the cell's own background hides, and none hidden beneath the
    /// last opaque picture over the whole cell. Its
    /// text layer takes the ink it receives, as `ink` flags it. Says
    /// whether it did.
    #[cfg(feature = "images")]
    fn pictured(
        &mut self,
        compose: &Composer,
        job: &Job,
        place: Option<Place>,
        (row, ink): (usize, u32),
    ) -> bool {
        use crate::picture::Mark;
        use vtium_image::Layer;
        if job.under.mark == Mark::Opaque {
            return false;
        }
        let pictures = &compose.pictures;
        pictures.meeting(&job.r, &mut self.meeting);
        // A cell with a background color of its own hides each picture
        // below backgrounds. When it hides every picture that meets it, it
        // draws through the cell program alone, as a cell with no picture
        // does, so its text rounds as that cell's text does.
        let shows = |l: Layer| l != Layer::BelowBackground || job.under.default_bg;
        if !self
            .meeting
            .iter()
            .any(|&i| pictures.pic(i).is_some_and(|p| shows(p.layer())))
        {
            return false;
        }
        let top = self.meeting.iter().rposition(|&i| {
            pictures
                .pic(i)
                .is_some_and(|p| p.opaque() && p.covers(&job.r) && shows(p.layer()))
        });
        let (r, g) = (self.turn.up_area(job.r), self.turn.up_area(job.g));
        let bare = Job {
            underline: None,
            over: None,
            ..*job
        };
        let sprites = &compose.sprites;
        let quad = |j: &Job, place: Option<Place>, flags: u32, beside: u32| {
            cell_quad(j, (r, g), (place, beside), (row, flags), sprites)
        };
        push(&mut self.cells, None, quad(&bare, None, 0, 0));
        self.picture_rects
            .resize_with(pictures.pics().len(), Vec::new);
        for &i in self.meeting.get(top.unwrap_or(0)..).unwrap_or_default() {
            let Some(p) = pictures.pic(i).filter(|p| shows(p.layer())) else {
                continue;
            };
            let part = area_rect(self.turn.up_area(meet_area(job.r, p.clip())));
            if let (Some(part), Some(rects)) = (part, self.picture_rects.get_mut(i)) {
                rects.push(part);
            }
        }
        let mark = job.under.mark;
        let text = Job {
            underline: job.underline.filter(|_| mark != Mark::Underline),
            ..*job
        };
        if place.is_some() || text.layered() || ink != 0 {
            let beside = self.beside(&compose.sprites, &text, place);
            let q = quad(&text, place, ink, beside);
            push(&mut self.text, place.map(|p| (p.texture, p.color)), q);
        }
        if job.under.selected
            && let Some(cell) = area_rect(r)
        {
            self.tints.push((cell, pixel(job.bg)));
        }
        if mark == Mark::Underline && job.underline.is_some() {
            let line = Job { over: None, ..*job };
            push(&mut self.marks, None, quad(&line, None, 0, 0));
        }
        true
    }
}

impl<G: Gl> GpuBack<'_, G> {
    /// The entry plus one, for its quad, of the ink beside the piece that
    /// `job` draws at `place`, when its underline skips ink; 0 when the
    /// job draws no such underline, or its piece has nothing beside it on
    /// the rows its underline's sprite in `sprites` covers.
    fn beside(&mut self, sprites: &Sprites, job: &Job, place: Option<Place>) -> u32 {
        let (Some((line, _)), Some(p)) = (job.underline, place) else {
            return 0;
        };
        let Some(sprite) = sprites.piece(line, job.piece).filter(|_| line.skips_ink) else {
            return 0;
        };
        let (o, cell) = (sprites.orientation(), self.glyphs.cell());
        let (w, h) = (i64::from(cell.width), i64::from(cell.height));
        // the sprite is turned as the display is, so each pixel of the
        // upright row is read where the display shows it
        let mut a = [0u8];
        let underlines = |v: usize| {
            let v = i64::try_from(v).unwrap_or(i64::MAX);
            (0..w).any(|u| {
                let (x, y) = shown_at(o, (u, v), (w, h));
                sprite.mask_line(x, y, &mut a);
                a[0] != 0
            })
        };
        self.glyphs.besides.entry(p.slot, underlines)
    }
}

/// Adds `quad` to the batch of `cells` that samples the texture of glyphs
/// `glyphs` and, as the quad's flags say, receives ink or not.
fn push(cells: &mut Vec<Cells>, glyphs: Option<(usize, bool)>, quad: Quad) {
    let ink = quad.params[1] >> 24 & RECEIVES != 0;
    match cells
        .iter_mut()
        .find(|c| c.glyphs == glyphs && c.ink == ink)
    {
        Some(c) => c.quads.push(quad),
        None => cells.push(Cells {
            glyphs,
            ink,
            quads: vec![quad],
        }),
    }
}

/// How many quads each batch of each of a frame's lists held, and how many
/// parts of each picture and tints, before the frame drew a row again: what
/// it holds past these counts the row's second drawing pushed.
struct Held {
    cells: Vec<usize>,
    bars: Vec<usize>,
    #[cfg(feature = "images")]
    text: Vec<usize>,
    #[cfg(feature = "images")]
    marks: Vec<usize>,
    #[cfg(feature = "images")]
    parts: Vec<usize>,
    #[cfg(feature = "images")]
    tints: usize,
}

impl Held {
    fn of<G: Gl>(back: &GpuBack<'_, G>) -> Held {
        let lens = |lists: &[Cells]| lists.iter().map(|c| c.quads.len()).collect();
        Held {
            cells: lens(&back.cells),
            bars: lens(&back.bars),
            #[cfg(feature = "images")]
            text: lens(&back.text),
            #[cfg(feature = "images")]
            marks: lens(&back.marks),
            #[cfg(feature = "images")]
            parts: back.picture_rects.iter().map(Vec::len).collect(),
            #[cfg(feature = "images")]
            tints: back.tints.len(),
        }
    }
}

impl<G: Gl> GpuBack<'_, G> {
    /// Removes what the frame drew in the cells of a row before it drew
    /// them again, `held` saying where the second drawing begins in each
    /// list: each quad of the first drawing, from whichever list, a part
    /// of a picture or a tint inside those cells. Each cell is then drawn
    /// once, with its second drawing, as the CPU path's drawing in order
    /// leaves it.
    fn replace_earlier(&mut self, held: &Held) {
        let mut again: Vec<[i32; 4]> = Vec::new();
        let lists = [(&self.cells, &held.cells)];
        #[cfg(feature = "images")]
        let lists = [
            lists[0],
            (&self.text, &held.text),
            (&self.marks, &held.marks),
        ];
        for (list, held) in lists {
            for (c, &from) in list.iter().zip(held.iter().chain(core::iter::repeat(&0))) {
                again.extend(
                    c.quads
                        .get(from..)
                        .unwrap_or_default()
                        .iter()
                        .map(|q| q.dst),
                );
            }
        }
        if again.is_empty() {
            return;
        }
        again.sort_unstable();
        again.dedup();
        let drawn_again = |dst: &[i32; 4]| again.binary_search(dst).is_ok();
        drop_earlier(&mut self.cells, &held.cells, drawn_again);
        // a bar lies inside its cell, so the cell's second drawing, which
        // pushes its bar again, replaces the bar of its first
        let inside =
            |dst: &[i32; 4]| dst_rect(*dst).is_some_and(|r| again.iter().any(|&a| contains(a, &r)));
        drop_earlier(&mut self.bars, &held.bars, inside);
        #[cfg(feature = "images")]
        {
            drop_earlier(&mut self.text, &held.text, drawn_again);
            drop_earlier(&mut self.marks, &held.marks, drawn_again);
            let inside = |r: &Rect| again.iter().any(|&dst| contains(dst, r));
            for (parts, &from) in self.picture_rects.iter_mut().zip(&held.parts) {
                keep_unless_earlier(parts, from, |r| inside(r));
            }
            keep_unless_earlier(&mut self.tints, held.tints, |(r, _)| inside(r));
        }
    }
}

/// Removes from each batch of `lists` the quads it held before `held`
/// counts that `drawn_again` names, and the batches left empty.
fn drop_earlier(lists: &mut Vec<Cells>, held: &[usize], drawn_again: impl Fn(&[i32; 4]) -> bool) {
    for (c, &from) in lists.iter_mut().zip(held) {
        keep_unless_earlier(&mut c.quads, from, |q| drawn_again(&q.dst));
    }
    lists.retain(|c| !c.quads.is_empty());
}

/// Removes from the first `from` of `items` those `earlier` names.
fn keep_unless_earlier<T>(items: &mut Vec<T>, from: usize, earlier: impl Fn(&T) -> bool) {
    let mut n = 0usize;
    items.retain(|t| {
        let before = n < from;
        n = n.saturating_add(1);
        !(before && earlier(t))
    });
}

/// Whether the quad's `dst` holds all of `r`.
fn contains(dst: [i32; 4], r: &Rect) -> bool {
    let [x, y, w, h] = dst.map(i64::from);
    let (rx, ry) = (i64::from(r.x), i64::from(r.y));
    rx >= x
        && ry >= y
        && rx.saturating_add(i64::from(r.width)) <= x.saturating_add(w)
        && ry.saturating_add(i64::from(r.height)) <= y.saturating_add(h)
}

/// Where `a` and `b` meet, empty when they do not.
#[cfg(feature = "images")]
fn meet_area(a: Area, b: Area) -> Area {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    Area {
        x,
        y,
        w: a.right().min(b.right()).saturating_sub(x).max(0),
        h: a.bottom().min(b.bottom()).saturating_sub(y).max(0),
    }
}

/// `a`'s part at or right of and below the origin, as a rectangle.
#[cfg(feature = "images")]
fn area_rect(a: Area) -> Option<Rect> {
    let clamp = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    dst_rect([clamp(a.x), clamp(a.y), clamp(a.w), clamp(a.h)])
}

impl<G: Gl> Back for GpuBack<'_, G> {
    fn border(&mut self, _: &mut Composer, (display, grid): (Rect, Option<Area>), _: Rgb, r: Rect) {
        let Some(r) = meet(r, display) else { return };
        let turn = self.turn;
        let Some(g) = grid else {
            self.clear.push(turn.up(r));
            return;
        };
        let to_u32 = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
        let (gx0, gy0, gx1, gy1) = (
            to_u32(g.x),
            to_u32(g.y),
            to_u32(g.right()),
            to_u32(g.bottom()),
        );
        let (rx1, ry1) = (r.x.saturating_add(r.width), r.y.saturating_add(r.height));
        let mut piece = |x0: u32, y0: u32, x1: u32, y1: u32| {
            if x0 < x1 && y0 < y1 {
                self.clear.push(turn.up(Rect {
                    x: x0,
                    y: y0,
                    width: x1.saturating_sub(x0),
                    height: y1.saturating_sub(y0),
                }));
            }
        };
        // above the grid, below it, and beside it on its lines
        piece(r.x, r.y, rx1, ry1.min(gy0));
        piece(r.x, r.y.max(gy1), rx1, ry1);
        let (my0, my1) = (r.y.max(gy0), ry1.min(gy1));
        piece(r.x, my0, rx1.min(gx0), my1);
        piece(r.x.max(gx1), my0, rx1, my1);
    }

    fn row<F: Face>(
        &mut self,
        compose: &mut Composer,
        font: &Font<F>,
        _: (usize, usize),
        row: usize,
    ) {
        if self.error.is_some() {
            return;
        }
        if row < self.next_row {
            let held = Held::of(self);
            self.push_row(compose, font, row);
            self.replace_earlier(&held);
        } else {
            self.next_row = row.saturating_add(1);
            self.push_row(compose, font, row);
        }
    }

    fn reaching(&self, _: &Composer) -> bool {
        self.glyphs.reaching()
    }

    fn reach<F: Face>(
        &mut self,
        _: &mut Composer,
        font: &Font<F>,
        key: (&[char], vtium_font::Width, vtium_font::Variant),
        piece: Piece,
    ) -> Reach {
        if self.error.is_some() {
            return Reach::NONE;
        }
        match self.glyphs.reach(self.gl, font, key, piece) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(e);
                Reach::NONE
            }
        }
    }

    const RECEIVES: bool = true;
}

impl<G: Gl> GpuBack<'_, G> {
    /// Pushes the quads of the jobs of grid row `row`.
    fn push_row<F: Face>(&mut self, compose: &Composer, font: &Font<F>, row: usize) {
        // a cell of a line of double size, a run's cell among them, draws
        // from the doubled sprites; a run on a line of the normal size
        // draws from the normal ones
        if compose.jobs.iter().any(|j| j.piece.sprite_set() != 0)
            && let Err(e) = self.glyphs.double_sprites(self.gl)
        {
            self.error = Some(e);
            return;
        }
        for (j, job) in compose.jobs.iter().enumerate() {
            let place = match job.text {
                Some((start, len, width, style)) => {
                    let text = compose
                        .text
                        .get(start..start.saturating_add(len))
                        .unwrap_or_default();
                    match self
                        .glyphs
                        .glyph(self.gl, font, (text, width, style), job.piece)
                    {
                        Ok(p) => p,
                        Err(e) => {
                            self.error = Some(e);
                            return;
                        }
                    }
                }
                None => None,
            };
            if let Some((bar, color)) = job.bar_area() {
                let q = bar_quad(self.turn.up_area(bar), color, (job.posx, row));
                push(&mut self.bars, None, q);
            }
            let (from, to) = compose.received.get(j).copied().unwrap_or_default();
            let ink = match compose.sources.get(from..to) {
                Some(sources) if !sources.is_empty() => {
                    match self.name_sources(compose, font, sources) {
                        Ok(ink) => ink,
                        Err(e) => {
                            self.error = Some(e);
                            return;
                        }
                    }
                }
                _ => 0,
            };
            #[cfg(feature = "images")]
            if self.pictured(compose, job, place, (row, ink)) {
                continue;
            }
            let beside = self.beside(&compose.sprites, job, place);
            let quad = cell_quad(
                job,
                (self.turn.up_area(job.r), self.turn.up_area(job.g)),
                (place, beside),
                (row, ink),
                &compose.sprites,
            );
            push(&mut self.cells, place.map(|p| (p.texture, p.color)), quad);
        }
    }

    /// Names each of `sources`, the glyphs whose ink a job receives, in the
    /// ink grid, so the cell shader finds its glyph's box and its color,
    /// and gives the job's flags: none when no glyph sends it ink, else
    /// [`RECEIVES`], with [`MARKED`] when a mark covers the job and
    /// [`DOWN`] on a display turned a quarter. Each source's glyph is looked
    /// up, which keeps its box for this frame.
    fn name_sources<F: Face>(
        &mut self,
        compose: &Composer,
        font: &Font<F>,
        sources: &[crate::compose::Source],
    ) -> Result<u32, Error> {
        let mut flags = 0;
        for s in sources {
            let (start, len, width, style) = s.text;
            let text = compose
                .text
                .get(start..start.saturating_add(len))
                .unwrap_or_default();
            let key = (text, width, style);
            let Some(place) = self.glyphs.glyph(self.gl, font, key, s.piece)? else {
                continue;
            };
            let Some((slot, _)) = place.boxed else {
                continue;
            };
            self.glyphs.ink.name(s.cell, slot, s.own, place.color);
            flags |= RECEIVES;
            if s.marked {
                flags |= MARKED;
            }
        }
        if flags != 0 && self.turn.orientation.turned() {
            flags |= DOWN;
        }
        Ok(flags)
    }
}

/// A bar cursor's bar as the cell shader's quad: one color over `r` in the
/// upright frame, with no glyph and no lines, which the shader draws as a
/// cell of nothing but its background. `cell` is the column and row of
/// the bar's cell in the grid.
fn bar_quad(r: Area, color: Rgb, cell: (usize, usize)) -> Quad {
    let i = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    let u = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    Quad {
        dst: [i(r.x), i(r.y), i(r.w), i(r.h)],
        src: [0, 0, u(cell.0), u(cell.1)],
        params: [pixel(color), pixel(color), 0, 0],
    }
}

/// The bits of a quad's `src[2]` that hold the column of its first cell.
/// The bits above them name the quad's entry of the ink beside its piece,
/// plus one ([`super::beside`]).
pub(crate) const COL: u32 = 0xffff;

/// The bits of a quad's `src[3]` that hold the row of its first cell. The
/// bits above them name the piece of a sized run's decorations the quad
/// draws ([`run_bits`]), and above those, from [`DOTS`], the width of a
/// dotted underline's dots.
pub(crate) const ROW: u32 = 0xffff;

/// The first bit of a quad's `src[3]` that holds the width in display
/// pixels of each dot of its dotted underline, and of each gap between
/// them, in 7 bits: 0 when the quad's underline is not dotted. The cell
/// shader leaves out each pixel of the underline that falls in a gap by
/// the pixel's place along the grid's line, as the CPU path does
/// (`crate::deco::in_gap`), so the dots keep one period from cell to cell.
pub(crate) const DOTS: u32 = 25;

// The widest dot the CPU path draws must fit in the bits from `DOTS` up, or
// the shader would read a narrower dot than the CPU path draws.
const _: () = assert!(crate::deco::DOT_MOST < 1 << (32 - DOTS));

/// The piece of its decorations that a cell of a sized run of scale 2 or
/// more on a line of the normal size draws, as the bits above [`ROW`] in a
/// quad's `src[3]`: the scale in bits 16 to 18, the cell's row in the
/// run's block in bits 19 to 21, and its column within one scaled cell in
/// bits 22 to 24. The cell shader reads the normal sprite at the pixel
/// that scaling it by the scale puts under each pixel of the cell, as
/// `crate::deco::scaled` cuts the CPU path's piece. 0 for every other
/// cell.
///
/// References:
/// - ADR-1308 D9, "Decorations scale with the text".
fn run_bits(job: &Job) -> u32 {
    let Some(run) = job.piece.run() else {
        return 0;
    };
    let scale = run.size.scale;
    if run.line != vtium_term::LineSize::Single || scale < 2 {
        return 0;
    }
    let row = run.row.min(scale.saturating_sub(1));
    let column = run.column.checked_rem(scale).unwrap_or(0);
    let bits = u32::from(scale & 7) | u32::from(row & 7) << 3 | u32::from(column & 7) << 6;
    bits << 16
}

/// A cell job of grid row `row` as the cell shader's quad: where it
/// draws, `r` in the upright frame; the glyph's texel under its top left,
/// the glyph lying at `place` and `g`, and the column and row of the job's
/// first cell, with `beside`, its entry of the ink beside its piece plus
/// one, above the column, and a sized run's piece of its decorations above
/// the row ([`run_bits`]); and its colors, each with a kind in its top
/// byte: the glyph's (none, a mask, a color glyph), `flags` with the
/// background, the underline's sprite and the sprite of the lines over the
/// glyph (the strike, the overline or both), each its place among the
/// sprites plus one, or 0 for none. The width of the job's dotted
/// underline's dots, from `sprites`, goes in the bits from [`DOTS`], above
/// the run's bits.
pub(crate) fn cell_quad(
    job: &Job,
    (r, g): (Area, Area),
    (place, beside): (Option<Place>, u32),
    (row, flags): (usize, u32),
    sprites: &Sprites,
) -> Quad {
    let i = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    let u = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    let (col, row) = (
        u32::try_from(job.posx).unwrap_or(COL).min(COL) | beside.min(COL) << SHIFT,
        u32::try_from(row).unwrap_or(ROW).min(ROW)
            | run_bits(job)
            | job
                .underline
                .map_or(0, |(line, _)| sprites.dots(line, job.piece))
                << DOTS,
    );
    let (src, kind) = match place {
        Some(p) => (
            [
                p.x.saturating_add(u(r.x.saturating_sub(g.x))),
                p.y.saturating_add(u(r.y.saturating_sub(g.y))),
                col,
                row,
            ],
            match (p.mixed, p.color) {
                (true, _) => 3,
                (false, true) => 2,
                (false, false) => 1,
            },
        ),
        None => ([0, 0, col, row], 0),
    };
    // a line's place among the sprites, every script's set in turn
    // (ADR-1202 D8)
    // and on a line of double size, among the doubled piece's set
    let set = job.piece.sprite_set().saturating_mul(LINES);
    let sprite = |line: Line| {
        let k = set.saturating_add(line.index());
        u32::try_from(k).map_or(0, |n| n.saturating_add(1))
    };
    let with = |c: Rgb, k: u32| pixel(c) | k << 24;
    // a color glyph's underline in a short cell skips nothing and runs
    // whole under the picture
    let color = sprites.skips_color();
    let gap = match job.underline {
        Some((line, _)) if line.skips_ink && kind != 0 && (kind != 2 || color) => {
            magnification(job.piece).min(15) << GAP | if color { COLOR } else { 0 }
        }
        _ => 0,
    };
    Quad {
        dst: [i(r.x), i(r.y), i(r.w), i(r.h)],
        src,
        params: [
            with(job.fg, kind),
            with(job.bg, flags | gap),
            job.underline.map_or(0, |(k, c)| with(c, sprite(k))),
            job.over.map_or(0, |(k, c)| with(c, sprite(k))),
        ],
    }
}
