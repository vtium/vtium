//! Replaying and drawing: each case and each scenario dump through
//! vtium-replay's code, the terminal it leaves drawn by vtium's renderer.
//!
//! A case is replayed once in each view it has a golden for, and the
//! terminal at its last block, after every test command, is drawn. A
//! scenario is played as `conformance run` plays it, for its transcript,
//! and replayed once more in the screen view, where the terminal at each
//! dump's block is drawn. The terminal is drawn with the cursor showing
//! where the terminal shows it, and blinking text in its shown phase.
//!
//! A picture draws its text as the console draws it with no font set:
//! through the console's built-in face, GNU Unifont in 8×16 cells, which
//! draws a cluster that the font has no glyph for by its NFC form, or by
//! its base with its combining marks over it. A screen that shows Arabic
//! draws instead in tier 2's scripts font set, the fonts tier 2's
//! `scripts-arabic` screen draws with: Iosevka Term in its 16×40 cell at
//! 40 pixels, with Noto Sans Arabic behind it, so each letter takes its
//! joined form. `make tier2-fonts` fetches those fonts. When they are
//! absent, the drawing fails and says so.
//!
//! References: ADR-1200 D22, the cluster with no glyph of its own;
//! ADR-1004 D6, Arabic joining; ADR-1503 D9, tier 2's fonts.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::Path;

use vtium::deferred::{Shown, read_shown, settle};
use vtium::font::Chosen;
use vtium_conformance::live;
use vtium_conformance::replay::{Input, run_with};
use vtium_conformance::runner::{Case, Corpus};
use vtium_conformance::scenario::Scenario;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::view::View;
use vtium_font::{Face, Font};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_specimens::draw::Image as Png;
use vtium_term::{Age, Frame, Glyph, Terminal};
use vtium_tier2::fonts;
use vtium_tier2::ppm::Image;

use crate::CONFORMANCE;
use crate::judge::{Masking, differing};
use crate::manifest::{Dump, Group, Judged, Live, Shot, Typeface};
use crate::select::{self, Dumped};

/// The height of the built-in face the pictures draw in, which gives GNU
/// Unifont its own 8×16 cell.
const HEIGHT: u32 = 16;

/// The locale the built-in face is chosen for: one that is not Chinese,
/// Japanese or Korean, so a cell of 16 pixels draws GNU Unifont.
const LOCALE: &str = "C";

/// The name of tier 2's scripts font set, as tier 2 loads it.
const SCRIPTS: &str = "tier2-scripts";

/// The Arabic blocks: Arabic, Arabic Supplement, Arabic Extended-B and -A,
/// Arabic Presentation Forms-A and -B, Arabic Extended-C and the Arabic
/// Mathematical Alphabetic Symbols. A screen that shows a character of
/// one of them draws in tier 2's scripts font set.
const ARABIC: [RangeInclusive<char>; 8] = [
    '\u{0600}'..='\u{06FF}',
    '\u{0750}'..='\u{077F}',
    '\u{0870}'..='\u{089F}',
    '\u{08A0}'..='\u{08FF}',
    '\u{FB50}'..='\u{FDFF}',
    '\u{FE70}'..='\u{FEFF}',
    '\u{10EC0}'..='\u{10EFF}',
    '\u{1EE00}'..='\u{1EEFF}',
];

/// A picture's shot; the picture, which is missing when the replay or the
/// drawing failed; and the live path's picture of the same screen when it
/// differs from the picture.
pub struct Drawn {
    pub shot: Shot,
    pub picture: Option<Image>,
    pub live: Option<Image>,
}

/// `term` drawn as the picture of a tier 1 golden, with tier 2's scripts
/// font set read from tier 2's font cache when the screen shows Arabic.
pub fn draw(term: &Terminal) -> Result<Image, String> {
    draw_from(term, &fonts::cache_dir())
}

