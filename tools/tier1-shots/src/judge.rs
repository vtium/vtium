//! The match check: an output against its golden, and the lines where
//! they differ.
//!
//! A replay case's output is compared after the masking that
//! `conformance run` applies (INTERFACE.md, "Comparing with the goldens"):
//! a nonzero `changed` or `frame-since` count reads as `N`, and a
//! placement's `changed` flag goes. A scenario's transcript is already
//! masked, so it is compared as it is. The lines that differ are found on
//! the compared text, and each is reported as its file holds it, so a
//! `changed 0` in the golden that the output prints as `changed 1` reads
//! as those two lines.

use vtium_conformance::runner::mask_line;

/// Which side of the comparison a differing line is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// A line of the golden that the output does not have.
    Golden,
    /// A line of the output that the golden does not have.
    Output,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Golden => "golden",
            Side::Output => "output",
        }
    }
}

/// One line where an output differs from its golden. `number` counts the
/// lines of its own side from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub side: Side,
    pub number: usize,
    pub text: String,
}

/// How the two sides are read before they are compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Masking {
    /// A replay case's golden: the counts are masked as `conformance run`
    /// masks them.
    Counts,
    /// A scenario's transcript, compared as it is.
    None,
}

/// The largest table of line pairs [`differing`] matches up; past it,
/// every line between the common head and tail is reported.
const PAIRS_MAX: usize = 16_000_000;

/// The lines where `output` differs from `golden`, in the order a unified
/// diff gives them: in each stretch that differs, the golden's lines,
/// then the output's. Empty when the two match.
pub fn differing(golden: &str, output: &str, masking: Masking) -> Vec<Line> {
    let raw_g: Vec<&str> = golden.lines().collect();
    let raw_o: Vec<&str> = output.lines().collect();
    let key = |l: &&str| match masking {
        Masking::Counts => mask_line(l),
        Masking::None => (*l).to_owned(),
    };
    let g: Vec<String> = raw_g.iter().map(key).collect();
    let o: Vec<String> = raw_o.iter().map(key).collect();
    let head = g.iter().zip(&o).take_while(|(a, b)| a == b).count();
    let tail = g[head..]
        .iter()
        .rev()
        .zip(o[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (&g[head..g.len() - tail], &o[head..o.len() - tail]);
    let mut out = Vec::new();
    let mut emit = |side: Side, from: usize, to: usize| {
        let raw = match side {
            Side::Golden => &raw_g,
            Side::Output => &raw_o,
        };
        for n in from..to {
            out.push(Line {
                side,
                number: head + n + 1,
                text: raw[head + n].to_owned(),
            });
        }
    };
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in common(a, b) {
        emit(Side::Golden, i, mi);
        emit(Side::Output, j, mj);
        (i, j) = (mi + 1, mj + 1);
    }
    emit(Side::Golden, i, a.len());
    emit(Side::Output, j, b.len());
    out
}

/// The pairs of equal lines of a longest common subsequence of `a` and
/// `b`, in order; none when the table would pass [`PAIRS_MAX`].
fn common(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 || n.saturating_mul(m) > PAIRS_MAX {
        return Vec::new();
    }
    // len[i][j]: the longest common subsequence of a[i..] and b[j..]
    let w = m + 1;
    let mut len = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            len[i * w + j] = if a[i] == b[j] {
                len[(i + 1) * w + j + 1] + 1
            } else {
                len[(i + 1) * w + j].max(len[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            (i, j) = (i + 1, j + 1);
        } else if len[(i + 1) * w + j] >= len[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(side: Side, number: usize, text: &str) -> Line {
        Line {
            side,
            number,
            text: text.to_owned(),
        }
    }

    #[test]
    fn an_output_equal_to_its_golden_differs_nowhere() {
        let text = "size 4x2\ncursor 0,0\nrow 0 |ab  |\n";
        assert_eq!(differing(text, text, Masking::Counts), []);
    }

    // the counts `conformance run` masks are equal when both are nonzero
    #[test]
    fn masked_counts_match_and_a_zero_count_against_a_nonzero_one_differs() {
        let golden = "changed 3\nplacement 1 0,0 changed\nchanged 0\n";
        let output = "changed 7\nplacement 1 0,0\nchanged 1\n";
        assert_eq!(
            differing(golden, output, Masking::Counts),
            [
                line(Side::Golden, 3, "changed 0"),
                line(Side::Output, 3, "changed 1"),
            ]
        );
    }

    #[test]
    fn a_transcript_is_compared_as_it_is() {
        let diff = differing("changed 3\n", "changed 7\n", Masking::None);
        assert_eq!(
            diff,
            [
                line(Side::Golden, 1, "changed 3"),
                line(Side::Output, 1, "changed 7"),
            ]
        );
    }

    // the last line of either side counts: a missing last line is a
    // difference, and so is a last line that changed
    #[test]
    fn the_last_line_is_compared() {
        assert_eq!(
            differing("a\nb\nc\n", "a\nb\n", Masking::Counts),
            [line(Side::Golden, 3, "c")]
        );
        assert_eq!(
            differing("a\nb\nc\n", "a\nb\nd\n", Masking::Counts),
            [line(Side::Golden, 3, "c"), line(Side::Output, 3, "d")]
        );
    }

    // a line added in the middle is one line, not every line after it
    #[test]
    fn an_added_line_is_reported_alone() {
        assert_eq!(
            differing("a\nb\nc\nd\n", "a\nb\nx\nc\nd\n", Masking::Counts),
            [line(Side::Output, 3, "x")]
        );
        assert_eq!(
            differing("a\nb\nc\nd\ne\n", "a\nx\nc\ny\ne\n", Masking::Counts),
            [
                line(Side::Golden, 2, "b"),
                line(Side::Output, 2, "x"),
                line(Side::Golden, 4, "d"),
                line(Side::Output, 4, "y"),
            ]
        );
    }
}
