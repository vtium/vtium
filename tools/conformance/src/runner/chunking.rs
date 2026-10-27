//! Chunking invariance through the replay (docs/architecture.md,
//! "Determinism"; vtium/vtium#14): a case replayed with extra cuts at
//! random offsets ends in the same screen, with the same replies and the
//! same test lines, as the case replayed with its own cuts. A cut feeds the
//! terminal in two calls, so a state that depended on where input was split
//! shows here.

use crate::command::{END, INTRO};
use crate::replay::{self, Input};

/// A seeded generator (SplitMix64), so a failure names the seed that
/// reproduces it.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`, or 0 when `n` is 0.
    pub fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).unwrap_or(u64::MAX);
        if n64 == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % n64).unwrap_or(0)
    }
}

/// The offsets a cut may take: none strictly inside a test command, from
/// the byte after its introducer's ESC to the end of its ST.
pub fn allowed(data: &[u8]) -> Vec<usize> {
    let mut inside = vec![false; data.len().saturating_add(1)];
    let mut at = 0;
    while let Some(start) =
        find(data.get(at..).unwrap_or_default(), INTRO).map(|i| i.saturating_add(at))
    {
        let body = start.saturating_add(INTRO.len());
        let end = find(data.get(body..).unwrap_or_default(), END).map_or(data.len(), |i| {
            body.saturating_add(i).saturating_add(END.len())
        });
        for slot in inside.iter_mut().take(end).skip(start.saturating_add(1)) {
            *slot = true;
        }
        at = end.max(start.saturating_add(1));
    }
    (0..=data.len())
        .filter(|&i| !inside.get(i).copied().unwrap_or(true))
        .collect()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// What chunking must not change: the last block without its replies, the
/// replies of the whole replay, and every test line in order.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub last: Vec<String>,
    pub replies: String,
    pub tests: Vec<String>,
}

pub fn outcome(out: &str) -> Outcome {
    let lines: Vec<&str> = out.lines().collect();
    let start = lines
        .iter()
        .rposition(|l| l.starts_with("checkpoint "))
        .map_or(0, |i| i.saturating_add(1));
    let last = lines
        .iter()
        .skip(start)
        .filter(|l| !l.starts_with("reply ") && !l.starts_with("host: ") && !l.starts_with("test "))
        .map(|l| (*l).to_owned())
        .collect();
    let replies = lines
        .iter()
        .filter_map(|l| l.strip_prefix("reply "))
        .collect();
    let tests = lines
        .iter()
        .filter(|l| l.starts_with("test "))
        .map(|l| (*l).to_owned())
        .collect();
    Outcome {
        last,
        replies,
        tests,
    }
}

/// Replays `input` whole and with `rounds` sets of extra cuts from `seed`,
/// and returns the first set of cuts whose outcome differs.
pub fn check(input: &Input, seed: u64, rounds: usize) -> Result<(), String> {
    let whole = replay::run(input).map_err(|f| format!("exits {}: {}", f.code, f.message))?;
    let want = outcome(&whole);
    let offsets = allowed(&input.data);
    let mut rng = Rng::new(seed);
    for _ in 0..rounds {
        let mut cuts = input.cuts.clone();
        for _ in 0..rng.below(8).saturating_add(1) {
            if let Some(&cut) = offsets.get(rng.below(offsets.len())) {
                cuts.push(cut);
            }
        }
        cuts.sort_unstable();
        let cut = Input {
            cuts: cuts.clone(),
            ..input.clone()
        };
        let got = replay::run(&cut)
            .map_err(|f| format!("cuts {cuts:?} exit {}: {}", f.code, f.message))?;
        if outcome(&got) != want {
            return Err(format!("seed {seed}: cuts {cuts:?} change the outcome"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cut_falls_inside_a_test_command() {
        let data = b"ab\x1b_tsm-test;sync-frames\x1b\\c";
        let ok = allowed(data);
        let start = 2;
        let end = data.len() - 1;
        assert!(ok.contains(&start) && ok.contains(&end) && ok.contains(&data.len()));
        assert!((start + 1..end).all(|i| !ok.contains(&i)));
    }

    #[test]
    fn the_outcome_leaves_out_where_the_blocks_fell() {
        let one = "size 2x1\nreply 1b\nrow 0 |a|\n";
        let two = "checkpoint 1\nsize 2x1\nreply 1b\nrow 0 ||\ncheckpoint 2\nsize 2x1\nrow 0 |a|\n";
        assert_eq!(outcome(one), outcome(two));
    }
}
