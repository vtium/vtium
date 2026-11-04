//! Composing a row's cells into display lines: each run of cells that
//! follow one another through the row scratch, a band of lines at a time,
//! each line written to the target once.

use vtium_font::{Face, Font, Style, Width as FontWidth};
use vtium_term::Rgb;

use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};
use crate::cache::GlyphCache;
use crate::deco::{Kind, Sprites};
#[cfg(feature = "images")]
use crate::picture::{Mark, Pictures, Under, over_px, tint};
use crate::stored::{Pens, Rows, Stored};
use crate::target::Target;

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
    pub(crate) underline: Option<(Kind, Rgb)>,
    pub(crate) strike: Option<Rgb>,
    /// The glyph's text in the row's text buffer, its width and style.
    pub(crate) text: Option<(usize, usize, FontWidth, Style)>,
    /// A color glyph draws as its silhouette, for a 1-bit display.
    pub(crate) silhouette: bool,
    /// The cell's background and mark, which decide how pictures over it
    /// draw.
    #[cfg(feature = "images")]
    pub(crate) under: Under,
}

impl Job {
    pub(crate) fn layered(&self) -> bool {
        self.underline.is_some() || self.strike.is_some()
    }

    /// Whether the cell's text layer draws anything: a glyph, which the
    /// cache has when `glyph` is set, an underline or a strike.
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
            scratch,
            pens,
            #[cfg(feature = "images")]
            pictures,
            #[cfg(feature = "images")]
            meeting,
        } = self;
        let run = jobs.get(start..end).unwrap_or_default();
        let mut top = area.y;
        while top < area.bottom() {
            let lines_here = int(band).min(area.bottom().saturating_sub(top));
            let bottom = top.saturating_add(lines_here);
            // the run of cells under one opaque picture being gathered
            #[cfg(feature = "images")]
            let mut covered: Option<Cover> = None;
            for job in run {
                let glyph = glyph(cache, font, text, job);
                let dx = usize::try_from(job.r.x.saturating_sub(area.x)).unwrap_or(0);
                let jw = usize::try_from(job.r.w).unwrap_or(0);
                let y0 = job.r.y.max(top);
                let at = usize::try_from(y0.saturating_sub(top))
                    .unwrap_or(0)
                    .saturating_mul(w)
                    .saturating_add(dx);
                // the job's lines in the band: the last ends at its width
                let n = usize::try_from(job.r.bottom().min(bottom).saturating_sub(y0)).unwrap_or(0);
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
                    glyph,
                    (sprites, lines),
                    (y0, w, jw),
                    out,
                ) {
                    Pictured::No => {}
                    Pictured::Drawn => continue,
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
                    Some(g) if !job.layered() && g.is_bits() => {
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
                        for (y, line) in (y0..).zip(lines_out) {
                            if let Some(line) = line.get_mut(..jw) {
                                cell_line(job, glyph, sprites, lines, y, line);
                            }
                        }
                    }
                }
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

/// The job's glyph from the cache, if it has one.
fn glyph<'c, F: Face>(
    cache: &'c mut GlyphCache,
    font: &Font<F>,
    text: &[char],
    job: &Job,
) -> Option<&'c Stored> {
    let (start, len, width, style) = job.text?;
    let chars = text.get(start..start.saturating_add(len))?;
    cache.get(font, chars, width, style)
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

/// One display line `y` of `job`, across its area, into `out`.
fn cell_line(
    job: &Job,
    glyph: Option<&Stored>,
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    out: &mut [u32],
) {
    let w = out.len();
    let x0 = job.r.x;
    lines.cov.resize(w, 0);
    lines.prem.resize(w, 0);
    lines.src.resize(w, 0);
    let cov = lines.cov.get_mut(..w).unwrap_or_default();
    let prem = lines.prem.get_mut(..w).unwrap_or_default();
    let src = lines.src.get_mut(..w).unwrap_or_default();
    let (gx, gy) = (x0.saturating_sub(job.g.x), y.saturating_sub(job.g.y));
    if !job.layered() {
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
    let tiles = job.tiles.get(..job.ncells).unwrap_or_default();
    let deco = |kind: Kind, c: Rgb, prem: &mut [u32], cov: &mut [u8]| {
        let Some(s) = sprites.get(kind) else { return };
        for t in tiles {
            s.mask_line(x0.saturating_sub(t.x), y.saturating_sub(t.y), cov);
            for (p, &a) in prem.iter_mut().zip(cov.iter()) {
                *p = tint_over(*p, a, c);
            }
        }
    };
    if let Some((kind, c)) = job.underline {
        deco(kind, c, prem, cov);
    }
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
        }
        None => {}
    }
    if let Some(c) = job.strike {
        deco(Kind::Strike, c, prem, cov);
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_bg(job.bg, p);
    }
}

/// What [`pictured`] did with a cell.
#[cfg(feature = "images")]
enum Pictured {
    /// No picture reaches it, or its mark covers them: it draws as any cell.
    No,
    /// It is drawn, with its pictures.
    Drawn,
    /// It is wholly under the opaque picture `0`, with nothing drawn over
    /// it: the cell is that picture's pixels, which its run draws.
    Covered(usize),
}

