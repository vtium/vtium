//! Composing a row's cells into display lines: each run of cells that
//! follow one another through the row scratch, a band of lines at a time,
//! each line written to the target once.
//!
//! A cell that receives ink from the glyphs around it (ADR-1306 D4) is
//! composed in this order, each pixel: its background, its underline, the
//! ink of the row above, the ink of the row below, the ink of the cells
//! beside it, its own glyph and the lines over it. The received ink is
//! blended into one premultiplied layer first, read from the neighbours'
//! glyphs in the cache, so the display's buffer is never read and no
//! background is ever drawn over another cell's ink.
//!
//! A cell's underline skips its glyph's ink: the line leaves out every
//! pixel within one pixel of the ink along the cell's row, so it stops
//! just short of a descender, leaves a gap of one pixel, and goes on after
//! it. Only the glyph's ink inside the cell's own area counts. The line
//! then shares no pixel with the glyph's ink, so drawing it under the glyph
//! gives the same pixels as drawing it over the glyph, and it stays under
//! the ink the cell receives from its neighbours. The underline cursor
//! keeps its whole line ([`Line::skips_ink`]).
//!
//! A color glyph's ink is only its pixels at or above an alpha threshold,
//! and its underline skips that ink only in a tall enough cell. Elsewhere
//! the line runs whole under the glyph, whose picture covers it. The rule
//! is in [`crate::skip`].

use vtium_font::{Face, Font, Variant, Width as FontWidth};
use vtium_term::Rgb;

use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};
use crate::cache::GlyphCache;
use crate::deco::{Line, Sprites, magnification};
#[cfg(feature = "images")]
use crate::picture::{Mark, Pictures, Under, over_px, tint};
use crate::renderer::double::Piece;
use crate::skip;
use crate::stored::{Pens, Rows, Stored};
use crate::target::{Orientation, Target};

pub(crate) fn int(v: impl TryInto<i64>) -> i64 {
    v.try_into().unwrap_or(i64::MAX)
}

/// A rectangle in display pixels that may reach past the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Area {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) w: i64,
    pub(crate) h: i64,
}

impl Area {
    pub(crate) fn bottom(&self) -> i64 {
        self.y.saturating_add(self.h)
    }

    pub(crate) fn right(&self) -> i64 {
        self.x.saturating_add(self.w)
    }

    /// The smallest area holding both.
    fn union(&self, o: &Area) -> Area {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Area {
            x,
            y,
            w: self.right().max(o.right()).saturating_sub(x),
            h: self.bottom().max(o.bottom()).saturating_sub(y),
        }
    }
}

/// One cell to draw.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Job {
    pub(crate) posx: usize,
    pub(crate) ncells: usize,
    /// Where it draws.
    pub(crate) r: Area,
    /// Where its glyph lies, which a wide glyph cut at the margin passes.
    pub(crate) g: Area,
    pub(crate) tiles: [Area; 2],
    pub(crate) fg: Rgb,
    pub(crate) bg: Rgb,
    pub(crate) underline: Option<(Line, Rgb)>,
    /// The lines drawn over the glyph, the strike-through and the
    /// overline, as one sprite in one color.
    pub(crate) over: Option<(Line, Rgb)>,
    /// The glyph's text in the row's text buffer, its width and style.
    pub(crate) text: Option<(usize, usize, FontWidth, Variant)>,
    /// A color glyph draws as its silhouette, for a 1-bit display.
    pub(crate) silhouette: bool,
    /// The piece of its glyph and decorations it draws: the whole glyph,
    /// or one cell of them doubled on a line of double size.
    pub(crate) piece: Piece,
    /// The cell's background and mark, which decide how pictures over it
    /// draw.
    #[cfg(feature = "images")]
    pub(crate) under: Under,
    /// A bar cursor's bar, which draws over everything else in the cell.
    pub(crate) bar: Option<Bar>,
}

/// The bar of a bar cursor: `area`, the display pixels it fills, in
/// `color`. In the grid's own pixels, before the display's orientation
/// turns them, the bar is `w` pixels wide and runs down the whole height
/// of the character's cells, along their left edge, or along their right
/// edge when `right`. The bar is drawn last, over the cell's letter, its
/// lines and any ink or picture in the cell, so the letter shows beside
/// the bar. A bar is opaque, so the CPU path fills its pixels and the GPU
/// path draws it as a quad of one color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bar {
    pub(crate) area: Area,
    pub(crate) color: Rgb,
    pub(crate) w: i64,
    pub(crate) right: bool,
}

/// A neighbour's glyph whose ink a cell receives (ADR-1306 D4).
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    not(feature = "gpu"),
    expect(
        dead_code,
        reason = "only the GPU path's composer reads a source's cell, own color and mark"
    )
)]
pub(crate) struct Source {
    /// The glyph's text in the row's text buffer, its width and style.
    pub(crate) text: (usize, usize, FontWidth, Variant),
    /// Where the glyph's cells lie on the display.
    pub(crate) g: Area,
    /// The block of cells its ink may draw in, on the display: ink outside
    /// it is cut.
    pub(crate) clip: Area,
    /// The color its ink takes in the receiving cell (ADR-1306 D6).
    pub(crate) color: Rgb,
    /// The glyph's first cell, its column and row, which the GPU path
    /// names to its shader.
    pub(crate) cell: (usize, usize),
    /// The color its ink takes in a cell that no mark covers, which is
    /// `color` unless `marked`.
    pub(crate) own: Rgb,
    /// A block cursor or the selection marks the receiving cell, so its
    /// ink takes the mark's text color there.
    pub(crate) marked: bool,
    /// The piece of the glyph that sends the ink: the whole glyph, or on a
    /// line of double size the one-cell piece that `cell` draws.
    pub(crate) piece: Piece,
}

