//! The engine's drawing: the `draw_*` methods, the glyphs of a cell, and
//! the halves of a line of double height that a line of double width takes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::spacing::{self, Band, Fit, SpacedCell};
use vtium_font::{
    Cell, Decorations, Doubled, Glyph, REACH_ROWS, SET_CHUNK_BYTES, Script, Style, Variant, Width,
};

use crate::font::Font;
use crate::frit::Source;
use crate::made::Made;
use crate::outline::{OutlineRequest, Placed, rasterise};
use crate::shaping::ZWJ;

#[cfg(doc)]
use super::Step;
use super::{
    Engine, Loaded, MadeSet, OwnCell, REGULAR, TextOut, default_fit, is_ligature_piece, ranges,
    steps,
};
use crate::serve::bitmap::{
    SmallerGlyph, bitmap_baseline, bitmap_origin, glyph_top, pack_bits, place_styled, script_bitmap,
};
use crate::serve::wire::script_baseline;
#[cfg(doc)]
use crate::serve::wire::{ASK_PARTS, ServeRequest};

impl Engine {
    /// The regular glyphs at `cell` for `want`, sorted codepoints, as
    /// `Engine::glyphs` draws them.
    pub fn draw(&self, cell: Cell, want: &[u32]) -> Result<Option<Vec<u8>>, String> {
        let mut made = BTreeMap::new();
        self.glyphs(
            cell,
            Band::NONE,
            (REGULAR.into(), None),
            want,
            (&[], false),
            &mut made,
        )
    }

    /// The regular glyphs at `cell` for `clusters`, sorted, as
    /// `Engine::glyphs` draws them.
    pub fn draw_clusters(
        &self,
        cell: Cell,
        clusters: &[Vec<u32>],
    ) -> Result<Option<Vec<u8>>, String> {
        let mut made = BTreeMap::new();
        self.glyphs(
            cell,
            Band::NONE,
            (REGULAR.into(), None),
            &[],
            (clusters, false),
            &mut made,
        )
    }

    /// The glyphs of `style` at `at`, a cell and the rows its line spacing
    /// adds (a plain cell adds none), for `want`, sorted codepoints, and
    /// `clusters`, sorted, as `Engine::glyphs` draws them, with the parts
    /// of clusters where `parts` says, as an [`ASK_PARTS`] gets them.
    pub fn draw_style(
        &self,
        at: impl Into<SpacedCell>,
        style: Style,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_variant(at, style.into(), want, (clusters, parts))
    }

    /// The glyphs of `variant`, a style and a script, at `at`, as
    /// [`Engine::draw_style`] draws a style's: a superscript or subscript
    /// ask's answer (ADR-1202 D11).
    pub fn draw_variant(
        &self,
        at: impl Into<SpacedCell>,
        variant: Variant,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_made(at, variant, want, (clusters, parts))
            .map(|(set, _)| set)
    }

