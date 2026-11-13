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
use crate::manifest::{Dump, Group, Judged, Shot};
use crate::select::{self, Dumped};

/// The blink phase every picture is drawn in: the cursor shown where the
/// terminal shows it, and blinking text shown.
const PHASE: Phase = Phase {
    cursor_on: true,
    blink_hidden: false,
    rapid_hidden: false,
};

/// A picture's shot, and the picture, which is missing when the replay or
/// the drawing failed.
pub type Drawn = (Shot, Option<Image>);

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

/// Replays `input`, drawing the terminal at each block of `blocks`, and
/// gives what the replay prints, or why it stopped with what it printed
/// first, and each picture by its block.
fn replay(
    input: &Input,
    blocks: &[usize],
) -> (
    Result<String, String>,
    BTreeMap<usize, Result<Image, String>>,
) {
    let mut drawn = BTreeMap::new();
    let out = run_with(input, &mut |block, term| {
        if blocks.contains(&block) {
            drawn.insert(block, draw(term));
        }
    });
    let out = out.map_err(|f| format!("the replay exits {}: {}\n{}", f.code, f.message, f.output));
    (out, drawn)
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
    };
    let mut picture = None;
    for (view, name, dir) in select::views(&set.root, case) {
        let input = match set.input(case, view) {
            Ok(input) => input,
            Err(why) => {
                shot.error = Some(why);
                return (shot, None);
            }
        };
        let last = input.cuts.len();
        let blocks: &[usize] = if view == View::Screen { &[last] } else { &[] };
        let (out, mut drawn) = replay(&input, blocks);
        if let Some(img) = drawn.remove(&last) {
            match img {
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
    (shot, picture)
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
    let (out, mut drawn) = replay(&input, &blocks);
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
            };
            let picture = match drawn.remove(&d.block) {
                Some(Ok(img)) => Some(img),
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
            (shot, picture)
        })
        .collect()
}