impl Job {
    /// The part of the job's area that its bar covers, if it has a bar
    /// and the bar meets its area: on a line of double size, only the
    /// display cell at the bar's edge of the character holds the bar.
    pub(crate) fn bar_area(&self) -> Option<(Area, Rgb)> {
        let b = self.bar?;
        let (a, r) = (b.area, self.r);
        let (x0, y0) = (a.x.max(r.x), a.y.max(r.y));
        let (x1, y1) = (a.right().min(r.right()), a.bottom().min(r.bottom()));
        (x0 < x1 && y0 < y1).then_some((
            Area {
                x: x0,
                y: y0,
                w: x1.saturating_sub(x0),
                h: y1.saturating_sub(y0),
            },
            b.color,
        ))
    }

    pub(crate) fn layered(&self) -> bool {
        self.underline.is_some() || self.over.is_some()
    }

    /// Whether the cell's text layer draws anything: a glyph, which the
    /// cache has when `glyph` is set, an underline, a strike or an overline.
    #[cfg(feature = "images")]
    fn draws_text(&self, glyph: bool) -> bool {
        glyph || self.layered()
    }
}

/// Lines a cell is composed in.
#[derive(Debug, Default)]
struct Lines {
    cov: Vec<u8>,
    prem: Vec<u32>,
    src: Vec<u32>,
    out: Vec<u32>,
    /// The glyph's ink along a line, and the pixels near it that an
    /// underline skips ([`near_ink`]).
    ink: Vec<u8>,
    near: Vec<u8>,
    /// On a display turned a quarter, the pixels near the ink of one cell,
    /// for the display columns its underline covers ([`Columns`]).
    columns: Columns,
}

/// What composes cells: the glyph cache, the decoration sprites, the row's
/// jobs and their text, and the row scratch.
#[derive(Debug)]
pub(crate) struct Composer {
    pub(crate) cache: GlyphCache,
    pub(crate) sprites: Sprites,
    lines: Lines,
    pub(crate) jobs: Vec<Job>,
    pub(crate) text: Vec<char>,
    /// The neighbours' glyphs the row's jobs receive ink from.
    pub(crate) sources: Vec<Source>,
    /// For each job, by its place among the jobs, the range of `sources`
    /// it receives ink from, in the order they are drawn; empty when no
    /// job receives any.
    pub(crate) received: Vec<(usize, usize)>,
    /// The ink a job receives from its neighbours, premultiplied, over the
    /// job's lines in the band.
    recv: Vec<u32>,
    scratch: Vec<u32>,
    pens: Pens,
    /// The frame's pictures, and the ones that draw on a job.
    #[cfg(feature = "images")]
    pub(crate) pictures: Pictures,
    #[cfg(feature = "images")]
    meeting: Vec<usize>,
}

impl Composer {
    pub(crate) fn new(cache: GlyphCache, sprites: Sprites) -> Composer {
        Composer {
            cache,
            sprites,
            lines: Lines::default(),
            jobs: Vec::new(),
            text: Vec::new(),
            sources: Vec::new(),
            received: Vec::new(),
            recv: Vec::new(),
            scratch: Vec::new(),
            pens: Pens::new((0, 0)),
            #[cfg(feature = "images")]
            pictures: Pictures::new(),
            #[cfg(feature = "images")]
            meeting: Vec::new(),
        }
    }

    /// The columns from the first job of several layers to the end of the
    /// last.
    pub(crate) fn layered_span(&self) -> Option<(usize, usize)> {
        let layered = self.jobs.iter().filter(|j| j.layered());
        let first = layered.clone().map(|j| j.posx).min()?;
        let end = layered
            .map(|j| j.posx.saturating_add(j.ncells))
            .max()
            .unwrap_or(first);
        Some((first, end))
    }

    /// `len` pixels of `px`, for a border line.
    pub(crate) fn solid(&mut self, len: usize, px: u32) -> &[u32] {
        self.lines.out.clear();
        self.lines.out.resize(len, px);
        &self.lines.out
    }

    /// Draws the row's jobs: each run of jobs whose cells follow one
    /// another is composed in the row scratch, whose lines are `strip`'s
    /// length and count at most, and copied out a line at a time.
    pub(crate) fn draw<F: Face>(
        &mut self,
        font: &Font<F>,
        strip: (usize, usize),
        target: &mut impl Target,
    ) {
        // no two jobs share a column
        if !self.jobs.is_sorted_by_key(|j| j.posx) {
            self.jobs.sort_unstable_by_key(|j| j.posx);
        }
        let mut start = 0;
        while let Some(first) = self.jobs.get(start) {
            let mut area = first.r;
            let mut next = first.posx.saturating_add(first.ncells);
            let mut end = start.saturating_add(1);
            while let Some(j) = self.jobs.get(end).filter(|j| j.posx == next) {
                area = area.union(&j.r);
                next = j.posx.saturating_add(j.ncells);
                end = end.saturating_add(1);
            }
            self.draw_run(font, (start, end), area, strip, target);
            start = end;
        }
    }

