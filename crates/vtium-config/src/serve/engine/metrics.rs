//! The engine's cell metrics: the frit's own cell, the sizes and
//! baselines its sources draw at, and the lines of each cell.

use vtium_font::spacing::Band;
use vtium_font::{Cell, Decorations, Script};

use crate::font::Font;

use super::{
    BOLD, Engine, FirstDrawn, Loaded, OwnCell, TextOut, built_in_metrics, own_regular, private_use,
};
use crate::serve::bitmap::bitmap_baseline;
use crate::serve::wire::script_baseline;
use crate::sources::{NOT_ACTIVE, SourceError};

/// Why the frit has no character cell of its own when [`Engine::load`]
/// left out its text font `spelling` for the reason `why`. The console
/// logs this reason as the font helper sends it, after a colon and before
/// a full stop of its own, so the reason starts in lower case and ends
/// with no full stop. It names the text font first, then why the font is
/// out, and then what draws the text instead. A font file that is not
/// among the active fonts comes with the command that activates it. Where
/// the frit names the font by anything but its file, such as a family, the
/// reason names the file the font helper found too, because that file is
/// the one to activate or mend.
///
/// References:
/// - ADR-1204 D5: a missing text font leaves the cell to the configuration.
/// - ADR-1205 D4: a font file the batch does not list, or that fails the
///   trust rule, counts as missing, and vtium names the file and why.
fn text_left_out(spelling: &str, why: &SourceError) -> String {
    const INSTEAD: &str = "The built-in font draws the text in its place, and the character cell comes from the configuration";
    let font = format!("the text font source={spelling}");
    match why {
        SourceError::Refused { path, because } => {
            let file = path.display().to_string();
            let font = if spelling.ends_with(&file) {
                font
            } else {
                format!("{font}, the font file {file},")
            };
            if because == NOT_ACTIVE {
                format!(
                    "{font} is not among the active fonts. {INSTEAD}. To activate the font file, run vtium-config activate {file} --install"
                )
            } else {
                format!("{font} counts as missing, because {because} {INSTEAD}")
            }
        }
        SourceError::Missing(why) => {
            format!(
                "{font} is missing: {}. {INSTEAD}",
                why.trim_end_matches('.')
            )
        }
        SourceError::Failed(why) => {
            format!(
                "{font} cannot be read: {}. {INSTEAD}",
                why.trim_end_matches('.')
            )
        }
    }
}

