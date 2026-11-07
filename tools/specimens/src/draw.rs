//! Drawing a page: its bytes through `vtium-term`, one frame through
//! `vtium-render` into a canvas in memory, as tier 2 draws its screens.

use std::sync::Arc;

use vtium_font::set::{Set, SetFace, builtin};
use vtium_font::sprite::Range;
use vtium_font::{Cell, Decorations, Face, Font, Glyph, RangeCoverage, SpritePolicy, Style};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Terminal, WidthMethod};

use crate::profiles::{Glyphs, Layout, Profile};
use crate::{bars, card};

/// An image, 8-bit RGB, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// An image `width` by `height` filled with `rgb`.
    pub fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Image {
        let n = width as usize * height as usize;
        Image {
            width,
            height,
            rgb: rgb.repeat(n),
        }
    }

    /// Copies `src` in with its top left at `x`, `y`; it must fit.
    pub fn blit(&mut self, src: &Image, x: u32, y: u32) {
        let (w, sw) = (self.width as usize * 3, src.width as usize * 3);
        for row in 0..src.height as usize {
            let at = (y as usize + row) * w + x as usize * 3;
            self.rgb[at..at + sw].copy_from_slice(&src.rgb[row * sw..(row + 1) * sw]);
        }
    }
}

/// What the profiles draw from beyond the built-in sets: the desktop
/// frit's glyphs at each outline profile's cell, and the tiny5 frit's at
/// its own, each or why the caller does not have them (`crate::fonts`).
pub struct Sources {
    pub desktop: Generated,
    pub tiny5: Generated,
}

/// A frit's glyphs at each cell, or why the caller does not have them.
pub type Generated = Result<Vec<CellGlyphs>, Absent>;

/// A frit's glyphs at one cell, as the console's store holds them once the
/// helper has answered: the set of every character the frit gives, as its
/// fill draws them, and, for each style a cluster shows in on the card,
/// the set of those clusters drawn whole, as the helper answers the
/// console's asks for them (ADR-1200 D21).
#[derive(Clone, Debug)]
pub struct CellGlyphs {
    pub cell: Cell,
    pub set: Arc<Set>,
    pub clusters: Vec<(Style, Arc<Set>)>,
}

/// Why a caller does not have a frit's glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Absent {
    /// The caller tried to generate them and could not, and says why: a
    /// pinned font is missing, or the engine refused the frit.
    Failed(String),
    /// The caller does not generate them, and names the other job that
    /// draws and judges the profiles drawn from them. `cargo test` names
    /// `make specimens-check` in CI this way.
    JudgedBy(&'static str),
}

impl std::fmt::Display for Absent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Absent::Failed(why) => f.write_str(why),
            Absent::JudgedBy(judge) => write!(f, "not drawn here; {judge} draws and judges it"),
        }
    }
}

impl Sources {
    /// The generated glyphs `glyphs` draws from, if it draws from any.
    fn generated(&self, glyphs: Glyphs) -> Option<&Generated> {
        match glyphs {
            Glyphs::Builtin { .. } => None,
            Glyphs::Desktop { .. } => Some(&self.desktop),
            Glyphs::Tiny5 { .. } => Some(&self.tiny5),
        }
    }
}

/// Why `profile` cannot be drawn here, if it cannot: the caller does not
/// have the set it needs. `crate::check` decides whether that skip fails.
pub fn skipped<'a>(profile: &Profile, sources: &'a Sources) -> Option<&'a Absent> {
    match sources.generated(profile.glyphs) {
        Some(Err(why)) => Some(why),
        _ => None,
    }
}