    /// Composes jobs `start` to `end`, which cover `area`, in the row
    /// scratch, a band of lines at a time, and copies each line out once.
    fn draw_run<F: Face>(
        &mut self,
        font: &Font<F>,
        (start, end): (usize, usize),
        area: Area,
        (line_max, band_max): (usize, usize),
        target: &mut impl Target,
    ) {
        let w = usize::try_from(area.w).unwrap_or(0).min(line_max);
        let band = band_max.max(1);
        let need = line_max.saturating_mul(band);
        if self.scratch.len() < need {
            self.scratch.resize(need, 0);
        }
        let Composer {
            cache,
            sprites,
            lines,
            jobs,
            text,
            sources,
            received,
            recv,
            scratch,
            pens,
            #[cfg(feature = "images")]
            pictures,
            #[cfg(feature = "images")]
            meeting,
        } = self;
        let run = jobs.get(start..end).unwrap_or_default();
        // the glyphs may differ from the last run's in the same places
        lines.columns.forget();
        let mut top = area.y;
        while top < area.bottom() {
            let lines_here = int(band).min(area.bottom().saturating_sub(top));
            let bottom = top.saturating_add(lines_here);
            // the run of cells under one opaque picture being gathered
            #[cfg(feature = "images")]
            let mut covered: Option<Cover> = None;
            for (k, job) in (start..).zip(run) {
                let dx = usize::try_from(job.r.x.saturating_sub(area.x)).unwrap_or(0);
                let jw = usize::try_from(job.r.w).unwrap_or(0);
                let y0 = job.r.y.max(top);
                let at = usize::try_from(y0.saturating_sub(top))
                    .unwrap_or(0)
                    .saturating_mul(w)
                    .saturating_add(dx);
                // the job's lines in the band: the last ends at its width
                let n = usize::try_from(job.r.bottom().min(bottom).saturating_sub(y0)).unwrap_or(0);
                let (from, to) = received.get(k).copied().unwrap_or_default();
                let received = to > from
                    && receive(
                        (cache, font, text),
                        sources.get(from..to).unwrap_or_default(),
                        job,
                        (y0, n, jw),
                        (recv, &mut lines.cov, &mut lines.src),
                    );
                let glyph = glyph(cache, font, text, job);
                let len = n
                    .checked_sub(1)
                    .map_or(0, |n| w.saturating_mul(n).saturating_add(jw));
                let Some(out) = scratch.get_mut(at..at.saturating_add(len)) else {
                    continue;
                };
                #[cfg(feature = "images")]
                match pictured(
                    (pictures, meeting),
                    job,
                    (glyph, received.then_some(recv.as_slice())),
                    (sprites, lines),
                    (y0, w, jw),
                    out,
                ) {
                    Pictured::No => {}
                    Pictured::Drawn => {
                        bar_over(job, (y0, n, w), out);
                        continue;
                    }
                    Pictured::Covered(pic) => {
                        let cell = Cover {
                            pic,
                            x: job.r.x,
                            end: job.r.x.saturating_add(job.r.w),
                            y0,
                            lines: n,
                            at,
                        };
                        covered = match covered {
                            Some(c) if c.joins(&cell) => Some(Cover { end: cell.end, ..c }),
                            other => {
                                if let Some(c) = other {
                                    c.draw(pictures, scratch, w);
                                }
                                Some(cell)
                            }
                        };
                        continue;
                    }
                }
                match glyph {
                    // the common cell, a bitmap glyph on its background
                    Some(g)
                        if !job.layered() && !received && g.is_bits() && g.layer().is_none() =>
                    {
                        let gx = job.r.x.saturating_sub(job.g.x);
                        let rows = Rows {
                            out,
                            stride: w,
                            width: jw,
                        };
                        pens.set((pixel(job.fg), pixel(job.bg)));
                        g.bits_rows(gx, y0.saturating_sub(job.g.y), pens, rows);
                    }
                    _ => {
                        let lines_out = out.chunks_mut(w.max(1));
                        for (i, (y, line)) in (y0..).zip(lines_out).enumerate() {
                            if let Some(line) = line.get_mut(..jw) {
                                let ink = received.then(|| recv_line(recv, i, jw)).flatten();
                                cell_line(job, glyph, sprites, lines, y, ink, line);
                            }
                        }
                    }
                }
                bar_over(job, (y0, n, w), out);
            }
            #[cfg(feature = "images")]
            if let Some(c) = covered.take() {
                c.draw(pictures, scratch, w);
            }
            for (i, y) in (top..bottom).enumerate() {
                let at = i.saturating_mul(w);
                if let (Some(line), Ok(x), Ok(y)) = (
                    scratch.get(at..at.saturating_add(w)),
                    u32::try_from(area.x),
                    u32::try_from(y),
                ) {
                    target.write(x, y, line);
                }
            }
            top = bottom;
        }
    }
}