/// `term` drawn as the picture of a tier 1 golden, with tier 2's scripts
/// font set read from `dir` when the screen shows Arabic: in the console's
/// built-in face at 16 pixels, else, for a screen that shows Arabic, in
/// the scripts font set at 40 pixels. The error names the font that
/// `dir` lacks and the command that fetches it.
pub fn draw_from(term: &Terminal, dir: &Path) -> Result<Image, String> {
    let frame = term.screen().frame(term.palette(), Age::NEVER);
    if typeface_of(&frame) == Typeface::Unifont {
        let face = Chosen::BUILTINS
            .face(HEIGHT, LOCALE)
            .ok_or("the console has no built-in face at 16 pixels")?;
        return draw_in(term, Font::new(face));
    }
    fonts::check_scripts(dir).map_err(|why| {
        format!(
            "this screen shows Arabic, which the pictures draw in tier 2's scripts font set, Iosevka Term with Noto Sans Arabic, and {why}"
        )
    })?;
    let text = fonts::scripts_frit_text(dir);
    let chosen = fonts::chosen_frit((SCRIPTS, text), dir, fonts::SCRIPTS_HEIGHT, |_| {
        Ok(shown(&frame))
    })?;
    draw_in(
        term,
        Font::new(fonts::face_at(&chosen, fonts::SCRIPTS_HEIGHT)?),
    )
}

/// The fonts [`draw`] draws `term` in.
pub fn typeface(term: &Terminal) -> Typeface {
    typeface_of(&term.screen().frame(term.palette(), Age::NEVER))
}

/// The fonts a picture of `frame` is drawn in: tier 2's scripts font set
/// when the frame shows a character of an [`ARABIC`] block, else the
/// built-in face.
fn typeface_of(frame: &Frame<'_>) -> Typeface {
    let arabic = |c: char| ARABIC.iter().any(|r| r.contains(&c));
    let shows = (0..frame.rows()).any(|row| {
        frame.cells(row).any(|cell| match &cell.glyph {
            Glyph::Char(c) => arabic(*c),
            Glyph::Cluster(t) => t.chars().any(arabic),
            Glyph::Empty => false,
        })
    });
    if shows {
        Typeface::Scripts
    } else {
        Typeface::Unifont
    }
}

/// What `frame` shows in each style, as the console reads it to ask the
/// glyph helper for a font set's glyphs: the characters, the clusters and
/// each joined Arabic letter.
fn shown(frame: &Frame<'_>) -> Vec<Shown> {
    let mut all = vec![Shown::default()];
    read_shown(frame, &mut all);
    settle(&mut all);
    all
}

/// `term` drawn in `font` as one frame on the CPU path, at the size of
/// its grid in `font`'s cells, with the renderer's default configuration.
/// The cursor shows where the terminal shows it, and blinking text shows.
/// The terminal's pictures draw with its text. The frame is read since
/// [`Age::NEVER`], so every cell draws, and the terminal does not change.
fn draw_in<F: Face>(term: &Terminal, font: Font<F>) -> Result<Image, String> {
    let cell = font.cell();
    let screen = term.screen();
    let size = |cells: usize, px: u32| {
        u32::try_from(cells)
            .ok()
            .and_then(|n| n.checked_mul(px))
            .ok_or_else(|| format!("a grid of {}x{} cells", screen.cols(), screen.rows()))
    };
    let (w, h) = (
        size(screen.cols(), cell.width)?,
        size(screen.rows(), cell.height)?,
    );
    let mut r = Renderer::new(font, w, h, Config::default());
    let mut canvas = Canvas::new(w, h, 0);
    let frame = screen.frame(term.palette(), Age::NEVER);
    let state = DrawState {
        cursor_on: true,
        blink_hidden: false,
        rapid_hidden: false,
        background: term.palette().bg(),
        ..DrawState::default()
    };
    r.draw(&frame, &mut canvas, 0, &state);
    Image::from_xrgb(w, h, canvas.pixels()).ok_or_else(|| format!("a display of {w}x{h} pixels"))
}

