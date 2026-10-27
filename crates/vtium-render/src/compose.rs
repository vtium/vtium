//! Composing a row's cells into display lines: a cell of one layer
//! straight into the target, the span of cells of several layers through
//! the row scratch, a band of lines at a time.

use vtium_font::{Face, Font, Style, Width as FontWidth};
use vtium_term::Rgb;

use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};
use crate::cache::GlyphCache;
use crate::deco::{Kind, Sprites};
use crate::stored::Stored;
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

    /// Draws the row's jobs: those outside `span` straight into the
    /// target, those inside it through the row scratch, whose lines are
    /// `strip`'s length and count at most.
    pub(crate) fn draw<F: Face>(
        &mut self,
        font: &Font<F>,
        span: Option<((usize, usize), Area)>,
        strip: (usize, usize),
        target: &mut impl Target,
    ) {
        let in_span = |j: &Job| span.is_some_and(|((a, b), _)| j.posx >= a && j.posx < b);
        let Composer {
            cache,
            sprites,
            lines,
            jobs,
            text,
            ..
        } = self;
        let mut out = core::mem::take(&mut lines.out);
        for job in jobs.iter().filter(|j| !in_span(j)) {
            let glyph = glyph(cache, font, text, job);
            out.clear();
            out.resize(usize::try_from(job.r.w).unwrap_or(0), 0);
            let Ok(x) = u32::try_from(job.r.x) else {
                continue;
            };
            match glyph {
                // the common cell, a bitmap glyph on its background, a cell
                // at a time
                Some(g) if !job.layered() && g.is_bits() => {
                    let (fg, bg) = (pixel(job.fg), pixel(job.bg));
                    let gx = job.r.x.saturating_sub(job.g.x);
                    let n = usize::try_from(job.r.h).unwrap_or(0);
                    let mut y = job.r.y;
                    g.bits_lines(
                        gx,
                        y.saturating_sub(job.g.y),
                        n,
                        (fg, bg),
                        &mut out,
                        |line| {
                            if let Ok(y) = u32::try_from(y) {
                                target.write(x, y, line);
                            }
                            y = y.saturating_add(1);
                        },
                    );
                }
                _ => {
                    for y in job.r.y..job.r.bottom() {
                        cell_line(job, glyph, sprites, lines, y, &mut out);
                        if let Ok(y) = u32::try_from(y) {
                            target.write(x, y, &out);
                        }
                    }
                }
            }
        }
        lines.out = out;
        if let Some((_, area)) = span {
            self.draw_span(font, area, strip, target);
        }
    }

    /// Composes the jobs inside `area` in the row scratch, a band of lines
    /// at a time, and copies each line out once.
    fn draw_span<F: Face>(
        &mut self,
        font: &Font<F>,
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
        } = self;
        let mut top = area.y;
        while top < area.bottom() {
            let lines_here = int(band).min(area.bottom().saturating_sub(top));
            let bottom = top.saturating_add(lines_here);
            for job in jobs.iter() {
                if job.r.x < area.x
                    || job.r.x.saturating_add(job.r.w) > area.x.saturating_add(area.w)
                {
                    continue;
                }
                let glyph = glyph(cache, font, text, job);
                let dx = usize::try_from(job.r.x.saturating_sub(area.x)).unwrap_or(0);
                let jw = usize::try_from(job.r.w).unwrap_or(0);
                for y in job.r.y.max(top)..job.r.bottom().min(bottom) {
                    let at = usize::try_from(y.saturating_sub(top))
                        .unwrap_or(0)
                        .saturating_mul(w)
                        .saturating_add(dx);
                    if let Some(out) = scratch.get_mut(at..at.saturating_add(jw)) {
                        cell_line(job, glyph, sprites, lines, y, out);
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

    /// Draws `c` in `fg` on `bg` over `at`, clipped to a `width` by
    /// `height` display.
    pub(crate) fn draw_alone<F: Face>(
        &mut self,
        font: &Font<F>,
        c: char,
        (fg, bg): (Rgb, Rgb),
        at: Area,
        (width, height): (u32, u32),
        target: &mut impl Target,
    ) {
        self.text.clear();
        self.text.push(c);
        let job = Job {
            posx: 0,
            ncells: 1,
            r: at,
            g: at,
            tiles: [Area::default(); 2],
            fg,
            bg,
            underline: None,
            strike: None,
            text: Some((0, 1, FontWidth::Narrow, Style::default())),
        };
        let (sw, sh) = (int(width), int(height));
        let Composer {
            cache,
            sprites,
            lines,
            text,
            ..
        } = self;
        let glyph = glyph(cache, font, text, &job);
        let x0 = at.x.max(0);
        let x1 = at.x.saturating_add(at.w).min(sw);
        for y in at.y.max(0)..at.bottom().min(sh) {
            let mut out = core::mem::take(&mut lines.out);
            out.clear();
            out.resize(usize::try_from(at.w).unwrap_or(0), 0);
            cell_line(&job, glyph, sprites, lines, y, &mut out);
            let from = usize::try_from(x0.saturating_sub(at.x)).unwrap_or(0);
            let to = usize::try_from(x1.saturating_sub(at.x)).unwrap_or(0);
            if let (Some(run), Ok(x), Ok(y)) =
                (out.get(from..to), u32::try_from(x0), u32::try_from(y))
            {
                target.write(x, y, run);
            }
            lines.out = out;
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
                g.bits_lines(gx, gy, 1, (pixel(job.fg), pixel(job.bg)), out, |_| {});
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