/// The job's bar over its `n` lines from display line `y0` in `out`, whose
/// lines are `w` apart and begin at the job's left edge: each pixel the bar
/// covers takes the bar's color.
fn bar_over(job: &Job, (y0, n, w): (i64, usize, usize), out: &mut [u32]) {
    let Some((bar, color)) = job.bar_area() else {
        return;
    };
    let (Ok(from), Ok(len)) = (
        usize::try_from(bar.x.saturating_sub(job.r.x)),
        usize::try_from(bar.w),
    ) else {
        return;
    };
    let px = pixel(color);
    for (i, y) in (y0..).take(n).enumerate() {
        if y < bar.y || y >= bar.bottom() {
            continue;
        }
        let at = i.saturating_mul(w).saturating_add(from);
        if let Some(line) = out.get_mut(at..at.saturating_add(len)) {
            line.fill(px);
        }
    }
}

/// The job's glyph from the cache, if it has one.
fn glyph<'c, F: Face>(
    cache: &'c mut GlyphCache,
    font: &Font<F>,
    text: &[char],
    job: &Job,
) -> Option<&'c Stored> {
    let (start, len, width, style) = job.text?;
    let chars = text.get(start..start.saturating_add(len))?;
    cache.get_piece(font, (chars, width, style), job.piece)
}

/// The ink `job` receives from the glyphs of `sources`, in their order,
/// over its `n` lines from `y0`, `jw` pixels wide, blended premultiplied
/// into `recv`. Each glyph's ink is cut to its block of cells and to the
/// job's cells. Returns whether any ink landed.
fn receive<F: Face>(
    (cache, font, text): (&mut GlyphCache, &Font<F>, &[char]),
    sources: &[Source],
    job: &Job,
    (y0, n, jw): (i64, usize, usize),
    (recv, cov, argb): (&mut Vec<u32>, &mut Vec<u8>, &mut Vec<u32>),
) -> bool {
    if sources.is_empty() {
        return false;
    }
    recv.clear();
    recv.resize(n.saturating_mul(jw), 0);
    let mut any = false;
    for s in sources {
        let (start, len, width, style) = s.text;
        let Some(chars) = text.get(start..start.saturating_add(len)) else {
            continue;
        };
        let Some(g) = cache.get_piece(font, (chars, width, style), s.piece) else {
            continue;
        };
        let x0 = job.r.x.max(s.clip.x);
        let x1 = job.r.right().min(s.clip.right());
        let (Ok(from), Ok(to)) = (
            usize::try_from(x0.saturating_sub(job.r.x)),
            usize::try_from(x1.saturating_sub(job.r.x)),
        ) else {
            continue;
        };
        if from >= to.min(jw) {
            continue;
        }
        cov.resize(to.saturating_sub(from), 0);
        argb.resize(to.saturating_sub(from), 0);
        for (i, line) in recv.chunks_mut(jw.max(1)).enumerate() {
            let y = y0.saturating_add(int(i));
            if y < s.clip.y || y >= s.clip.bottom() {
                continue;
            }
            let Some(seg) = line.get_mut(from..to) else {
                continue;
            };
            let (gx, gy) = (x0.saturating_sub(s.g.x), y.saturating_sub(s.g.y));
            if g.is_rgba() {
                // a color emoji's edge keeps its own colors, inside a mark
                // too (ADR-1302 D5, D8)
                g.argb_line(gx, gy, argb);
                for (p, &c) in seg.iter_mut().zip(argb.iter()) {
                    any |= c != 0;
                    *p = argb_over(*p, c);
                }
                continue;
            }
            g.mask_line(gx, gy, cov);
            for (p, &a) in seg.iter_mut().zip(cov.iter()) {
                any |= a != 0;
                *p = tint_over(*p, a, s.color);
            }
        }
    }
    any
}

/// Line `i` of the received ink `recv`, whose lines are `jw` long, when
/// any ink lies on it.
fn recv_line(recv: &[u32], i: usize, jw: usize) -> Option<&[u32]> {
    let at = i.saturating_mul(jw);
    recv.get(at..at.saturating_add(jw))
        .filter(|l| l.iter().any(|&p| p != 0))
}

/// A color glyph's line as its silhouette in the job's foreground, where
/// the job asks for it.
fn silhouette(job: &Job, src: &mut [u32]) {
    if job.silhouette {
        for s in src {
            *s = crate::mono::silhouette(*s, job.fg);
        }
    }
}

/// One display line `y` of `job`, across its area, into `out`, with
/// `ink`, the line of ink it receives from its neighbours, when any lands
/// on it.
fn cell_line(
    job: &Job,
    glyph: Option<&Stored>,
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    ink: Option<&[u32]>,
    out: &mut [u32],
) {
    let w = out.len();
    let x0 = job.r.x;
    let Scratch {
        cov,
        prem,
        src,
        ink: glyph_ink,
        near,
        columns,
    } = Scratch::of(lines, w);
    let (gx, gy) = (x0.saturating_sub(job.g.x), y.saturating_sub(job.g.y));
    if !job.layered() && ink.is_none() && glyph.is_none_or(|g| g.layer().is_none()) {
        match glyph {
            None => out.fill(pixel(job.bg)),
            Some(g) if g.is_bits() => {
                g.bits_line(gx, gy, (pixel(job.fg), pixel(job.bg)), out);
            }
            Some(g) if g.is_rgba() => {
                g.argb_line(gx, gy, src);
                silhouette(job, src);
                for (o, &s) in out.iter_mut().zip(src.iter()) {
                    *o = over_bg(job.bg, s);
                }
            }
            Some(g) => {
                g.mask_line(gx, gy, cov);
                for (o, &a) in out.iter_mut().zip(cov.iter()) {
                    *o = mask(job.fg, job.bg, a);
                }
            }
        }
        return;
    }
    prem.fill(0);
    if let Some(u) = job.underline {
        let skip = glyph.map(|glyph| Skip {
            glyph,
            ink: glyph_ink,
            near,
            columns,
        });
        deco_over(job, sprites, u, y, skip, prem, cov);
    }
    received_over(prem, ink);
    match glyph {
        Some(g) if g.is_rgba() => {
            g.argb_line(gx, gy, src);
            silhouette(job, src);
            for (p, &s) in prem.iter_mut().zip(src.iter()) {
                *p = argb_over(*p, s);
            }
        }
        Some(g) => {
            g.mask_line(gx, gy, cov);
            for (p, &a) in prem.iter_mut().zip(cov.iter()) {
                *p = tint_over(*p, a, job.fg);
            }
            color_layer_over(job, g, (gx, gy), (prem, src));
        }
        None => {}
    }
    if let Some(o) = job.over {
        deco_over(job, sprites, o, y, None, prem, cov);
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_bg(job.bg, p);
    }
}