/// The font `profile` draws with.
pub fn font(profile: &Profile, sources: &Sources) -> Result<Font<CardFace>, String> {
    let set = match profile.glyphs {
        Glyphs::Builtin { set, .. } => {
            builtin(set).ok_or_else(|| format!("no built-in set {set} in this build"))?
        }
        glyphs @ (Glyphs::Desktop { .. } | Glyphs::Tiny5 { .. }) => {
            let (width, height) = profile.cell;
            let sets = sources
                .generated(glyphs)
                .ok_or("no generated glyphs")?
                .as_ref()
                .map_err(Absent::to_string)?;
            let at = sets
                .iter()
                .find(|g| g.cell == Cell { width, height })
                .ok_or_else(|| {
                    format!(
                        "the {} frit has no glyphs at {width}x{height}",
                        glyphs.set()
                    )
                })?;
            let height = profile.glyphs.height();
            let mut clusters = Vec::new();
            for (style, set) in &at.clusters {
                let face = SetFace::new(Arc::clone(set), height).ok_or_else(|| {
                    format!("the {} frit's clusters have no strikes", glyphs.set())
                })?;
                clusters.push((*style, face));
            }
            let base = SetFace::new(Arc::clone(&at.set), height)
                .ok_or_else(|| format!("the set {} has no strikes", glyphs.set()))?;
            return Ok(Font::new(CardFace { base, clusters }));
        }
    };
    let base = SetFace::new(set, profile.glyphs.height())
        .ok_or_else(|| format!("the set {} has no strikes", profile.glyphs.set()))?;
    Ok(Font::new(CardFace {
        base,
        clusters: Vec::new(),
    }))
}

/// The face a profile draws through, as the console's store answers for
/// a frit (ADR-1200 D22): a cluster of more than one character from the
/// glyph the helper drew for it whole, where it drew one in the cell's
/// style, and everything else from the set of single characters, a
/// cluster the helper drew nothing for as its first character. A built-in
/// set has no clusters.
#[derive(Clone, Debug)]
pub struct CardFace {
    base: SetFace,
    clusters: Vec<(Style, SetFace)>,
}

impl CardFace {
    /// What a joined Arabic letter's key draws from the set of single
    /// characters when the helper drew nothing whole for it, as the
    /// console draws it (ADR-1004 D6): the presentation form of its joined
    /// form, where the set holds that, else its letter. Any other cluster
    /// is drawn as it is. A lam-alef ligature the set holds neither whole
    /// nor as its presentation form draws its lam, where the console draws
    /// lam and alef side by side.
    fn unjoined(&self, cluster: &[char]) -> Vec<char> {
        const ZWJ: char = '\u{200d}';
        if let Some(form) = vtium::deferred::presentation(cluster)
            && self.base.has_glyph(form)
        {
            return vec![form];
        }
        let keyed = cluster.first() == Some(&ZWJ) || cluster.last() == Some(&ZWJ);
        if keyed || vtium::deferred::presentation(cluster).is_some() {
            return cluster.iter().copied().filter(|&c| c != ZWJ).collect();
        }
        cluster.to_vec()
    }

    /// The glyph the helper drew whole for `cluster` in `style`, if any.
    fn whole(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        if cluster.len() < 2 {
            return None;
        }
        self.clusters
            .iter()
            .filter(|(s, _)| *s == style)
            .find_map(|(_, face)| face.cluster(cluster, style))
    }
}

impl Face for CardFace {
    fn cell(&self) -> Cell {
        self.base.cell()
    }

    fn size_step(&self) -> u32 {
        self.base.size_step()
    }

    fn has_glyph(&self, c: char) -> bool {
        self.base.has_glyph(c)
    }

    fn coverage(&self, range: Range) -> RangeCoverage {
        self.base.coverage(range)
    }

    fn sprite_policy(&self) -> SpritePolicy {
        self.base.sprite_policy()
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.whole(cluster, style)
            .or_else(|| self.base.draw(&self.unjoined(cluster), style))
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.base.last_resort(style)
    }

    fn decorations(&self) -> Decorations {
        self.base.decorations()
    }

    fn scale(&self) -> u32 {
        self.base.scale()
    }

    // the helper draws a frit's glyphs at the profile's cell, at scale 1,
    // so a cluster's glyph is already unscaled
    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.whole(cluster, style)
            .or_else(|| self.base.draw_unscaled(&self.unjoined(cluster), style))
    }

    fn last_resort_unscaled(&self, style: Style) -> Option<Glyph> {
        self.base.last_resort_unscaled(style)
    }
}

/// A style and the clusters, as scalar values, the card shows in it.
pub type StyleClusters = (Style, Vec<Vec<u32>>);

