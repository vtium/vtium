//! Playing a scenario through vtium-replay's code: the checks that it can
//! run, its replay input in each view it needs, its transcript, the
//! verdict of its assertions, and the chunking check over it.

use std::path::PathBuf;

use super::{Dump, Item, Scenario, Sort, transcript};
use crate::probes::{Outcome, Replay, check_expectations, judge_blocks};
use crate::replay::{self, Input};
use crate::runner::chunking::{Rng, allowed};
use crate::runner::mask_line;
use crate::spec::Spec;
use crate::view::View;

/// What a behaviour scenario's play gives: its transcript and the verdict
/// of its assertions, which pass when it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Played {
    pub transcript: String,
    pub assertions: Outcome,
}

/// The reason `--write` gives a new scenario on the provisional list.
pub const NEW_SCENARIO: &str = "a new scenario; its transcript waits for the maintainer's review";

/// Writes `transcript` as the transcript of the scenario `name` in `dir`,
/// and returns whether it added the scenario to the provisional list at
/// `list`. A transcript that exists is refused unless `replace` says to
/// rewrite it, in a golden commit; a new one goes on the list, and a
/// rewritten one leaves the list as it is.
pub fn write_transcript(
    dir: &std::path::Path,
    list: &std::path::Path,
    name: &str,
    transcript: &str,
    replace: bool,
) -> Result<bool, String> {
    let path = dir.join(format!("{name}.transcript"));
    if path.exists() && !replace {
        return Err(format!(
            "{name}.transcript exists. A transcript changes only in a golden commit, written with `--write --replace {name}`."
        ));
    }
    let text = std::fs::read_to_string(list).map_err(|e| format!("{}: {e}", list.display()))?;
    let listed = crate::runner::own::provisional(&text)?;
    std::fs::write(&path, transcript).map_err(|e| format!("{}: {e}", path.display()))?;
    if replace || listed.contains_key(name) {
        return Ok(false);
    }
    let mut text = text;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{name}\t{NEW_SCENARIO}\n"));
    std::fs::write(list, text).map_err(|e| format!("{}: {e}", list.display()))?;
    Ok(true)
}

/// A behaviour scenario's verdict against its transcript on file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The transcript matches, and the assertions pass.
    Pass,
    /// The transcript differs, with where, but the provisional list names
    /// the scenario, so it judges nothing.
    Provisional(String),
    /// Why the scenario fails the run.
    Fail(String),
}

/// Judges a play of a behaviour scenario: `played` is the play or why it
/// did not run, `on_file` the transcript on file, and `provisional`
/// whether the provisional list names the scenario. Its assertions fail
/// the run even while its transcript is provisional.
pub fn verdict(
    played: &Result<Played, String>,
    on_file: Option<&str>,
    provisional: bool,
) -> Verdict {
    let played = match played {
        Ok(p) => p,
        Err(why) => return Verdict::Fail(why.clone()),
    };
    if let Outcome::Fail(why) | Outcome::Unjudged(why) = &played.assertions {
        return Verdict::Fail(format!("an assertion fails: {why}"));
    }
    let Some(want) = on_file else {
        return Verdict::Fail(
            "it has no transcript. Write one with `conformance scenarios --write NAME`.".to_owned(),
        );
    };
    match transcript::differs(want, &played.transcript) {
        None => Verdict::Pass,
        Some(why) if provisional => Verdict::Provisional(why),
        Some(why) => Verdict::Fail(why),
    }
}

/// vtium-replay's code, as the scenarios meet it: the interface it
/// declares and the fixtures it serves.
pub struct Runner<'a> {
    replay: Replay<'a>,
    fixtures: PathBuf,
}

impl<'a> Runner<'a> {
    /// The runner for the implementation whose `--describe` is `describe`,
    /// serving fixtures from `fixtures`.
    pub fn new(spec: &'a Spec, describe: &str, fixtures: PathBuf) -> Result<Self, String> {
        Ok(Self {
            replay: Replay::new(spec, describe)?.with_fixtures(fixtures.clone()),
            fixtures,
        })
    }