/// The lines of [`Lines`] that compose one display line `w` pixels wide.
struct Scratch<'a> {
    cov: &'a mut [u8],
    prem: &'a mut [u32],
    src: &'a mut [u32],
    /// The glyph's ink along the line, and the pixels near it.
    ink: &'a mut [u8],
    near: &'a mut [u8],
    columns: &'a mut Columns,
}

/// What an underline that skips ink reads: the job's glyph, scratch for
/// the glyph's ink along a line and for the pixels near it, and the
/// pixels near the ink of the cell's columns on a display turned a
/// quarter.
struct Skip<'a> {
    glyph: &'a Stored,
    ink: &'a mut [u8],
    near: &'a mut [u8],
    columns: &'a mut Columns,
}

impl Scratch<'_> {
    fn of(lines: &mut Lines, w: usize) -> Scratch<'_> {
        let Lines {
            cov,
            prem,
            src,
            ink,
            near,
            columns,
            ..
        } = lines;
        cov.resize(w, 0);
        prem.resize(w, 0);
        src.resize(w, 0);
        ink.resize(w, 0);
        near.resize(w, 0);
        Scratch {
            cov: cov.get_mut(..w).unwrap_or_default(),
            prem: prem.get_mut(..w).unwrap_or_default(),
            src: src.get_mut(..w).unwrap_or_default(),
            ink: ink.get_mut(..w).unwrap_or_default(),
            near: near.get_mut(..w).unwrap_or_default(),
            columns,
        }
    }
}

/// `line` tinted `c` over the premultiplied line `prem`, across the job's
/// cells, at display line `y`. A dotted underline leaves out its gaps by
/// each pixel's place along the grid's line ([`Sprites::leave_gaps`]), so
/// its dots keep one period from cell to cell.
///
/// `skip` holds the job's glyph and scratch lines. When it is given, a
/// line that skips ink leaves out the pixels near the glyph's ink. It looks
/// for them only where the line's sprite has a coverage above 0. Upright,
/// or turned half round, [`near_ink`] finds them along the whole display
/// line, once on each display line the sprite covers. On a display turned
/// a quarter, the cell's row runs down a display column, so [`Columns`]
/// finds them once for each display column the sprite covers, down the
/// whole cell.
fn deco_over(
    job: &Job,
    sprites: &Sprites,
    (line, c): (Line, Rgb),
    y: i64,
    skip: Option<Skip<'_>>,
    prem: &mut [u32],
    cov: &mut [u8],
) {
    let Some(s) = sprites.piece(line, job.piece) else {
        return;
    };
    let o = sprites.orientation();
    let color = sprites.skips_color();
    let mut skip = skip.filter(|_| line.skips_ink);
    let mut near_found = false;
    for (col, t) in (job.posx..).zip(job.tiles.get(..job.ncells).unwrap_or_default()) {
        let at = (job.r.x.saturating_sub(t.x), y.saturating_sub(t.y));
        s.mask_line(at.0, at.1, cov);
        sprites.leave_gaps((line, job.piece), col, at, cov);
        if let Some(Skip {
            glyph,
            ink,
            near,
            columns,
        }) = skip.as_mut()
            && cov.iter().any(|&a| a != 0)
        {
            if o.turned() {
                if let Some(i) = columns.line(job, y) {
                    for (x, a) in cov.iter_mut().enumerate().filter(|(_, a)| **a != 0) {
                        if columns.near(job, (glyph, color), o, (x, i)) {
                            *a = 0;
                        }
                    }
                }
            } else {
                if !near_found {
                    near_ink(job, (glyph, color), o, y, (ink, near));
                    near_found = true;
                }
                for (a, &n) in cov.iter_mut().zip(near.iter()) {
                    if n != 0 {
                        *a = 0;
                    }
                }
            }
        }
        for (p, &a) in prem.iter_mut().zip(cov.iter()) {
            *p = tint_over(*p, a, c);
        }
    }
}