/// `img` as a PNG.
pub fn png(img: &Image) -> Vec<u8> {
    vtium_specimens::png::encode(&Png {
        width: img.width(),
        height: img.height(),
        rgb: img.rgb().to_vec(),
    })
}

/// One screen of a replay, at one block: the picture path's drawing of
/// the terminal and the fonts it is drawn in, and the live path's bytes
/// for the same terminal with the grid's size they draw.
pub struct Screen {
    pub picture: Result<Image, String>,
    pub typeface: Typeface,
    pub live: Vec<u8>,
    pub cols: usize,
    pub rows: usize,
}

impl Screen {
    /// The live path's drawing of this screen: its bytes read back through
    /// a terminal of the grid's size and drawn as the picture path draws.
    pub fn live_picture(&self) -> Result<Image, String> {
        draw(&live::read_back(&self.live, self.cols, self.rows))
    }

    /// The size in pixels of one character cell of `img`, this screen's
    /// picture.
    fn cell(&self, img: &Image) -> (u32, u32) {
        let per = |px: u32, cells: usize| px / u32::try_from(cells.max(1)).unwrap_or(u32::MAX);
        (per(img.width(), self.cols), per(img.height(), self.rows))
    }

    /// The live path's drawing when it differs from the picture, or when
    /// either drawing fails; `None` when the two agree.
    pub fn live_differs(&self) -> Option<Result<Image, String>> {
        let live = self.live_picture();
        match (&self.picture, &live) {
            (Ok(a), Ok(b)) if a == b => None,
            _ => Some(live),
        }
    }
}

/// Replays `input` once, and at each block that `blocks` names, or at every
/// block when it is `None`, takes the picture and the live path's bytes
/// from the same terminal: the one replay both paths come from. Gives what
/// the replay prints, or why it stopped with what it printed first, and
/// each block's screen.
pub fn replay(
    input: &Input,
    blocks: Option<&[usize]>,
) -> (Result<String, String>, BTreeMap<usize, Screen>) {
    let mut screens = BTreeMap::new();
    let out = run_with(input, &mut |block, term| {
        if blocks.is_none_or(|b| b.contains(&block)) {
            let s = term.screen();
            screens.insert(
                block,
                Screen {
                    picture: draw(term),
                    typeface: typeface(term),
                    live: live::screen(term),
                    cols: s.cols(),
                    rows: s.rows(),
                },
            );
        }
    });
    let out = out.map_err(|f| format!("the replay exits {}: {}\n{}", f.code, f.message, f.output));
    (out, screens)
}

/// The picture of `case`, a case of `set` in `group`, and its shot:
/// replayed in each view it has a golden for, judged against each golden,
/// and drawn as its last block leaves the terminal.
pub fn case(set: &Corpus, group: Group, case: &Case, provisional: Option<&String>) -> Drawn {
    let mut shot = Shot {
        group,
        name: case.name.clone(),
        dump: None,
        width: 0,
        height: 0,
        typeface: Typeface::Unifont,
        cell: (0, 0),
        provisional: provisional.cloned(),
        goldens: Vec::new(),
        error: None,
        live: Live {
            command: format!("make tier1-play CASE={}", case.name),
            matches: false,
        },
    };
    let mut picture = None;
    let mut live_picture = None;
    for (view, name, dir) in select::views(&set.root, case) {
        let input = match set.input(case, view) {
            Ok(input) => input,
            Err(why) => {
                shot.error = Some(why);
                return Drawn {
                    shot,
                    picture: None,
                    live: None,
                };
            }
        };
        let last = input.cuts.len();
        let blocks: &[usize] = if view == View::Screen { &[last] } else { &[] };
        let (out, mut drawn) = replay(&input, Some(blocks));
        if let Some(screen) = drawn.remove(&last) {
            let differs = screen.live_differs();
            shot.live.matches = differs.is_none();
            live_picture = differs.and_then(Result::ok);
            shot.typeface = screen.typeface;
            if let Ok(img) = &screen.picture {
                shot.cell = screen.cell(img);
            }
            match screen.picture {
                Ok(img) => picture = Some(img),
                Err(why) => shot.error = Some(format!("the drawing fails: {why}")),
            }
        }
        let output = match out {
            Ok(out) => out,
            Err(why) => {
                shot.error.get_or_insert(why.clone());
                why
            }
        };
        let path = format!("{CONFORMANCE}/{}/{dir}/{}.txt", group.name(), case.name);
        let differs = match set.golden(case, dir) {
            Ok(golden) => differing(&golden, &output, Masking::Counts),
            Err(why) => {
                shot.error.get_or_insert(why);
                Vec::new()
            }
        };
        shot.goldens.push(Judged {
            view: name.to_owned(),
            path,
            differs,
        });
    }
    if picture.is_none() && shot.error.is_none() {
        shot.error = Some("the replay drew no picture".to_owned());
    }
    if let Some(img) = &picture {
        (shot.width, shot.height) = (img.width(), img.height());
    }
    Drawn {
        shot,
        picture,
        live: live_picture,
    }
}

