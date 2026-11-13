//! Replaying and drawing: each case and each scenario dump through
//! vtium-replay's code, the terminal it leaves drawn by tier 2's renderer.
//!
//! A case is replayed once in each view it has a golden for, and the
//! terminal at its last block, after every test command, is drawn. A
//! scenario is played as `conformance run` plays it, for its transcript,
//! and replayed once more in the screen view, where the terminal at each
//! dump's block is drawn. The terminal is drawn with the cursor showing
//! where the terminal shows it, and blinking text in its shown phase.

use std::collections::BTreeMap;
use std::path::Path;

use vtium_conformance::live;
use vtium_conformance::replay::{Input, run_with};
use vtium_conformance::runner::{Case, Corpus};
use vtium_conformance::scenario::Scenario;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::view::View;
use vtium_specimens::draw::Image as Png;
use vtium_term::Terminal;
use vtium_tier2::draw::{Phase, draw_terminal};
use vtium_tier2::ppm::Image;

use crate::CONFORMANCE;
use crate::judge::{Masking, differing};
use crate::manifest::{Dump, Group, Judged, Live, Shot};
use crate::select::{self, Dumped};

/// The blink phase every picture is drawn in: the cursor shown where the
/// terminal shows it, and blinking text shown.
const PHASE: Phase = Phase {
    cursor_on: true,
    blink_hidden: false,
    rapid_hidden: false,
};

/// A picture's shot; the picture, which is missing when the replay or the
/// drawing failed; and the live path's picture of the same screen when it
/// differs from the picture.
pub struct Drawn {
    pub shot: Shot,
    pub picture: Option<Image>,
    pub live: Option<Image>,
}

/// `term` drawn as the picture of a tier 1 golden.
pub fn draw(term: &Terminal) -> Result<Image, String> {
    draw_terminal(term, PHASE)
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
/// the terminal, and the live path's bytes for the same terminal with the
/// grid's size they draw.
pub struct Screen {
    pub picture: Result<Image, String>,
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