/// Marks in `near` each pixel of display line `y`, across `job`'s area,
/// that lies within the gap of `glyph`'s ink along the cell's row. `near`
/// holds one byte for each pixel. The ink's own pixels count too. This
/// works on a display that is upright or turned half round by `o`, where
/// the cell's row runs along the display line. The gap is one pixel of the
/// normal cell, so a magnified cell's gap is [`magnification`] display
/// pixels wide. Ink is any pixel of a text glyph with a coverage above 0,
/// and, when `color` is set, any pixel of a color glyph or of the color
/// layer whose alpha reaches the threshold ([`skip::inked`]). Only the ink
/// inside the job's area counts, and in a piece of a magnified character
/// the ink of the character's pieces beside it, which the glyph carries
/// ([`Stored::beside`]). `ink` is scratch for the glyph's line.
fn near_ink(
    job: &Job,
    (glyph, color): (&Stored, bool),
    o: Orientation,
    y: i64,
    (ink, near): (&mut [u8], &mut [u8]),
) {
    near.fill(0);
    if let Some(beside) = glyph.beside() {
        let (dx, dy) = (job.r.x.saturating_sub(job.g.x), y.saturating_sub(job.g.y));
        for (x, n) in (dx..).zip(near.iter_mut()) {
            if beside.skips(o, (x, dy)) {
                *n = 1;
            }
        }
    }
    let gap = usize::try_from(magnification(job.piece)).unwrap_or(1);
    let gx = job.r.x.saturating_sub(job.g.x);
    for g in core::iter::once(glyph).chain(glyph.layer()) {
        g.mask_line(gx, y.saturating_sub(job.g.y), ink);
        let rgba = g.is_rgba();
        for (x, _) in ink
            .iter()
            .enumerate()
            .filter(|&(_, &a)| skip::inked(rgba, a, color))
        {
            // the pixel, and the pixels before and after it that lie in
            // the job
            let end = x.saturating_add(gap).saturating_add(1);
            let beside = x.saturating_sub(gap)..end.min(near.len());
            for n in near.get_mut(beside).into_iter().flatten() {
                *n = 1;
            }
        }
    }
}

/// On a display turned a quarter, the pixels near the ink of one cell
/// that its underline skips, the ones [`near_ink`] marks upright. The
/// cell's row then runs down a display column, so the pixels beside a
/// pixel along the row are the ones above and below it. An underline
/// covers only the few display columns that hold the upright rows it lies
/// on, and it covers them down the whole cell. So vtium works the pixels
/// out once for each display column the underline covers, the first time
/// the underline needs that column, for every display line of the cell.
/// Worked out on each display line instead, they would read the lines
/// before and after it too, across the whole width of the cell.
#[derive(Debug, Default)]
struct Columns {
    /// The cell they are for: its first column in the row, the area it
    /// draws, and where its glyph lies. `None` when they are for no cell.
    key: Option<(usize, Area, Area)>,
    /// For each display column of the cell, where its lines start in
    /// `near`, plus 1; 0 for a column not yet worked out.
    at: Vec<usize>,
    /// For each column worked out, one byte for each display line of the
    /// cell, from its top: 1 where the pixel lies near the ink.
    near: Vec<u8>,
    /// Scratch for one column: where the glyph has ink, line by line.
    ink: Vec<u8>,
}

impl Columns {
    /// Forgets the cell they are for, whose glyph a later cell in the same
    /// place need not share.
    fn forget(&mut self) {
        self.key = None;
    }

    /// Makes the columns `job`'s, and gives display line `y` counted from
    /// the top of `job`'s area; `None` for a line outside the area.
    fn line(&mut self, job: &Job, y: i64) -> Option<usize> {
        let key = (job.posx, job.r, job.g);
        if self.key != Some(key) {
            self.key = Some(key);
            self.at.clear();
            self.at.resize(usize::try_from(job.r.w).unwrap_or(0), 0);
            self.near.clear();
        }
        let h = usize::try_from(job.r.h).unwrap_or(0);
        usize::try_from(y.saturating_sub(job.r.y))
            .ok()
            .filter(|&i| i < h)
    }

    /// Whether display pixel `x`, `i` of `job`'s area, counted from its top
    /// left, lies near `glyph`'s ink on a display turned a quarter by `o`,
    /// with whether its color ink counts, as [`near_ink`] decides it along
    /// an upright row: within the gap of
    /// ink in the lines above and below it inside the area, or near the ink
    /// beside a piece. They must be `job`'s ([`Columns::line`]). `false`
    /// for a pixel outside the area.
    fn near(
        &mut self,
        job: &Job,
        glyph: (&Stored, bool),
        o: Orientation,
        (x, i): (usize, usize),
    ) -> bool {
        let Some(at) = self.at.get(x).copied() else {
            return false;
        };
        let start = match at.checked_sub(1) {
            Some(start) => start,
            None => {
                let start = self.near.len();
                self.column(job, glyph, o, x);
                if let Some(a) = self.at.get_mut(x) {
                    *a = start.saturating_add(1);
                }
                start
            }
        };
        self.near
            .get(start.saturating_add(i))
            .is_some_and(|&n| n != 0)
    }