    /// The glyphs of `variant` at `at` for `want` and `clusters`, as
    /// [`Engine::draw_variant`] draws them, and how the engine made each
    /// superscript or subscript glyph of a codepoint it drew, by the
    /// codepoint ([`Made`], ADR-1202 D14): none for normal glyphs, and none
    /// for clusters.
    pub fn draw_made(
        &self,
        at: impl Into<SpacedCell>,
        variant: Variant,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<MadeSet, String> {
        let at = at.into();
        let mut made = BTreeMap::new();
        let set = self.glyphs(
            at.cell,
            at.band,
            (variant, None),
            want,
            (clusters, parts),
            &mut made,
        )?;
        Ok((set, made))
    }

    /// The glyphs of `variant` at `at` for `want`, sorted codepoints, drawn
    /// at the double size `doubled`, as a double ask's answer
    /// ([`ServeRequest::doubled`]): a set whose strikes are at the cell
    /// [`Doubled::cell`] gives for `at`'s cell.
    pub fn draw_doubled(
        &self,
        at: impl Into<SpacedCell>,
        kind: (Variant, Doubled),
        want: &[u32],
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_doubled_clusters(at, kind, want, &[])
    }

    /// [`Engine::draw_doubled`] for `want` and for `clusters`, sorted, as a
    /// double ask that names clusters is answered: each cluster shaped
    /// whole at the double size, its color glyphs among them.
    pub fn draw_doubled_clusters(
        &self,
        at: impl Into<SpacedCell>,
        (variant, doubled): (Variant, Doubled),
        want: &[u32],
        clusters: &[Vec<u32>],
    ) -> Result<Option<Vec<u8>>, String> {
        let at = at.into();
        let mut made = BTreeMap::new();
        self.glyphs(
            at.cell,
            at.band,
            (variant, Some(doubled)),
            want,
            (clusters, false),
            &mut made,
        )
    }

    /// One note for each `script-source=` font that gives no superscript
    /// or subscript in the frit's own cell (ADR-1202 D12): the font whose
    /// height, at no whole multiple that fits the cell, lies from one half
    /// to three quarters of the height its source draws at there. The own
    /// cell is the frit's `cell=`, else its first source's when that is a
    /// bitmap font; a frit whose own cell needs a height has no notes. Each
    /// note names the frit and the line, as a warning does.
    pub fn script_notes(&self) -> Vec<String> {
        let own = self.cell.or_else(|| match self.own_cell(0) {
            Ok(OwnCell::Bitmap(cell)) => Some(cell),
            _ => None,
        });
        let Some(cell) = own else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        for ((_, loaded), smaller) in self.sources.iter().zip(&self.smaller) {
            let Loaded::Bitmap(font) = loaded else {
                continue;
            };
            let scale =
                (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
            let normal = font.cell.height * scale;
            for small in smaller {
                let Some((line, spelling)) = &small.named else {
                    continue;
                };
                if scale > 0
                    && vtium_font::script::smaller(normal, cell.width, &[small.font.cell]).is_some()
                {
                    continue;
                }
                let lowest = normal.div_ceil(2);
                let highest = normal * 3 / 4;
                notes.push(format!(
                    "{}:{line}: script-source={spelling} gives no superscripts or subscripts in the font set's own {}x{} character cell. Its characters are {} pixels high, and there a smaller size must be from {lowest} to {highest} pixels high, from half to three quarters of the {normal}-pixel characters of the font above it.",
                    self.name, cell.width, cell.height, small.font.cell.height
                ));
            }
        }
        notes
    }

    /// The glyph that a smaller size of bitmap source `i` gives `c` for a
    /// superscript or subscript (ADR-1202 D3): the strike that
    /// [`vtium_font::script::smaller`] chooses among the source's own
    /// strike, `font` drawn at `scale`, and its smaller sizes, for a normal
    /// glyph `font`'s height times `scale` and cells `cell_width` across.
    /// The source's own strike at a smaller multiple draws `c` as its glyph
    /// `g`. `None` when no strike lies in the range, and when the chosen
    /// strike lacks `c` or draws it at another width than `g`; the glyph
    /// is then made small by D4. Only the source's own font and the
    /// smaller sizes it names are asked (D15).
    fn smaller_glyph<'a>(
        &'a self,
        i: usize,
        (font, scale): (&'a Font, u32),
        cell_width: u32,
        c: u32,
        g: u32,
    ) -> Option<SmallerGlyph<'a>> {
        let others = self.smaller.get(i).map_or(&[][..], Vec::as_slice);
        let strikes: Vec<Cell> = std::iter::once(font.cell)
            .chain(others.iter().map(|f| f.font.cell))
            .collect();
        let normal = font.cell.height.checked_mul(scale)?;
        let chosen = vtium_font::script::smaller(normal, cell_width, &strikes)?;
        let f = match chosen.strike {
            0 => font,
            n => &others.get(n - 1)?.font,
        };
        let glyph = match chosen.strike {
            0 => g,
            _ => *f.map.get(&c)?,
        };
        let (width, rows) = f.glyphs.get(glyph as usize)?;
        let (own, _) = font.glyphs.get(g as usize)?;
        (width == own).then_some(SmallerGlyph {
            strike: chosen.strike,
            glyph,
            multiple: chosen.multiple,
            font: f,
            rows,
        })
    }

    /// How a script glyph that a smaller size `small` of bitmap source `i`
    /// gives was made, for a source drawn at `scale` (ADR-1202 D3, D14):
    /// the source's own font at a smaller multiple, another height of a
    /// CPI source, or a `script-source=` font, named by its spelling.
    fn made_smaller(&self, i: usize, scale: u32, small: SmallerGlyph<'_>) -> Made {
        let (height, multiple) = (small.font.cell.height, small.multiple);
        let named = small
            .strike
            .checked_sub(1)
            .and_then(|n| self.smaller.get(i)?.get(n))
            .map(|f| f.named.as_ref());
        match named {
            None => Made::Multiple {
                normal: scale,
                small: multiple,
            },
            Some(None) => Made::Height { height, multiple },
            Some(Some((_, spelling))) => Made::Named {
                spelling: spelling.clone(),
                height,
                multiple,
            },
        }
    }

    /// How the glyph that source `source` gives for `c` uses a row made
    /// taller by line spacing (ADR-1302 D13): the `fit=` of the source's
    /// set that holds `c`, else `c`'s default ([`default_fit`]).
    fn fit(source: &Source, c: u32) -> Fit {
        source.fit(c).unwrap_or_else(|| default_fit(c))
    }

    /// `request`, for source `i`'s normal glyphs at `cell` in the text band,
    /// asking for its glyphs in `script` instead (ADR-1202 D1, D2, D5,
    /// D8): drawn at two thirds of the size its normal glyphs are drawn at
    /// ([`vtium_font::SCRIPT_SCALE`]), rasterised at that size rather than
    /// scaled from a full-size raster, on the script baseline that the
    /// first source's own box ([`Engine::own_box`]) and its baseline `base`
    /// give ([`script_baseline`]), with no
    /// ink above or below the whole cell. `None` when the size the source
    /// draws at cannot be read.
    fn script_request(
        &self,
        request: OutlineRequest,
        (i, fitted): (usize, bool),
        (cell, band, base): (Cell, Band, u32),
        script: Script,
    ) -> Option<OutlineRequest> {
        let size = self.drawn_size((i, fitted), cell.height)?;
        let (num, den) = vtium_font::SCRIPT_SCALE;
        let small = size * num as f32 / den as f32;
        Some(OutlineRequest {
            cluster_width: Default::default(),
            sizes: vec![small.to_bits()],
            baselines: vec![script_baseline(
                script,
                self.own_box(cell),
                base,
                cell.height,
            )],
            reaches: vec![(band.above, band.below)],
            script,
            ..request
        })
    }

    /// The size, in pixels to the em, outline source `i` draws its normal
    /// glyphs at in a text band `height` pixels high: the first source's
    /// size for a later source ([`Engine::size`]), and its own size for the
    /// first source and for a merged symbol (`fitted`, [`Engine::fitted`]).
    /// `None` for a bitmap source, or one whose font cannot be read.
    fn drawn_size(&self, (i, fitted): (usize, bool), height: u32) -> Option<f32> {
        if let Some(size) = self.size(i, height).filter(|_| !fitted) {
            return Some(size);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        crate::outline::size(font, family, height).ok()
    }

    /// `request`, source `i`'s request at the text band `cell`, drawn at
    /// the double size `doubled` instead. The glyphs take the size and the
    /// baseline that the normal cell gives them. On a line of double height
    /// both are twice as large. On a line of double width the request
    /// stretches each outline to twice its width
    /// ([`OutlineRequest::double_width`]). Each glyph is then the normal
    /// glyph at twice its width, and twice its height on a line of double
    /// height, rendered from its outlines at that size. `None` when the
    /// source has no size or no baseline at the cell.
    fn doubled_request(
        &self,
        request: OutlineRequest,
        (i, fitted): (usize, bool),
        cell: Cell,
        doubled: Doubled,
    ) -> Option<OutlineRequest> {
        let rows = doubled.rows();
        let size = self.drawn_size((i, fitted), cell.height)? * rows as f32;
        let baseline = match fitted {
            true => self.own_baseline(i, cell.height),
            false => self.baseline(i, cell.height),
        }?;
        Some(OutlineRequest {
            sizes: vec![size.to_bits()],
            baselines: vec![baseline.checked_mul(rows)?],
            ..request
        })
    }

    /// The glyphs of `variant` at `cell` for `want`, sorted codepoints, and
    /// `clusters`, sorted, as a vitrified set of that one cell whose
    /// strikes are of the variant's style and script; `None` when no source
    /// gives any. A superscript or subscript glyph comes from the source
    /// the steps pick for the normal glyph (ADR-1202 D15), made small by
    /// that source's kind ([`Engine::script_request`],
    /// [`script_bitmap`]). Where line
    /// spacing adds `band` to the cell, each glyph is drawn in the cell's
    /// text band and placed in the whole cell ([`spaced`]). Each
    /// [`Step`] of the style ([`steps`]) draws in turn what the steps
    /// before it left: each codepoint from the first source in the frit's
    /// order that the step's face is and that gives it, and each cluster
    /// from the first such outline source that covers its first scalar
    /// value other than U+200D and shapes it (ADR-1200 D21), with the
    /// step's synthesis drawn over the glyph (ADR-1302 D4). A codepoint or
    /// cluster that asks for emoji presentation
    /// ([`crate::shaping::asks_for_emoji`]) is first drawn, by the same
    /// order, from the first source that gives it a color glyph, and only
    /// when none does from the first that gives it any (ADR-1200 D21). A source that
    /// fails as it draws is left out, and the next one gives its glyphs.
    /// Where `parts_asked`, as for an [`ASK_PARTS`], the set also holds the
    /// part of a cluster asked for that a source draws only without some of
    /// its marks, under the part's own key (ADR-1200 D22 step 3). Each
    /// superscript or subscript glyph of a codepoint goes into `made` with
    /// the way the engine made it (ADR-1202 D14).
    pub(in crate::serve) fn glyphs(
        &self,
        cell: Cell,
        band: Band,
        (variant, doubled): (Variant, Option<Doubled>),
        want: &[u32],
        (clusters, parts_asked): (&[Vec<u32>], bool),
        made: &mut BTreeMap<u32, Made>,
    ) -> Result<Option<Vec<u8>>, String> {
        let Variant { style, script } = variant;
        // the glyphs are drawn in the text band, and the strike's baseline
        // and lines move down by the rows added above it
        let whole = cell;
        let cell = Cell {
            width: whole.width,
            height: whole
                .height
                .checked_sub(band.added())
                .filter(|&h| h > 0)
                .ok_or("line spacing that leaves the cell no text band")?,
        };
        let (base, lines) = self.metrics_at(cell);
        // a bitmap source's glyphs stand on this baseline in the text band
        let text_base = base;
        // a superscript or subscript strike reports the lines of its own
        // size, where its glyphs lie (ADR-1202 D8); a normal strike's
        // overline stays on the cell's top row
        let script_lines = self.script_lines(cell, script, (base, lines));
        let (base, lines) = (
            base.saturating_add(band.above),
            Decorations {
                underline: script_lines.underline.saturating_add(band.above),
                strike: script_lines.strike.saturating_add(band.above),
                overline: match script {
                    Script::Baseline => 0,
                    _ => script_lines.overline.saturating_add(band.above),
                },
                ..script_lines
            },
        );
        // a double ask draws each glyph in the cell its size gives, and its
        // strikes report the normal cell's baseline and lines at that size;
        // the sources are measured at the normal cell all the same
        if doubled.is_some() && script != Script::Baseline {
            return Err("a superscript or subscript at a double size".to_owned());
        }
        let rows = doubled.map_or(1, Doubled::rows);
        let (drawn, drawn_whole, drawn_band) = match doubled {
            None => (cell, whole, band),
            Some(d) => (
                d.cell(cell).ok_or("a double size past the largest cell")?,
                d.cell(whole).ok_or("a double size past the largest cell")?,
                d.band(band),
            ),
        };
        let (base, lines) = (
            base.saturating_mul(rows),
            Decorations {
                underline: lines.underline.saturating_mul(rows),
                underline_thickness: lines.underline_thickness.saturating_mul(rows),
                strike: lines.strike.saturating_mul(rows),
                strike_thickness: lines.strike_thickness.saturating_mul(rows),
                overline: lines.overline.saturating_mul(rows),
            },
        );
        let strike = |format| {
            StrikeBuilder::new(format, drawn_whole, style)
                .script(script)
                .baseline(base)
                .decorations(lines)
                .compress(false)
        };
        let (mut bits, mut r8, mut rgba) = (
            strike(PixelFormat::Bits),
            strike(PixelFormat::R8),
            strike(PixelFormat::Rgba),
        );
        let mut mapped: BTreeSet<u32> = BTreeSet::new();
        // color glyphs in one cell held back for a text glyph from a later
        // source, the first source's for each character
        let mut in_one_cell: Vec<(char, Placed, Fit)> = Vec::new();
        let mut shaped: BTreeSet<&[u32]> = BTreeSet::new();
        // the piece keys of ligature runs that a source has already
        // decided, whether it joins them or not
        let mut decided: BTreeSet<&[u32]> = BTreeSet::new();
        let mut parts: BTreeSet<Vec<u32>> = BTreeSet::new();
        let steps = steps(style);
        // an outline source with a strike at this cell draws the strike's
        // glyphs first, and its outline the characters the strike lacks;
        // a bitmap source with a strike at this cell draws the strike alone
        //
        // References:
        // - ADR-1204 D2: an outline source's strikes, and the outline for
        //   what they lack; a bitmap source's strike for each cell.
        let order = steps.iter().flat_map(|step| {
            self.sources
                .iter()
                .enumerate()
                .filter(move |(_, (source, _))| source.style == step.face)
                .flat_map(move |(i, (source, loaded))| {
                    let strike = self.strike_at(i, cell);
                    let own = match (loaded, strike) {
                        (Loaded::Bitmap(_), Some(_)) => None,
                        _ => Some((i, (source, loaded), step.synth)),
                    };
                    strike
                        .map(|strike| (i, (source, strike), step.synth))
                        .into_iter()
                        .chain(own)
                })
        });
        // what asks for emoji presentation goes first to the sources in
        // order that give it a color glyph, then everything left to the
        // sources in order (ADR-1200 D21)
        let passes = [true, false]
            .into_iter()
            .flat_map(|color| order.clone().map(move |source| (color, source)));
        // a merged symbol font's private-use symbols are drawn in a request
        // of their own, at the font's own size ([`Engine::fitted`]), before
        // the rest of what the source gives
        let passes = passes.flat_map(|pass| [true, false].map(|fitted| (pass, fitted)));
        // what a double ask leaves to the console, which doubles the pixels
        // of its glyph at the normal size: the codepoints a bitmap source
        // draws; and on a line of double width, the codepoints the
        // rasteriser leaves out because a color glyph draws them
        // ([`Outline::color`]), which `squashed` holds and which are drawn
        // below from a line of double height
        let mut left_out: BTreeSet<u32> = BTreeSet::new();
        let mut squashed: BTreeSet<u32> = BTreeSet::new();
        let wide_line = doubled == Some(Doubled::Width);
        // where the built-in font draws the text in place of a missing text
        // font, it takes the text font's place in the order: after the
        // pass for color glyphs and after the sources of a bold or italic
        // face, the turn of the first regular source after the text font's
        // line leaves each character the built-in font holds, and each
        // cluster led by one, to the console, which draws them from the
        // built-in font. A layer spliced in before that line draws first.
        let built_in = self.built_in_at(cell);
        let text_line = match &self.text_left_out {
            Some(TextOut::LeftOut { line, .. } | TextOut::Dropped(_, line)) => *line,
            Some(TextOut::NoneOwn) | None => 0,
        };
        let mut standing: Option<BTreeSet<u32>> = None;
        for ((color_only, (i, (source, loaded), synth)), fitted) in passes {
            if self.failed.borrow().contains_key(&i) {
                continue;
            }
            if !color_only
                && source.style == REGULAR
                && source.line > text_line
                && standing.is_none()
            {
                use vtium_font::Face;
                standing = Some(match &built_in {
                    Some(face) => want
                        .iter()
                        .copied()
                        .filter(|c| !mapped.contains(c))
                        .filter(|&c| char::from_u32(c).is_some_and(|ch| face.has_glyph(ch)))
                        .collect(),
                    None => BTreeSet::new(),
                });
            }
            let built_in_draws = |c: u32| standing.as_ref().is_some_and(|s| s.contains(&c));
            let asked = |k: &[u32]| !color_only || crate::shaping::asks_for_emoji(k);
            let left: Vec<u32> = want
                .iter()
                .copied()
                .filter(|&c| {
                    !mapped.contains(&c)
                        && !built_in_draws(c)
                        && !left_out.contains(&c)
                        && source.covers(c)
                        && asked(&[c])
                        && self.fitted(i, c) == fitted
                })
                .collect();
            let lead = |k: &[u32]| k.iter().copied().find(|&c| c != ZWJ);
            // a cluster is shaped whole with the rest of the source's glyphs
            let unshaped: Vec<&[u32]> = clusters
                .iter()
                .map(Vec::as_slice)
                .filter(|k| {
                    !fitted
                        && !wide_line
                        && !shaped.contains(k)
                        && !decided.contains(k)
                        && lead(k).is_some_and(|c| source.covers(c) && !built_in_draws(c))
                        && asked(k)
                })
                .collect();
            // The font that draws a ligature run's first character decides
            // the run's pieces. If that font is a bitmap one, or does not
            // join the run, the run draws unjoined: a later source never
            // gets the pieces, so another font's ligature is never drawn
            // among the first font's characters.
            //
            // References:
            // - ADR-1308 D8: a bitmap font draws no ligatures, and a run the
            //   font does not join draws each cell alone.
            if !color_only && !fitted {
                let draws = |c: u32| {
                    source.covers(c)
                        && match loaded {
                            Loaded::Bitmap(font) => font.map.contains_key(&c),
                            Loaded::Outline { mapped, .. } => mapped.binary_search(&c).is_ok(),
                        }
                };
                decided.extend(
                    clusters
                        .iter()
                        .map(Vec::as_slice)
                        .filter(|k| is_ligature_piece(k) && lead(k).is_some_and(draws)),
                );
            }
            match loaded {
                // a bitmap font has no color glyph
                Loaded::Bitmap(_) if left.is_empty() || color_only => {}
                Loaded::Bitmap(font) => {
                    let scale = (cell.width / font.cell.width.max(1))
                        .min(cell.height / font.cell.height.max(1));
                    if scale == 0 {
                        continue;
                    }
                    // a bitmap font's glyph doubles its pixels on a line of
                    // double size, which the console does
                    if doubled.is_some() {
                        left_out.extend(left.iter().filter(|c| font.map.contains_key(c)));
                        continue;
                    }
                    let baseline = bitmap_baseline(font);
                    // a glyph's ink may use the rows spacing adds and then a
                    // row of the whole cell (ADR-1306 D1, D8)
                    let reach = (
                        band.above
                            .saturating_add(whole.height.saturating_mul(REACH_ROWS)),
                        band.below
                            .saturating_add(whole.height.saturating_mul(REACH_ROWS)),
                    );
                    // each glyph drawn, by the strike it comes from (0 for
                    // the source's own, else a smaller size's place, 1 on),
                    // its glyph there and its fit: whether it went to the R8
                    // strike rather than the 1-bit one, and its number there
                    let mut placed: BTreeMap<(usize, u32, Fit), (bool, u32)> = BTreeMap::new();
                    for c in left {
                        let (Some(&g), Some(ch)) = (font.map.get(&c), char::from_u32(c)) else {
                            continue;
                        };
                        let fit = Engine::fit(source, c);
                        // a script glyph comes from a smaller size of the
                        // source where one lies in D3's range
                        let small = match script {
                            Script::Baseline => None,
                            _ => self.smaller_glyph(i, (font, scale), cell.width, c, g),
                        };
                        let how = match (script, small) {
                            (Script::Baseline, _) => None,
                            (_, Some(s)) => Some(self.made_smaller(i, scale, s)),
                            (_, None) => Some(Made::Scaled),
                        };
                        let key = match small {
                            Some(s) => (s.strike, s.glyph, fit),
                            None => (0, g, fit),
                        };
                        let (gray, index) = match placed.get(&key) {
                            Some(&at) => at,
                            None => {
                                let Some((width, rows)) = font.glyphs.get(g as usize) else {
                                    continue;
                                };
                                let span = cell.width * width.cells();
                                let drawn = match small {
                                    // the smaller size's glyph whole, in its
                                    // style, against the text band, its ink
                                    // cut to the row (ADR-1202 D3, D7, D8)
                                    Some(s) => {
                                        let f = s.font;
                                        let size = (
                                            f.cell.width * width.cells() * s.multiple,
                                            f.cell.height * s.multiple,
                                        );
                                        let (_, top) = vtium_font::script::place(
                                            script,
                                            size,
                                            (span, cell.height),
                                            Band::NONE,
                                        );
                                        Some(place_styled(
                                            s.rows,
                                            *width,
                                            f.cell,
                                            (cell, i64::from(top)),
                                            s.multiple,
                                            synth,
                                            (bitmap_baseline(f), (0, 0)),
                                        ))
                                    }
                                    None => None,
                                };
                                let top = glyph_top(scale, baseline, text_base);
                                // a script glyph is drawn upright and
                                // unstyled, and takes its style once it is
                                // small (ADR-1202 D7)
                                let drawn_synth = match script {
                                    Script::Baseline => synth,
                                    _ => Style::default(),
                                };
                                let (own, ink) = match drawn {
                                    Some(drawn) => drawn,
                                    None => place_styled(
                                        rows,
                                        *width,
                                        font.cell,
                                        (cell, top),
                                        scale,
                                        drawn_synth,
                                        (baseline, reach),
                                    ),
                                };
                                // a script glyph with no smaller size is the
                                // normal glyph in the text band made small,
                                // and keeps no ink past its cells (ADR-1202
                                // D4, D8)
                                let (own, ink) = match script {
                                    Script::Baseline => (own, ink),
                                    _ if small.is_some() => (own, ink),
                                    _ => {
                                        let origin = bitmap_origin(
                                            (font.cell, *width, scale),
                                            (span, cell.height),
                                            (top, baseline),
                                            synth,
                                        );
                                        match script_bitmap(
                                            &own,
                                            (span, cell.height),
                                            *width,
                                            (script, &origin),
                                        ) {
                                            Some(small) => (small, None),
                                            None => continue,
                                        }
                                    }
                                };
                                let Some((own, ink)) = spacing::place_rows(
                                    &own,
                                    (span, cell.height),
                                    vtium_font::Format::Mask,
                                    ink.as_ref(),
                                    (band, fit, true),
                                ) else {
                                    continue;
                                };
                                // a glyph of 0 and 255 alone is one bit; a
                                // script glyph scaled with gray edges is R8
                                if !own.iter().all(|&p| p == 0 || p == 255) {
                                    let i = r8.glyph(*width, own);
                                    placed.insert(key, (true, i));
                                    r8.map(ch, i);
                                    mapped.insert(c);
                                    made.extend(how.map(|how| (c, how)));
                                    continue;
                                }
                                let i = bits.glyph(*width, pack_bits(&own, span));
                                // a box past a set's chunk is not sent, and
                                // the glyph is cut to its cells (ADR-1306 D3)
                                let ink = ink
                                    .map(|b| (pack_bits(&b.pixels, b.width), b))
                                    .filter(|(packed, _)| packed.len() <= SET_CHUNK_BYTES as usize);
                                if let Some((packed, b)) = ink {
                                    bits.ink(i, (b.left, b.top), (b.width, b.height), packed);
                                }
                                placed.insert(key, (false, i));
                                (false, i)
                            }
                        };
                        if gray {
                            r8.map(ch, index);
                        } else {
                            bits.map(ch, index);
                        }
                        mapped.insert(c);
                        made.extend(how.map(|how| (c, how)));
                    }
                }
                Loaded::Outline { .. } if left.is_empty() && unshaped.is_empty() => {}
                Loaded::Outline { font, family, .. } => {
                    // a cluster's scalar values join the cover, as an empty
                    // cover is every codepoint; only those wanted are taken
                    let mut cover: Vec<u32> = left.clone();
                    cover.extend(unshaped.iter().flat_map(|k| k.iter().copied()));
                    cover.retain(|&c| source.covers(c));
                    cover.sort_unstable();
                    cover.dedup();
                    if cover.is_empty() {
                        continue;
                    }
                    let request = OutlineRequest {
                        font: Arc::clone(font),
                        family: family.clone(),
                        cells: vec![(drawn.width, drawn.height)],
                        cover: ranges(&cover),
                        clusters: unshaped.iter().map(|k| crate::shaping::ask(k)).collect(),
                        cluster_width: self.cluster_width.get(),
                        baselines: match fitted {
                            true => self.own_baseline(i, cell.height),
                            false => self.baseline(i, cell.height),
                        }
                        .into_iter()
                        .collect(),
                        sizes: self
                            .size(i, cell.height)
                            .filter(|_| !fitted)
                            .map(f32::to_bits)
                            .into_iter()
                            .collect(),
                        // a glyph's ink may use the rows spacing adds and
                        // then a row of the whole cell (ADR-1306 D1, D8)
                        reaches: vec![(
                            drawn_band
                                .above
                                .saturating_add(drawn_whole.height.saturating_mul(REACH_ROWS)),
                            drawn_band
                                .below
                                .saturating_add(drawn_whole.height.saturating_mul(REACH_ROWS)),
                        )],
                        script: Script::Baseline,
                        hinting: self.hinting,
                        synth,
                        // a fallback source: any source but the first of its
                        // style, the frit's own face for that style, draws
                        // a glyph too big for its cells smaller (ADR-1200 D17)
                        fit: self
                            .sources
                            .iter()
                            .position(|(s, _)| s.style == source.style)
                            != Some(i),
                        double_width: doubled == Some(Doubled::Width),
                    };
                    // A source drawn 1-bit still sends its glyphs in the
                    // 8-bit strike, at a byte a pixel, and not in the 1-bit
                    // strike, which would hold them in an eighth of the
                    // memory. The console takes every glyph of a 1-bit
                    // strike for a bitmap font's glyph: it makes the
                    // glyph's superscripts and subscripts from smaller
                    // multiples of the strike, and adds any style the
                    // strike lacks by widening and shearing pixels. The
                    // helper draws this source's script sizes and styles
                    // from the outline itself, so the console would draw
                    // them differently from the 8-bit strike.
                    let request = self.rendered(request, i);
                    let request = match doubled {
                        None => request,
                        Some(d) => match self.doubled_request(request, (i, fitted), cell, d) {
                            Some(request) => request,
                            None => continue,
                        },
                    };
                    let request = match script {
                        Script::Baseline => request,
                        _ => match self.script_request(
                            request,
                            (i, fitted),
                            (cell, band, text_base),
                            script,
                        ) {
                            Some(request) => request,
                            None => continue,
                        },
                    };
                    let outline = match rasterise(&request) {
                        Ok(outline) => outline,
                        Err(why) => {
                            self.failed.borrow_mut().insert(i, why);
                            continue;
                        }
                    };
                    // the color glyphs a line of double width leaves out,
                    // which it draws below from a line of double height
                    for &c in &outline.color {
                        if !mapped.contains(&c) && left.binary_search(&c).is_ok() {
                            left_out.insert(c);
                            squashed.insert(c);
                        }
                    }
                    let Some(s) = outline.strikes.first() else {
                        continue;
                    };
                    let mut placed: BTreeMap<(u32, Fit), Option<u32>> = BTreeMap::new();
                    let mut index =
                        |(g, fit): (u32, Fit), target: &mut StrikeBuilder, glyph: &Placed| {
                            *placed.entry((g, fit)).or_insert_with(|| {
                                put_placed(target, glyph, (drawn, drawn_band, fit))
                            })
                        };
                    for &(c, g) in &s.map {
                        let (Some(glyph), Some(ch)) = (s.glyphs.get(g as usize), char::from_u32(c))
                        else {
                            continue;
                        };
                        if mapped.contains(&c)
                            || left.binary_search(&c).is_err()
                            || (color_only && !glyph.color)
                        {
                            continue;
                        }
                        // a character that presents as text by default
                        // takes a later source's text glyph before a color
                        // glyph in one cell (ADR-1302 D14, step 2)
                        if glyph.color
                            && !glyph.wide
                            && !color_only
                            && script == Script::Baseline
                            && !crate::shaping::emoji_by_default(c)
                        {
                            if !in_one_cell.iter().any(|(d, ..)| *d == ch) {
                                in_one_cell.push((ch, glyph.clone(), Engine::fit(source, c)));
                            }
                            continue;
                        }
                        let target = if glyph.color { &mut rgba } else { &mut r8 };
                        let Some(at) = index((g, Engine::fit(source, c)), target, glyph) else {
                            continue;
                        };
                        target.map(ch, at);
                        mapped.insert(c);
                        if script != Script::Baseline {
                            let how = if glyph.color {
                                Made::Color
                            } else {
                                Made::Outline
                            };
                            made.insert(c, how);
                        }
                    }
                    for (key, g) in &s.clusters {
                        let Some(glyph) = s.glyphs.get(*g as usize) else {
                            continue;
                        };
                        // a cluster asked for, or the part of one the font
                        // draws without the marks it has no glyph for,
                        // which the console lays them over (ADR-1200 D22
                        // step 3); a part goes once, and never under the
                        // key of a cluster asked for
                        let asked = unshaped.iter().find(|k| **k == key.as_slice()).copied();
                        let part = parts_asked
                            && asked.is_none()
                            && !parts.contains(key)
                            && !clusters.contains(key)
                            && unshaped.iter().any(|k| crate::shaping::is_part(key, k));
                        if asked.is_none() && !part {
                            continue;
                        }
                        if color_only && !glyph.color {
                            continue;
                        }
                        let chars: Option<Vec<char>> =
                            key.iter().map(|&c| char::from_u32(c)).collect();
                        let Some(chars) = chars else {
                            continue;
                        };
                        let target = if glyph.color { &mut rgba } else { &mut r8 };
                        let fit = Engine::fit(source, lead(key).unwrap_or_default());
                        let Some(at) = index((*g, fit), target, glyph) else {
                            continue;
                        };
                        target.cluster(&chars, at);
                        match asked {
                            Some(asked) => {
                                shaped.insert(asked);
                            }
                            None => {
                                parts.insert(key.clone());
                            }
                        }
                    }
                }
            }
        }
        for (ch, glyph, fit) in in_one_cell {
            let c = u32::from(ch);
            if mapped.contains(&c) {
                continue;
            }
            if let Some(at) = put_placed(&mut rgba, &glyph, (drawn, drawn_band, fit)) {
                rgba.map(ch, at);
                mapped.insert(c);
            }
        }
        // a line of double width takes its color glyphs and its clusters
        // from a line of double height, each row pair made one row. Where
        // the double-height cell passes the largest cell, the line leaves
        // them to the console, which doubles the pixels of their glyphs at
        // the normal size, as it does for a bitmap font's
        let mut halved = false;
        let tall_fits =
            Doubled::Height.cell(cell).is_some() && Doubled::Height.cell(whole).is_some();
        if wide_line && tall_fits && (!squashed.is_empty() || !clusters.is_empty()) {
            let want: Vec<u32> = squashed.into_iter().collect();
            let tall = self.glyphs(
                whole,
                band,
                (variant, Some(Doubled::Height)),
                &want,
                (clusters, parts_asked),
                made,
            )?;
            if let Some(tall) = tall {
                let tall = tall_set(tall, whole)?;
                halved = halve_into(&tall, (&want, variant), (&mut r8, &mut rgba));
            }
        }
        if mapped.is_empty() && shaped.is_empty() && parts.is_empty() && !halved {
            return Ok(None);
        }
        let mut set = SetBuilder::new(&self.name);
        for s in [bits, r8, rgba] {
            if !s.is_empty() {
                set = set.strike(s);
            }
        }
        set.write().map(Some).map_err(|e| e.to_string())
    }
}

/// `glyph`, placed by the helper's outline engine in cells of `cell`,
/// added to `target` placed in a cell taller by `band` as `fit` says
/// (ADR-1302 D13), with its ink box where it has one that fits a set's
/// chunk; a box past a chunk is not sent, and the glyph is cut to its cells
/// (ADR-1306 D3). Gives the glyph's number in `target`.
fn put_placed(
    target: &mut StrikeBuilder,
    glyph: &Placed,
    (cell, band, fit): (Cell, Band, Fit),
) -> Option<u32> {
    let width = if glyph.wide {
        Width::Wide
    } else {
        Width::Narrow
    };
    let format = if glyph.color {
        vtium_font::Format::Rgba
    } else {
        vtium_font::Format::Mask
    };
    let size = (cell.width * width.cells(), cell.height);
    let (pixels, ink) = spacing::place_rows(
        &glyph.pixels,
        size,
        format,
        glyph.ink.as_ref(),
        (band, fit, false),
    )?;
    let at = target.glyph(width, pixels);
    let ink = ink.filter(|i| i.pixels.len() <= SET_CHUNK_BYTES as usize);
    if let Some(ink) = ink {
        target.ink(at, (ink.left, ink.top), (ink.width, ink.height), ink.pixels);
    }
    Some(at)
}

/// The set `bytes` that the engine drew for a line of double height at
/// `whole`, the cell of the normal size, read back as the face that draws
/// it, so that a line of double width can take its glyphs.
fn tall_set(bytes: Vec<u8>, whole: Cell) -> Result<SetFace, String> {
    let set = Set::from_bytes(bytes).map_err(|e| format!("{e:?}"))?;
    let tall = Doubled::Height
        .cell(whole)
        .ok_or("a double size past the largest cell")?;
    SetFace::new(Arc::new(set), tall.height)
        .ok_or_else(|| format!("no strike at {} pixels", tall.height))
}

/// Adds to the strikes of a line of double width, `r8` for coverage and
/// `rgba` for color, each glyph that `tall` holds in `variant`: the glyph
/// of each codepoint of `want` and of each cluster its cluster map holds,
/// drawn for a line of double height. Each glyph comes in at half its
/// height ([`halved`]), so it is the glyph of the normal size at twice its
/// width, as a line of double width shows it. A glyph whose ink passes its
/// cells keeps its ink box, halved the same way ([`halved_ink`]), so its
/// marks reach past the line as at the normal size. Gives whether any
/// glyph went in.
///
/// The engine draws an outline glyph for a line of double width by
/// stretching its outline sideways before it rasterises it. It cannot do
/// that for a color glyph, which is a bitmap or layers of color, or for a
/// cluster, whose glyphs the shaper places. So those two come from the
/// glyphs of a line of double height, which are twice as wide and twice as
/// tall, made half as tall.
fn halve_into(
    tall: &SetFace,
    (want, variant): (&[u32], Variant),
    (r8, rgba): (&mut StrikeBuilder, &mut StrikeBuilder),
) -> bool {
    let mut put = |glyph: &Glyph| -> Option<(bool, u32)> {
        let pixels = halved(glyph)?;
        let color = glyph.format() == vtium_font::Format::Rgba;
        let target = if color { &mut *rgba } else { &mut *r8 };
        let at = target.glyph(glyph.cells(), pixels);
        if let Some((place, size, ink)) = halved_ink(glyph) {
            target.ink(at, place, size, ink);
        }
        Some((color, at))
    };
    let mut maps: Vec<(char, bool, u32)> = Vec::new();
    for ch in want.iter().filter_map(|&c| char::from_u32(c)) {
        if let Some(glyph) = tall.draw_variant(&[ch], variant)
            && let Some((color, at)) = put(&glyph)
        {
            maps.push((ch, color, at));
        }
    }
    let mut keys: Vec<(Vec<char>, bool, u32)> = Vec::new();
    for (_, key) in tall.set().strike_clusters() {
        if let Some(glyph) = tall.draw_variant(&key, variant)
            && let Some((color, at)) = put(&glyph)
        {
            keys.push((key, color, at));
        }
    }
    for &(ch, color, at) in &maps {
        if color { &mut *rgba } else { &mut *r8 }.map(ch, at);
    }
    for (key, color, at) in &keys {
        if *color { &mut *rgba } else { &mut *r8 }.cluster(key, *at);
    }
    !maps.is_empty() || !keys.is_empty()
}

/// The pixels of `glyph` at half its height: each pair of rows made one
/// row by the mean of each pixel's bytes, rounded to the nearest. A color
/// glyph's colors are premultiplied by their alpha, so their mean is the
/// mean color. `None` for a glyph of an odd height, which no cell of a
/// line of double height has.
pub(in crate::serve) fn halved(glyph: &Glyph) -> Option<Vec<u8>> {
    let height = glyph.height();
    if !height.is_multiple_of(2) {
        return None;
    }
    let row = usize::try_from(glyph.width())
        .ok()?
        .checked_mul(usize::try_from(glyph.format().bytes_per_pixel()).ok()?)?;
    let mut out = Vec::with_capacity(glyph.pixels().len() / 2);
    for pair in glyph.pixels().chunks_exact(row.checked_mul(2)?) {
        let (upper, lower) = pair.split_at(row);
        out.extend(
            upper
                .iter()
                .zip(lower)
                .map(|(&a, &b)| (u16::from(a) + u16::from(b)).div_ceil(2) as u8),
        );
    }
    Some(out)
}

/// An ink box's place, size and pixels, as [`halved_ink`] gives them.
type HalvedInk = ((i32, i32), (u32, u32), Vec<u8>);

/// The ink box of `glyph` at half its height, as [`halved`] makes its
/// pixels: each pair of the glyph's rows made one row, counted from the
/// glyph's top, so a box that starts on an odd row above the glyph takes
/// a row of its own that holds that row's ink at half. Its place, its size
/// and its pixels; `None` for a glyph with no ink box, one of an odd
/// height, or a box that, halved, no longer passes the glyph's cells.
pub(in crate::serve) fn halved_ink(glyph: &Glyph) -> Option<HalvedInk> {
    let ink = glyph.ink()?;
    let height = i64::from(glyph.height());
    if !glyph.height().is_multiple_of(2) || ink.pixels().len() > SET_CHUNK_BYTES as usize {
        return None;
    }
    let bpp = usize::try_from(glyph.format().bytes_per_pixel()).ok()?;
    let row = usize::try_from(ink.width()).ok()?.checked_mul(bpp)?;
    let (top, bottom) = (
        i64::from(ink.top()),
        i64::from(ink.top()) + i64::from(ink.height()),
    );
    let (first, end) = (top.div_euclid(2), (bottom + 1).div_euclid(2));
    let line = |y: i64| -> Option<&[u8]> {
        let at = usize::try_from(y - top).ok()?.checked_mul(row)?;
        ink.pixels().get(at..at.checked_add(row)?)
    };
    let mut out = Vec::with_capacity(row * usize::try_from(end - first).ok()?);
    for y in first..end {
        let (upper, lower) = (line(2 * y), line(2 * y + 1));
        for x in 0..row {
            let byte = |l: Option<&[u8]>| u16::from(l.and_then(|l| l.get(x)).copied().unwrap_or(0));
            out.push((byte(upper) + byte(lower)).div_ceil(2) as u8);
        }
    }
    let past = first < 0 || end > height / 2 || ink.left() < 0 || ink.width() > glyph.width();
    past.then_some((
        (ink.left(), i32::try_from(first).ok()?),
        (ink.width(), u32::try_from(end - first).ok()?),
        out,
    ))
}