impl Engine {
    /// The frit's own cell (ADR-1200 D17), which its first source sets and
    /// later sources draw into. The first source is the frit's text font,
    /// its own first regular source. When [`Engine::load`] left the text
    /// font out, or the frit has no source of its own, the built-in font
    /// draws the text in its place and the frit has no own cell. The
    /// reason then names the text font and says why the font helper left
    /// it out. The console takes the cell of its configuration, as for a
    /// console with no font named. A text font that has failed as it drew is left out for the
    /// engine's life, as it is for glyphs, and the frit's next own regular
    /// source sets the cell. A layer's source never sets it. A bitmap first
    /// source gives its own cell, once every later bitmap source's cell
    /// fits in it, so each draws at a whole factor (the operator,
    /// 2026-10-28). An outline first source gives the cell `height` pixels
    /// high that the W rule gives it (the maintainer, 2026-11-01); the
    /// console names the height, and at a height of 0 there is none. An outline
    /// source whose W rule fails fails as a source that fails to draw does.
    /// Else why the frit has none: a missing text font, a text font the
    /// console left out after it made a helper fail, a later bitmap source
    /// whose cell does not fit, no height to draw an outline font at, or
    /// no source of its own left.
    ///
    /// References:
    /// - ADR-1204 D5: a missing text font leaves the cell to the
    ///   configuration, and a layer's font never sets the cell.
    pub fn own_cell(&self, height: u32) -> Result<OwnCell, String> {
        match &self.text_left_out {
            Some(TextOut::LeftOut { spelling, why, .. }) => {
                return Err(text_left_out(spelling, why));
            }
            Some(TextOut::Dropped(text, _)) => {
                return Err(format!(
                    "vtium left out the text font source={text} after it made the font helper fail, so the built-in font draws the text in its place"
                ));
            }
            Some(TextOut::NoneOwn) => {
                return Err(
                    "the font set has no font of its own, only layers, so the built-in font draws the text, and the cell comes from the configuration"
                        .to_owned(),
                );
            }
            None => {}
        }
        let mut why = Vec::new();
        let mut sources = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, (source, _))| own_regular(source));
        let (first, cell) = loop {
            let Some((i, (source, loaded))) = sources.next() else {
                why.push("the frit has no source of its own left to take a cell from".to_owned());
                return Err(why.join("; "));
            };
            if let Some(failed) = self.failed.borrow().get(&i) {
                why.push(format!("source={}: {failed}", source.spelling));
                continue;
            }
            match loaded {
                Loaded::Bitmap(font) if self.struck_bitmap(i) && height > 0 => {
                    self.decide_first(true);
                    break (source, self.struck_cell(i, font, height)?);
                }
                Loaded::Bitmap(font) => {
                    self.decide_first(true);
                    break (source, font.cell);
                }
                Loaded::Outline { .. } if height == 0 => {
                    return Err(format!(
                        "source={} is an outline font, whose cell needs the height it is drawn at",
                        source.spelling
                    ));
                }
                Loaded::Outline { font, family, .. } => {
                    // the frit's bold face joins the W rule (ADR-1200 D17)
                    let mut faces: Vec<(&[u8], &str)> = vec![(font, family)];
                    faces.extend(self.bold_face());
                    match crate::outline::cell_width(&faces, height) {
                        Ok(width) => {
                            self.decide_first(false);
                            return Ok(OwnCell::Outline(Cell { width, height }));
                        }
                        Err(e) => {
                            why.push(format!("source={}: {e}", source.spelling));
                            self.failed.borrow_mut().insert(i, e);
                        }
                    }
                }
            }
        };
        let first_at = self
            .sources
            .iter()
            .position(|(s, _)| std::ptr::eq(s, first))
            .unwrap_or(0);
        // a bitmap source with strikes takes the cell of one strike for the
        // height, and a later bitmap source that does not fit that strike's
        // cell draws nothing there, as it draws nothing in any cell it does
        // not fit
        if self.struck_bitmap(first_at) && height > 0 {
            return Ok(OwnCell::Bitmap(cell));
        }
        for (_, (source, loaded)) in self.sources.iter().enumerate().skip(first_at + 1) {
            match loaded {
                Loaded::Bitmap(font)
                    if font.cell.width > cell.width || font.cell.height > cell.height =>
                {
                    return Err(format!(
                        "source={}'s {}x{} cell does not fit source={}'s {}x{}",
                        source.spelling,
                        font.cell.width,
                        font.cell.height,
                        first.spelling,
                        cell.width,
                        cell.height
                    ));
                }
                Loaded::Bitmap(_) | Loaded::Outline { .. } => {}
            }
        }
        Ok(OwnCell::Bitmap(cell))
    }

    /// The frit's bold face, which the W rule measures beside the regular
    /// face (ADR-1200 D17), so a bold face wider than the regular one
    /// still fits the cell: the first outline source of the frit's own,
    /// never a layer's, that it gave `style=bold` and that has not failed,
    /// its bytes and its family. `None` when the frit names none.
    fn bold_face(&self) -> Option<(&[u8], &str)> {
        let failed = self.failed.borrow();
        self.sources
            .iter()
            .enumerate()
            .filter(|(i, (source, _))| {
                source.style == BOLD && source.layer.is_none() && !failed.contains_key(i)
            })
            .find_map(|(_, (_, loaded))| match loaded {
                Loaded::Outline { font, family, .. } => Some((&font[..], family.as_str())),
                Loaded::Bitmap(_) => None,
            })
    }

    /// The size, in pixels to the em, source `i` draws at in a cell
    /// `height` pixels high when it is an outline source after an outline
    /// first source: the first source's size (ADR-1200 D17). The first
    /// source is the text font ([`Engine::text_font`]), the frit's own
    /// first regular source and never a layer's, whose metrics every strike
    /// reports ([`Engine::metrics_at`]). `None` draws the source at
    /// its own size, so its ascent and descent span the height, as the
    /// first source itself and every source behind a bitmap first source
    /// do.
    pub(super) fn size(&self, i: usize, height: u32) -> Option<f32> {
        let first = self.text_font()?;
        if i == first {
            return None;
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(first) else {
            return None;
        };
        crate::outline::size(font, family, height).ok()
    }

    /// Source `i`'s baseline in a cell `height` pixels high, found once for
    /// the engine's life (ADR-1302 D3); `None` for a bitmap source or one
    /// whose font cannot be read, which the rasteriser then refuses. An
    /// outline source that draws at the first source's size
    /// ([`Engine::size`]) draws on the first source's baseline too
    /// (ADR-1200 D17), so a fallback font whose ascent reaches higher than
    /// the first source's, as Noto Sans Hebrew's and Noto Sans Arabic's
    /// reach above DejaVu Sans Mono's, stands on the same row as the first
    /// source's letters. Any other outline source stands on its own
    /// baseline.
    pub(in crate::serve) fn baseline(&self, i: usize, height: u32) -> Option<u32> {
        if let Some(&b) = self.baselines.borrow().get(&(i, height)) {
            return Some(b);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        if self.size(i, height).is_some() {
            return self.baseline(self.text_font()?, height);
        }
        let b = crate::outline::baseline(font, family, height, None, self.hinting_for(i)).ok()?;
        self.baselines.borrow_mut().insert((i, height), b);
        Some(b)
    }

    /// Source `i`'s own baseline in a cell `height` pixels high: where the
    /// source stands when it draws at its own size, its ascent and descent
    /// spanning the cell ([`Engine::fitted`]). Found once for the engine's
    /// life, as [`Engine::baseline`] finds the others.
    pub(in crate::serve) fn own_baseline(&self, i: usize, height: u32) -> Option<u32> {
        if self.size(i, height).is_none() {
            return self.baseline(i, height);
        }
        // the cache holds (i, height) for a source that draws at its own
        // size only, so a source at the first source's size keeps its own
        // baseline under a key no other lookup uses
        let key = (i.checked_add(self.sources.len())?, height);
        if let Some(&b) = self.baselines.borrow().get(&key) {
            return Some(b);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        let b = crate::outline::baseline(font, family, height, None, self.hinting_for(i)).ok()?;
        self.baselines.borrow_mut().insert(key, b);
        Some(b)
    }

    /// Whether source `i`'s glyph for `c` is a merged symbol, fitted to the
    /// first source's cell: `c` is a private-use character, where Nerd
    /// Fonts and Powerline put their symbols, and source `i` is an outline
    /// source with `cover=` lines that would otherwise draw at the first
    /// source's size ([`Engine::size`]). Such a glyph draws at the source's
    /// own size, at which the source's ascent and descent span the cell,
    /// and on the source's own baseline at that size. A symbol font made
    /// for merging, such as Symbols Nerd Font Mono, lays out its symbols
    /// in that box, so they fill the cell's height as the font means them
    /// to: a Powerline separator runs from the cell's top to its bottom,
    /// and an icon stands in the middle of the cell. A symbol whose ink
    /// still passes its cells is drawn smaller, as any later source's
    /// glyph is.
    ///
    /// Letters a `cover=` source gives, such as the ideographs of
    /// `cover=gb2312`, keep the first source's size and baseline, so every
    /// script on a line shares the first source's baseline.
    ///
    /// References:
    /// - ADR-1200 D17: later sources draw at the first source's size, on its baseline.
    /// - ADR-1302 D1: font merging fits a merged symbol before ink reaches past its cell.
    pub(super) fn fitted(&self, i: usize, c: u32) -> bool {
        if !private_use(c) {
            return false;
        }
        let first = self.text_font();
        let outline = |i: usize| matches!(self.sources.get(i), Some((_, Loaded::Outline { .. })));
        let merged = self
            .sources
            .get(i)
            .is_some_and(|(source, _)| !source.cover.is_empty());
        merged && first.is_some_and(|first| first != i && outline(first)) && outline(i)
    }

    /// The strike that source `i` draws from in a text band `cell`, where
    /// one of its strikes draws there and not the source's own font.
    ///
    /// For an outline source, it is the tallest of its strikes that fits
    /// the cell, when the cell is no taller than the tallest strike. Above
    /// that height, and in a cell that no strike fits, the source draws its
    /// outline. A character the strike lacks draws from the outline at
    /// every height.
    ///
    /// For a bitmap source, the source and its strikes are one font at
    /// several heights, and the tallest of them that fits the cell draws
    /// alone. So a cell between two strikes' heights draws the shorter
    /// strike, and a cell taller than every strike draws the tallest at the
    /// largest whole factor that fits. `None` when the source's own font is
    /// that tallest one, or when nothing fits the cell, where the source
    /// draws nothing.
    ///
    /// References:
    /// - ADR-1204 D2: an outline source's strikes up to the tallest
    ///   strike's height, and a bitmap source's strike for each cell.
    pub(super) fn strike_at(&self, i: usize, cell: Cell) -> Option<&Loaded> {
        let strikes = self.strikes.get(i)?;
        let height = |l: &Loaded| match l {
            Loaded::Bitmap(font) => Some(font.cell),
            Loaded::Outline { .. } => None,
        };
        let fits = |c: Cell| c.width <= cell.width && c.height <= cell.height;
        let tallest_fitting = || {
            strikes
                .iter()
                .filter(|l| height(l).is_some_and(fits))
                .max_by_key(|l| height(l).map_or(0, |c| c.height))
        };
        match self.sources.get(i)? {
            (_, Loaded::Bitmap(own)) => {
                let strike = tallest_fitting()?;
                let tall = height(strike)?.height;
                (!fits(own.cell) || tall > own.cell.height).then_some(strike)
            }
            (_, Loaded::Outline { .. }) => {
                let tallest = strikes.iter().filter_map(height).map(|c| c.height).max()?;
                if cell.height > tallest {
                    return None;
                }
                tallest_fitting()
            }
        }
    }

    /// Whether source `i` is a bitmap source with strikes, one font at
    /// several heights.
    fn struck_bitmap(&self, i: usize) -> bool {
        matches!(self.sources.get(i), Some((_, Loaded::Bitmap(_))))
            && self.strikes.get(i).is_some_and(|s| !s.is_empty())
    }

    /// The row of a text band `cell` where the top of the glyphs of a
    /// bitmap first source goes, `font` drawn at `scale`, where `at` is the
    /// source's place. A source of one height fills the cell down to its
    /// bottom row. A bitmap source with strikes is centred in the cell,
    /// which a cell between two strikes' heights or taller than every
    /// strike leaves room in.
    ///
    /// References:
    /// - ADR-1204 D2: the strike drawn in a cell between two strikes, or
    ///   above the tallest, is centred.
    fn bitmap_top(&self, at: Option<usize>, font: &Font, cell: Cell, scale: u32) -> u32 {
        let room = cell
            .height
            .saturating_sub(font.cell.height.saturating_mul(scale));
        if at.is_some_and(|i| self.struck_bitmap(i)) {
            room / 2
        } else {
            room
        }
    }

    /// The cell that bitmap source `i`, whose own font is `own`, gives at
    /// `height`, the automatic rule's height, as its strikes give it: the
    /// cell of the tallest of the source and its strikes that is no taller
    /// than `height`. The console takes a whole multiple of that cell up to
    /// `height`, so above the tallest strike's height the cell is a whole
    /// multiple of the tallest strike's. Else why there is none: no strike
    /// is that short, and the built-in font draws the text.
    ///
    /// References:
    /// - ADR-1204 D2: the cell of the tallest strike no taller than the
    ///   automatic rule's height, and a whole multiple of the tallest above
    ///   it.
    fn struck_cell(&self, i: usize, own: &Font, height: u32) -> Result<Cell, String> {
        let strikes = self.strikes.get(i).map_or(&[][..], Vec::as_slice);
        let cells = std::iter::once(own.cell).chain(strikes.iter().filter_map(|l| match l {
            Loaded::Bitmap(font) => Some(font.cell),
            Loaded::Outline { .. } => None,
        }));
        let shortest = cells
            .clone()
            .map(|c| c.height)
            .min()
            .unwrap_or(own.cell.height);
        cells
            .filter(|c| c.height <= height)
            .max_by_key(|c| c.height)
            .ok_or_else(|| {
                let source = self.sources.get(i).map_or("", |(s, _)| s.spelling.as_str());
                format!(
                    "the shortest strike of source={source} is {shortest} pixels high, taller than the {height}-pixel cell, so the built-in font draws the text in the cell of the configuration"
                )
            })
    }

    /// The frit's first regular source, the one whose baseline and lines
    /// every glyph at `cell` shares, as it draws in that text band: the
    /// text font at its place in `sources`, as its strike at that cell
    /// where it has one ([`Engine::strike_at`]), else as itself, so its
    /// baseline and lines are the strike's where the strike draws, as a
    /// bitmap first source's are. Where the built-in font draws the text
    /// in place of a missing text font, it is the built-in font's cell and
    /// baseline at that height. A layer's source is never the first.
    fn first_drawn(&self, cell: Cell) -> Option<FirstDrawn<'_>> {
        if let Some(face) = self.built_in_at(cell) {
            return Some(FirstDrawn::BuiltIn(Loaded::Bitmap(built_in_metrics(&face))));
        }
        let first = self.text_font()?;
        let (_, own) = self.sources.get(first)?;
        Some(FirstDrawn::Source(
            first,
            self.strike_at(first, cell).unwrap_or(own),
        ))
    }

    /// The first regular source's own box in a text band `cell` (ADR-1202
    /// D1), as its first row and its rows: a bitmap source's cell at the
    /// whole factor it draws at, on the band's bottom row; an outline
    /// source's rows from its ascent to its descent about its baseline;
    /// else the whole band.
    pub(super) fn own_box(&self, cell: Cell) -> (u32, u32) {
        let band = (0, cell.height);
        let Some(first) = self.first_drawn(cell) else {
            return band;
        };
        match first.loaded() {
            Loaded::Bitmap(font) => {
                let scale = (cell.width / font.cell.width.max(1))
                    .min(cell.height / font.cell.height.max(1));
                let tall = font.cell.height * scale;
                if scale == 0 {
                    return band;
                }
                (self.bitmap_top(first.at(), font, cell, scale), tall)
            }
            Loaded::Outline { font, family, .. } => {
                let (Some(base), Ok((above, below))) = (
                    first.at().and_then(|i| self.baseline(i, cell.height)),
                    crate::outline::own_box(font, family, cell.height, None),
                ) else {
                    return band;
                };
                let top = base.saturating_sub(above);
                (top, (base + below).min(cell.height) - top)
            }
        }
    }

    /// The baseline and the underline and strike-through a strike at
    /// `cell` reports (ADR-1302 D3, ADR-1200 D2): those of the frit's
    /// first regular source, the font that sets the cell, for an outline
    /// font, so every answer at one cell reports the same ones even after
    /// that source fails as it draws. A bitmap first source reports the
    /// lines of a face with no metrics of its own, and the baseline it is
    /// sheared about ([`bitmap_baseline`]) as its glyphs sit in the cell,
    /// scaled and on the cell's bottom row, so a reader that shears its
    /// glyphs for italic (vtium-font's set reader) leans them as the helper
    /// does. One whose font cannot be read, or that fits the cell at no
    /// whole factor, reports the cell's bottom row.
    pub(in crate::serve) fn metrics_at(&self, cell: Cell) -> (u32, Decorations) {
        let plain = (cell.height, Decorations::for_height(cell.height));
        let Some(first) = self.first_drawn(cell) else {
            return plain;
        };
        if let Loaded::Bitmap(font) = first.loaded() {
            let scale =
                (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
            if scale == 0 {
                return plain;
            }
            let top = self.bitmap_top(first.at(), font, cell, scale);
            return (top + bitmap_baseline(font) * scale, plain.1);
        }
        let Loaded::Outline { font, family, .. } = first.loaded() else {
            return plain;
        };
        let Some(at) = first.at() else {
            return plain;
        };
        let Some(base) = self.baseline(at, cell.height) else {
            return plain;
        };
        let lines = crate::outline::decorations(
            font,
            family,
            (cell.height, base),
            None,
            self.hinting_for(at),
        )
        .unwrap_or(plain.1);
        (base, lines)
    }

    /// The lines of a cell in `script` at `cell`, the text band, whose
    /// normal glyphs stand on row `base` with the lines `normal`
    /// ([`Engine::metrics_at`]): the frit's first source's, found by its
    /// kind (ADR-1202 D8 step 3). An outline source's are measured at the
    /// script size from the script baseline ([`script_baseline`]), with the
    /// overline on the top row of the small own box. A bitmap source with a
    /// smaller size in D3's range takes that size's lines, the lines of its
    /// height at the multiple it is drawn at, placed whole against the
    /// band; one with none takes its normal lines through D4's map. Both
    /// come from vtium-font's script rule ([`vtium_font::script::lines`]),
    /// which the console's own fonts take too. The baseline's lines are
    /// `normal`.
    pub(super) fn script_lines(
        &self,
        cell: Cell,
        script: Script,
        (base, normal): (u32, Decorations),
    ) -> Decorations {
        use vtium_font::script::lines;
        if script == Script::Baseline {
            return normal;
        }
        let own = self.own_box(cell);
        let mapped = lines::mapped(normal, script, own, cell.height, Band::NONE);
        let Some(first) = self.first_drawn(cell) else {
            return mapped;
        };
        match first.loaded() {
            Loaded::Bitmap(font) => {
                let scale = (cell.width / font.cell.width.max(1))
                    .min(cell.height / font.cell.height.max(1));
                let others = first
                    .at()
                    .and_then(|i| self.smaller.get(i))
                    .map_or(&[][..], Vec::as_slice);
                let strikes: Vec<Cell> = std::iter::once(font.cell)
                    .chain(others.iter().map(|f| f.font.cell))
                    .collect();
                let chosen =
                    font.cell.height.checked_mul(scale).and_then(|normal| {
                        vtium_font::script::smaller(normal, cell.width, &strikes)
                    });
                let Some(chosen) = chosen else {
                    return mapped;
                };
                let Some(&own) = strikes.get(chosen.strike) else {
                    return mapped;
                };
                let j = chosen.multiple;
                let size = (own.width * j, own.height * j);
                let at = lines::at_multiple(Decorations::for_height(own.height), own.height, j);
                lines::placed(at, script, size, (cell.width, cell.height), Band::NONE)
            }
            Loaded::Outline { font, family, .. } => {
                let Some(at) = first.at() else {
                    return mapped;
                };
                let Ok(size) = crate::outline::size(font, family, cell.height) else {
                    return mapped;
                };
                let (num, den) = vtium_font::SCRIPT_SCALE;
                let small = size * num as f32 / den as f32;
                let baseline = script_baseline(script, own, base, cell.height);
                let measured = crate::outline::decorations(
                    font,
                    family,
                    (cell.height, baseline),
                    Some(small),
                    self.hinting_for(at),
                );
                let Ok(measured) = measured else {
                    return mapped;
                };
                let (_, tall) = vtium_font::script::size((1, own.1));
                let (_, top) =
                    vtium_font::script::place(script, (1, tall), (1, cell.height), Band::NONE);
                Decorations {
                    overline: top.min(cell.height.saturating_sub(measured.underline_thickness)),
                    ..measured
                }
            }
        }
    }

    /// Why no source gives a glyph at `cell`: each bitmap source whose own
    /// cell does not fit it, and each source that failed as it drew.
    pub fn none_at(&self, cell: Cell) -> String {
        let failed = self.failed.borrow();
        let unfit: Vec<String> = self
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, (source, loaded))| match (failed.get(&i), loaded) {
                (Some(why), _) => Some(format!("source={}: {why}", source.spelling)),
                (None, Loaded::Bitmap(f))
                    if f.cell.width > cell.width || f.cell.height > cell.height =>
                {
                    Some(format!(
                        "source={}: its {}x{} cell does not fit",
                        source.spelling, f.cell.width, f.cell.height
                    ))
                }
                _ => None,
            })
            .collect();
        let mut why = format!("the frit gives no glyph at {}x{}", cell.width, cell.height);
        if !unfit.is_empty() {
            why.push_str(": ");
            why.push_str(&unfit.join("; "));
        }
        why
    }

    /// The most bytes one glyph at `cell` can take: two cells of
    /// premultiplied RGBA when a source is an outline font, which may draw
    /// in color, else two cells of 1-bit rows.
    pub(in crate::serve) fn glyph_bytes_max(&self, cell: Cell) -> usize {
        let outline = self
            .sources
            .iter()
            .any(|(_, l)| matches!(l, Loaded::Outline { .. }));
        let (w, h) = (cell.width.saturating_mul(2), cell.height);
        let bytes = if outline {
            w.saturating_mul(h).saturating_mul(4)
        } else {
            w.div_ceil(8).saturating_mul(h)
        };
        usize::try_from(bytes).unwrap_or(usize::MAX)
    }
}
