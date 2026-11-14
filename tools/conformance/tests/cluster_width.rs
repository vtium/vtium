//! `cluster-width = single` against `double` on every case of the corpus
//! and of vtium's own cases (ADR-1006 D8).

use std::path::Path;

use vtium_conformance::replay::{run, run_with_cluster_width};
use vtium_conformance::runner::Corpus;
use vtium_conformance::view::View;
use vtium_term::ClusterWidth;

/// vtium's own cases that hold a cluster with a spacing mark or a virama
/// conjunct, which take more cells under `double` than under `single`.
const WIDENED: [&str; 4] = [
    "width-tamil",
    "width-devanagari",
    "width-malayalam",
    "width-kannada",
];

// ADR-1006 D8: under `single`, every cluster keeps the cells that ADR-1200
// D21 gives it, so `single` prints every case of the corpus and every case
// of vtium's own byte for byte as `double`, the reference host's rule,
// prints it. `make conformance` judges `double` against the goldens, so
// `single` matches them too. The cases that hold such a cluster are the
// test's positive control: under `single` each prints differently.
#[test]
fn single_replays_every_case_without_such_a_cluster_as_double_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut same = 0;
    let mut widened = Vec::new();
    for dir in ["corpus", "vtium"] {
        let corpus = Corpus::open(&root.join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}"));
        for case in &corpus.cases {
            for view in [View::Screen, View::Frame] {
                let input = corpus
                    .input(case, view)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let double = run(&input).map_err(|f| f.output);
                let single =
                    run_with_cluster_width(&input, ClusterWidth::Single).map_err(|f| f.output);
                if WIDENED.contains(&case.name.as_str()) {
                    assert_ne!(double, single, "{dir}/{} {view:?}", case.name);
                    widened.push(case.name.clone());
                } else {
                    assert_eq!(double, single, "{dir}/{} {view:?}", case.name);
                    same += 1;
                }
            }
        }
    }
    widened.dedup();
    assert_eq!(widened, WIDENED, "each widened case was replayed");
    assert!(same > 600, "only {same} replays compared");
}
