//! The message bars' screens: what vtium draws of its own over a display's
//! frame (ADR-1401 D5, D7), the notices and the on-screen display, on the
//! screens after the card's pages, one or two as the profile table says.
//!
//! A display shows one box at its top, so the screen stacks bands, each
//! the top of a drawing of its own: the display, cleared, with one overlay
//! over it, drawn by the console's own overlay through tier 2's overlay
//! screens (`tools/tier2/src/draw.rs`), and a line beneath the overlay
//! saying what it is. The bands are four notices at once, the most a head
//! shows; the OSD's full line sharing the box with a notice; the full line
//! alone; and the minimal dot in the top-right cell. Each band is as tall as
//! the rows its overlay covers, its label's row and a blank row.

use vtium_font::{Face, Font};
use vtium_render::{Canvas, Config, DrawState, Mark, Renderer};
use vtium_term::Age;
use vtium_tier2::draw::{Overlay, Words};
use vtium_tier2::notices;

use crate::draw::{Sources, font, terminal};
use crate::profiles::{Layout, Profile};

/// The four notices a head shows at most, oldest first.
const FOUR: &[Words] = &[
    notices::head_file_one_error,
    notices::no_keyboard,
    notices::anti_lockout,
    notices::rescue,
];

/// Each band's overlay and the line beneath it, top to bottom. Each label
/// fits the 40 columns of the smallest grid.
const BANDS: [(Overlay, &str); 4] = [
    (
        Overlay {
            line: None,
            notices: FOUR,
            mark: None,
            scheme: None,
        },
        "Above: four notices, the most at once",
    ),
    (
        Overlay {
            line: Some(notices::osd_active),
            notices: &[notices::no_keyboard],
            mark: None,
            scheme: None,
        },
        "Above: the OSD line with a notice",
    ),
    (
        Overlay {
            line: Some(notices::osd_active),
            notices: &[],
            mark: None,
            scheme: None,
        },
        "Above: the OSD line alone",
    ),
    (
        Overlay {
            line: None,
            notices: &[],
            mark: Some(Mark::Focus),
            scheme: None,
        },
        "Above right: the minimal OSD dot",
    ),
];

/// A display of `profile`'s size, its cleared grid then `label` on `row`
/// drawn by the renderer, and `overlay` over it; returns the canvas and how
/// far down the overlay reaches, in pixels.
fn band<F: Face>(
    profile: &Profile,
    layout: &Layout,
    make: &impl Fn() -> Result<Font<F>, String>,
    overlay: Option<(&Overlay, usize, &str)>,
) -> Result<(Canvas, u32), String> {
    let mut bytes = String::from("\x1b[?25l\x1b[H\x1b[2J");
    if let Some((_, row, label)) = overlay {
        bytes.push_str(&format!("\x1b[{};1H\x1b[2m{label}\x1b[m", row + 1));
    }
    let term = terminal(layout, bytes.as_bytes())?;
    let (w, h) = (profile.width, profile.height);
    let mut canvas = Canvas::new(w, h, 0);
    let config = Config {
        depth: profile.display,
        ..Config::default()
    };
    let mut r = Renderer::new(make()?, w, h, config);
    let state = DrawState {
        background: term.palette().bg(),
        ..DrawState::default()
    };
    let frame = term.screen().frame(term.palette(), Age::NEVER);
    r.draw(&frame, &mut canvas, 0, &state);
    let mut bottom = 0;
    if let Some((overlay, _, _)) = overlay {
        let drawn = overlay.console()?.draw(
            &r,
            (layout.cols, layout.rows),
            vtium_render::Orientation::Normal,
            &mut canvas,
        );
        if drawn.is_empty() {
            return Err(format!("{}: an overlay drew nothing", profile.name));
        }
        bottom = drawn.iter().map(|a| a.y + a.height).max().unwrap_or(0);
    }
    Ok((canvas, bottom))
}

/// The message bars' screens on `profile`'s display, each its XRGB pixels
/// before the panel's conversion: every band on one screen, or, for a
/// profile of two, the four notices on the first and the rest on the
/// second. A band that does not fit its screen is refused.
pub fn screens(profile: &Profile, sources: &Sources) -> Result<Vec<Vec<u32>>, String> {
    let groups: Vec<&[(Overlay, &str)]> = match profile.bar_screens {
        1 => vec![&BANDS[..]],
        2 => vec![&BANDS[..1], &BANDS[1..]],
        n => return Err(format!("{}: {n} message bar screens", profile.name)),
    };
    let layout = profile.layout()?;
    let make = || font(profile, sources);
    let ch = profile.cell.1;
    let top = (profile.height - layout.rows as u32 * ch) / 2;
    let blank = band(profile, &layout, &make, None)?.0.pixels().to_vec();
    let width = profile.width as usize;
    let mut screens = Vec::new();
    for group in groups {
        let mut out = blank.clone();
        let mut y = 0;
        for (overlay, label) in group {
            // the first row wholly below the overlay holds the label
            let (_, bottom) = band(profile, &layout, &make, Some((overlay, 0, label)))?;
            let row = bottom.saturating_sub(top).div_ceil(ch) as usize;
            let (canvas, _) = band(profile, &layout, &make, Some((overlay, row, label)))?;
            let height = top + (row as u32 + 2) * ch;
            if y + height > profile.height {
                return Err(format!(
                    "{}: the message bars do not fit {} screens",
                    profile.name, profile.bar_screens
                ));
            }
            let (start, rows) = (y as usize * width, height as usize * width);
            out[start..start + rows].copy_from_slice(&canvas.pixels()[..rows]);
            y += height;
        }
        screens.push(out);
    }
    Ok(screens)
}