/// The clusters of more than one character that the card shows on the
/// pages of `profiles`, in each style a cell draws its glyph in, as the
/// console reads what a display shows to ask the helper for it
/// ([`vtium::deferred::read_shown`]), the keys of joined Arabic letters
/// among them: each style once, its clusters sorted and once, as scalar
/// values.
pub fn clusters_shown(profiles: &[&Profile]) -> Result<Vec<StyleClusters>, String> {
    let mut shown: Vec<vtium::deferred::Shown> = Vec::new();
    for profile in profiles {
        let layout = profile.layout()?;
        for page in 0..layout.pages {
            let term = terminal(&layout, &card::page(profile, &layout, page))?;
            let frame = term.screen().frame(term.palette(), Age::NEVER);
            vtium::deferred::read_shown(&frame, &mut shown);
        }
    }
    vtium::deferred::settle(&mut shown);
    let mut all: Vec<StyleClusters> = shown
        .into_iter()
        .filter(|one| !one.clusters.is_empty())
        .map(|one| {
            let keys = one
                .clusters
                .iter()
                .map(|k| k.iter().map(|&c| u32::from(c)).collect())
                .collect();
            (one.style, keys)
        })
        .collect();
    all.sort_by_key(|(s, _)| (s.bold, s.italic));
    Ok(all)
}

/// A terminal of `layout`'s grid that has taken `bytes`, as a host starts
/// one: the Unicode width method, and what RIS returns to.
pub fn terminal(layout: &Layout, bytes: &[u8]) -> Result<Terminal, String> {
    let mut term = Terminal::new(layout.cols, layout.rows, 0);
    term.set_default_width_method(WidthMethod::Unicode);
    term.screen_mut().set_width_method(WidthMethod::Unicode);
    let mut rest = bytes;
    while !rest.is_empty() {
        let taken = term.input(rest);
        let replies = term.take_replies();
        if taken == 0 && replies.is_empty() {
            return Err("the terminal stopped taking input".to_owned());
        }
        rest = &rest[taken..];
    }
    Ok(term)
}

/// Each page of the card on `profile`'s display, then the message bars'
/// screens ([`bars`]).
pub fn screens(profile: &Profile, sources: &Sources) -> Result<Vec<Image>, String> {
    let cell = font(profile, sources)?.cell();
    if (cell.width, cell.height) != profile.cell {
        return Err(format!(
            "{}: the set draws {}x{} cells, the table says {}x{}",
            profile.name, cell.width, cell.height, profile.cell.0, profile.cell.1
        ));
    }
    let layout = profile.layout()?;
    (0..layout.pages)
        .map(|page| {
            let bytes = card::page(profile, &layout, page);
            let term = terminal(&layout, &bytes)?;
            let mut canvas = Canvas::new(profile.width, profile.height, 0);
            let config = Config {
                depth: profile.display,
                ..Config::default()
            };
            let mut r = Renderer::new(
                font(profile, sources)?,
                profile.width,
                profile.height,
                config,
            );
            let state = DrawState {
                background: term.palette().bg(),
                ..DrawState::default()
            };
            let frame = term.screen().frame(term.palette(), Age::NEVER);
            r.draw(&frame, &mut canvas, 0, &state);
            Ok(image(profile, canvas.pixels()))
        })
        .chain(
            bars::screens(profile, sources)
                .map(|bars| bars.iter().map(|b| image(profile, b)).collect::<Vec<_>>())
                .map_or_else(|e| vec![Err(e)], |v| v.into_iter().map(Ok).collect()),
        )
        .collect()
}

/// `profile`'s display of XRGB `pixels` as an image, through its panel's
/// conversion if it has one.
fn image(profile: &Profile, pixels: &[u32]) -> Image {
    let rgb = pixels
        .iter()
        .flat_map(|&p| {
            let rgb = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
            match profile.panel {
                Some(panel) => [panel.show(rgb); 3],
                None => rgb,
            }
        })
        .collect();
    Image {
        width: profile.width,
        height: profile.height,
        rgb,
    }
}

/// The space between screens in a collage and round its edge, in pixels.
const GAP: u32 = 16;