    pub fn spec(&self) -> &Spec {
        self.replay.spec
    }

    /// Why vtium-replay cannot play `s`, or None: a view it lacks, a test
    /// command it does not offer, or a feature it does not declare.
    pub fn absent(&self, s: &Scenario) -> Option<String> {
        let mut views = s.views();
        views.push(s.view);
        if let Some(v) = views.iter().find(|v| !self.replay.views.contains(v.name())) {
            return Some(format!("vtium-replay has no {} view", v.name()));
        }
        let (names, mut features) = s.requires();
        for name in &names {
            if let Some(Some(f)) = self.spec().commands.get(name) {
                features.insert(f.clone());
            }
        }
        let lacking: Vec<&str> = features
            .iter()
            .filter(|f| !self.replay.features.contains(*f))
            .map(String::as_str)
            .collect();
        if !lacking.is_empty() {
            return Some(format!(
                "vtium-replay does not declare the feature{} {}",
                if lacking.len() == 1 { "" } else { "s" },
                lacking.join(", ")
            ));
        }
        names
            .iter()
            .find(|n| !self.replay.commands.contains(*n))
            .map(|n| format!("vtium-replay offers no test command {n}"))
    }

    /// The fixture `s` names that the fixture directory lacks, or None.
    fn missing_fixture(&self, s: &Scenario) -> Option<String> {
        s.fixtures
            .iter()
            .find(|f| !self.fixtures.join(f).is_file())
            .map(|f| format!("the fixture {f} is not in {}", self.fixtures.display()))
    }

    /// The replay input of `s` in `view` with `extra` cuts, and for each
    /// of its cuts the command whose cut it is, or None for an extra cut.
    /// An extra cut at a command's cut goes before it.
    pub fn input(&self, s: &Scenario, view: View, extra: &[usize]) -> (Input, Vec<Option<usize>>) {
        let compiled = s.compile();
        let mut tagged: Vec<(usize, Option<usize>)> = compiled
            .command_cuts
            .iter()
            .enumerate()
            .map(|(i, &k)| (compiled.cuts.get(k).copied().unwrap_or_default(), Some(i)))
            .collect();
        tagged.extend(extra.iter().map(|&cut| (cut, None)));
        tagged.sort_by_key(|&(cut, tag)| (cut, tag.is_some()));
        let input = Input {
            view,
            cols: s.cols,
            rows: s.rows,
            data: compiled.data,
            cuts: tagged.iter().map(|(cut, _)| *cut).collect(),
            fixtures: self.fixtures.clone(),
        };
        (input, tagged.into_iter().map(|(_, tag)| tag).collect())
    }

    /// Plays the behaviour scenario `s` in every view it needs, with
    /// `extra` cuts, and prints its transcript and judges its assertions.
    /// An error says why it was not played.
    pub fn play(&self, s: &Scenario, extra: &[usize]) -> Result<Played, String> {
        if s.sort != Sort::Behaviour {
            return Err(format!(
                "{} is a capability scenario, which `conformance probes` judges",
                s.name
            ));
        }
        if let Some(why) = self.absent(s) {
            return Err(format!("not run: {why}"));
        }
        if let Some(why) = self.missing_fixture(s) {
            return Err(format!("not judged: {why}"));
        }
        let mut outputs = Vec::new();
        let mut tags = Vec::new();
        for d in s.views() {
            let view = match d {
                Dump::Frame => View::Frame,
                _ => View::Screen,
            };
            let (input, t) = self.input(s, view, extra);
            let out = replay::run(&input)
                .map_err(|f| format!("the replay exited {}: {}", f.code, f.message))?;
            if let Some(line) = out
                .lines()
                .find(|l| !self.spec().parses(l, d.name(), false))
            {
                return Err(format!(
                    "the replay printed {line:?}, which is not a line of the {} view's grammar",
                    d.name()
                ));
            }
            tags = t;
            outputs.push((d, out));
        }
        let runs: Vec<(Dump, &str)> = outputs.iter().map(|(d, o)| (*d, o.as_str())).collect();
        let transcript = transcript::print(s, &tags, &runs)?;
        let first = outputs.first().map(|(_, o)| o.as_str()).unwrap_or_default();
        let assertions = self.assertions(s, &tags, first);
        Ok(Played {
            transcript,
            assertions,
        })
    }