    /// Appends to `near` display column `x` of `job`'s area, down its
    /// whole height: each pixel within the gap of `glyph`'s ink, or of its
    /// color layer's, in the lines above and below it inside the area, the
    /// ink's own pixels among them, and each pixel near the ink beside a
    /// piece. `color` says whether a color glyph's ink counts, as for
    /// [`near_ink`].
    fn column(&mut self, job: &Job, (glyph, color): (&Stored, bool), o: Orientation, x: usize) {
        let h = usize::try_from(job.r.h).unwrap_or(0);
        let gx = job.r.x.saturating_sub(job.g.x).saturating_add(int(x));
        let dy = job.r.y.saturating_sub(job.g.y);
        self.ink.clear();
        for line in (dy..).take(h) {
            let ink = core::iter::once(glyph)
                .chain(glyph.layer())
                .any(|g| skip::inked(g.is_rgba(), g.coverage(gx, line), color));
            self.ink.push(u8::from(ink));
        }
        let gap = usize::try_from(magnification(job.piece)).unwrap_or(1);
        let beside = glyph.beside();
        for (i, line) in (dy..).take(h).enumerate() {
            let end = i.saturating_add(gap).saturating_add(1).min(h);
            let inked = self
                .ink
                .get(i.saturating_sub(gap)..end)
                .is_some_and(|w| w.contains(&1));
            let near = inked || beside.is_some_and(|b| b.skips(o, (gx, line)));
            self.near.push(u8::from(near));
        }
    }
}

/// The color layer of the glyph `g`, if it has one, over the premultiplied
/// line `prem`, its line at `gx`, `gy` read through `src`: the color glyphs
/// of a sized run that mixes them with text, in their own colors over the
/// text.
fn color_layer_over(
    job: &Job,
    g: &Stored,
    (gx, gy): (i64, i64),
    (prem, src): (&mut [u32], &mut [u32]),
) {
    let Some(layer) = g.layer() else {
        return;
    };
    layer.argb_line(gx, gy, src);
    silhouette(job, src);
    for (p, &s) in prem.iter_mut().zip(src.iter()) {
        *p = argb_over(*p, s);
    }
}

/// The received ink `ink` over the premultiplied line `prem`, where it
/// lies over the cell's underline and under its own glyph.
fn received_over(prem: &mut [u32], ink: Option<&[u32]>) {
    for (p, &s) in prem.iter_mut().zip(ink.unwrap_or_default()) {
        *p = argb_over(*p, s);
    }
}

/// What [`pictured`] did with a cell.
#[cfg(feature = "images")]
enum Pictured {
    /// No picture reaches it, its mark covers them, or its own background
    /// hides every one: it draws as any cell.
    No,
    /// It is drawn, with its pictures.
    Drawn,
    /// It is wholly under the opaque picture `0`, with nothing drawn over
    /// it: the cell is that picture's pixels, which its run draws.
    Covered(usize),
}

/// Cells side by side under one opaque picture, whose every display line
/// is copied from the picture at once, one copy for the whole run instead
/// of one a cell: picture `pic` from display column `x` to `end`, its
/// `lines` lines from `y0`, the first at `at` in the row scratch.
#[cfg(feature = "images")]
#[derive(Clone, Copy)]
struct Cover {
    pic: usize,
    x: i64,
    end: i64,
    y0: i64,
    lines: usize,
    at: usize,
}

#[cfg(feature = "images")]
impl Cover {
    /// Whether `next` continues this run on the same lines.
    fn joins(&self, next: &Cover) -> bool {
        self.pic == next.pic && self.end == next.x && self.y0 == next.y0 && self.lines == next.lines
    }

    /// The run's lines from its picture, into the row scratch whose lines
    /// are `w` apart.
    fn draw(&self, pictures: &mut Pictures, scratch: &mut [u32], w: usize) {
        let len = usize::try_from(self.end.saturating_sub(self.x)).unwrap_or(0);
        for k in 0..self.lines {
            let at = self.at.saturating_add(k.saturating_mul(w));
            if let Some(line) = scratch.get_mut(at..at.saturating_add(len)) {
                let y = self.y0.saturating_add(i64::try_from(k).unwrap_or(i64::MAX));
                pictures.over(self.pic, y, self.x, line);
            }
        }
    }
}