/// The collage's gray, behind and between the screens.
const COLLAGE_GRAY: [u8; 3] = [0x40, 0x40, 0x40];

/// `screens`, all one size, in rows of as many as make the collage nearest
/// square, in order, on gray.
pub fn collage(screens: &[Image]) -> Image {
    let n = screens.len() as u32;
    let across = (1..=n).find(|a| a * a >= n).unwrap_or(1);
    let down = n.div_ceil(across);
    let (w, h) = (screens[0].width, screens[0].height);
    let mut out = Image::filled(
        across * (w + GAP) + GAP,
        down * (h + GAP) + GAP,
        COLLAGE_GRAY,
    );
    for (i, s) in screens.iter().enumerate() {
        let i = i as u32;
        out.blit(
            s,
            GAP + (i % across) * (w + GAP),
            GAP + (i / across) * (h + GAP),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{BLOCK_COLS, BLOCK_ROWS};
    use crate::profiles::PROFILES;
    use vtium_term::Glyph;

    /// The text of each cell of `term`'s screen, a wide cell's second
    /// column empty.
    fn cells(term: &Terminal) -> Vec<Vec<String>> {
        let frame = term.screen().frame(term.palette(), Age::NEVER);
        (0..frame.rows())
            .map(|row| {
                (0..frame.cols())
                    .map(|col| match frame.cell(row, col).map(|c| c.glyph) {
                        Some(Glyph::Char(c)) => c.to_string(),
                        Some(Glyph::Cluster(s)) => s.to_owned(),
                        _ => String::new(),
                    })
                    .collect()
            })
            .collect()
    }

    /// A set at an 8x16 cell in `style`: each of `chars` mapped to an R8
    /// glyph one cell wide of `shade`, and each of `clusters` to a glyph
    /// two cells wide of `shade` plus 1.
    fn set(style: Style, shade: u8, chars: &[char], clusters: &[&[char]]) -> SetFace {
        use vtium_font::Width;
        use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
        let cell = Cell {
            width: 8,
            height: 16,
        };
        let mut strike = StrikeBuilder::new(PixelFormat::R8, cell, style);
        let one = strike.glyph(Width::Narrow, vec![shade; 8 * 16]);
        for &c in chars {
            strike.map(c, one);
        }
        let two = strike.glyph(Width::Wide, vec![shade + 1; 16 * 16]);
        for key in clusters {
            strike.cluster(key, two);
        }
        let bytes = SetBuilder::new("t").strike(strike).write().unwrap();
        SetFace::new(Arc::new(Set::from_bytes(bytes).unwrap()), 16).unwrap()
    }

    // ADR-1200 D22: a profile's face draws a cluster from the glyph the
    // helper drew for it whole in the cell's style, as the console's store
    // does, and a cluster the helper drew nothing for, or one in a style it
    // was not asked in, as its first character
    #[test]
    fn a_cluster_draws_whole_where_the_helper_drew_it() {
        let regular = Style::default();
        let bold = Style {
            bold: true,
            italic: false,
        };
        let vs16 = ['\u{2764}', '\u{fe0f}'];
        let vs15 = ['\u{2764}', '\u{fe0e}'];
        let face = CardFace {
            base: set(regular, 10, &['\u{2764}'], &[]),
            clusters: vec![(regular, set(regular, 20, &[], &[&vs16]))],
        };
        let shade = |g: vtium_font::Glyph| (g.width(), g.pixels()[0]);
        assert_eq!(face.draw(&vs16, regular).map(shade), Some((16, 21)));
        assert_eq!(face.draw(&vs15, regular).map(shade), Some((8, 10)));
        assert_eq!(face.draw(&['\u{2764}'], regular).map(shade), Some((8, 10)));
        assert_eq!(face.draw(&vs16, bold).map(|g| g.width()), Some(8));
        assert_eq!(
            face.draw_unscaled(&vs16, regular).map(shade),
            Some((16, 21))
        );
    }

    // The desktop profiles' card shows the heart suit with VS16 and a flag, which
    // the helper is asked for whole, and no cluster of one character
    #[test]
    fn the_card_asks_for_its_clusters_whole() {
        let profiles: Vec<&Profile> = PROFILES
            .iter()
            .filter(|p| matches!(p.glyphs, crate::profiles::Glyphs::Desktop { .. }))
            .collect();
        assert!(!profiles.is_empty());
        let shown = clusters_shown(&profiles).unwrap();
        let regular = &shown
            .iter()
            .find(|(s, _)| *s == Style::default())
            .unwrap()
            .1;
        assert!(regular.contains(&vec![0x2665, 0xfe0f]), "{regular:x?}");
        assert!(regular.contains(&vec![0x1f1fa, 0x1f1f8]), "{regular:x?}");
        assert!(
            shown
                .iter()
                .all(|(_, keys)| keys.iter().all(|k| k.len() > 1))
        );
    }

    // Each block draws the same cells wherever a page puts it: on a grid of
    // its own, 40x30, and in the place each built profile's layout gives
    // it, so no line overflows its block. The header, which names the
    // grid, and the last block's margin line, whose dots run to the grid's
    // margin and whose wide character wraps to the first two columns of the
    // row below, are the exceptions.
    #[test]
    fn every_block_draws_the_same_cells_on_every_page() {
        for p in PROFILES {
            let l = p.layout().unwrap();
            let alone = Layout::new(BLOCK_COLS, BLOCK_ROWS).unwrap();
            for page in 0..l.pages {
                let placed = cells(&terminal(&l, &card::page(p, &l, page)).unwrap());
                let wrapped = l
                    .blocks(page)
                    .last()
                    .and_then(|b| l.row(b.slice, BLOCK_ROWS - 1).map(|r| b.top + r));
                for b in l.blocks(page) {
                    let (n, left) = (b.block, b.left);
                    let own = cells(&terminal(&alone, &card::page(p, &alone, n)).unwrap());
                    let rows = if n == card::BLOCKS - 1 {
                        28
                    } else {
                        BLOCK_ROWS
                    };
                    for row in (0..rows).filter(|&r| n != 0 || !(1..=3).contains(&r)) {
                        let Some(at) = l.row(b.slice, row).map(|r| b.top + r) else {
                            continue;
                        };
                        let from = if Some(at) == wrapped && left == 0 {
                            2
                        } else {
                            0
                        };
                        assert_eq!(
                            placed[at][left + from..left + BLOCK_COLS],
                            own[row][from..],
                            "{} block {n} row {row}",
                            p.name
                        );
                    }
                }
            }
        }
    }

    // The last block's wide character, written where only the grid's last
    // column is left, wraps whole to the first column of the row below,
    // and the last column stays blank.
    #[test]
    fn the_wide_character_at_the_margin_wraps_whole() {
        for p in PROFILES {
            let l = p.layout().unwrap();
            let page = l.pages - 1;
            let b = l.blocks(page).last().unwrap();
            let at = |row| b.top + l.row(b.slice, row).unwrap();
            let grid = cells(&terminal(&l, &card::page(p, &l, page)).unwrap());
            assert_eq!(grid[at(28)][l.cols - 1], "", "{}", p.name);
            assert_eq!(grid[at(28)][l.cols - 2], "·", "{}", p.name);
            assert_eq!(grid[at(29)][0], "界", "{}", p.name);
        }
    }

    #[test]
    fn a_collage_of_four_is_two_by_two_on_gray() {
        let red = Image::filled(4, 3, [255, 0, 0]);
        let c = collage(&[red.clone(), red.clone(), red.clone(), red]);
        assert_eq!((c.width, c.height), (2 * 4 + 3 * GAP, 2 * 3 + 3 * GAP));
        assert_eq!(&c.rgb[..3], &COLLAGE_GRAY);
        let at = |x: u32, y: u32| {
            let i = ((y * c.width + x) * 3) as usize;
            [c.rgb[i], c.rgb[i + 1], c.rgb[i + 2]]
        };
        assert_eq!(at(GAP, GAP), [255, 0, 0]);
        assert_eq!(at(GAP * 2 + 4, GAP * 2 + 3), [255, 0, 0]);
        assert_eq!(at(GAP + 4, GAP), COLLAGE_GRAY);
    }
}
