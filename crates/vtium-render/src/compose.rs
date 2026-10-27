//! Composing a row's cells into display lines: each run of cells that
//! follow one another through the row scratch, a band of lines at a time,
//! each line written to the target once.

use vtium_font::{Face, Font, Style, Width as FontWidth};
use vtium_term::Rgb;

use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};
use crate::cache::GlyphCache;
use crate::deco::{Kind, Sprites};
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

    fn right(&self) -> i64 {
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
}

impl Job {
    pub(crate) fn layered(&self) -> bool {
        self.underline.is_some() || self.strike.is_some()
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
    sprites: Sprites,
    lines: Lines,
    pub(crate) jobs: Vec<Job>,
    pub(crate) text: Vec<char>,
    scratch: Vec<u32>,
    pens: Pens,
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
        } = self;
        let run = jobs.get(start..end).unwrap_or_default();
        let mut top = area.y;
        while top < area.bottom() {
            let lines_here = int(band).min(area.bottom().saturating_sub(top));
            let bottom = top.saturating_add(lines_here);
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