    /// Judges a behaviour scenario's assertions: the k-th `check` judges
    /// the block at the cut of the command before it, in the scenario's
    /// view, with every reply, host request and `test` line since the
    /// check before it.
    fn assertions(&self, s: &Scenario, tags: &[Option<usize>], output: &str) -> Outcome {
        let blocks = transcript::blocks(output);
        let mut at = Vec::new();
        for (j, tag) in tags.iter().enumerate() {
            if let Some(i) = tag {
                at.resize(at.len().max(i.saturating_add(1)), 0);
                at[*i] = j;
            }
        }
        // the block index of each check's command
        let mut checks = Vec::new();
        let mut commands = 0usize;
        for (item, _) in &s.items {
            match item {
                Item::Command(_) => commands = commands.saturating_add(1),
                Item::Check => {
                    checks.push(at.get(commands.saturating_sub(1)).copied().unwrap_or(0))
                }
                _ => {}
            }
        }
        let mut judged: Vec<Vec<String>> = Vec::new();
        let mut from = 0;
        for &j in &checks {
            let mut lines = Vec::new();
            for b in blocks.iter().take(j.saturating_add(1)).skip(from) {
                lines.extend(b.tests.iter().map(|t| mask_line(t)));
                lines.extend(b.effects.iter().cloned());
            }
            if let Some(b) = blocks.get(j) {
                lines.extend(b.body.iter().map(|l| mask_line(l)));
            }
            judged.push(lines);
            from = j.saturating_add(1);
        }
        let refs: Vec<Vec<&str>> = judged
            .iter()
            .map(|b| b.iter().map(String::as_str).collect())
            .collect();
        judge_blocks(&s.probe(), &refs, false)
    }

    /// Plays the capability scenario `s` as the probe runner played a
    /// probe: a cut at each `check`, and its assertions judged.
    pub fn capability(&self, s: &Scenario) -> Outcome {
        self.replay.run(&s.probe())
    }

    /// The chunking check over a behaviour scenario: played again with 1
    /// to 8 extra cuts at random offsets outside its test commands, in
    /// `rounds` rounds from `seed`, its transcript must not change.
    pub fn chunking(&self, s: &Scenario, seed: u64, rounds: usize) -> Result<(), String> {
        let whole = self.play(s, &[])?.transcript;
        let offsets = allowed(&s.compile().data);
        let mut rng = Rng::new(seed);
        for _ in 0..rounds {
            let mut extra = Vec::new();
            for _ in 0..rng.below(8).saturating_add(1) {
                if let Some(&cut) = offsets.get(rng.below(offsets.len())) {
                    extra.push(cut);
                }
            }
            extra.sort_unstable();
            let got = self.play(s, &extra)?.transcript;
            if let Some(why) = transcript::differs(&whole, &got) {
                return Err(format!(
                    "seed {seed}: the extra cuts {extra:?} change the transcript. {why}"
                ));
            }
        }
        Ok(())
    }
}

/// Checks a behaviour scenario's assertions before it runs, as the probe
/// runner checked a probe's: each expected line parses under the grammar
/// of the scenario's view, and each expression matches its example.
pub fn check_assertions(s: &Scenario, spec: &Spec) -> Result<(), String> {
    check_expectations(&s.file, &s.probe(), spec, false)
}