/// Cells side by side under one opaque picture, whose every display line
/// is copied from the picture at once, as the C reference sent such a run
/// as one request: picture `pic` from display column `x` to `end`, its
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
    glyph: Option<&Stored>,
    (sprites, lines): (&Sprites, &mut Lines),
    (y0, w, jw): (i64, usize, usize),
    out: &mut [u32],
) -> Pictured {
    use vtium_image::Layer;
    if job.under.mark == Mark::Opaque {
        return Pictured::No;
    }
    pictures.meeting(&job.r, meeting);
    if meeting.is_empty() {
        return Pictured::No;
    }
    // the last opaque picture over the whole cell hides what lies under it,
    // but for one below backgrounds that the cell's own background hides;
    // finding it needs no pixels, and only it and what lies over it are
    // made ready, so a hidden picture is never scaled
    let top = meeting.iter().rposition(|&i| {
        pictures.pic(i).is_some_and(|p| {
            p.opaque()
                && p.covers(&job.r)
                && (p.layer() != Layer::BelowBackground || job.under.default_bg)
        })
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
        if plain && (hides_text || !job.draws_text(glyph.is_some())) {
            return Pictured::Covered(i);
        }
    }
    for (y, line) in (y0..).zip(out.chunks_mut(w.max(1))) {
        let Some(line) = line.get_mut(..jw) else {
            continue;
        };
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
            let Some(layer) = pictures.pic(i).map(|p| p.layer()) else {
                continue;
            };
            if layer == Layer::BelowBackground && !job.under.default_bg {
                continue;
            }
            if !text && layer == Layer::AboveText {
                text_over(job, glyph, sprites, lines, y, line);
                text = true;
            }
            pictures.over(i, y, x0, line);
        }
        if !text {
            text_over(job, glyph, sprites, lines, y, line);
        }
        // the selection tints the cell and its pictures, and the cursor
        // lies above the selection
        if job.under.selected {
            for p in line.iter_mut() {
                *p = tint(*p, job.bg);
            }
        }
        mark_over(job, glyph, sprites, lines, y, line);
    }
    Pictured::Drawn
}

/// `kind` tinted `c` over the premultiplied line `prem`, across the job's
/// cells, at display line `y`.
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn deco_over(
    job: &Job,
    sprites: &Sprites,
    (kind, c): (Kind, Rgb),
    y: i64,
    prem: &mut [u32],
    cov: &mut [u8],
) {
    let Some(s) = sprites.get(kind) else { return };
    for t in job.tiles.get(..job.ncells).unwrap_or_default() {
        s.mask_line(job.r.x.saturating_sub(t.x), y.saturating_sub(t.y), cov);
        for (p, &a) in prem.iter_mut().zip(cov.iter()) {
            *p = tint_over(*p, a, c);
        }
    }
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
    }
}

/// The job's text layer at display line `y`, premultiplied as a cell of
/// several layers composes it, over the pictures and the background in
/// `out`: its underline, glyph and strike, less what vtium's mark draws
/// above the pictures.
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn text_over(
    job: &Job,
    glyph: Option<&Stored>,
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    out: &mut [u32],
) {
    if !job.draws_text(glyph.is_some()) {
        return;
    }
    let w = out.len();
    lines.cov.resize(w, 0);
    lines.prem.resize(w, 0);
    lines.src.resize(w, 0);
    let cov = lines.cov.get_mut(..w).unwrap_or_default();
    let prem = lines.prem.get_mut(..w).unwrap_or_default();
    let src = lines.src.get_mut(..w).unwrap_or_default();
    prem.fill(0);
    let mark = job.under.mark;
    if let Some(u) = job.underline.filter(|_| mark != Mark::Underline) {
        deco_over(job, sprites, u, y, prem, cov);
    }
    if let Some(g) = glyph.filter(|_| mark != Mark::Bar) {
        glyph_over(job, g, y, (&mut *prem, &mut *cov, src));
    }
    if let Some(c) = job.strike {
        deco_over(job, sprites, (Kind::Strike, c), y, prem, cov);
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_px(*o, p);
    }
}

/// vtium's mark at display line `y`, over everything else in `out`: the
/// bar of a bar cursor, or an underline cursor's line.
#[cfg(feature = "images")]
#[deny(clippy::float_arithmetic)]
fn mark_over(
    job: &Job,
    glyph: Option<&Stored>,
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    out: &mut [u32],
) {
    if matches!(job.under.mark, Mark::None | Mark::Opaque) {
        return;
    }
    let w = out.len();
    lines.cov.resize(w, 0);
    lines.prem.resize(w, 0);
    lines.src.resize(w, 0);
    let cov = lines.cov.get_mut(..w).unwrap_or_default();
    let prem = lines.prem.get_mut(..w).unwrap_or_default();
    let src = lines.src.get_mut(..w).unwrap_or_default();
    prem.fill(0);
    match job.under.mark {
        Mark::Bar => {
            let Some(g) = glyph else { return };
            glyph_over(job, g, y, (&mut *prem, &mut *cov, src));
        }
        Mark::Underline => {
            let Some(u) = job.underline else { return };
            deco_over(job, sprites, u, y, prem, cov);
        }
        Mark::None | Mark::Opaque => return,
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_px(*o, p);
    }
}
