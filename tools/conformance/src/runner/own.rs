//! vtium's own cases: streams for behaviour the copied corpus does
//! not hold, under `vtium/`, beside the copied corpus and laid out as it
//! is, with `cases.list`, `cases/` and the goldens in `golden/` and
//! `golden-frame/`. `provisional.tsv` lists the cases, of this set or of
//! the corpus, whose goldens the maintainer has not reviewed yet: such a
//! golden judges nothing, so a difference from it is reported and fails
//! nothing. Every other case's golden judges.

use std::collections::BTreeMap;

/// The cases `provisional.tsv` lists, `CASE REASON` a line, with the
/// reason each waits for review.
pub fn provisional(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let line_no = n.saturating_add(1);
        let Some((case, reason)) = line.split_once('\t') else {
            return Err(format!("provisional.tsv:{line_no}: want CASE<TAB>REASON"));
        };
        if case.is_empty() || reason.trim().is_empty() {
            return Err(format!("provisional.tsv:{line_no}: want CASE<TAB>REASON"));
        }
        if out.insert(case.to_owned(), reason.to_owned()).is_some() {
            return Err(format!("provisional.tsv:{line_no}: {case} listed twice"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provisional_list_reads_each_case_once_with_its_reason() {
        let list = provisional("# CASE\tREASON\nsix-a\tnot reviewed\n\nsix-b\tnew\n");
        assert_eq!(list.map(|l| l.len()), Ok(2));
        assert!(provisional("six-a\n").is_err());
        assert!(provisional("six-a\t \n").is_err());
        assert!(provisional("six-a\tx\nsix-a\ty\n").is_err());
    }
}
