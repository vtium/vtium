//! Drawing a page: its bytes through `vtium-term`, one frame through
//! `vtium-render` into a canvas in memory, as tier 2 draws its screens.

use vtium_font::set::{SetFace, builtin};
use vtium_font::{Face, Font};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Terminal, WidthMethod};

use crate::card;
use crate::profiles::{Glyphs, Layout, Profile};

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

/// The font `profile` draws with; an error for a pending profile.
pub fn font(profile: &Profile) -> Result<Font<SetFace>, String> {
    match profile.glyphs {
        Glyphs::Builtin { set, height } => {
            let s = builtin(set).ok_or_else(|| format!("no built-in set {set} in this build"))?;
            let face =
                SetFace::new(s, height).ok_or_else(|| format!("the set {set} has no strikes"))?;
            Ok(Font::new(face))
        }
        Glyphs::Pending { issues, .. } => Err(format!(
            "{} waits on {}",
            profile.name,
            issues
                .iter()
                .map(|i| format!("#{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The layout of the card on `profile`'s display in `font`'s cells.
pub fn layout<F: Face>(profile: &Profile, font: &Font<F>) -> Result<Layout, String> {
    let cell = font.cell();
    let cols = (profile.width / cell.width) as usize;
    let rows = (profile.height / cell.height) as usize;
    Layout::new(cols, rows).ok_or_else(|| {
        format!(
            "{}: a grid of {cols}x{rows} holds no block of {}x{}",
            profile.name,
            card::BLOCK_COLS,
            card::BLOCK_ROWS
        )
    })
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

/// Each page of the card on `profile`'s display.
pub fn screens(profile: &Profile) -> Result<Vec<Image>, String> {
    let probe = font(profile)?;
    let layout = layout(profile, &probe)?;
    (0..layout.pages)
        .map(|page| {
            let bytes = card::page(profile, &layout, page);
            let term = terminal(&layout, &bytes)?;
            let mut canvas = Canvas::new(profile.width, profile.height, 0);
            let mut r = Renderer::new(
                font(profile)?,
                profile.width,
                profile.height,
                Config::default(),
            );
            let state = DrawState {
                background: term.palette().bg(),
                ..DrawState::default()
            };
            let frame = term.screen().frame(term.palette(), Age::NEVER);
            r.draw(&frame, &mut canvas, 0, &state);
            let rgb = canvas
                .pixels()
                .iter()
                .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
                .collect();
            Ok(Image {
                width: profile.width,
                height: profile.height,
                rgb,
            })
        })
        .collect()
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

    // Each block draws the same cells wherever a page puts it: on a grid of
    // its own, 40x30, and in the place each built profile's layout gives
    // it, so no line overflows its block. The header, which names the
    // grid, and the last block's margin line, whose dots run to the grid's
    // margin and whose wide character wraps to the first two columns of the
    // row below, are the exceptions.
    #[test]
    fn every_block_draws_the_same_cells_on_every_page() {
        for p in PROFILES {
            let Ok(f) = font(p) else { continue };
            let l = layout(p, &f).unwrap();
            let alone = Layout::new(BLOCK_COLS, BLOCK_ROWS).unwrap();
            for page in 0..l.pages {
                let placed = cells(&terminal(&l, &card::page(p, &l, page)).unwrap());
                let wrapped = l
                    .blocks(page)
                    .last()
                    .map(|(_, top, _)| top + BLOCK_ROWS - 1);
                for (n, top, left) in l.blocks(page) {
                    let own = cells(&terminal(&alone, &card::page(p, &alone, n)).unwrap());
                    let rows = if n == card::BLOCKS - 1 {
                        28
                    } else {
                        BLOCK_ROWS
                    };
                    for row in (0..rows).filter(|&r| n != 0 || !(1..=3).contains(&r)) {
                        let from = if Some(top + row) == wrapped && left == 0 {
                            2
                        } else {
                            0
                        };
                        assert_eq!(
                            placed[top + row][left + from..left + BLOCK_COLS],
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
            let Ok(f) = font(p) else { continue };
            let l = layout(p, &f).unwrap();
            let page = l.pages - 1;
            let (_, top, _) = l.blocks(page).last().unwrap();
            let grid = cells(&terminal(&l, &card::page(p, &l, page)).unwrap());
            assert_eq!(grid[top + 28][l.cols - 1], "", "{}", p.name);
            assert_eq!(grid[top + 28][l.cols - 2], "·", "{}", p.name);
            assert_eq!(grid[top + 29][0], "界", "{}", p.name);
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