/// Draws `job`'s lines from `y0` into `out`, lines `w` apart and `jw`
/// wide, where any of the frame's pictures reach it. A
/// cell under a block cursor, which covers every picture, draws as any
/// other cell does; a selected cell is tinted with the selection's color
/// over its pictures.
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn pictured(
    (pictures, meeting): (&mut Pictures, &mut Vec<usize>),
    job: &Job,
    (glyph, received): (Option<&Stored>, Option<&[u32]>),
    (sprites, lines): (&Sprites, &mut Lines),
    (y0, w, jw): (i64, usize, usize),
    out: &mut [u32],
) -> Pictured {
    use vtium_image::Layer;
    if job.under.mark == Mark::Opaque {
        return Pictured::No;
    }
    pictures.meeting(&job.r, meeting);
    // A picture below backgrounds shows only where a cell's background is
    // the default, so a cell with a background color of its own hides it.
    // When every picture that meets the cell is hidden that way, the cell
    // draws as a cell with no picture does. The picture path would give
    // the same colors, but it composes the text over the background in two
    // roundings instead of one, so partly covered text pixels would come
    // out a level off from the same text on a plain cell.
    let shows = |l: Layer| l != Layer::BelowBackground || job.under.default_bg;
    if !meeting
        .iter()
        .any(|&i| pictures.pic(i).is_some_and(|p| shows(p.layer())))
    {
        return Pictured::No;
    }
    // the last opaque picture over the whole cell hides what lies under it;
    // finding it needs no pixels, and only it and what lies over it are
    // made ready, so a hidden picture is never scaled
    let top = meeting.iter().rposition(|&i| {
        pictures
            .pic(i)
            .is_some_and(|p| p.opaque() && p.covers(&job.r) && shows(p.layer()))
    });
    for &i in meeting.get(top.unwrap_or(0)..).unwrap_or_default() {
        pictures.ready(i);
    }
    let x0 = job.r.x;
    // an opaque picture over the whole cell with nothing drawn over it, as
    // a sixel over blank cells is: the cell is the picture's pixels
    if let Some(k) = top.filter(|&k| k.saturating_add(1) == meeting.len()) {
        let i = meeting.get(k).copied().unwrap_or(usize::MAX);
        let hides_text = pictures
            .pic(i)
            .is_some_and(|p| p.layer() == Layer::AboveText);
        let plain = job.under.mark == Mark::None && !job.under.selected;
        let receives = received.is_some();
        if plain && (hides_text || !(job.draws_text(glyph.is_some()) || receives)) {
            return Pictured::Covered(i);
        }
    }
    for (i, (y, line)) in (y0..).zip(out.chunks_mut(w.max(1))).enumerate() {
        let Some(line) = line.get_mut(..jw) else {
            continue;
        };
        let ink = received.and_then(|r| recv_line(r, i, jw));
        line.fill(pixel(job.bg));
        let mut text = false;
        let rest = match top {
            Some(k) => {
                let i = meeting.get(k).copied().unwrap_or(usize::MAX);
                pictures.over(i, y, x0, line);
                text = pictures
                    .pic(i)
                    .is_some_and(|p| p.layer() == Layer::AboveText);
                meeting.get(k.saturating_add(1)..).unwrap_or_default()
            }
            None => meeting.as_slice(),
        };
        // ADR-1100 D4, from the bottom: the layer below backgrounds where
        // the background is the default, the layer below text, the text,
        // the layer above it, and last vtium's mark
        for &i in rest {
            let Some(layer) = pictures.pic(i).map(|p| p.layer()).filter(|&l| shows(l)) else {
                continue;
            };
            if !text && layer == Layer::AboveText {
                text_over(job, (glyph, ink), sprites, lines, y, line);
                text = true;
            }
            pictures.over(i, y, x0, line);
        }
        if !text {
            text_over(job, (glyph, ink), sprites, lines, y, line);
        }
        // the selection tints the cell and its pictures, and the cursor
        // lies above the selection
        if job.under.selected {
            for p in line.iter_mut() {
                *p = tint(*p, job.bg);
            }
        }
        mark_over(job, sprites, lines, y, line);
    }
    Pictured::Drawn
}

/// The job's glyph tinted over the premultiplied line `prem`.
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn glyph_over(job: &Job, g: &Stored, y: i64, lines: (&mut [u32], &mut [u8], &mut [u32])) {
    let (prem, cov, src) = lines;
    let (gx, gy) = (job.r.x.saturating_sub(job.g.x), y.saturating_sub(job.g.y));
    if g.is_rgba() {
        g.argb_line(gx, gy, src);
        silhouette(job, src);
        for (p, &s) in prem.iter_mut().zip(src.iter()) {
            *p = argb_over(*p, s);
        }
    } else {
        g.mask_line(gx, gy, cov);
        for (p, &a) in prem.iter_mut().zip(cov.iter()) {
            *p = tint_over(*p, a, job.fg);
        }
        color_layer_over(job, g, (gx, gy), (prem, src));
    }
}

/// The job's text layer at display line `y`, premultiplied as a cell of
/// several layers composes it, over the pictures and the background in
/// `out`: its underline, the ink it receives from its neighbours, its
/// glyph and the lines over it, less what vtium's mark draws above the pictures
/// (ADR-1306 D7).
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn text_over(
    job: &Job,
    (glyph, ink): (Option<&Stored>, Option<&[u32]>),
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    out: &mut [u32],
) {
    if !job.draws_text(glyph.is_some()) && ink.is_none() {
        return;
    }
    let Scratch {
        cov,
        prem,
        src,
        ink: glyph_ink,
        near,
        columns,
    } = Scratch::of(lines, out.len());
    prem.fill(0);
    let mark = job.under.mark;
    if let Some(u) = job.underline.filter(|_| mark != Mark::Underline) {
        let skip = glyph.map(|glyph| Skip {
            glyph,
            ink: glyph_ink,
            near,
            columns,
        });
        deco_over(job, sprites, u, y, skip, prem, cov);
    }
    received_over(prem, ink);
    if let Some(g) = glyph {
        glyph_over(job, g, y, (&mut *prem, &mut *cov, src));
    }
    if let Some(o) = job.over {
        deco_over(job, sprites, o, y, None, prem, cov);
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_px(*o, p);
    }
}

/// vtium's mark at display line `y`, over everything else in `out`: an
/// underline cursor's line. A bar cursor's bar is drawn over the whole
/// cell once the cell is composed ([`bar_over`]).
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn mark_over(job: &Job, sprites: &Sprites, lines: &mut Lines, y: i64, out: &mut [u32]) {
    let Some(u) = job.underline.filter(|_| job.under.mark == Mark::Underline) else {
        return;
    };
    let Scratch { cov, prem, .. } = Scratch::of(lines, out.len());
    prem.fill(0);
    deco_over(job, sprites, u, y, None, prem, cov);
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_px(*o, p);
    }
}