/// The pictures of each screen `s` dumps, each with its shot: the
/// scenario played for its transcript, which is judged against the one on
/// file at `transcript`, and replayed in the screen view, where each
/// dump's block is drawn.
pub fn scenario(
    runner: &Runner,
    s: &Scenario,
    transcript: &Path,
    provisional: Option<&String>,
) -> Vec<Drawn> {
    let dumps: Vec<Dumped> = select::dumps(s);
    let path = format!("{CONFORMANCE}/{}/{}.transcript", select::SCENARIOS, s.name);
    let mut error = None;
    let differs = match (runner.play(s, &[]), std::fs::read_to_string(transcript)) {
        (Ok(played), Ok(on_file)) => differing(&on_file, &played.transcript, Masking::None),
        (Err(why), _) => {
            error = Some(why);
            Vec::new()
        }
        (_, Err(e)) => {
            error = Some(format!("{}: {e}", transcript.display()));
            Vec::new()
        }
    };
    let (input, _) = runner.input(s, View::Screen, &[]);
    let blocks: Vec<usize> = dumps.iter().map(|d| d.block).collect();
    let (out, mut drawn) = replay(&input, Some(&blocks));
    if let Err(why) = out {
        error.get_or_insert(why);
    }
    let of = dumps.len();
    dumps
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let mut shot = Shot {
                group: Group::Scenarios,
                name: s.name.clone(),
                dump: Some(Dump {
                    number: i + 1,
                    of,
                    command: d.echo,
                    line: d.line,
                }),
                width: 0,
                height: 0,
                typeface: Typeface::Unifont,
                cell: (0, 0),
                provisional: provisional.cloned(),
                goldens: vec![Judged {
                    view: "transcript".to_owned(),
                    path: path.clone(),
                    differs: differs.clone(),
                }],
                error: error.clone(),
                live: Live {
                    command: format!("make tier1-play SCENARIO={}", s.name),
                    matches: false,
                },
            };
            let mut live_picture = None;
            let screen = drawn.remove(&d.block);
            let differs = screen.as_ref().map(Screen::live_differs);
            if let Some(screen) = &screen {
                shot.typeface = screen.typeface;
                if let Ok(img) = &screen.picture {
                    shot.cell = screen.cell(img);
                }
            }
            let picture = match screen.map(|s| s.picture) {
                Some(Ok(img)) => {
                    let differs = differs.flatten();
                    shot.live.matches = differs.is_none();
                    live_picture = differs.and_then(Result::ok);
                    Some(img)
                }
                Some(Err(why)) => {
                    shot.error
                        .get_or_insert(format!("the drawing fails: {why}"));
                    None
                }
                None => {
                    shot.error
                        .get_or_insert("the replay stopped before this dump".to_owned());
                    None
                }
            };
            if let Some(img) = &picture {
                (shot.width, shot.height) = (img.width(), img.height());
            }
            Drawn {
                shot,
                picture,
                live: live_picture,
            }
        })
        .collect()
}
