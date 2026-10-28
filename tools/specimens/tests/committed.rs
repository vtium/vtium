//! The committed specimens against a fresh drawing of them, so `cargo test`
//! fails on a change that moves one until the moved files are committed.

use std::path::Path;

#[test]
fn every_committed_specimen_matches_its_drawing() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specimens");
    let faults = vtium_specimens::check::check(&dir).unwrap();
    assert!(
        faults.is_empty(),
        "{}\n`make specimens` redraws them",
        faults.join("\n")
    );
}
